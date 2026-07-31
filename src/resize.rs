use alloc::boxed::Box;
use core::{marker::PhantomData, ptr::null_mut};

use crossbeam_utils::CachePadded;

use crate::{
    MPMCQueue,
    NewSized,
    Resize,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
    utils::Backoff,
};

/// A dynamically sized concurrent queue.
///
/// Operations on this queue are lock-free if `resize` is not actively used.
/// During an ongoing `resize` operation, `pop` and operations depending on it may block on stalled `pushes`.
pub struct GrowableQueueCore<T, Q> {
    cores: [AtomicPtr<Q>; 2],
    push_epoch: CachePadded<AtomicUsize>,
    pop_epoch: CachePadded<AtomicUsize>,
    active_pushes: CachePadded<[AtomicUsize; 2]>,
    active_reads: CachePadded<[AtomicUsize; 2]>,
    is_resizing: AtomicBool,
    _slot: PhantomData<T>,
}

impl<T, Q> GrowableQueueCore<T, Q>
where
    Q: NewSized,
{
    /// Constructs a new `Queue` with capacity `size`.
    #[track_caller]
    pub(crate) fn new(size: usize) -> GrowableQueueCore<T, Q> {
        GrowableQueueCore {
            cores: [
                AtomicPtr::new(Box::into_raw(Box::new(Q::with_size(size)))),
                AtomicPtr::new(Box::into_raw(Box::new(Q::with_size(1)))),
            ],
            active_pushes: [AtomicUsize::new(0), AtomicUsize::new(0)].into(),
            active_reads: [AtomicUsize::new(0), AtomicUsize::new(0)].into(),
            push_epoch: AtomicUsize::new(0).into(),
            pop_epoch: AtomicUsize::new(0).into(),
            is_resizing: AtomicBool::new(false),
            _slot: PhantomData,
        }
    }
}

impl<T, Q> Drop for GrowableQueueCore<T, Q> {
    fn drop(&mut self) {
        let left = self.cores[0].swap(null_mut(), Ordering::Acquire);
        // Safety:
        // No concurrent drops of this ds can happen.
        // This queue was allocated in `new` or in `grow_by` with `Box::into_raw` and was not deallocated since then.
        _ = unsafe { Box::from_raw(left) };

        let right = self.cores[1].swap(null_mut(), Ordering::Acquire);
        // Safety:
        // No concurrent drops of this ds can happen.
        // This queue was allocated in `new` or in `grow_by` with `Box::into_raw` and was not deallocated since then.
        _ = unsafe { Box::from_raw(right) };
    }
}

// we need SeqCst on epoch atomics and active_* atomics on the synchronization points,
// because we have to synchronize between an asymmetric read/write - write/read pattern across the two

impl<T, Q> Resize for GrowableQueueCore<T, Q>
where
    Q: NewSized + MPMCQueue<Item = T>,
{
    fn resize(&self, size: usize) -> bool {
        if size == 0 {
            return false;
        }
        #[cfg(loom)]
        crate::sync::atomic::fence(Ordering::SeqCst);
        let push_epoch = self.push_epoch.load(Ordering::SeqCst);
        let pop_epoch = self.pop_epoch.load(Ordering::SeqCst);

        if pop_epoch != push_epoch {
            return false;
        }

        if self.active_reads[(push_epoch + 1) % 2].load(Ordering::Acquire) != 0 {
            // could happen if some thread started reading before pop_epoch got updated
            return false;
        }

        if self.is_resizing.swap(true, Ordering::AcqRel) {
            return false;
        }

        if self.push_epoch.load(Ordering::Acquire) != push_epoch {
            // could happen if an entire resize happens between load and this check
            self.is_resizing.store(false, Ordering::Release);
            return false;
        }

        // at this poitn we know that
        // a) no concurrent resize is happening
        // b) since pop_epoch == push_epoch the old queue is empty.
        // c) since pop_epoch == push_epoch AND active_reads == 0, we know that active_reads is STILL 0, becasue noone will acces the stale queue

        let old_idx = (push_epoch + 1) % 2;
        let mut backoff = Backoff::new();

        // wait on any stale readers of the old queue.
        // Note that at any point during AND after this loop other threads may still register as NEW readers on the OLD queue;
        // This is safe, because they will revalidate the epoch after registration and NOT actually read the underlying queue.
        // However this menas that we may spin for longer than strictly necessary to ensure noone is actually reading the old queue.
        while {
            crate::sync::atomic::fence(Ordering::SeqCst);
            self.active_reads[old_idx].load(Ordering::Acquire) != 0
                || self.active_pushes[old_idx].load(Ordering::Acquire) != 0
        } {
            backoff.backoff();
        }

        let new_queue = Box::into_raw(Box::new(Q::with_size(size)));

        // Safety:
        // since pop_epoch == push_epoch all concurrent threads acces the queue at push_epoch % 2.
        // pop ensures that no pushes are in flight to the old queue anymore and that it is empty. We can safely drop it.
        let old_queue = self.cores[old_idx].swap(new_queue, Ordering::AcqRel);

        self.push_epoch.fetch_add(1, Ordering::Release);

        // Safety:
        // old_queue was ocnstucted from a Box::into_raw and is dropped only once, as ensured by epoch guards
        let q = unsafe { Box::from_raw(old_queue) };

        #[cfg(not(any(loom, shuttle)))]
        debug_assert!(q.pop().is_none());
        #[cfg(any(loom, shuttle))]
        assert!(q.pop().is_none());

        self.is_resizing.store(false, Ordering::Release);
        true
    }
}

impl<T, Q> GrowableQueueCore<T, Q> {
    fn get_queue(&self, epoch: usize) -> &Q {
        let queue = self.cores[epoch % 2].load(Ordering::Acquire);
        // Safety:
        // It is guranteed by `resize` that no concurrent mutable access can happen to any queue in cores.
        // It is safe to access it concurrently via shared ref, as long as queue core is Sync.
        unsafe { &*queue }
    }

    fn register_push(&self, target_epoch: usize) -> bool {
        #[cfg(loom)]
        crate::sync::atomic::fence(Ordering::SeqCst);
        self.active_reads[target_epoch % 2].fetch_add(1, Ordering::SeqCst);

        #[cfg(loom)]
        crate::sync::atomic::fence(Ordering::SeqCst);
        let current_push = self.push_epoch.load(Ordering::SeqCst);

        // It is safe to read if the target epoch is still structurally active
        if target_epoch != current_push {
            self.deregister_reader(target_epoch);
            return false;
        }
        true
    }

    fn register_pop(&self, target_epoch: usize) -> bool {
        #[cfg(loom)]
        crate::sync::atomic::fence(Ordering::SeqCst);
        self.active_reads[target_epoch % 2].fetch_add(1, Ordering::SeqCst);

        #[cfg(loom)]
        crate::sync::atomic::fence(Ordering::SeqCst);
        let current_pop = self.pop_epoch.load(Ordering::SeqCst);

        // It is safe to read if the target epoch is still structurally active
        if target_epoch != current_pop {
            self.deregister_reader(target_epoch);
            return false;
        }
        true
    }

    fn deregister_reader(&self, epoch: usize) {
        self.active_reads[epoch % 2].fetch_sub(1, Ordering::Release);
    }
}

impl<T, Q> GrowableQueueCore<T, Q>
where
    Q: MPMCQueue<Item = T>,
{
    fn try_pop_from(
        &self,
        epoch: usize,
        registration: impl Fn(&Self, usize) -> bool,
    ) -> Result<Option<T>, ()> {
        if !registration(self, epoch) {
            return Err(());
        }

        let item = self.get_queue(epoch).pop();

        self.deregister_reader(epoch);

        Ok(item)
    }
}

impl<T, Q> MPMCQueue for GrowableQueueCore<T, Q>
where
    Q: MPMCQueue<Item = T>,
{
    type Item = T;

    fn push(&self, item: Self::Item) -> Result<(), Self::Item> {
        let mut backoff = Backoff::new();
        loop {
            let push_epoch = self.push_epoch.load(Ordering::Acquire);
            self.active_pushes[push_epoch % 2].fetch_add(1, Ordering::Release);

            #[cfg(loom)]
            crate::sync::atomic::fence(Ordering::SeqCst);
            if self.push_epoch.load(Ordering::SeqCst) == push_epoch {
                let r = self.get_queue(push_epoch).push(item);

                self.active_pushes[push_epoch % 2].fetch_sub(1, Ordering::Release);
                return r;
            }
            self.active_pushes[push_epoch % 2].fetch_sub(1, Ordering::Release);
            backoff.backoff();
        }
    }

    fn pop(&self) -> Option<Self::Item> {
        #[cfg(any(shuttle, loom))]
        let mut backoff = Backoff::new();

        // if push_epoch != pop_epoch, we need to drain the old queue.
        // In order to provide `empty-linearizability` we do a double collect over BOTH queues.
        //
        // If push_epoch == pop_epoch we only need to do another sweep IF push_epoch has changed by the end of this call
        for _ in 0..2 {
            loop {
                let push_epoch = self.push_epoch.load(Ordering::Acquire);
                let pop_epoch = self.pop_epoch.load(Ordering::Acquire);

                if pop_epoch != push_epoch {
                    // drain old buffer

                    // it is safe to call get_queue on pop_epoch here, since no resize can happen while we have not updated pop_epoch and reads on this epoch are happening
                    let Ok(item) = self.try_pop_from(pop_epoch, Self::register_pop) else {
                        #[cfg(any(shuttle, loom))]
                        backoff.backoff();
                        continue;
                    };

                    if item.is_some() {
                        return item;
                    }

                    if self.active_pushes[pop_epoch % 2].load(Ordering::Acquire) == 0 {
                        let Ok(item) = self.try_pop_from(pop_epoch, Self::register_pop) else {
                            #[cfg(any(shuttle, loom))]
                            backoff.backoff();
                            continue;
                        };

                        if item.is_some() {
                            return item;
                        }

                        _ = self.pop_epoch.compare_exchange_weak(
                            pop_epoch,
                            pop_epoch + 1,
                            Ordering::AcqRel,
                            Ordering::Relaxed,
                        );

                        #[cfg(any(shuttle, loom))]
                        backoff.backoff();
                        continue;
                    }

                    // at this point the old queue did not contain any items, even though items are in-flight. At this point the new queue may already contain items.
                    // We face a tradeoff:
                    //
                    // a) continue in the inner loop and block on the active_pushers -> violates non-blocking/obstruction-freedom guarantees
                    // b) check the new queue -> opens up the possibilty for item reordering, even in spsc scenarios, i.e. violates FIFO guarantees + linearizability
                    // It is worth noting here that the extend of reordering per item is bounded exactly by the number of threads concurrently executing `pop` during a `push` AND `resize`.
                    // c) bail, even though the queue is non-empty -> violates linearizability (and emptiness assumptions) in that case.
                    // Even worse: if some active_pusher is indefinitely dead, we will henceforth only bail. Thus this option implicitly blocks on the stalled pusher.
                    //
                    // c is of course unaccaptable.
                    //
                    // This would be circumventeable iff a helping mechanims where added or the old queue were inactivated, both of which is not possible given the opaque inner queue type.
                    //
                    // the fundamental question is:
                    // Do we want complete lock-freedom in `push` and `pop`, or do we want strict 0-FIFO ordering always.
                    // This tradeoff may fall differently in different contexts, however it seems reasonable to relax strict FIFO guarantees
                    // and accept N-FIFO semantics while resizing the queue. N-FIFO semantics should in practice keep most of the benefits of 0-FIFO, while stll preserving lock-freedom.
                    // Note that we do still preserve `empty-linearizability` here by double collecting:
                    // if the first iteration turns out to be double None, we have two possibilities:
                    // a) the old queue was truly empty at the point of popping from the new queue
                    // b) the old queue was non-empty at that point
                    // if the second iteration returns an item, this doesnt matter
                    // if it returns None also (for the old queue), then we have know that a was correct OR the item was popped by someone else, in which case we are also safe.
                }

                let Ok(item) = self.try_pop_from(push_epoch, Self::register_push) else {
                    #[cfg(any(shuttle, loom))]
                    backoff.backoff();
                    continue;
                };

                if item.is_none() && push_epoch != self.push_epoch.load(Ordering::Acquire) {
                    #[cfg(any(shuttle, loom))]
                    backoff.backoff();
                    continue;
                }

                if item.is_some() || push_epoch == pop_epoch {
                    return item;
                }
                // else do the second collect pass if we come from the slow path
                #[cfg(any(loom, shuttle))]
                backoff.backoff();
                break;
            }
        }

        // there was a linearizable time point during an iteration where both queues where truly empty
        None
    }

    fn capacity(&self) -> usize {
        // the capacity of the currently active queue, i.e. the number of elements that can be pushed directly after resize
        loop {
            let push_epoch = self.push_epoch.load(Ordering::Acquire);
            if !self.register_push(push_epoch) {
                continue;
            }
            let cap = self.get_queue(push_epoch).capacity();
            self.deregister_reader(push_epoch);
            return cap;
        }
    }

    fn len(&self) -> usize {
        // the total elements in the queue. Note that len can be > capacity.
        loop {
            let push_epoch = self.push_epoch.load(Ordering::Acquire);
            if !self.register_push(push_epoch) {
                continue;
            }

            let pop_epoch = self.pop_epoch.load(Ordering::Acquire);
            let pop_len = if pop_epoch != push_epoch {
                if !self.register_pop(pop_epoch) {
                    self.deregister_reader(push_epoch);
                    continue;
                }

                let pop_len = self.get_queue(pop_epoch).len();
                self.deregister_reader(pop_epoch);
                pop_len
            } else {
                0
            };

            let len = self.get_queue(push_epoch).len() + pop_len;
            self.deregister_reader(push_epoch);
            return len;
        }
    }

    fn is_empty(&self) -> bool {
        // the queue is empty if pop() returns None
        loop {
            let push_epoch = self.push_epoch.load(Ordering::Acquire);
            if !self.register_push(push_epoch) {
                continue;
            }

            let pop_epoch = self.pop_epoch.load(Ordering::Acquire);
            let pop_is_empty = if pop_epoch != push_epoch {
                if !self.register_pop(pop_epoch) {
                    self.deregister_reader(push_epoch);
                    continue;
                }

                let pop_is_empty = self.get_queue(pop_epoch).is_empty();
                self.deregister_reader(pop_epoch);
                pop_is_empty
            } else {
                true
            };

            let is_empty = self.get_queue(push_epoch).is_empty() && pop_is_empty;
            self.deregister_reader(push_epoch);
            return is_empty;
        }
    }

    fn is_full(&self) -> bool {
        // the queue is full if push() fails
        loop {
            let push_epoch = self.push_epoch.load(Ordering::Acquire);
            if !self.register_push(push_epoch) {
                continue;
            }
            let is_full = self.get_queue(push_epoch).is_full();
            self.deregister_reader(push_epoch);

            return is_full;
        }
    }
}

impl<T, Q> NewSized for GrowableQueueCore<T, Q>
where
    Q: NewSized,
{
    #[track_caller]
    fn with_size(size: usize) -> GrowableQueueCore<T, Q> {
        GrowableQueueCore::new(size)
    }
}

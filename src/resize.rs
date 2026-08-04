use alloc::boxed::Box;
use core::{marker::PhantomData, ptr::null_mut};

use crossbeam_utils::CachePadded;

use crate::{
    BoundedCollection,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
    utils::Backoff,
};

/// A dynamically resizable wrapper around a [`BoundedCollection`].
///
/// This type implements an algorithm that allows any [`BoundedCollection`] to be dynamically resized while preserving some of its properties.
///
/// ## Progress Guarantees:
///
/// - **Lock Freedom**: if the wrapped collection is lock-free, all corresponding operations on `Resizable` are also lock-free.
/// - **Obstruction Freedom**: if the wrapped collection exposes obstruction-free methods, all corresponding operations on `Resizable` are also obstruction-free.
///
/// `Resizable::resize` is blocking both on allocator and stale readers and writers.
///
/// ## Ordering and Consistency Guarantees:
///
/// - **Relaxed FIFO**: if the wrapped collection has FIFO ordering, `Resizable` has **k-FIFO** ordering with asymmetric delay and rank error.
/// - **Linearizability**: if the wrapped collection is linearizable, all operations on `Resizable` are also linearizable with respect to its relaxed FIFO specification.
/// - **Empty-Linearizability**: if the wrapped collection is linearizale, all operations on [`Resizable`] are empty-linearizable with respect to the wrapped collections specification.
///
/// If no call to `resize` happens, or in steady-state, `Resizable` has strict FIFO ordering and is strictly linearizable, given the same holds for the wrapped collection.
///
/// For more intormation on rank error and delay consult the crate level documentation.
#[derive(Debug)]
pub struct Resizable<Q> {
    cores: [AtomicPtr<Q>; 2],
    push_epoch: CachePadded<AtomicUsize>,
    pop_epoch: CachePadded<AtomicUsize>,
    active_pushes: CachePadded<[AtomicUsize; 2]>,
    active_reads: CachePadded<[AtomicUsize; 2]>,
    is_resizing: AtomicBool,
    _marker: PhantomData<Box<Q>>,
}

impl<Q> Resizable<Q> {
    /// Constructs a new `Resizable` from two raw [`BoundedCollection`] objects.
    pub fn from_parts(left: Q, right: Q) -> Self {
        Self {
            cores: [
                AtomicPtr::new(Box::into_raw(Box::new(left))),
                AtomicPtr::new(Box::into_raw(Box::new(right))),
            ],
            active_pushes: [AtomicUsize::new(0), AtomicUsize::new(0)].into(),
            active_reads: [AtomicUsize::new(0), AtomicUsize::new(0)].into(),
            push_epoch: AtomicUsize::new(0).into(),
            pop_epoch: AtomicUsize::new(0).into(),
            is_resizing: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }
}

impl<Q> Resizable<Q>
where
    Q: BoundedCollection,
{
    /// Constructs a new `Resizable` with capacity `size`.
    #[track_caller]
    pub fn with_capacity(size: usize) -> Resizable<Q> {
        Self::from_parts(Q::with_capacity(size), Q::with_capacity(1))
    }

    /// Constructs a new `Resizable`
    #[track_caller]
    pub fn new() -> Self {
        Self::from_parts(Q::with_capacity(1), Q::with_capacity(1))
    }
}

impl<Q> Default for Resizable<Q>
where
    Q: Default,
{
    fn default() -> Self {
        Self::from_parts(Q::default(), Q::default())
    }
}

impl<Q> Drop for Resizable<Q> {
    fn drop(&mut self) {
        let left = self.cores[0].swap(null_mut(), Ordering::Acquire);
        if !left.is_null() {
            // Safety:
            // No concurrent drops of this ds can happen.
            // This queue was allocated in `new` or in `grow_by` with `Box::into_raw` and was not deallocated since then.
            // we just checked that its non-null
            _ = unsafe { Box::from_raw(left) };
        }
        let right = self.cores[1].swap(null_mut(), Ordering::Acquire);
        if !right.is_null() {
            // Safety:
            // No concurrent drops of this ds can happen.
            // This queue was allocated in `new` or in `grow_by` with `Box::into_raw` and was not deallocated since then.
            // we just checked that its non-null
            _ = unsafe { Box::from_raw(right) };
        }
    }
}

// we need SeqCst on epoch atomics and active_* atomics on the synchronization points,
// because we have to synchronize between an asymmetric read/write - write/read pattern across the two

impl<Q> Resizable<Q>
where
    Q: BoundedCollection,
{
    /// Attempts to resize the capacity of the collection to `size` slots.
    ///
    /// **Note:** This method may block (on the allocator) or fail spuriously.
    /// Further a growth event may not be considered finished in regards of an other `resize` being possible until some time after the call to `resize`.
    ///
    /// Returns `true` if the resize was successfull, or `false` if
    /// it failed. Failure can occur due to thread
    /// contention, incomplete migration of the previous resize, i.e. staleness of the datastructure,
    /// or other implementation-specific/spurious conditions.
    pub fn resize(&self, size: usize) -> bool {
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

        let old_idx = (push_epoch + 1) % 2;

        // wait on any stale readers of the old queue.
        // Note that at any point during AND after this check other threads may still register as NEW readers on the OLD queue;
        // This is safe, because they will revalidate the epoch after registration and NOT actually read the underlying queue.
        // However this means that we may spuriously fail for longer than strictly necessary to ensure noone is actually reading the old queue.
        crate::sync::atomic::fence(Ordering::SeqCst);
        if self.active_reads[old_idx].load(Ordering::Acquire) != 0
            || self.active_pushes[old_idx].load(Ordering::Acquire) != 0
        {
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

        // at this point we know that
        // a) no concurrent resize is happening
        // b) since pop_epoch == push_epoch the old queue is empty.
        // c) since (b) and active_* were both 0 at some point t, all conccurrent ops that are registered on active_*[old_idx] will now revalidate their epoch before accessing the queue

        let new_queue = Box::into_raw(Box::new(Q::with_capacity(size)));

        // Safety:
        // since pop_epoch == push_epoch all concurrent threads acces the queue at push_epoch % 2.
        // pop ensures that no pushes are in flight to the old queue anymore and that it is empty. We can safely drop it.
        let old_queue = self.cores[old_idx].swap(new_queue, Ordering::AcqRel);

        self.push_epoch.fetch_add(1, Ordering::Release);

        // Safety:
        // old_queue was ocnstucted from a Box::into_raw and is dropped only once, as ensured by epoch guards
        let q = unsafe { Box::from_raw(old_queue) };

        #[cfg(not(any(loom, shuttle)))]
        debug_assert!(q.try_pop().is_none());
        #[cfg(any(loom, shuttle))]
        assert!(q.try_pop().is_none());

        self.is_resizing.store(false, Ordering::Release);
        true
    }
}

impl<Q> Resizable<Q> {
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

impl<Q> Resizable<Q>
where
    Q: BoundedCollection,
{
    fn try_pop_from(
        &self,
        epoch: usize,
        registration: impl Fn(&Self, usize) -> bool,
    ) -> Result<Option<Q::Item>, ()> {
        if !registration(self, epoch) {
            return Err(());
        }

        let item = self.get_queue(epoch).try_pop();

        self.deregister_reader(epoch);

        Ok(item)
    }
}

impl<Q> BoundedCollection for Resizable<Q>
where
    Q: BoundedCollection,
{
    type Item = Q::Item;

    fn try_push(&self, item: Self::Item) -> Result<(), Self::Item> {
        let mut backoff = Backoff::new();
        loop {
            let push_epoch = self.push_epoch.load(Ordering::Acquire);
            self.active_pushes[push_epoch % 2].fetch_add(1, Ordering::SeqCst);

            #[cfg(loom)]
            crate::sync::atomic::fence(Ordering::SeqCst);
            if self.push_epoch.load(Ordering::SeqCst) == push_epoch {
                let r = self.get_queue(push_epoch).try_push(item);

                self.active_pushes[push_epoch % 2].fetch_sub(1, Ordering::Release);
                return r;
            }
            self.active_pushes[push_epoch % 2].fetch_sub(1, Ordering::Release);
            backoff.backoff();
        }
    }

    fn try_pop(&self) -> Option<Self::Item> {
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

                    // at this point the old queue did not contain any items, even though items are in-flight. At this point the new container may already contain items.
                    // We face a tradeoff:
                    //
                    // a) continue in the inner loop and block on the active_pushers -> violates non-blocking/obstruction-freedom guarantees
                    // b) check the new container -> opens up the possibilty for item reordering, even in spsc scenarios, i.e. violates FIFO guarantees + linearizability
                    // It is worth noting here that the extend of reordering per item (i.e. the rank error) is bounded exactly by the number of threads concurrently executing `push`
                    // and the number of items reordered (i.e. the delay) is bounded by exactly those threads calling `pop` in this scenario.
                    // In practice the rank error and delay are much lower, because a specific schedule is reuqired for a reordering to happen.
                    // c) bail, even though the container is non-empty -> violates linearizability (and emptiness assumptions) in that case.
                    // Even worse: if some active_pusher is indefinitely dead, we will henceforth only bail. Thus this option implicitly blocks on the stalled pusher.
                    //
                    // c is of course unaccaptable.
                    //
                    // This would be circumventeable iff a helping mechanims where added or the old container were inactivated,
                    // both of which is not possible given the opaque inner container type and without large changes to the algorithm.
                    //
                    // the fundamental question is:
                    // Do we want complete lock-freedom in `push` and `pop`, or do we want strict 0-FIFO ordering + linearizability always.
                    // This tradeoff may fall differently in different contexts, however it seems reasonable to relax strict FIFO guarantees and linearizability
                    // and accept N-FIFO semantics while resizing the queue. N-FIFO semantics should in practice keep most of the benefits of 0-FIFO, while still preserving lock-freedom.
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

        // there was a linearizable time point during the iteration where both collections where truly empty
        None
    }

    fn capacity(&self) -> usize {
        // the capacity of the currently active collection, i.e. the number of elements that can be pushed directly after resize
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
        // the total elements in the collections. Note that len can be > capacity.
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
        // the collection is empty if pop() returns None
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
        // the collection is full if push() fails
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

    fn with_capacity(capacity: usize) -> Self {
        Resizable::with_capacity(capacity)
    }
}

// convenience methods

#[cfg(not(any(loom, shuttle, echeneis)))]
impl<Q> Resizable<Q> {
    /// Deconstructs a `Resizable` into its components.
    ///
    /// Returns the left and right raw collections currently used by this object.
    pub fn into_parts(self) -> [Box<Q>; 2] {
        let left = self.cores[0].swap(null_mut(), Ordering::Acquire);
        // Safety:
        // No concurrent owners of this can happen.
        // This collection was allocated in `new` or in `grow_by` with `Box::into_raw` and was not deallocated since then.
        // the collection will be dropped after this.
        let left = unsafe { Box::from_raw(left) };

        let right = self.cores[1].swap(null_mut(), Ordering::Acquire);
        // Safety:
        // No concurrent owners of this can happen.
        // This collection was allocated in `new` or in `grow_by` with `Box::into_raw` and was not deallocated since then.
        // the collection will be dropped after this.
        let right = unsafe { Box::from_raw(right) };

        [left, right]
    }

    /// returns mutable references to both wrapped collections.
    pub fn parts_mut(&mut self) -> [&mut Q; 2] {
        // Safety:
        // We are the only one accessing the wrapped collections.
        // No need for synchronization.
        // left and right can only be null during Drop.
        let left = unsafe { &mut **self.cores[0].get_mut() };
        // Safety:
        // We are the only one accessing the wrapped collections.
        // No need for synchronization.
        // left and right can only be null during Drop.
        let right = unsafe { &mut **self.cores[1].get_mut() };

        [left, right]
    }

    /// Deconstructs a `Resizable` into its currently active component.
    ///
    /// Returns the raw collection currently used by this object.
    pub fn into_current(mut self) -> Box<Q> {
        let push_epoch = *self.push_epoch.get_mut();
        let parts = self.into_parts();
        parts.into_iter().nth(push_epoch % 2).unwrap()
    }

    /// Returns a mutable reference to the currently active raw collection.
    pub fn current_mut(&mut self) -> &mut Q {
        // Safety:
        // We are the only one accessing the wrapped collections.
        // No need for synchronization.
        // left and right can only be null during Drop.
        unsafe { &mut **self.cores[*self.push_epoch.get_mut() % 2].get_mut() }
    }
}

#[cfg(not(any(loom, shuttle, echeneis)))]
impl<Q> Resizable<Q>
where
    Q: BoundedCollection,
{
    /// Attempts to pop an item from the collection.
    ///
    /// This method cicumvents the logic synchronization of `Resizable::try_pop`.
    pub fn pop_mut(&mut self) -> Option<Q::Item> {
        let pop_idx = *self.pop_epoch.get_mut() % 2;
        let push_idx = *self.push_epoch.get_mut() % 2;

        if pop_idx == push_idx {
            self.current_mut().try_pop()
        } else {
            let parts = self.parts_mut();
            parts[pop_idx].try_pop().or(parts[push_idx].try_pop())
        }
    }

    /// Attmepts to push an item into the collection.
    ///
    /// This method cicumvents the synchronization logic of `Resizable::try_push`.
    pub fn push_mut(&mut self, item: Q::Item) -> Result<(), Q::Item> {
        self.current_mut().try_push(item)
    }

    /// Clears the collection.
    pub fn clear(&mut self) {
        for core in self.parts_mut() {
            while core.try_pop().is_some() {}
        }
    }

    /// Migrates all remaining stale items in the old queue into the currently active queue, while ensuring enough capacity
    pub fn migrate(&mut self) {
        let pop_epoch = *self.pop_epoch.get_mut();
        let push_epoch = *self.push_epoch.get_mut();

        if pop_epoch == push_epoch {
            return;
        }

        let parts = self.parts_mut();
        let pop_queue = &parts[pop_epoch % 2];
        let push_queue = &parts[push_epoch % 2];

        let total_items = pop_queue.len() + push_queue.len();
        let empty_collection = Q::with_capacity(total_items);

        while let Some(item) = pop_queue.try_pop() {
            _ = empty_collection.try_push(item);
        }

        while let Some(item) = push_queue.try_pop() {
            _ = empty_collection.try_push(item);
        }

        let new_queue = Box::into_raw(Box::new(empty_collection));

        // Safety:
        // since pop_epoch == push_epoch all concurrent threads access the queue at push_epoch % 2.
        // pop ensures that no pushes are in flight to the old queue anymore and that it is empty. We can safely drop it.
        let old_queue = self.cores[pop_epoch % 2].swap(new_queue, Ordering::Relaxed);

        self.push_epoch.fetch_add(1, Ordering::Relaxed);
        self.pop_epoch.store(push_epoch + 1, Ordering::Relaxed);

        // Safety:
        // old_queue was consrtucted from a Box::into_raw and is dropped only once, as ensured by epoch guards
        let _q = unsafe { Box::from_raw(old_queue) };
    }

    // TODO add drain(..)
}

#[cfg(not(any(loom, shuttle, echeneis)))]
impl<Q: BoundedCollection> Extend<Q::Item> for Resizable<Q> {
    fn extend<I: IntoIterator<Item = Q::Item>>(&mut self, iter: I) {
        for item in iter {
            if let Err(item) = self.push_mut(item) {
                // we ran out of space
                // ensure that we can resize
                self.migrate();
                let cap = self.capacity();
                // make more space
                self.resize(cap * 2);
                _ = self.push_mut(item);
            }
        }
    }
}

/// An iterator over a [`Resizable`]
pub struct IntoIter<Q: BoundedCollection> {
    old: Option<Box<Q>>,
    new: Option<Box<Q>>,
}

impl<Q: BoundedCollection> Iterator for IntoIter<Q> {
    type Item = Q::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(old_q) = &mut self.old {
            if let Some(item) = old_q.try_pop() {
                return Some(item);
            }
            self.old = None;
        }

        self.new.as_mut().and_then(|q| q.try_pop())
    }
}

#[cfg(not(any(loom, shuttle, echeneis)))]
impl<Q: BoundedCollection> IntoIterator for Resizable<Q> {
    type IntoIter = IntoIter<Q>;
    type Item = Q::Item;

    fn into_iter(mut self) -> Self::IntoIter {
        let push_epoch = *self.push_epoch.get_mut();
        let [left, right] = self.into_parts();

        #[allow(clippy::manual_is_multiple_of)]
        let (old, new) = if push_epoch % 2 == 0 {
            (right, left)
        } else {
            (left, right)
        };

        IntoIter {
            old: Some(old),
            new: Some(new),
        }
    }
}

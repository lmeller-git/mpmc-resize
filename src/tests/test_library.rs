#![allow(dead_code)]
//! Testing for mpmc-resize
//! Fucntionality is tested on a queue model, but of course other models could also be chosen and tested.
//!
//! Tests adapted from nblf-queue's test suite.
//! <https://github.com/lmeller-git/nblf-queue/tree/main/src/tests>

use std::collections::VecDeque;

use crate::{BoundedCollection, Resizable, sync::Mutex};

pub(crate) type ResizeLockedDeque<T> = Resizable<BoundedDeque<T>>;

#[derive(Debug)]
pub(crate) struct BoundedDeque<T> {
    deque: Mutex<VecDeque<T>>,
    max_capacity: usize,
}

impl<T> BoundedCollection for BoundedDeque<T> {
    type Item = T;

    fn with_capacity(capacity: usize) -> Self {
        Self {
            deque: Mutex::new(VecDeque::with_capacity(capacity)),
            max_capacity: capacity,
        }
    }

    fn try_push(&self, item: Self::Item) -> Result<(), Self::Item> {
        let mut guard = self.deque.lock();
        if guard.len() >= self.max_capacity {
            return Err(item);
        }
        guard.push_back(item);
        Ok(())
    }

    fn try_pop(&self) -> Option<Self::Item> {
        let mut guard = self.deque.lock();
        guard.pop_front()
    }

    fn len(&self) -> usize {
        self.deque.lock().len()
    }

    fn capacity(&self) -> usize {
        self.max_capacity
    }

    fn is_empty(&self) -> bool {
        self.deque.lock().is_empty()
    }
}

impl<Q> Resizable<Q>
where
    Q: BoundedCollection,
{
    pub(crate) fn force_push(&self, item: Q::Item) -> Option<Q::Item> {
        let mut item_container = None;
        self.force_push_and_do(item, |item| {
            item_container.replace(item);
        });
        item_container
    }

    pub(crate) fn force_push_and_do<F>(&self, mut item: Q::Item, mut f: F)
    where
        F: FnMut(Q::Item),
    {
        let mut backoff = crate::utils::Backoff::new();
        while let Err(item_) = self.try_push(item) {
            item = item_;
            backoff.backoff();
            if let Some(next_popped_item) = self.try_pop() {
                f(next_popped_item);
            }
        }
    }
}

#[cfg(not(loom))]
pub(crate) use stubs::*;

#[cfg(not(loom))]
mod stubs {
    use alloc::vec::Vec;

    use crate::{
        BoundedCollection,
        Resizable,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            thread,
            thread::scope,
        },
    };

    pub(crate) fn smoke<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32>,
    {
        q.try_push(7).unwrap();
        assert_eq!(q.try_pop(), Some(7));
        q.try_push(8).unwrap();
        assert_eq!(q.try_pop(), Some(8));
        assert!(q.try_pop().is_none());
    }

    pub(crate) fn smoke_long<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32>,
    {
        q.try_push(7).unwrap();
        assert_eq!(q.try_pop(), Some(7));
        q.try_push(8).unwrap();
        q.try_push(9).unwrap();
        assert_eq!(q.try_pop(), Some(8));
        assert_eq!(q.try_pop(), Some(9));
        assert!(q.try_pop().is_none());
    }

    pub(crate) fn len_empty_full<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = ()>,
    {
        assert_eq!(q.capacity(), 2);

        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
        assert!(!q.is_full());

        q.try_push(()).unwrap();

        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());
        assert!(!q.is_full());

        q.try_push(()).unwrap();

        assert_eq!(q.len(), 2);
        assert!(!q.is_empty());
        assert!(q.is_full());

        q.try_pop().unwrap();

        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());
        assert!(!q.is_full());
    }

    pub(crate) struct Drops(std::rc::Rc<AtomicUsize>);

    impl Drop for Drops {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::Release);
        }
    }

    pub(crate) fn drops<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = Box<Drops>>,
    {
        let counter = std::rc::Rc::new(AtomicUsize::new(q.capacity()));

        for _ in 0..q.capacity() {
            assert!(q.try_push(Box::new(Drops(counter.clone()))).is_ok());
        }

        drop(q);

        assert_eq!(counter.load(Ordering::Acquire), 0);
    }

    pub(crate) fn len<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 30;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 25_000;
        #[cfg(any(miri, loom, shuttle))]
        const CAP: usize = 40;
        #[cfg(not(any(miri, loom, shuttle)))]
        const CAP: usize = 1000;
        const ITERS: usize = CAP / 20;

        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
        assert_eq!(q.capacity(), CAP);

        for _ in 0..CAP / 10 {
            for i in 0..ITERS {
                q.try_push(i as u32).unwrap();
                assert_eq!(q.len(), i + 1);
            }

            for i in 0..ITERS {
                q.try_pop().unwrap();
                assert_eq!(q.len(), ITERS - i - 1);
            }
        }
        assert_eq!(q.len(), 0);
        assert!(q.is_empty());

        for i in 0..CAP {
            q.try_push(i as u32).unwrap();
            assert_eq!(q.len(), i + 1);
        }

        assert!(q.is_full());
        assert_eq!(q.len(), CAP);

        for _ in 0..CAP {
            q.try_pop().unwrap();
        }
        assert_eq!(q.len(), 0);
        assert!(q.is_empty());

        scope(|scope| {
            scope.spawn(|| {
                for i in 0..COUNT {
                    loop {
                        if let Some(x) = q.try_pop() {
                            assert_eq!(x, i as u32);
                            break;
                        }
                    }
                    let len = q.len();
                    assert!(len <= CAP);
                }
            });

            scope.spawn(|| {
                for i in 0..COUNT {
                    while q.try_push(i as u32).is_err() {}
                    let len = q.len();
                    assert!(len <= CAP);
                }
            });
        });
        assert_eq!(q.len(), 0);
    }

    pub(crate) fn force_push<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32>,
    {
        assert!(q.is_empty());

        for i in 0..q.capacity() {
            assert!(q.try_push(i as u32).is_ok());
        }

        assert!(q.is_full());

        assert!(q.try_push(42).is_err());

        for i in 0..q.capacity() {
            assert!(q.force_push(42).is_some_and(|item| item == i as u32));
        }

        assert!(q.is_full());
    }

    pub(crate) fn spsc<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 50;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 300_000;

        scope(|scope| {
            scope.spawn(|| {
                for i in 0..COUNT {
                    loop {
                        if let Some(x) = q.try_pop() {
                            assert_eq!(x, i as u32);
                            break;
                        }
                        crate::utils::Backoff::new().backoff();
                    }
                }
                assert!(q.try_pop().is_none());
            });

            scope.spawn(|| {
                for i in 0..COUNT {
                    while q.try_push(i as u32).is_err() {
                        crate::utils::Backoff::new().backoff();
                    }
                }
            });
        });
    }

    pub(crate) fn mpsc<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 10;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 30_000;
        const THREADS: usize = 4;

        let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..COUNT {
                        while q.try_push(i as u32).is_err() {
                            crate::utils::Backoff::new().backoff();
                        }
                    }
                });
            }
            for _ in 0..THREADS {
                for _ in 0..COUNT {
                    let n = loop {
                        if let Some(x) = q.try_pop() {
                            break x;
                        }
                        crate::utils::Backoff::new().backoff();
                    };
                    v[n as usize].fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }

    pub(crate) fn mpmc<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 20;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 75_000;
        const THREADS: usize = 4;

        let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for _ in 0..COUNT {
                        let n = loop {
                            if let Some(x) = q.try_pop() {
                                break x;
                            }
                            crate::utils::Backoff::new().backoff();
                        };
                        v[n as usize].fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..COUNT {
                        while q.try_push(i as u32).is_err() {
                            crate::utils::Backoff::new().backoff();
                        }
                    }
                });
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }

    pub(crate) fn mpmc_ring_buffer<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 20;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 75_000;
        const THREADS: usize = 2;

        let t = AtomicUsize::new(THREADS);
        let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    loop {
                        match t.load(Ordering::SeqCst) {
                            0 => {
                                while let Some(n) = q.try_pop() {
                                    v[n as usize].fetch_add(1, Ordering::SeqCst);
                                }
                                break;
                            }

                            _ => {
                                while let Some(n) = q.try_pop() {
                                    v[n as usize].fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }
                        crate::utils::Backoff::new().backoff();
                    }
                });
            }

            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..COUNT {
                        q.force_push_and_do(i as u32, |n| {
                            v[n as usize].fetch_add(1, Ordering::SeqCst);
                        });
                    }

                    t.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }

    pub(crate) fn linearizable<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 50;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 25_000;
        const THREADS: usize = 4;

        scope(|scope| {
            for _ in 0..THREADS / 2 {
                scope.spawn(|| {
                    for _ in 0..COUNT {
                        while q.try_push(42).is_err() {
                            crate::utils::Backoff::new().backoff();
                        }
                        q.try_pop().unwrap();
                    }
                });

                scope.spawn(|| {
                    for _ in 0..COUNT {
                        let try_popped = &mut false;
                        q.force_push_and_do(42, |_| {
                            if *try_popped {
                                panic!("try_popped multiple items")
                            }
                            *try_popped = true;
                        });
                        if !*try_popped {
                            q.try_pop().unwrap();
                        }
                    }
                });
            }
        });
    }

    pub(crate) fn mpmc_ring_buf_ptr<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = Box<usize>> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 50;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 75_000;
        const THREADS: usize = 2;

        let t = AtomicUsize::new(THREADS);
        let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    loop {
                        match t.load(Ordering::SeqCst) {
                            0 => {
                                while let Some(n) = q.try_pop() {
                                    v[*n].fetch_add(1, Ordering::SeqCst);
                                }
                                break;
                            }

                            _ => {
                                while let Some(n) = q.try_pop() {
                                    v[*n].fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }
                        crate::utils::Backoff::new().backoff();
                    }
                });
            }

            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..COUNT {
                        q.force_push_and_do(Box::new(i), |n| {
                            v[*n].fetch_add(1, Ordering::SeqCst);
                        });
                    }

                    t.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }

    pub(crate) fn smoke_grow<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32>,
    {
        let initial_cap = q.capacity();

        for i in 0..initial_cap {
            assert!(q.try_push(i as u32).is_ok());
        }

        assert!(q.is_full());
        assert!(q.try_push(42).is_err());

        assert!(q.resize(initial_cap * 2));
        assert_eq!(q.capacity(), initial_cap * 2);
        assert!(!q.is_full());

        let current_len = q.len();

        for i in initial_cap..(initial_cap * 2) {
            assert!(q.try_push(i as u32).is_ok());
        }

        assert!(q.len() > current_len);

        for i in 0..(q.len()) {
            assert_eq!(q.try_pop(), Some(i as u32));
        }

        assert!(q.is_empty());
    }

    pub(crate) fn smoke_shrink<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32>,
    {
        let initial_cap = q.capacity();

        for i in 0..initial_cap {
            assert!(q.try_push(i as u32).is_ok());
        }

        assert!(q.is_full());
        assert!(q.try_push(42).is_err());

        assert!(q.resize(initial_cap / 2));
        assert_eq!(q.capacity(), initial_cap / 2);

        assert!(!q.is_empty());

        let current_len = q.len();

        for _ in 0..q.len() {
            assert!(q.try_pop().is_some());
        }

        assert!(q.try_pop().is_none());

        assert!(q.is_empty());
        assert!(q.len() < current_len);

        assert!(q.resize(1));
        assert_eq!(q.capacity(), 1);
        assert!(q.try_push(42).is_ok());
        assert!(q.is_full());
    }

    pub(crate) fn mpsc_grow<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 20;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 10_000;
        const THREADS: usize = 4;
        const GROW_STEP: usize = 10;

        let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..COUNT {
                        loop {
                            if q.try_push(i as u32).is_ok() {
                                break;
                            }
                            _ = q.resize(GROW_STEP + q.capacity());
                            crate::utils::Backoff::new().backoff();
                        }
                    }
                });
            }

            for _ in 0..THREADS {
                for _ in 0..COUNT {
                    let n = loop {
                        if let Some(x) = q.try_pop() {
                            break x;
                        }
                        crate::utils::Backoff::new().backoff();
                    };
                    v[n as usize].fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }

    pub(crate) fn mpmc_resize<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 30;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 75_000;
        #[cfg(any(miri, loom, shuttle))]
        const RESIZE_ITER: usize = 5;
        #[cfg(not(any(miri, loom, shuttle)))]
        const RESIZE_ITER: usize = 100;
        const RESIZERS: usize = 2;
        const THREADS: usize = 4;

        let v = (0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..COUNT {
                        while q.try_push(i as u32).is_err() {
                            _ = q.resize(10 + q.capacity());
                            crate::utils::Backoff::new().backoff();
                        }
                    }
                });
            }

            for _ in 0..THREADS {
                scope.spawn(|| {
                    for _ in 0..COUNT {
                        let n = loop {
                            if let Some(x) = q.try_pop() {
                                break x;
                            }
                            crate::utils::Backoff::new().backoff();
                        };
                        v[n as usize].fetch_add(1, Ordering::SeqCst);
                    }
                });
            }

            for _ in 0..RESIZERS {
                scope.spawn(|| {
                    let mut backoff = crate::utils::Backoff::new();
                    for _ in 0..RESIZE_ITER {
                        q.resize(2 + q.capacity());
                        backoff.backoff();
                    }
                });
            }

            for _ in 0..RESIZERS {
                scope.spawn(|| {
                    let mut backoff = crate::utils::Backoff::new();
                    for _ in 0..RESIZE_ITER {
                        q.resize(q.capacity().max(2) - 2);
                        backoff.backoff();
                    }
                });
            }
        });

        for c in v {
            assert_eq!(c.load(Ordering::SeqCst), THREADS);
        }
    }

    pub(crate) fn grow_storm<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const THREADS: usize = 2;
        #[cfg(not(any(miri, loom, shuttle)))]
        const THREADS: usize = 8;
        #[cfg(any(miri, loom, shuttle))]
        const ITERS: usize = 10;
        #[cfg(not(any(miri, loom, shuttle)))]
        const ITERS: usize = 2000;

        let tracking_vector = (0..ITERS).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();

        scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for i in 0..ITERS {
                        if i % 5 == 0 {
                            let _ = q.resize(2 + q.capacity());
                        }

                        let mut backoff = crate::utils::Backoff::new();
                        loop {
                            if q.try_push(i as u32).is_ok() {
                                break;
                            }
                            backoff.backoff();
                        }
                    }
                });

                scope.spawn(|| {
                    for i in 0..ITERS {
                        if i % 3 == 0 {
                            let _ = q.resize(1 + q.capacity());
                        }

                        let mut backoff = crate::utils::Backoff::new();
                        let item = loop {
                            if let Some(x) = q.try_pop() {
                                break x;
                            }
                            backoff.backoff();
                        };
                        tracking_vector[item as usize].fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });

        for count in tracking_vector {
            assert_eq!(count.load(Ordering::SeqCst), THREADS);
        }
    }

    pub(crate) fn oscillation_grow<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(not(any(miri, loom, shuttle)))]
        const ITER: usize = 100;
        #[cfg(any(miri, loom, shuttle))]
        const ITER: usize = 10;

        let total_try_popped = Arc::new(AtomicUsize::new(0));
        let total_try_pushed = Arc::new(AtomicUsize::new(0));

        scope(|scope| {
            scope.spawn(|| {
                for _ in 0..10 {
                    let mut backoff = crate::utils::Backoff::new();
                    for _ in 0..50 {
                        if q.resize(10 + q.capacity()) {
                            break;
                        }
                        backoff.backoff();
                    }
                    thread::yield_now();
                }
            });

            scope.spawn(|| {
                for _ in 1..ITER {
                    let mut try_pushes = 0;
                    let mut backoff_inner = crate::utils::Backoff::new();

                    let cap = q.capacity();

                    while try_pushes < cap {
                        if q.try_push(42).is_ok() {
                            try_pushes = total_try_pushed.fetch_add(1, Ordering::SeqCst) + 1;
                        }
                        backoff_inner.backoff();
                    }

                    while q.try_pop().is_some() {
                        total_try_popped.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
        });

        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
        assert_eq!(
            total_try_popped.load(Ordering::SeqCst),
            total_try_pushed.load(Ordering::SeqCst)
        );
    }

    pub(crate) fn len_grow<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 30;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 20_000;
        #[cfg(any(miri, loom, shuttle))]
        const CAP: usize = 40;
        #[cfg(not(any(miri, loom, shuttle)))]
        const CAP: usize = 500;
        const ITERS: usize = CAP / 20;

        assert_eq!(q.len(), 0);
        assert_eq!(q.capacity(), CAP);

        for _ in 0..CAP / 10 {
            for i in 0..ITERS {
                q.try_push(i as u32).unwrap();
                assert_eq!(q.len(), i + 1);
            }

            for i in 0..ITERS {
                q.try_pop().unwrap();
                assert_eq!(q.len(), ITERS - i - 1);
            }
        }
        assert_eq!(q.len(), 0);
        assert!(q.is_empty());

        for i in 0..CAP {
            q.try_push(i as u32).unwrap();
            assert_eq!(q.len(), i + 1);
        }

        assert!(q.is_full());
        assert_eq!(q.len(), CAP);

        for _ in 0..CAP {
            q.try_pop().unwrap();
        }
        assert_eq!(q.len(), 0);

        scope(|scope| {
            scope.spawn(|| {
                for _ in 0..COUNT {
                    loop {
                        if let Some(x) = q.try_pop() {
                            // nop strict 0-FIFO ordering during resize
                            assert!(x < COUNT as u32);
                            break;
                        }
                        crate::utils::Backoff::new().backoff();
                    }
                    let _len = q.len();
                }
            });

            scope.spawn(|| {
                for i in 0..COUNT {
                    let mut backoff = crate::utils::Backoff::new();
                    while q.try_push(i as u32).is_err() {
                        backoff.backoff();
                    }
                    let _len = q.len();
                }
            });

            scope.spawn(|| {
                #[cfg(any(miri, loom, shuttle))]
                const GROW_ITERS: usize = 3;
                #[cfg(not(any(miri, loom, shuttle)))]
                const GROW_ITERS: usize = 25;

                let mut backoff = crate::utils::Backoff::new();
                for _ in 0..GROW_ITERS {
                    _ = q.resize(CAP / 2 + q.capacity());
                    backoff.backoff();
                }
            });
        });

        assert_eq!(q.len(), 0);
    }

    pub(crate) fn suppl_methods_chaos<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(not(any(miri, loom, shuttle)))]
        const ITERS: usize = 10_000;
        #[cfg(any(miri, loom, shuttle))]
        const ITERS: usize = 30;
        #[cfg(not(any(miri, loom, shuttle)))]
        const GROW_CYCLES: usize = 500;
        #[cfg(any(miri, loom, shuttle))]
        const GROW_CYCLES: usize = 20;
        const GROW_STEP: usize = 10;

        let initial_cap = q.capacity();

        let total_grows = Arc::new(AtomicUsize::new(0));

        scope(|scope| {
            scope.spawn(|| {
                let mut last_cap = initial_cap;
                for _ in 0..ITERS {
                    let current_cap = q.capacity();

                    assert!(
                        current_cap >= last_cap,
                        "Monotonicity broken: Capacity shrank from {last_cap} to {current_cap}!"
                    );
                    last_cap = current_cap;

                    _ = q.is_full();
                }
            });

            scope.spawn(|| {
                for _ in 0..ITERS {
                    _ = q.len();
                    _ = q.is_empty();
                }
            });

            scope.spawn(|| {
                for i in 0..ITERS {
                    _ = q.try_push(i as u32);
                    _ = q.try_pop();
                }
            });

            scope.spawn(|| {
                for _ in 0..GROW_CYCLES {
                    if q.resize(GROW_STEP + q.capacity()) {
                        total_grows.fetch_add(1, Ordering::SeqCst);
                    }
                    thread::yield_now();
                }
            });
        });

        let final_cap = q.capacity();
        let expected_min_cap = initial_cap + (total_grows.load(Ordering::SeqCst) * GROW_STEP);
        assert!(
            final_cap >= expected_min_cap,
            "Structural integrity failed: Expected capacity >= {expected_min_cap}, but got {final_cap}",
        );
    }

    pub(crate) fn drops_resized<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = Box<Drops>>,
    {
        let counter = std::rc::Rc::new(AtomicUsize::new(q.capacity() + 5));

        for _ in 0..q.capacity() {
            assert!(q.try_push(Box::new(Drops(counter.clone()))).is_ok());
        }

        assert!(q.resize(5));

        for _ in 0..5 {
            assert!(q.try_push(Box::new(Drops(counter.clone()))).is_ok());
        }

        drop(q);

        assert_eq!(counter.load(Ordering::Acquire), 0);
    }

    pub(crate) fn linearizable_during_resize<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = u32> + Sync,
    {
        #[cfg(any(miri, loom, shuttle))]
        const COUNT: usize = 50;
        #[cfg(not(any(miri, loom, shuttle)))]
        const COUNT: usize = 25_000;
        #[cfg(any(miri, loom, shuttle))]
        const RESIZE_COUNT: usize = 5;
        #[cfg(not(any(miri, loom, shuttle)))]
        const RESIZE_COUNT: usize = 50;
        const THREADS: usize = 4;

        scope(|scope| {
            for _ in 0..THREADS / 2 {
                scope.spawn(|| {
                    for _ in 0..COUNT {
                        while q.try_push(42).is_err() {
                            crate::utils::Backoff::new().backoff();
                        }
                        q.try_pop().unwrap();
                    }
                });

                scope.spawn(|| {
                    for _ in 0..COUNT {
                        let try_popped = &mut false;
                        q.force_push_and_do(42, |_| {
                            if *try_popped {
                                panic!("try_popped multiple items")
                            }
                            *try_popped = true;
                        });
                        if !*try_popped {
                            q.try_pop().unwrap();
                        }
                    }
                });
            }

            scope.spawn(|| {
                for _ in 0..RESIZE_COUNT {
                    _ = q.resize(q.capacity() + 2);
                    thread::yield_now();
                }
            });
        });
    }

    pub(crate) fn push_pop_resize<Q>(q: Resizable<Q>)
    where
        Q: BoundedCollection<Item = i32> + Sync + Send + 'static,
    {
        const ITER: usize = 100;
        const RESIZE_ITER: usize = 5;

        let q = Arc::new(q);

        let received = Arc::new(
            (0..ITER)
                .map(|_| AtomicBool::new(false))
                .collect::<Vec<_>>(),
        );

        let q1 = q.clone();
        let try_push = thread::spawn(move || {
            for i in 0..ITER {
                while q1.try_push(i as i32).is_err() {
                    thread::yield_now();
                }
            }
        });

        let q2 = q.clone();
        let resize = thread::spawn(move || {
            for _ in 0..RESIZE_ITER {
                _ = q2.resize(q2.capacity() + 1);
                thread::yield_now();
            }
        });

        let q3 = q.clone();
        let rec = received.clone();
        let try_pop = thread::spawn(move || {
            for _ in 0..ITER {
                let item = loop {
                    if let Some(x) = q3.try_pop() {
                        break x;
                    }
                    thread::yield_now();
                };

                let prev_seen = rec[item as usize].swap(true, Ordering::SeqCst);
                assert!(!prev_seen, "Duplicate item try_popped: {}", item);
            }
        });

        try_push.join().unwrap();
        try_pop.join().unwrap();
        resize.join().unwrap();

        assert!(received.iter().all(|seen| seen.load(Ordering::SeqCst)));
        assert_eq!(q.len(), 0);
    }
}

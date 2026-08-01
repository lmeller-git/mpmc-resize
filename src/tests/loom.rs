use crate::{
    BoundedCollection,
    Resizable,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        thread,
    },
    tests::test_library::ResizeLockedDeque,
};

pub(crate) fn linearizable<Q>(q: Resizable<Q>)
where
    Q: BoundedCollection<Item = u32> + Sync + 'static,
{
    const COUNT: usize = 1;
    const THREADS: usize = 2;
    let q = Arc::new(q);

    let mut threads = Vec::new();

    for _ in 0..THREADS / 2 {
        let q2 = q.clone();
        threads.push(thread::spawn(move || {
            for _ in 0..COUNT {
                while q2.try_push(42).is_err() {
                    thread::yield_now();
                }
                q2.try_pop().unwrap();
            }
        }));

        let q = q.clone();
        threads.push(thread::spawn(move || {
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
        }));
    }

    for t in threads {
        t.join().unwrap();
    }
}

pub(crate) fn spsc<Q>(q: Resizable<Q>)
where
    Q: BoundedCollection<Item = u32> + Send + Sync + 'static,
{
    const COUNT: usize = 2;

    let q = Arc::new(q);

    let q_consumer = q.clone();
    let consumer = thread::spawn(move || {
        for i in 0..COUNT {
            loop {
                if let Some(x) = q_consumer.try_pop() {
                    assert_eq!(x, i as u32);
                    break;
                }
                crate::utils::Backoff::new().backoff();
            }
        }
        assert!(q_consumer.try_pop().is_none());
    });

    let q_producer = q.clone();
    let producer = thread::spawn(move || {
        for i in 0..COUNT {
            while q_producer.try_push(i as u32).is_err() {
                crate::utils::Backoff::new().backoff();
            }
        }
    });

    consumer.join().unwrap();
    producer.join().unwrap();
}

pub(crate) fn mpsc<Q>(q: Resizable<Q>)
where
    Q: BoundedCollection<Item = u32> + Send + Sync + 'static,
{
    const COUNT: usize = 2;
    const THREADS: usize = 2;

    let q = Arc::new(q);
    let v = Arc::new((0..COUNT).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let q = q.clone();
            thread::spawn(move || {
                for i in 0..COUNT {
                    while q.try_push(i as u32).is_err() {
                        crate::utils::Backoff::new().backoff();
                    }
                }
            })
        })
        .collect();

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

    for h in handles {
        h.join().unwrap();
    }

    for c in v.iter() {
        assert_eq!(c.load(Ordering::SeqCst), THREADS);
    }
}

pub(crate) fn push_pop_resize<Q>(q: Resizable<Q>)
where
    Q: BoundedCollection<Item = i32> + Send + Sync + 'static,
{
    const ITER: usize = 2;
    const RESIZE_ITER: usize = 1;

    let q = Arc::new(q);
    let received = Arc::new(
        (0..ITER)
            .map(|_| AtomicBool::new(false))
            .collect::<Vec<_>>(),
    );

    let q1 = q.clone();
    let try_push = thread::Builder::new()
        .name("try_push".into())
        .spawn(move || {
            for i in 0..ITER {
                while q1.try_push(i as i32).is_err() {
                    thread::yield_now();
                }
            }
        })
        .unwrap();

    let q2 = q.clone();
    let resize = thread::Builder::new()
        .name("resize".into())
        .spawn(move || {
            for _ in 0..RESIZE_ITER {
                _ = q2.resize(q2.capacity() + 1);
                thread::yield_now();
            }
        })
        .unwrap();

    let q3 = q.clone();
    let rec = received.clone();
    let try_pop = thread::Builder::new()
        .name("try_pop".into())
        .spawn(move || {
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
        })
        .unwrap();

    try_push.join().unwrap();
    try_pop.join().unwrap();
    resize.join().unwrap();

    assert!(received.iter().all(|seen| seen.load(Ordering::SeqCst)));
    assert_eq!(q.len(), 0);
}

pub(crate) fn linearizable_during_resize<Q>(q: Resizable<Q>)
where
    Q: BoundedCollection<Item = u32> + Send + Sync + 'static,
{
    const COUNT: usize = 1;
    const THREADS: usize = 2;
    let q = Arc::new(q);

    let mut threads = Vec::new();

    for _ in 0..THREADS / 2 {
        let q2 = q.clone();
        threads.push(thread::spawn(move || {
            for _ in 0..COUNT {
                while q2.try_push(42).is_err() {
                    thread::yield_now();
                }
                q2.try_pop().unwrap();
            }
        }));

        let q = q.clone();
        threads.push(thread::spawn(move || {
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
        }));
    }

    let q = q.clone();
    threads.push(thread::spawn(move || {
        _ = q.resize(q.capacity() + 1);
    }));

    for t in threads {
        t.join().unwrap();
    }
}

#[test]
fn linearizable_impl() {
    loom::model(|| {
        let q: ResizeLockedDeque<_> = Resizable::with_capacity(3);
        linearizable(q);
    });
}

#[test]
fn spsc_impl() {
    loom::model(|| {
        let q: ResizeLockedDeque<_> = Resizable::with_capacity(3);
        spsc(q);
    });
}

#[test]
fn mpsc_impl() {
    loom::model(|| {
        let q: ResizeLockedDeque<_> = Resizable::with_capacity(3);
        mpsc(q);
    });
}

#[test]
fn push_pop_resize_impl() {
    loom::model(|| {
        let q: ResizeLockedDeque<_> = Resizable::with_capacity(1);
        push_pop_resize(q);
    });
}

#[test]
fn linearizable_during_resize_impl() {
    loom::model(|| {
        let q: ResizeLockedDeque<_> = Resizable::with_capacity(2);
        linearizable_during_resize(q);
    });
}

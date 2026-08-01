use crate::{
    BoundedCollection,
    Resizable,
    tests::test_library::{
        Drops,
        ResizeLockedDeque,
        drops,
        drops_resized,
        force_push,
        grow_storm,
        len,
        len_empty_full,
        len_grow,
        linearizable,
        mpmc,
        mpmc_resize,
        mpmc_ring_buf_ptr,
        mpmc_ring_buffer,
        mpsc,
        mpsc_grow,
        oscillation_grow,
        smoke,
        smoke_grow,
        smoke_long,
        smoke_shrink,
        spsc,
        suppl_methods_chaos,
    },
};

#[test]
fn smoke_mut() {
    let mut queue: ResizeLockedDeque<_> = Resizable::with_capacity(4);

    assert!(queue.push_mut(10).is_ok());
    assert!(queue.push_mut(20).is_ok());
    assert!(queue.push_mut(30).is_ok());

    assert_eq!(queue.pop_mut(), Some(10));
    assert_eq!(queue.pop_mut(), Some(20));
    assert_eq!(queue.pop_mut(), Some(30));
    assert_eq!(queue.pop_mut(), None);
}

#[test]
fn mut_access() {
    let mut queue: ResizeLockedDeque<_> = Resizable::with_capacity(4);
    _ = queue.push_mut(1);

    {
        let current = queue.current_mut();
        assert_eq!(current.len(), 1);
    }

    {
        let parts = queue.parts_mut();
        assert_eq!(parts[0].len() + parts[1].len(), 1);
    }
}

#[test]
fn clear_mut() {
    let mut queue: ResizeLockedDeque<_> = Resizable::with_capacity(4);
    _ = queue.push_mut(1);
    _ = queue.push_mut(2);
    _ = queue.push_mut(3);

    queue.clear();

    assert_eq!(queue.pop_mut(), None);
}

#[test]
fn deconstruct() {
    let mut queue: ResizeLockedDeque<_> = Resizable::with_capacity(4);
    _ = queue.push_mut(42);

    let current_box = queue.into_current();
    assert_eq!(current_box.len(), 1);
}

#[test]
fn deconstruct_complete() {
    let mut queue: ResizeLockedDeque<_> = Resizable::with_capacity(4);
    _ = queue.push_mut(100);

    let [left, right] = queue.into_parts();
    assert_eq!(left.len() + right.len(), 1);
}

#[test]
fn extend() {
    let mut queue: ResizeLockedDeque<_> = Resizable::with_capacity(2);

    queue.extend([1, 2, 3, 4, 5]);

    let items: Vec<i32> = queue.into_iter().collect();
    assert_eq!(items, vec![1, 2, 3, 4, 5]);
}

#[test]
fn into_iter() {
    let mut queue: ResizeLockedDeque<_> = Resizable::with_capacity(2);
    _ = queue.push_mut(1);
    _ = queue.push_mut(2);

    queue.resize(4);
    _ = queue.push_mut(3);
    _ = queue.push_mut(4);

    let collected: Vec<i32> = queue.into_iter().collect();
    assert_eq!(collected, vec![1, 2, 3, 4]);
}

#[test]
fn migrate() {
    let mut queue: ResizeLockedDeque<_> = Resizable::with_capacity(2);
    _ = queue.push_mut(10);
    _ = queue.push_mut(20);

    queue.resize(4);
    _ = queue.push_mut(30);

    queue.migrate();

    let raw_queue = queue.current_mut();

    assert_eq!(raw_queue.try_pop(), Some(10));
    assert_eq!(raw_queue.try_pop(), Some(20));
    assert_eq!(raw_queue.try_pop(), Some(30));
    assert_eq!(raw_queue.try_pop(), None);
}

#[test]
fn smoke_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(2);
    smoke(q);
}

#[test]
fn smoke_shrink_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(2);
    smoke_shrink(q);
}

#[test]
fn smoke_long_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(10);
    smoke_long(q);
}

#[test]
fn len_empty_full_impl() {
    let q: ResizeLockedDeque<()> = Resizable::with_capacity(2);
    len_empty_full(q);
}

#[test]
fn drops_impl() {
    let q: ResizeLockedDeque<Box<Drops>> = Resizable::with_capacity(2);
    drops(q);
}

#[test]
fn len_impl() {
    #[cfg(miri)]
    const CAP: usize = 40;
    #[cfg(not(miri))]
    const CAP: usize = 1000;

    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(CAP);
    len(q);
}

#[test]
fn spsc_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(3);
    spsc(q);
}

#[test]
fn mpsc_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(3);
    mpsc(q);
}

#[test]
fn mpmc_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(3);
    mpmc(q);
}

#[test]
fn mpmc_ring_buffer_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(3);
    mpmc_ring_buffer(q);
}

#[test]
fn linearizable_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(4);
    linearizable(q);
}

#[test]
fn mpmc_ring_buf_ptr_impl() {
    let q: ResizeLockedDeque<Box<usize>> = Resizable::with_capacity(4);
    mpmc_ring_buf_ptr(q);
}

#[test]
fn force_push_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(4);
    force_push(q);
}

#[test]
fn drops_resized_impl() {
    let q: ResizeLockedDeque<Box<Drops>> = Resizable::with_capacity(2);
    drops_resized(q);
}

#[test]
fn smoke_grow_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(4);
    smoke_grow(q);
}

#[test]
fn mpsc_grow_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(4);
    mpsc_grow(q);
}

#[test]
fn mpmc_resize_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(4);
    mpmc_resize(q);
}

#[test]
fn len_grow_impl() {
    #[cfg(miri)]
    const CAP: usize = 40;
    #[cfg(not(miri))]
    const CAP: usize = 500;

    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(CAP);
    len_grow(q);
}

#[test]
fn grow_storm_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(2);
    grow_storm(q);
}

#[test]
fn oscillation_grow_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(2);
    oscillation_grow(q);
}

#[test]
fn suppl_methods_chaos_impl() {
    let q: ResizeLockedDeque<u32> = Resizable::with_capacity(2);
    suppl_methods_chaos(q);
}

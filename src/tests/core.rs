use crate::{
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

use crate::{
    Resizable,
    tests::test_library::{
        ResizeLockedDeque,
        grow_storm,
        len_grow,
        linearizable,
        linearizable_during_resize,
        mpmc,
        mpmc_resize,
        mpmc_ring_buffer,
        mpsc,
        mpsc_grow,
        oscillation_grow,
        push_pop_resize,
        spsc,
        suppl_methods_chaos,
    },
};

#[test]
fn spsc_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(3);
            spsc(q);
        },
        100,
        4,
    );
}

#[test]
fn mpmc_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(3);
            mpmc(q);
        },
        100,
        4,
    );
}

#[test]
fn mpmc_ring_buffer_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(3);
            mpmc_ring_buffer(q);
        },
        100,
        4,
    );
}

#[test]
fn mpsc_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(3);
            mpsc(q);
        },
        100,
        4,
    );
}

#[test]
fn linearizable_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(4);
            linearizable(q);
        },
        100,
        4,
    );
}

#[test]
fn mpsc_grow_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(4);
            mpsc_grow(q);
        },
        100,
        4,
    );
}

#[test]
fn mpmc_resize_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(4);
            mpmc_resize(q);
        },
        100,
        4,
    );
}

#[test]
fn len_grow_impl() {
    const CAP: usize = 40;
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(CAP);
            len_grow(q);
        },
        100,
        4,
    );
}

#[test]
fn grow_storm_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(4);
            grow_storm(q);
        },
        100,
        4,
    );
}

#[test]
fn oscillation_grow_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(4);
            oscillation_grow(q);
        },
        100,
        4,
    );
}

#[test]
fn suppl_methods_chaos_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(4);
            suppl_methods_chaos(q);
        },
        100,
        4,
    );
}

#[test]
fn linearizable_during_resize_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(4);
            linearizable_during_resize(q);
        },
        5000,
        100,
    );
}

#[test]
fn push_pop_resize_impl() {
    shuttle::check_pct(
        || {
            let q: ResizeLockedDeque<_> = Resizable::with_capacity(4);
            push_pop_resize(q);
        },
        1000,
        20,
    )
}

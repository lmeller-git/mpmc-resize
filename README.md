[![Codecov](https://codecov.io/github/lmeller-git/mpmc-resize/coverage.svg?branch=main)](https://codecov.io/gh/lmeller-git/mpmc-resize)
![CI Test](https://github.com/lmeller-git/mpmc-resize/actions/workflows/test.yml/badge.svg?branch=main)
![Safety Test](https://github.com/lmeller-git/mpmc-resize/actions/workflows/safety.yml/badge.svg?branch=main)
![no_std Test](https://github.com/lmeller-git/mpmc-resize/actions/workflows/nostd.yml/badge.svg?branch=main)
[![Crates.io](https://img.shields.io/crates/v/mpmc-resize)](https://crates.io/crates/mpmc-resize)
[![Docs.rs](https://docs.rs/mpmc-resize/badge.svg)](https://docs.rs/mpmc-resize)


# mpmc-resize

A construction that transform a bounded collection into a resizable collection.

<!-- cargo-rdme start -->

This crate implements a generic construction which allows dynamically resizing a wrapped datastructure, while preserving some key properties.

### Property Preservation

#### Progress Guarantees:

- **Lock Freedom**: if the wrapped collection is lock-free, all corresponding operations on [`Resizable`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html) are also lock-free.
- **Obstruction Freedom**: if the wrapped collection exposes obstruction-free methods, all corresponding operations on [`Resizable`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html) are also obstruction-free.

[`Resizable::resize`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html#method.resize) is blocking both on allocator and stale readers and writers.

#### Ordering and Consistency Guarantees:

- **Empty-Linearizability**: if the wrapped collection is empty-linearizable, all corresponding operations on [`Resizable`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html) are also empty-linearizable.
- **Relaxed FIFO**: if the wrapped collection has FIFO ordering, [`Resizable`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html) has **k-FIFO** ordering, where k is the highest number of threads concurrently calling [`BoundedCollection::try_pop`](https://docs.rs/mpmc-resize/latest/mpmc_resize/trait.BoundedCollection.html#tymethod.try_pop) during a [`Resizable::resize`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html#method.resize).

Specifically, for any item $x$, its rank displacement $k$ is strictly bounded by:

$$k \le K - P$$
and the total number of reorderd items per resize by
$$n \le min(K, L, M) - P$$

where
 $K$ is the number of concurrent calls to `try_push`
 $M$ is the number of concurren calls to `try_pop` overlapping with the $K$ executions
 $P$ is the number of calls to `try_pop` after the first $P$ of the $K$ pushes have finished and before any of the $M$ pops have finished
 $L$ is the number of pushes after $L$ of the $K$ executions have finished but before the $M$ executions have finished.

If no call to [`Resizable::resize`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html#method.resize) happens, or in steady-state, [`Resizable`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html) has strict FIFO ordering and is strictly linearizable, given the same holds for the wrapped collection.


### Limitations

#### Overhead

Preserving lock-free progress guarantees and linearizability across dynamic epoch shifts introduces some operational overhead:

- **Steady-State:** In a single-producer single-consumer (SPSC) scenario with no active resizes, wrapping a queue (e.g., `Resizable<ArrayQueue>`) achieves roughly **40% of the throughput** of the raw underlying queue.
- **Active Resizing:** Dynamically triggering resizes under load reduces throughput by an additional **~30%** during migration bursts. However, the throughput gained from higher buffer capacity can offset this cost over time.
- **Reordering in practice:** Item reordering is rare in practice and depends strongly on the number of threads concurrently invoking [`BoundedCollection::try_pop`](https://docs.rs/mpmc-resize/latest/mpmc_resize/trait.BoundedCollection.html#tymethod.try_pop) and [`BoundedCollection::try_push`](https://docs.rs/mpmc-resize/latest/mpmc_resize/trait.BoundedCollection.html#tymethod.try_push) during an active [`Resizable::resize`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html#method.resize).

Benchmarks can be found in `benches/main_benchmarks.rs`.

#### Resizing Frequency

The maximum frequency of successful resizes is currently bounded by the speed with which stale accesses get processed.
Before a second resize can happen, all items of the old raw collection must be removed and all accesses must be concluded.

If a thread gets permanently preempted while accessing a stale raw collection, no further items get removed, then no resize can happen again.

### Usage

To use [`Resizable`](https://docs.rs/mpmc-resize/latest/mpmc_resize/resize/struct.Resizable.html) on your type, you must implement [`BoundedCollection`](https://docs.rs/mpmc-resize/latest/mpmc_resize/trait.BoundedCollection.html) for it.

```rust
// Create a Resizable wrapping your datastructure
let container = Resizable::<MyQueue<i32>>::with_capacity(2);

// It implements all operations of a BoundedCollection
assert!(container.try_push(42).is_ok());
assert!(container.try_push(10).is_ok());
assert!(container.is_full());

// The container can now be dynamically resized, even if the wrapped datastructure cannot
assert!(container.resize(4));
assert!(!container.is_full());

assert!(container.try_push(30).is_ok());
assert_eq!(container.try_pop(), Some(42));
```

For more info on how to implement [`BoundedCollection`](https://docs.rs/mpmc-resize/latest/mpmc_resize/trait.BoundedCollection.html), refer to its documentation.

### Platform Support

All platforms supporting native atomic operations are supported.

The feature `atomic-fallback` may be used, if no native atomic operations are available.

### Feature Flags

- `std`: Enables `std` support.
- `atomic-fallback`:  Uses the `portable-atomic` fallback feature if native atomics are missing. It is discouraged to use this feature, as fallback atomics internally rely on locks.
- `default`: None

### Testing

Currently testing is based on:

- **Miri** - to validate pointer arithmetic and catch undefined behavior.
- **Loom and Shuttle** - to test for race conditions and non-blocking invariants.
- **ASan** - to check for memory corruption.

<!-- cargo-rdme end -->

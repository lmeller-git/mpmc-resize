//! This crate implements a generic construction which allows dynamically resizing a wrapped datastructure, while preserving some key properties.
//!
//! ## Property Preservation
//!
//! ### Progress Guarantees:
//!
//! - **Lock Freedom**: if the wrapped collection is lock-free, all corresponding operations on [`Resizable`] are also lock-free.
//! - **Obstruction Freedom**: if the wrapped collection exposes obstruction-free methods, all corresponding operations on [`Resizable`] are also obstruction-free.
//!
//! [`Resizable::resize`] is blocking both on allocator and stale readers and writers.
//!
//! ### Ordering and Consistency Guarantees:
//!
//! - **Empty-Linearizability**: if the wrapped collection is empty-linearizable, all corresponding operations on [`Resizable`] are also empty-linearizable.
//! - **Relaxed FIFO**: if the wrapped collection has FIFO ordering, [`Resizable`] has **k-FIFO** ordering, where k is the highest number of threads concurrently calling [`BoundedCollection::try_pop`] during a [`Resizable::resize`].
//!
//! Specifically, for any item $x$, its rank displacement $k$ is strictly bounded by:
//!
//! $$k \le K$$
//!
//! And its delay by:
//!
//! $$n \le M$$
//!
//! where
//!  $K$ is the number of concurrent calls to `try_push`
//!  $M$ is the number of concurren calls to `try_pop` overlapping with the $K$ executions
//!
//! For the reasoning behind this bound consult the document `docs/Relaxation.md`.
//!
//! If no call to [`Resizable::resize`] happens, or in steady-state, [`Resizable`] has strict FIFO ordering and is strictly linearizable, given the same holds for the wrapped collection.
//!
//!
//! ## Limitations
//!
//! ### Overhead
//!
//! Preserving lock-free progress guarantees and linearizability across dynamic epoch shifts introduces some operational overhead:
//!
//! - **Steady-State:** In a single-producer single-consumer (SPSC) scenario with no active resizes, wrapping a queue (e.g., `Resizable<ArrayQueue>`) achieves roughly **40% of the throughput** of the raw underlying queue.
//! - **Active Resizing:** Dynamically triggering resizes under load reduces throughput by an additional **~30%** during migration bursts. However, the throughput gained from higher buffer capacity can offset this cost over time.
//! - **Reordering in practice:** Item reordering is rare in practice and depends strongly on the number of threads concurrently invoking [`BoundedCollection::try_pop`] and [`BoundedCollection::try_push`] during an active [`Resizable::resize`].
//!
//! Benchmarks can be found in `benches/main_benchmarks.rs`.
//!
//! ### Resizing Frequency
//!
//! The maximum frequency of successful resizes is currently bounded by the speed with which stale accesses get processed.
//! Before a second resize can happen, all items of the old raw collection must be removed and all accesses must be concluded.
//!
//! If a thread gets permanently preempted while accessing a stale raw collection, no further items get removed, then no resize can happen again.
//!
//! ## Usage
//!
//! To use [`Resizable`] on your type, you must implement [`BoundedCollection`] for it.
//!
//! ```rust
//! # use mpmc_resize::{BoundedCollection, Resizable};
//! # use std::sync::Mutex;
//! # use std::collections::VecDeque;
//! #
//! # struct MyQueue<T> { deque: Mutex<VecDeque<T>>, cap: usize }
//! # impl<T> BoundedCollection for MyQueue<T> {
//! #     type Item = T;
//! #     fn with_capacity(cap: usize) -> Self { Self { deque: Mutex::new(VecDeque::with_capacity(cap)), cap } }
//! #     fn try_push(&self, item: T) -> Result<(), T> {
//! #         let mut g = self.deque.lock().unwrap();
//! #         if g.len() >= self.cap { Err(item) } else { g.push_back(item); Ok(()) }
//! #     }
//! #     fn try_pop(&self) -> Option<T> { self.deque.lock().unwrap().pop_front() }
//! #     fn len(&self) -> usize { self.deque.lock().unwrap().len() }
//! #     fn capacity(&self) -> usize { self.cap }
//! # }
//! // Create a Resizable wrapping your datastructure
//! let container = Resizable::<MyQueue<i32>>::with_capacity(2);
//!
//! // It implements all operations of a BoundedCollection
//! assert!(container.try_push(42).is_ok());
//! assert!(container.try_push(10).is_ok());
//! assert!(container.is_full());
//!
//! // The container can now be dynamically resized, even if the wrapped datastructure cannot
//! assert!(container.resize(4));
//! assert!(!container.is_full());
//!
//! assert!(container.try_push(30).is_ok());
//! assert_eq!(container.try_pop(), Some(42));
//! ```
//!
//! For more info on how to implement [`BoundedCollection`], refer to its documentation.
//!
//! ## Platform Support
//!
//! All platforms supporting native atomic operations are supported.
//!
//! The feature `atomic-fallback` may be used, if no native atomic operations are available.
//!
//! ## Feature Flags
//!
//! - `std`: Enables `std` support.
//! - `atomic-fallback`:  Uses the `portable-atomic` fallback feature if native atomics are missing. It is discouraged to use this feature, as fallback atomics internally rely on locks.
//! - `default`: None
//!
//! ## Testing
//!
//! Currently testing is based on:
//!
//! - **Miri** - to validate pointer arithmetic and catch undefined behavior.
//! - **Loom and Shuttle** - to test for race conditions and non-blocking invariants.
//! - **ASan** - to check for memory corruption.

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]
#![deny(clippy::missing_safety_doc, clippy::undocumented_unsafe_blocks)]
#![warn(unsafe_op_in_unsafe_fn)]

#[cfg(any(feature = "std", test))]
extern crate std;

extern crate alloc;

mod resize;
mod sync;
mod utils;

#[cfg(test)]
mod tests;

pub use resize::{IntoIter, Resizable};

/// This trait is used to describe a collection that is wrapped by a [`Resizable`].
///
/// Fallible operations on this type may spuriously fail.
///
/// `len`, `is_empty` and `is_full` will never be used for synchronization.
pub trait BoundedCollection {
    /// The item stored in the collection.
    type Item;

    /// Constructs a new instance of this type with capacity `capacity`.
    ///
    /// This method is explicitly allowed to block.
    fn with_capacity(capacity: usize) -> Self;

    /// Attempts to push an item into the collection.
    /// Returns the item as an error if the collection is full.
    fn try_push(&self, item: Self::Item) -> Result<(), Self::Item>;
    /// Attempts to pop an item from the collection.
    /// Returns `None` if the collection is empty.
    fn try_pop(&self) -> Option<Self::Item>;
    /// Returns the current length of the collection.
    /// The returned value may be stale under concurrent access and should not be used for synchronization.
    fn len(&self) -> usize;
    /// Returns the total capacity of the collection.
    fn capacity(&self) -> usize;

    /// Indicates whether the collection is empty.
    /// The returned value may be stale under concurrent access and should not be used for synchronization.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Indicates whether the collection is full.
    /// The returned value may be stale under concurrent access and should not be used for synchronization.
    fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }
}

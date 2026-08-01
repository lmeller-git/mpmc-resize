//! TODO

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

pub use resize::Resizable;

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

    /// Attempts to push an push into the collection.
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

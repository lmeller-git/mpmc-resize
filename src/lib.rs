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

pub use resize::GrowableQueueCore;

/// The main trait used to interface with a `MPMCQueue`.
/// All implementations provided by this crate are atomic and non-blocking.
/// Fallible operations of this trait may fail spuriously.
///
/// # Examples
///
/// ```rust
/// use nblf_queue::{StaticQueue, MPMCQueue};
///
/// let q: StaticQueue<_, 2> = StaticQueue::new();
///
/// assert!(q.push(42).is_ok());
/// assert!(q.push(2).is_ok());
///
/// assert_eq!(q.len(), 2);
/// assert!(q.is_full());
///
/// assert_eq!(q.force_push(0), Some(42));
/// assert!(q.is_full());
///
/// assert_eq!(q.pop(), Some(2));
/// assert_eq!(q.pop(), Some(0));
/// assert_eq!(q.len(), 0);
/// assert!(q.is_empty());
/// ```
pub trait MPMCQueue {
    /// The item stored in the queue
    type Item;

    /// Attempts to push an item into the queue.
    /// Returns the item as an error if the queue is full.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nblf_queue::{StaticQueue, MPMCQueue};
    ///
    /// let q: StaticQueue<_, 2> = StaticQueue::new();
    ///
    /// assert!(q.push(10).is_ok());
    /// assert!(q.push(20).is_ok());
    /// assert_eq!(q.push(30), Err(30));
    /// assert_eq!(q.pop(), Some(10));
    /// ```
    fn push(&self, item: Self::Item) -> Result<(), Self::Item>;
    /// Attempts to pop an item from the queue.
    /// Returns `None` if the queue was empty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nblf_queue::{StaticQueue, MPMCQueue};
    ///
    /// let q: StaticQueue<_, 2> = StaticQueue::new();
    ///
    /// assert!(q.push(10).is_ok());
    /// assert!(q.push(42).is_ok());
    /// assert_eq!(q.pop(), Some(10));
    /// assert_eq!(q.pop(), Some(42));
    /// assert!(q.pop().is_none());
    /// ```
    fn pop(&self) -> Option<Self::Item>;
    /// Returns the current len of the queue.
    /// The returned value may be stale under concurrent access and should not be used for synchronization.
    fn len(&self) -> usize;
    /// Returns the total capacity of the queue.
    fn capacity(&self) -> usize;

    /// Indicates whether the queue is empty.
    /// The returned value may be stale under concurrent access and should not be used for synchronization.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Indicates whether the queue is full.
    /// The returned value may be stale under concurrent access and should not be used for synchronization.
    fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    /// Pushes an item into the queue, removing an existing item if the queue is full.
    ///
    /// If the queue is full, this method will remove items until space becomes available.
    /// The last removed item is returned.
    ///
    /// Under contention this method may spin for some time, however it will never block.
    ///
    /// Note that the behaviour of this method depends on both `push` and `pop`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nblf_queue::{StaticQueue, MPMCQueue};
    ///
    /// let q: StaticQueue<_, 2> = StaticQueue::new();
    ///
    /// assert!(q.force_push(10).is_none());
    /// assert!(q.force_push(20).is_none());
    /// assert_eq!(q.force_push(30), Some(10));
    /// assert_eq!(q.pop(), Some(20));
    /// ```
    fn force_push(&self, item: Self::Item) -> Option<Self::Item> {
        let mut item_container = None;
        self.force_push_and_do(item, |item| {
            item_container.replace(item);
        });
        item_container
    }

    /// Pushes an item into the queue, removing an existing item if the queue is full.
    ///
    /// If the queue is full, this method will remove items until space becomes available.
    /// The provided closure will be called on each removed item.
    ///
    /// Under contention this method may spin for some time, however it will never block, provided the passed closure does not block.
    ///
    /// Note that the behaviour of this method depends on both `push` and `pop`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nblf_queue::{StaticQueue, MPMCQueue};
    ///
    /// let q: StaticQueue<_, 2> = StaticQueue::new();
    ///
    /// q.force_push_and_do(10, |item| {});
    /// q.force_push_and_do(20, |item| {});
    /// q.force_push_and_do(30, |item| {
    ///     assert_eq!(item, 10)
    /// });
    /// assert_eq!(q.pop(), Some(20));
    /// ```
    fn force_push_and_do<F>(&self, mut item: Self::Item, mut f: F)
    where
        F: FnMut(Self::Item),
    {
        let mut backoff = utils::Backoff::new();
        while let Err(item_) = self.push(item) {
            item = item_;
            backoff.backoff();
            if let Some(next_popped_item) = self.pop() {
                f(next_popped_item);
            }
        }
    }
}

/// An extension trait for `MPMCQueue` that allows dynamic resizing of the queue.
///
/// This trait makes **no** guarantees regarding the blocking behavior of the `resize` method itself.
/// It only guarantees that the core `MPMCQueue` operations maintain their original guarantees
/// and that the resize operation is atomically published to other threads.
///
/// # Examples
///
/// ```rust
/// #[cfg(feature = "dynamic")]
/// fn run() {
///  use nblf_queue::{DynamicQueue, MPMCQueue, Resize};
///
///  let q = DynamicQueue::new(1);
///
///  assert!(q.push(1).is_ok());
///  assert!(q.is_full());
///
///  assert!(q.resize(2 + q.capacity()));
///
///  assert_eq!(q.capacity(), 3);
///  assert!(!q.is_full());
///
///  assert!(q.push(2).is_ok());
///  assert!(q.push(3).is_ok());
///
///  assert_eq!(q.pop(), Some(1));
///  assert_eq!(q.pop(), Some(2));
///  assert_eq!(q.pop(), Some(3));
///
///  assert!(q.is_empty());
/// }
///
/// #[cfg(feature = "dynamic")]
/// run();
/// ```
pub trait Resize: MPMCQueue {
    /// Attempts to resize the capacity of the queue to `size` slots.
    ///
    /// **Note:** This method may block or fail spuriously.
    /// Further a growth event may not be considered finished in regards of an other `resize` being possible until some time after the call to `resize`.
    ///
    /// Returns `true` if the resize was successfull, or `false` if
    /// it failed. Failure can occur due to allocator exhaustion, thread
    /// contention, or other implementation-specific conditions.
    fn resize(&self, size: usize) -> bool;
}

/// a collection that can be coinstructed with a cap
pub trait NewSized {
    /// new with cap
    fn with_size(size: usize) -> Self;
}

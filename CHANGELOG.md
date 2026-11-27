# Version 0.1.5

- [FIXED]: Resizable::extend did not call Resizable::resize in release builds, silently dropping items.
- [FIXED]: Upgrade some atomic orderings in Resizable.
- [FIXED]: Resizable::pop_mut silently dropped items if both collections contained items.
- [FIXED]: Resizable::drain did not expect FIFO ordering in all cases.
- [CHANGED]: Resizable pop_mut and Resizable::clear methods may now finish ongoing resizes.
- [ADDED]: Add Resizable::try_shrink_stale_mut and Resizable::try_shrink_stale.

# Version 0.1.4

- [FIXED]: race in Resizable::resize
- [ADDED]: add Resizable::drain

# Version 0.1.3

- [FIXED]: race in Resizable::resize on stale accesses to old sub collection.

# Version 0.1.2

- [CHANGED]: Resizable::resize now only blocks on allocator. Previous blocks on stale readers are now failures.

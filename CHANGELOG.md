# Version 0.1.5

- [FIXED]: Resizable::extend did not call Resizable::resize in release builds, silently dropping items.
- [FIXED]: Upgrade some atomic orderings in Resizable.

# Version 0.1.4

- [FIXED]: race in Resizable::resize
- [ADDED]: add Resizable::drain

# Version 0.1.3

- [FIXED]: race in Resizable::resize on stale accesses to old sub collection.

# Version 0.1.2

- [CHANGED]: Resizable::resize now only blocks on allocator. Previous blocks on stale readers are now failures.

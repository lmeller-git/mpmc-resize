# Version 0.1.3

- [FIXED]: race in Resize::resize on stale accesses to old sub collection.

# Version 0.1.2

- [CHANGED]: Resize::resize now only blocks on allocator. Previous blocks on stale readers are now failures.

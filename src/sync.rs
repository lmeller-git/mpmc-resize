#![allow(unused_imports)]
#![allow(clippy::disallowed_modules)]

#[cfg(all(not(loom), not(shuttle), not(echeneis)))]
pub(crate) use core_::*;
#[cfg(all(echeneis, test))]
pub(crate) use echeneis_::*;
#[cfg(all(loom, test))]
pub(crate) use loom_::*;
#[cfg(all(shuttle, test))]
pub(crate) use shuttle_::*;

#[cfg(all(shuttle, test))]
mod shuttle_ {
    #[allow(unused_imports)]
    pub(crate) use shuttle::hint;
    pub(crate) use shuttle::{
        sync::{Arc, Weak, atomic},
        thread,
    };

    pub(crate) mod cell {
        #[derive(Debug)]
        pub(crate) struct UnsafeCell<T>(core::cell::UnsafeCell<T>);

        #[allow(dead_code)]
        impl<T> UnsafeCell<T> {
            pub(crate) fn new(data: T) -> UnsafeCell<T> {
                UnsafeCell(core::cell::UnsafeCell::new(data))
            }

            pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
                f(self.0.get())
            }
        }

        impl<T: Default> Default for UnsafeCell<T> {
            fn default() -> Self {
                Self::new(T::default())
            }
        }
    }
}

#[cfg(all(loom, test))]
mod loom_ {
    // no Weak in loom
    pub(crate) use std::sync::Weak;

    pub(crate) use loom::{
        cell,
        hint,
        sync::{Arc, atomic},
        thread,
    };
}

#[cfg(all(not(loom), not(shuttle), not(echeneis)))]
mod core_ {
    pub(crate) mod cell {
        #[derive(Debug)]
        pub(crate) struct UnsafeCell<T>(core::cell::UnsafeCell<T>);

        #[allow(dead_code)]
        impl<T> UnsafeCell<T> {
            pub(crate) fn new(data: T) -> UnsafeCell<T> {
                UnsafeCell(core::cell::UnsafeCell::new(data))
            }

            pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
                f(self.0.get())
            }
        }

        impl<T: Default> Default for UnsafeCell<T> {
            fn default() -> Self {
                Self::new(T::default())
            }
        }
    }
    #[cfg(any(feature = "alloc", test))]
    pub(crate) use alloc::sync::{Arc, Weak};
    pub(crate) use core::hint;
    #[cfg(any(feature = "std", test))]
    pub(crate) use std::thread;

    pub(crate) use portable_atomic as atomic;
}

#[cfg(all(echeneis, test))]
mod echeneis_ {
    pub(crate) use echeneis::sync::atomic;
    pub(crate) mod cell {
        #[derive(Debug)]
        pub(crate) struct UnsafeCell<T>(core::cell::UnsafeCell<T>);

        #[allow(dead_code)]
        impl<T> UnsafeCell<T> {
            pub(crate) fn new(data: T) -> UnsafeCell<T> {
                UnsafeCell(core::cell::UnsafeCell::new(data))
            }

            pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
                f(self.0.get())
            }
        }

        impl<T: Default> Default for UnsafeCell<T> {
            fn default() -> Self {
                Self::new(T::default())
            }
        }
    }
    #[cfg(any(feature = "alloc", test))]
    pub(crate) use alloc::sync::{Arc, Weak};
    pub(crate) use core::hint;
    #[cfg(any(feature = "std", test))]
    pub(crate) use std::thread;
}

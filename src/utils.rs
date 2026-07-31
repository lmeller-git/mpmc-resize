#[cfg(all(not(shuttle), not(loom)))]
const MAX_SPINLOOP: usize = 1024;

pub(crate) struct Backoff {
    #[cfg(all(not(shuttle), not(loom)))]
    state: usize,
}

impl Backoff {
    pub(crate) fn new() -> Self {
        #[cfg(all(not(shuttle), not(loom)))]
        return Self { state: 1 };
        #[cfg(any(shuttle, loom))]
        return Self {};
    }

    pub(crate) fn backoff(&mut self) {
        #[cfg(all(not(shuttle), not(loom)))]
        {
            for _ in 0..self.state {
                crate::sync::hint::spin_loop();
            }
            self.state = (self.state * 2).min(MAX_SPINLOOP);
        }
        #[cfg(any(shuttle, loom))]
        crate::sync::thread::yield_now();
    }
}

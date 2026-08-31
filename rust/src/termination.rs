//! Graceful terminal cleanup for the signals handled by the TypeScript panes.

#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};

/// A process signal flag that can be checked without work in the signal handler.
#[derive(Debug)]
pub(crate) struct Termination {
    #[cfg(unix)]
    requested: Arc<AtomicBool>,
}

impl Termination {
    /// Handle the same graceful-exit signals as the TypeScript terminal panes.
    pub(crate) fn install() -> Self {
        #[cfg(unix)]
        {
            use signal_hook::consts::signal::{SIGHUP, SIGTERM};

            let requested = Arc::new(AtomicBool::new(false));
            let _ = signal_hook::flag::register(SIGTERM, Arc::clone(&requested));
            let _ = signal_hook::flag::register(SIGHUP, Arc::clone(&requested));
            Self { requested }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }

    pub(crate) fn requested(&self) -> bool {
        #[cfg(unix)]
        {
            self.requested.load(Ordering::Relaxed)
        }
        #[cfg(not(unix))]
        {
            let _ = self;
            false
        }
    }
}

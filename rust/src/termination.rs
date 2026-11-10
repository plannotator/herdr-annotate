//! Graceful terminal cleanup for the signals handled by the TypeScript panes.
//!
//! The pane loops check [`Termination::requested`] between `event::poll` calls. Once the
//! terminal hangs up, crossterm's unix reader never returns: `read` gives `Ok(0)` forever and
//! `event::poll` spins at full CPU (crossterm-rs/crossterm#793), so the loop never sees the
//! signal. A watch thread ends the process when the loop cannot.

#[cfg(unix)]
use std::os::fd::BorrowedFd;
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(unix)]
use std::time::Duration;

/// How often the watch thread checks; also how long a signal is left to the pane loop.
#[cfg(unix)]
const INTERVAL: Duration = Duration::from_millis(500);

/// Exit status for a hang-up: the status SIGHUP itself would have left.
#[cfg(unix)]
const HANGUP_STATUS: i32 = 129;

/// A process signal flag that can be checked without work in the signal handler.
#[derive(Debug)]
pub(crate) struct Termination {
    /// The last graceful-exit signal received, or 0.
    #[cfg(unix)]
    signal: Arc<AtomicUsize>,
}

impl Termination {
    /// Handle the same graceful-exit signals as the TypeScript terminal panes.
    pub(crate) fn install() -> Self {
        #[cfg(unix)]
        {
            use signal_hook::consts::signal::{SIGHUP, SIGTERM};

            let signal = Arc::new(AtomicUsize::new(0));
            for number in [SIGTERM, SIGHUP] {
                let _ =
                    signal_hook::flag::register_usize(number, Arc::clone(&signal), number as usize);
            }
            watch(Arc::clone(&signal));
            Self { signal }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }

    pub(crate) fn requested(&self) -> bool {
        #[cfg(unix)]
        {
            self.signal.load(Ordering::Relaxed) != 0
        }
        #[cfg(not(unix))]
        {
            let _ = self;
            false
        }
    }
}

/// Exit when the terminal hangs up, or when a signal is still pending one interval after it
/// arrived. Without the thread the pane runs as before.
#[cfg(unix)]
fn watch(signal: Arc<AtomicUsize>) {
    let terminal = terminal([rustix::stdio::stdin(), rustix::stdio::stdout()]);
    let _ = std::thread::Builder::new()
        .name("termination".to_owned())
        .spawn(move || {
            let mut pending = 0;
            loop {
                std::thread::sleep(INTERVAL);
                if terminal.is_some_and(hung_up) {
                    // Not `restore`: it reports a failure on stderr, which is the hung-up
                    // terminal too, and that print panics inside ratatui's panic hook.
                    let _ = ratatui::try_restore();
                    std::process::exit(HANGUP_STATUS);
                }
                if pending != 0 {
                    // The loop had a whole interval and is stuck, maybe in a write the
                    // terminal is not reading. That write holds stdout, so leave it alone.
                    std::process::exit(128 + pending as i32);
                }
                pending = signal.load(Ordering::Relaxed);
            }
        });
}

/// The first descriptor that is a terminal. A pipe on stdin is passed over: a pipe whose
/// writer has finished reports a hang-up too.
#[cfg(unix)]
fn terminal(fds: [BorrowedFd<'_>; 2]) -> Option<BorrowedFd<'_>> {
    fds.into_iter().find(|fd| rustix::termios::isatty(fd))
}

/// `poll` reports the hang-up. `tcgetattr` and `isatty` cannot: on macOS both keep
/// succeeding on a terminal whose other end has closed.
#[cfg(unix)]
fn hung_up(fd: BorrowedFd<'_>) -> bool {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};

    let mut fds = [PollFd::new(&fd, PollFlags::IN)];
    let now = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    poll(&mut fds, Some(&now)).is_ok()
        && fds
            .iter()
            .any(|entry| entry.revents().contains(PollFlags::HUP))
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::expect_used, reason = "tests assert by panicking")]

    use std::os::fd::{AsFd as _, OwnedFd};

    use rustix::fs::{Mode, OFlags};
    use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};

    use super::*;

    /// A pseudo-terminal: the controller Herdr holds, and the terminal a pane holds.
    fn pty() -> (OwnedFd, OwnedFd) {
        let controller = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("openpt");
        // Close-on-exec, so a child another test spawns meanwhile cannot hold the controller
        // open and keep the hang-up from happening.
        rustix::io::fcntl_setfd(&controller, rustix::io::FdFlags::CLOEXEC).expect("cloexec");
        grantpt(&controller).expect("grantpt");
        unlockpt(&controller).expect("unlockpt");
        let name = ptsname(&controller, Vec::new()).expect("ptsname");
        let tty = rustix::fs::open(
            name.as_c_str(),
            OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .expect("open");
        (controller, tty)
    }

    #[test]
    fn a_terminal_reads_as_hung_up_only_after_its_other_end_closes() {
        let (controller, tty) = pty();
        assert!(!hung_up(tty.as_fd()));
        drop(controller);
        // A child that another test spawned in the moment before close-on-exec was set can
        // still hold the controller; the hang-up follows once it exits.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !hung_up(tty.as_fd()) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(hung_up(tty.as_fd()));
    }

    #[test]
    fn piped_stdin_is_passed_over_for_the_terminal() {
        let (reader, writer) = std::io::pipe().expect("pipe");
        drop(writer);
        let (_controller, tty) = pty();
        assert!(hung_up(reader.as_fd()), "a finished pipe reports a hang-up");
        let watched = terminal([reader.as_fd(), tty.as_fd()]).expect("a terminal");
        assert!(!hung_up(watched));
    }
}

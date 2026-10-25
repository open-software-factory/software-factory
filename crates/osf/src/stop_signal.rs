//! The signal watcher `osf sandbox run` installs: it records SIGINT or
//! SIGTERM in an atomic and lets the run loop remove the sandbox.

use std::sync::atomic::{AtomicI32, Ordering};

/// The last signal that arrived, or 0 for none.
static PENDING: AtomicI32 = AtomicI32::new(0);

/// Records `signal` for [`take`]; async-signal-safe.
extern "C" fn on_signal(signal: libc::c_int) {
    PENDING.store(signal, Ordering::SeqCst);
}

/// Installs [`on_signal`] for SIGINT and SIGTERM.
///
/// # Errors
/// Returns the OS error when `sigaction` refuses either handler.
pub fn install() -> std::io::Result<()> {
    // SAFETY: the action is zeroed and filled with a valid handler and empty mask.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_signal as *const () as libc::sighandler_t;
        action.sa_flags = 0;
        libc::sigemptyset(&raw mut action.sa_mask);
        for signal in [libc::SIGINT, libc::SIGTERM] {
            if libc::sigaction(signal, &raw const action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
    }
    Ok(())
}

/// Takes the last signal, resetting the pending value to none.
#[must_use]
pub fn take() -> Option<i32> {
    let signal = PENDING.swap(0, Ordering::SeqCst);
    (signal != 0).then_some(signal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_returns_a_stored_signal_once_then_none() {
        PENDING.store(libc::SIGTERM, Ordering::SeqCst);
        assert_eq!(take(), Some(libc::SIGTERM));
        assert_eq!(take(), None);
    }
}

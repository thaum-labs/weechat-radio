//! Per-test hang guard and always-shown CI lines for the harness tests
//! (QA #259). Shared by `#[path]`, so each test binary gets its own copy.

use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

/// Longest one harness test may run. The slowest planned case is about
/// 40 min on a loaded runner; the CI job itself stops at 240 min.
pub const TEST_LIMIT: Duration = Duration::from_secs(60 * 60);

/// Held for the length of a test. If the test is still running after
/// `limit`, the binary stops with the test's name in the log, instead of the
/// whole CI job sitting until its 4 h timeout.
pub struct Watchdog(#[allow(dead_code)] mpsc::Sender<()>);

/// Starts the guard for the test running on this thread (libtest names the
/// thread after the test).
pub fn watchdog(limit: Duration) -> Watchdog {
    let name = std::thread::current()
        .name()
        .unwrap_or("unnamed test")
        .to_string();
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        if let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(limit) {
            ci_line(&format!(
                "TEST HUNG: {name} still running after {} min; stopping this test binary",
                limit.as_secs() / 60
            ));
            std::process::exit(101);
        }
    });
    Watchdog(tx)
}

/// One line straight to stderr, so it shows in the CI log even when the test
/// passes (libtest only shows `eprintln!` output of a failed test).
#[allow(dead_code)]
pub fn ci_line(line: &str) {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
}

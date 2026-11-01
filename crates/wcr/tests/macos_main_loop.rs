//! SPDX-License-Identifier: Apache-2.0
//! macOS (QA #237): while wcr's tokio runtime runs on its worker thread, the
//! main thread serves its run loop, so a job posted to it (the way the
//! Bluetooth code opens and closes channels) runs within 1 s, on the main
//! thread, and the idle loop does not spin.
//!
//! No test harness: the harness would run tests off the main thread.
//! This proves the run loop, not that a real radio's audio arrives.

#[cfg(target_os = "macos")]
fn main() {
    use std::time::{Duration, Instant};
    use wcr::main_loop::{is_main_thread, on_main, run_with_main_loop};

    fn cpu_seconds() -> f64 {
        let mut u: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
        let t = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
        t(u.ru_utime) + t(u.ru_stime)
    }

    let (ran_on_main, took, idle_cpu) = run_with_main_loop(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async {
            // Let the main thread settle into its run loop, idle.
            let cpu0 = cpu_seconds();
            tokio::time::sleep(Duration::from_secs(2)).await;
            let idle_cpu = cpu_seconds() - cpu0;
            tokio::task::spawn_blocking(move || {
                let t = Instant::now();
                let r = on_main(Duration::from_secs(1), is_main_thread);
                (r, t.elapsed(), idle_cpu)
            })
            .await
            .expect("blocking task")
        })
    });
    assert_eq!(
        ran_on_main,
        Some(true),
        "a job posted to the main run loop did not run on the main thread within 1 s"
    );
    assert!(
        idle_cpu < 0.5,
        "process used {idle_cpu:.2} s CPU in 2 s idle: something spins"
    );
    println!("macos_main_loop: ok, job ran on main in {took:?}, idle CPU {idle_cpu:.3} s / 2 s");
}

#[cfg(not(target_os = "macos"))]
fn main() {}

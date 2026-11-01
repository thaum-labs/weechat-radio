//! SPDX-License-Identifier: Apache-2.0
//! macOS: the process main thread's run loop.
//!
//! In a command-line process IOBluetooth delivers every RFCOMM callback
//! (`rfcommChannelOpenComplete`, `rfcommChannelData`) to the main thread's
//! run loop, whichever thread opened the channel (QA #237). So `wcr` runs its
//! work on a worker thread and keeps the main thread in its run loop
//! ([`run_with_main_loop`]), and Bluetooth code hands its IOBluetooth calls
//! to the main thread ([`on_main`]). In `wcr gui` the window's own event loop
//! is the main run loop and serves [`on_main`] the same way.

use std::ffi::c_void;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    static kCFRunLoopCommonModes: *const c_void;
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: u8) -> i32;
    fn CFRunLoopWakeUp(rl: *mut c_void);
    fn CFAbsoluteTimeGetCurrent() -> f64;
    fn CFRunLoopTimerCreate(
        allocator: *const c_void,
        fire_date: f64,
        interval: f64,
        flags: usize,
        order: isize,
        callout: extern "C" fn(*mut c_void, *mut c_void),
        context: *mut c_void,
    ) -> *mut c_void;
    fn CFRunLoopAddTimer(rl: *mut c_void, timer: *mut c_void, mode: *const c_void);
}

extern "C" {
    /// What `dispatch_get_main_queue()` returns.
    static _dispatch_main_q: u8;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
    fn pthread_main_np() -> i32;
}

/// True on the process main thread.
pub fn is_main_thread() -> bool {
    unsafe { pthread_main_np() != 0 }
}

type Job = Box<dyn FnOnce() + Send>;

extern "C" fn run_job(context: *mut c_void) {
    let job = unsafe { Box::from_raw(context as *mut Job) };
    // A panic must not unwind into libdispatch.
    let _ = std::panic::catch_unwind(AssertUnwindSafe(job));
}

/// Runs `f` on the main thread and waits up to `wait` for its result. On the
/// main thread itself it just runs `f`. `None` means the main run loop did
/// not get to it in time (nothing is serving it); `f` may still run later.
pub fn on_main<T: Send + 'static>(
    wait: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    if is_main_thread() {
        return Some(f());
    }
    let (tx, rx) = mpsc::channel();
    let job: Job = Box::new(move || {
        let _ = tx.send(f());
    });
    unsafe {
        dispatch_async_f(
            std::ptr::addr_of!(_dispatch_main_q) as *const c_void,
            Box::into_raw(Box::new(job)) as *mut c_void,
            run_job,
        );
        CFRunLoopWakeUp(CFRunLoopGetMain());
    }
    rx.recv_timeout(wait).ok()
}

extern "C" fn keep_alive(_timer: *mut c_void, _info: *mut c_void) {}

/// Runs `work` on a worker thread while this thread (the main thread) serves
/// its run loop, and returns what `work` returned once it finishes.
pub fn run_with_main_loop<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let done = Arc::new(AtomicBool::new(false));
    let done_w = done.clone();
    let worker = std::thread::Builder::new()
        .name("wcr-work".into())
        .spawn(move || {
            let out = std::panic::catch_unwind(AssertUnwindSafe(work));
            done_w.store(true, Ordering::SeqCst);
            unsafe { CFRunLoopWakeUp(CFRunLoopGetMain()) };
            out
        })
        .expect("start the wcr worker thread");
    unsafe {
        // With no source or timer the run loop returns at once
        // (kCFRunLoopRunFinished) and this thread would spin; an hourly timer
        // that does nothing keeps it asleep until there is work.
        let hour = 3600.0;
        let timer = CFRunLoopTimerCreate(
            std::ptr::null(),
            CFAbsoluteTimeGetCurrent() + hour,
            hour,
            0,
            0,
            keep_alive,
            std::ptr::null_mut(),
        );
        if !timer.is_null() {
            CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
        }
    }
    while !done.load(Ordering::SeqCst) {
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.5, 0) };
    }
    match worker.join() {
        Ok(Ok(out)) => out,
        Ok(Err(panic)) | Err(panic) => std::panic::resume_unwind(panic),
    }
}

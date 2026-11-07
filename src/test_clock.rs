//! Test-only scheduling allowance. Never used by the shipped application.
#![allow(dead_code)]
use std::time::{Duration, Instant};

/// A queued runnable needs approximately load / CPUs scheduling turns. Keep the
/// original budget on an idle host; sample at each wait, not once at build time.
/// POCKETUI_TEST_TIME_FACTOR supplies an explicit extra factor for constrained CI.
/// This changes hang guards, never a simulated clock or a product deadline.
pub fn timeout(base: Duration) -> Duration {
    let cores = std::thread::available_parallelism().map_or(1, usize::from) as f64;
    let load = std::fs::read_to_string("/proc/loadavg").ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(0.0);
    let extra = std::env::var("POCKETUI_TEST_TIME_FACTOR").map_or(1.0, |s| {
        let n: f64 = s.parse().expect("POCKETUI_TEST_TIME_FACTOR must be a number >= 1");
        assert!(n.is_finite() && (1.0..=64.0).contains(&n), "invalid test time factor");
        n
    });
    base.mul_f64((load / cores).ceil().clamp(1.0, 64.0) * extra)
}

/// The observed event determines success. The scaled deadline only detects a hang.
/// Check the event before the deadline: a descheduled observer can see a completed
/// operation on its first turn back. A missing event still fails with its name.
pub fn wait_for(mut ready: impl FnMut() -> bool, base: Duration, event: &str) {
    let end = Instant::now() + timeout(base);
    loop {
        if ready() { return; }
        assert!(Instant::now() < end, "no progress waiting for {event}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Per-thread work, excluding scheduler delays. Only performance tests use this.
#[derive(Clone, Copy)]
pub struct CpuInstant(Duration);
impl CpuInstant {
    fn current() -> Duration {
        let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) }, 0);
        Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
    }
    pub fn now() -> Self { Self(Self::current()) }
    pub fn elapsed(self) -> Duration { Self::current().saturating_sub(self.0) }
}

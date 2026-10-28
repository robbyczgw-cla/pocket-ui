//! What run-ahead and the frames nobody sees cost a core, without a screen: CPU time of this thread per refresh.
//!
//!     run_ahead_bench <core.so> <rom> <system> [refreshes] [paced]
//!
//! `paced` waits for the next 1/60 s after each refresh, as a game does, so the CPU governor (ondemand) picks the
//! clock it picks in play; without it the core runs flat out and the governor goes to the highest clock. Each line
//! ends with the clock of the CPU the thread was on right after the measurement.
//!
//! Runs the game for a few seconds first (past the boot), then `refreshes` (default 1200) of each:
//! plain (picture and sound, as pocket-core plays), 4 frames as fast-forward 4x runs them (three without
//! picture or sound, GET_AUDIO_VIDEO_ENABLE), 4 frames all made (fast-forward before), and run-ahead 1 and 2
//! (`run_ahead::RunAhead::step`). The game itself sees no buttons. Run it in the virtual device or on the
//! device (docs/HOST.md, Run-ahead): the numbers don't include drawing, which pocket-core's `--bench` adds.

use std::path::Path;

use pocket_ui::libretro::{Core, AV_VIDEO};
use pocket_ui::run_ahead::{Clock, RunAhead};

fn cpu_ms() -> f64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime only writes the timespec it is given.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1e6
}

/// Mean, median and 99th percentile of `v`.
fn summary(mut v: Vec<f64>) -> String {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
    let at = |q: usize| v.get((v.len() * q / 100).min(v.len().saturating_sub(1))).copied().unwrap_or(0.0);
    let clock = Clock::read(Path::new("/")).map_or("clock unknown".into(), |c| format!("cpu{} at {}/{} MHz", c.cpu, c.cur_mhz, c.max_mhz));
    format!("mean {mean:.3} ms  median {:.3}  p99 {:.3}  [{clock}]", at(50), at(99))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: run_ahead_bench <core.so> <rom> <system> [refreshes] [paced]");
        std::process::exit(2);
    }
    let (core_so, rom, system) = (Path::new(&args[1]), Path::new(&args[2]), args[3].as_str());
    let n: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1200);
    let paced = args.iter().any(|a| a == "paced");
    let host = pocket_ui::hostsys::find(system);
    let options: Vec<(&str, &str)> = host.map_or(&[][..], |h| h.options).to_vec();
    let dir = std::env::temp_dir().join("run-ahead-bench");
    let _ = std::fs::create_dir_all(&dir);
    let mut core = Core::load(core_so, rom, Path::new("/storage/roms/bios"), &dir, &options).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let period_ms = 1000.0 / 60.0;
    for _ in 0..300 {
        core.run(0);
    }
    let state = core.save_state().unwrap_or_default();
    println!(
        "{} ({system}), state {} KB, quirks {:#x}",
        core.name,
        state.len() / 1024,
        core.serialization_quirks()
    );

    let measure = |core: &mut Core, f: &mut dyn FnMut(&mut Core)| {
        if !state.is_empty() {
            let _ = core.load_state(&state);
        }
        let mut v = Vec::with_capacity(n);
        let period = std::time::Duration::from_micros(16_667);
        let mut next = std::time::Instant::now();
        for _ in 0..n {
            let c0 = cpu_ms();
            f(core);
            v.push(cpu_ms() - c0);
            if paced {
                next += period;
                if let Some(wait) = next.checked_duration_since(std::time::Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
        }
        v
    };
    let plain = measure(&mut core, &mut |c| {
        c.run(0);
    });
    println!("plain              {}", summary(plain));
    let ff_old = measure(&mut core, &mut |c| {
        for _ in 0..4 {
            c.run(0);
        }
    });
    println!("ff 4x, all made    {}", summary(ff_old));
    let ff_new = measure(&mut core, &mut |c| {
        for i in 0..4 {
            c.run_with(0, if i == 3 { AV_VIDEO } else { 0 });
        }
    });
    println!("ff 4x, 3 skipped   {}", summary(ff_new));
    for frames in 1..=2 {
        let mut ra = RunAhead::new();
        ra.set_core(core_so, system);
        ra.configure(frames);
        ra.admit(core.has_states(), false, core.serialization_quirks());
        let v = measure(&mut core, &mut |c| {
            ra.step(c, 1, 0, 0, period_ms, |_, _| {});
        });
        let s = ra.stats;
        let r = s.refreshes.max(1) as f64;
        println!(
            "run-ahead {frames}        {}  (save {:.3} ms, load {:.3} ms; compared {} same / {} different{})",
            summary(v),
            s.save_ms / r,
            s.load_ms / r,
            ra.same,
            ra.different,
            ra.off().map(|o| format!("; off: {o:?}")).unwrap_or_default()
        );
        if let Some(line) = ra.report() {
            println!("    {line}");
        }
    }
}

//! How long Settings → Games → Game check takes and what it costs, on a device or a copy of a card:
//! `game_check_bench <es_systems.cfg>`. Runs on one thread at the check's priority, prints
//! the findings, the time, the peak memory, the time to stop, and how long the shelf's own read takes alone and beside it.
//! Reads only; writes nothing.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pocket_ui::game_check::{check, Input};
use pocket_ui::library;

fn hwm_kb() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status.lines().find(|l| l.starts_with("VmHWM")).and_then(|l| l.split_whitespace().nth(1)).and_then(|n| n.parse().ok()).unwrap_or(0)
}

fn input(es: &Path) -> Input {
    let catalog = library::all_systems(es).unwrap();
    let roms = library::roms_root(&catalog).unwrap();
    let shelf = library::load(es).unwrap().into_iter().map(|s| (s.name.clone(), s.games.into_iter().map(|g| g.path).collect())).collect();
    Input { catalog, shelf, bios_dir: roms.join("bios"), roms, h700: true }
}

fn main() {
    let es = std::env::args().nth(1).expect("usage: game_check_bench <es_systems.cfg>");
    let es = Path::new(&es);
    let t = Instant::now();
    let systems = library::load(es).unwrap();
    let shelf_alone = t.elapsed();
    let games: usize = systems.iter().map(|s| s.games.len()).sum();
    drop(systems);
    println!("games {games}; shelf's read alone {shelf_alone:?}");
    let before = hwm_kb();
    let input = Arc::new(input(es));

    let check_once = |input: &Input| {
        let stop = AtomicBool::new(false);
        check(input, &stop, &mut |_, _, _| {}).unwrap()
    };
    let t = Instant::now();
    let report = check_once(&input);
    println!("check (first) {:?}: {} of {} ready, {} findings", t.elapsed(), report.ready, report.total, report.findings.len());
    for f in report.findings.iter().take(60) {
        println!("  {:?} | {} | {} | {}", f.group, f.name, f.system, f.what);
    }
    let t = Instant::now();
    let _ = check_once(&input);
    println!("check (again) {:?}; peak memory {} KB -> {} KB", t.elapsed(), before, hwm_kb());

    let warm: Vec<_> = (0..5)
        .map(|_| {
            let t = Instant::now();
            let _ = library::load(es).unwrap();
            t.elapsed()
        })
        .collect();
    println!("shelf's read alone, warm {warm:?}");

    // The shelf's read beside a running check, at the check's priority.
    let (stop, running) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(true)));
    let worker = {
        let (input, stop, running) = (input.clone(), stop.clone(), running.clone());
        std::thread::spawn(move || {
            // SAFETY: setpriority on this thread's own id only changes its scheduling.
            unsafe {
                let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
                libc::setpriority(libc::PRIO_PROCESS, tid, 10);
            }
            while !stop.load(Ordering::Relaxed) {
                let _ = check(&input, &stop, &mut |_, _, _| {});
            }
            running.store(false, Ordering::Relaxed);
        })
    };
    let mut beside = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let _ = library::load(es).unwrap();
        beside.push(t.elapsed());
    }
    let t = Instant::now();
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    println!("shelf's read beside the check {beside:?}; stopped in {:?}", t.elapsed());
}

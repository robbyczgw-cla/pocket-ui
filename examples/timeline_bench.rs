//! What Earlier (`timeline`) costs a core, without a screen: the state's size, the serialize on the emulation thread
//! (what the game would wait for), and the writer thread's time for the file and its picture.
//!
//!     timeline_bench <core.so> <rom> <system> [captures]
//!
//! Runs the game for five seconds first (past the boot), then takes `captures` states (default 20) a refresh apart,
//! timing each `retro_serialize`, and says where the cost guard would take them (`timeline::moment`: in play, or only
//! in a pause). Writes them through `timeline::Writer` into a scratch folder in /tmp, then removes it. Run it on the
//! device or in the virtual device for the device's numbers (docs/HOST.md, "Earlier"); on a computer the serialize is
//! several times faster.

use std::path::Path;
use std::time::Instant;

use pocket_ui::libretro::Core;
use pocket_ui::timeline::{self, Caps, Moment, Timeline};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: timeline_bench <core.so> <rom> <system> [captures]");
        std::process::exit(2);
    }
    let (core_so, rom, system) = (Path::new(&args[1]), Path::new(&args[2]), args[3].as_str());
    let n: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(20);
    let host = pocket_ui::hostsys::find(system);
    let options: Vec<(&str, &str)> = host.map_or(&[][..], |h| h.options).to_vec();
    let dir = std::env::temp_dir().join(format!("timeline-bench-{}", std::process::id()));
    let _ = std::fs::create_dir_all(dir.join("savestates").join(system));
    let core = Core::load(core_so, rom, Path::new("/storage/roms/bios"), &dir, &options).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    for _ in 0..300 {
        core.run(0);
    }
    let size = core.state_size();
    let period_ms = 1000.0 / 60.0;
    let mut ms = Vec::with_capacity(n);
    let mut tl = Timeline::new(Caps { min_free: 0, ..Caps::default() });
    let base = dir.join("savestates").join(system).join("bench");
    let mut frame = None;
    for _ in 0..n {
        let (f, ..) = core.run(0);
        frame = f.or(frame);
        let t0 = Instant::now();
        let Some(state) = core.save_state() else {
            eprintln!("{}: no state", core.name);
            std::process::exit(1)
        };
        let took = t0.elapsed().as_secs_f32() * 1000.0;
        ms.push(took);
        tl.submit_state(&base, state, frame.clone(), took, Moment::InPlay);
        tl.wait();
    }
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = ms.iter().sum::<f32>() / ms.len().max(1) as f32;
    let at = |q: usize| ms[(ms.len() * q / 100).min(ms.len() - 1)];
    let w = tl.written();
    let guard = |m: f32| match timeline::moment(false, Some(m), None, period_ms) {
        Moment::InPlay => "in play",
        Moment::Quiet => "in a pause",
    };
    println!(
        "{} ({system}): state {} KB; serialize mean {mean:.2} ms, median {:.2}, max {:.2} ms; writer {:.1} ms a state with its picture; \
         guard: {} (by size before a measurement: {}); 9 states take {:.1} MB of the {} MB a game may have",
        core.name,
        size.unwrap_or(0) / 1024,
        at(50),
        at(100),
        w.write_ms / w.written.max(1) as f64,
        guard(mean),
        match timeline::moment(false, None, size, period_ms) {
            Moment::InPlay => "in play",
            Moment::Quiet => "in a pause",
        },
        size.unwrap_or(0) as f64 * 9.0 / 1e6,
        timeline::GAME_CAP / 1_000_000
    );
    drop(tl);
    let _ = std::fs::remove_dir_all(&dir);
}

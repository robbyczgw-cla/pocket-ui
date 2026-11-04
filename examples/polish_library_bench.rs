use std::{path::Path, time::Instant};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = Path::new(&args[1]);
    let es = Path::new(&args[2]);
    for i in 0..6 {
        let c = pocket_ui::run_ahead::thread_cpu_ms();
        let t = Instant::now();
        let systems = pocket_ui::library::load_shown(es, data).unwrap();
        println!(
            "scan run={i} games={} wall_ms={:.3} cpu_ms={:.3}",
            systems.iter().map(|s| s.games.len()).sum::<usize>(),
            t.elapsed().as_secs_f64() * 1000.,
            pocket_ui::run_ahead::thread_cpu_ms() - c
        );
        let key = pocket_ui::library_cache::Key::now(es).unwrap();
        if i == 0 {
            pocket_ui::library_cache::write(data, &key, &systems).unwrap();
        }
        let c = pocket_ui::run_ahead::thread_cpu_ms();
        let t = Instant::now();
        let cached = pocket_ui::library_cache::read(data, &key).unwrap();
        println!(
            "cache run={i} games={} wall_ms={:.3} cpu_ms={:.3}",
            cached.iter().map(|s| s.games.len()).sum::<usize>(),
            t.elapsed().as_secs_f64() * 1000.,
            pocket_ui::run_ahead::thread_cpu_ms() - c
        );
    }
}

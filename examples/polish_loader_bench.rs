//! Burst benchmark: 20 real fitted decodes; reports when the newest request finishes.
use std::{sync::mpsc, time::Instant};

fn main() {
    let root = std::env::args().nth(1).expect("cover directory");
    let (tx, rx) = mpsc::channel();
    let t = Instant::now();
    let loader = pocket_ui::picture_loader::Loader::start(move |key, img| {
        let _ = tx.send((key, img.is_some()));
    });
    for i in 0..20 {
        loader.request((format!("{root}/zz Perf Game {i:04} (World).png").into(), 300, 400));
    }
    for i in 0..20 {
        let (key, ok) = rx.recv_timeout(std::time::Duration::from_secs(30)).expect("every request answered");
        assert!(ok, "missing fixture: {:?}", key.0);
        println!("completed={} ok={} wall_ms={:.3} key={:?}", i + 1, ok, t.elapsed().as_secs_f64() * 1000., key.0);
    }
}

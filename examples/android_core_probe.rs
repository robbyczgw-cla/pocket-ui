//! Headless core/content contract probe; Android emulator or host, no audio device or gamepad required.
use pocket_ui::libretro::Core;
use std::path::Path;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() >= 4, "android_core_probe CORE ROM SCRATCH [FRAMES]");
    let dir = Path::new(&args[3]);
    std::fs::create_dir_all(dir).unwrap();
    let core = Core::load(Path::new(&args[1]), Path::new(&args[2]), dir, dir, &[]).unwrap();
    println!("loaded {}: geometry {:?}", core.name, core.av.geometry);
    let mut pictures = 0;
    for tick in 0..args.get(4).and_then(|s|s.parse().ok()).unwrap_or(180) {
        let (frame, _, av, geometry) = core.run(0);
        if let Some(frame) = frame {
            pictures += 1;
            if pictures == 1 { println!("first picture {}x{} at {tick}", frame.width, frame.height); }
        }
        if av.is_some() || geometry.is_some() { println!("tick {tick}: av {av:?} geometry {geometry:?}"); }
    }
    assert!(pictures > 0, "the core must deliver a picture");
    println!("PASS: {pictures} pictures");
}

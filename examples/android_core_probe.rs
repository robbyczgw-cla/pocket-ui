//! Headless core/content and cold Continue probe; Android emulator or host, no audio device or gamepad required.
use pocket_ui::libretro::{Core, Frame, Resume};
use std::path::Path;

fn visible(frame: &Frame) -> bool {
    frame.to_rgb().iter().any(|b| *b != 0)
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() >= 4, "android_core_probe CORE ROM SCRATCH [FRAMES] [save|resume]");
    let dir = Path::new(&args[3]);
    std::fs::create_dir_all(dir).unwrap();
    let library = Path::new(&args[1]);
    let mut core = Core::load(library, Path::new(&args[2]), dir, dir, &[]).unwrap();
    core.apply_state_policy(&pocket_ui::core_options::core_id(library));
    println!("loaded {}: geometry {:?}", core.name, core.av.geometry);
    let phase = args.get(5).map(String::as_str);
    assert!(matches!(phase, None | Some("save" | "resume")), "phase must be save or resume");
    let auto = dir.join("continue.pauto");
    if phase == Some("resume") {
        // Match the player's timing/geometry bootstrap, then restore in a fresh process.
        core.run(0);
        assert_eq!(core.resume(&auto), Resume::Resumed);
        let (frame, ..) = core.run(0);
        let frame = frame.expect("the first post-load run must redraw the saved scene");
        assert!(visible(&frame), "the first frame after loading this visible scene is black");
        println!("PASS: first resumed picture {}x{} is visible", frame.width, frame.height);
        return;
    }
    let mut pictures = 0;
    let mut last = None;
    for tick in 0..args.get(4).and_then(|s|s.parse().ok()).unwrap_or(180) {
        let (frame, _, av, geometry) = core.run(0);
        if let Some(frame) = frame {
            pictures += 1;
            if pictures == 1 { println!("first picture {}x{} at {tick}", frame.width, frame.height); }
            last = Some(frame);
        }
        if av.is_some() || geometry.is_some() { println!("tick {tick}: av {av:?} geometry {geometry:?}"); }
    }
    assert!(pictures > 0, "the core must deliver a picture");
    if phase == Some("save") {
        assert!(visible(last.as_ref().unwrap()), "choose a frame count that ends on a visible scene for this test");
        core.write_sram().unwrap();
        pocket_ui::state_file::write(&auto, &core.save_state().expect("the core must supply a state")).unwrap();
        println!("PASS: visible scene saved; run resume in a second process");
    } else {
        println!("PASS: {pictures} pictures");
    }
}

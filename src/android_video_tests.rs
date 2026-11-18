//! The Android software-core picture through its production GLES compositor.
#[path = "../android/src/player_graphics.rs"]
mod graphics;
#[path = "../android/src/player_video.rs"]
mod video;
#[allow(dead_code)]
#[path = "../android/src/picture.rs"]
mod picture;
use glow::HasContext;
use crate::{image::Image, touch::Area};

#[test]
fn every_android_earlier_moment_keeps_its_own_picture_including_the_session_start() {
    let dir = crate::testutil::TestDir::new("android-earlier-picture");
    for system in ["psx", "snes", "gba", "megadrive", "gb", "nes"] {
        for slot in 0..6 {
            let path = dir.join(format!("{system}.earlier{slot}"));
            let picture = crate::slots::preview_path(&path);
            for color in [0xff0000, 0x0000ff] {
                let image = Image { w: 3, h: 2, px: vec![color; 6] };
                video::moment(&path, b"moment", Some(&image)).unwrap();
                assert_eq!(crate::state_file::read(&path).unwrap(), b"moment");
                assert_eq!(crate::image::load_png(&picture).expect("Earlier must show the held picture").px, image.px);
            }
            video::moment(&path, b"no frame", None).unwrap();
            assert!(!picture.exists(), "a missing frame must not show the previous moment's picture");
        }
    }
}

#[test]
fn android_resolution_changes_keep_the_reported_aspect_in_play_and_behind_pause() {
    let _lock = crate::hw_render::TEST_GLOBALS.lock().unwrap_or_else(|e|e.into_inner());
    let dir = crate::testutil::TestDir::new("android-changing-geometry");
    let library = dir.join("geometry_libretro.so");
    let result = std::process::Command::new("cc").args(["-shared", "-fPIC", "-std=c11", "-O1", "-Wall", "-Wextra", "-Werror"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/video_contract.c")).arg("-o").arg(&library).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let rom = dir.join("Intro.gb"); std::fs::write(&rom, b"fixture").unwrap();
    let mut core = crate::libretro::Core::load(&library, &rom, &dir, &dir, &[]).unwrap();
    let mut settings = picture::Settings::load(&dir, "psx", std::path::Path::new("k"), Default::default());
    settings.set_values(&[("scaling", ["aspect", "native", "fullscreen"][0 as usize].to_string())]).unwrap();
    settings.set_values(&[("frame", ["none", "auto", "crt"][0 as usize].to_string())]).unwrap();
    for tick in 0..18 {
        let (frame, _, av, geometry) = core.run(1 << 2);
        if let Some(av) = av { core.av = av; }
        if let Some(geometry) = geometry { core.av.geometry = geometry; }
        let frame = frame.unwrap();
        let expected = if tick % 6 == 5 { 16.0 / 9.0 } else { 4.0 / 3.0 };
        assert!((core.av.geometry.aspect_ratio as f64 - expected).abs() < 0.0001);
        assert_eq!((frame.width, frame.height), (core.av.geometry.base_width, core.av.geometry.base_height));
        for (w,h) in [(1920,1080), (1280,960), (960,960)] {
            let space = settings.place((w,h), (30,40,w-60,h-80), (900,700), (frame.width,frame.height), core.av.geometry.aspect_ratio as f64, false);
            let (_,_,live_w,live_h) = space.on_window(h);
            assert!((live_w as f64 / live_h as f64 - expected).abs() < 0.004, "live frame {tick}: {space:?}");
            let paused = space.in_overlay(h, (30,40,w-60,h-80), (900,700));
            let physical_w = paused.w as f64 * (w-60) as f64 / 900.0;
            let physical_h = paused.h as f64 * (h-80) as f64 / 700.0;
            assert!((physical_w / physical_h - expected).abs() < 0.01, "paused frame {tick}: {paused:?}");
        }
    }
}

fn compositor() -> Option<(graphics::Graphics, crate::headless_gl::Headless, glow::NativeFramebuffer)> {
    let (gl, headless) = match crate::headless_gl::context() {
        Ok(context) => context,
        Err(error) if std::env::var_os("POCKET_REQUIRE_GL").is_some() => panic!("no headless GLES: {error}"),
        Err(error) => { eprintln!("Android compositor skipped: {error}"); return None; }
    };
    eprintln!("Android compositor through {}", headless.renderer);
    let fbo = unsafe {
        let fbo = gl.create_framebuffer().unwrap();
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        let color = gl.create_renderbuffer().unwrap();
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, 32, 32);
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(color));
        assert_eq!(gl.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE);
        fbo
    };
    Some((graphics::Graphics::from_context(gl).unwrap(), headless, fbo))
}

// Keep context ownership in the compositor; a second loader only reads the same current context.
fn read() -> Vec<u8> {
    let lib = unsafe { libloading::Library::new("libEGL.so.1").unwrap() };
    let get: libloading::Symbol<unsafe extern "C" fn(*const std::ffi::c_char) -> *const std::ffi::c_void> = unsafe {lib.get(b"eglGetProcAddress").unwrap()};
    let gl = unsafe { glow::Context::from_loader_function_cstr(|n| get(n.as_ptr())) };
    let mut rgba=vec![0;32*32*4];
    unsafe {gl.read_pixels(0,0,32,32,glow::RGBA,glow::UNSIGNED_BYTE,glow::PixelPackData::Slice(&mut rgba));assert_eq!(gl.get_error(),glow::NO_ERROR);}
    rgba
}

#[test]
fn android_native_picture_survives_cached_and_changed_touch_overlays() {
    let Some((mut gfx, context, _fbo)) = compositor() else {return};
    let image=Image{w:160,h:144,px:vec![0x40_80_c0;160*144]};
    let overlay=vec![0;32*32];
    let rect=Area::new(0,0,32,32);
    for dirty in [true,false,false,true,false] {
        gfx.present(&overlay,(32,32),(0,0,32,32),Some((&image,rect)),dirty,&[],None,((32,32),(0,0,32,32)));
        assert!(read().chunks_exact(4).all(|p|p[..3]==[64,128,192]),"a cached or changed touch overlay must leave the game visible");
    }
    drop(gfx); drop(context);
}

#[test]
fn android_picture_returns_after_pause_and_a_cancelled_crt_transition() {
    let Some((mut gfx, context, _fbo)) = compositor() else {return};
    let image=Image{w:3,h:2,px:vec![0xff_80_40;6]};let transparent=vec![0;32*32];let sheet=vec![0x10_20_30;32*32];let rect=Area::new(0,0,32,32);
    gfx.present(&sheet,(32,32),(0,0,32,32),None,true,&[],None,((32,32),(0,0,32,32)));
    gfx.present(&transparent,(32,32),(0,0,32,32),Some((&image,rect)),true,&[],None,((32,32),(0,0,32,32)));
    assert!(read().chunks_exact(4).all(|p|p[..3]==[255,128,64]));
    let quads=crate::crt_on::plan(std::time::Duration::from_millis(30),(0.,0.,32.,32.));
    gfx.present(&transparent,(32,32),(0,0,32,32),Some((&image,rect)),false,&quads,None,((32,32),(0,0,32,32)));
    gfx.present(&transparent,(32,32),(0,0,32,32),Some((&image,rect)),false,&[],None,((32,32),(0,0,32,32)));
    assert!(read().chunks_exact(4).all(|p|p[..3]==[255,128,64]),"input cancelling the CRT transition must restore the picture on the next present");
    drop(gfx); drop(context);
}

#[test]
fn software_core_formats_padded_rows_and_null_frames_keep_the_visible_scene() {
    let _lock = crate::hw_render::TEST_GLOBALS.lock().unwrap_or_else(|e|e.into_inner());
    let Some((mut gfx, context, _fbo)) = compositor() else {return};
    let dir=crate::testutil::TestDir::new("android-video-contract");
    let library=dir.join("video_contract_libretro.so");
    let result=std::process::Command::new("cc").args(["-shared","-fPIC","-std=c11","-O1","-Wall","-Wextra","-Werror"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/video_contract.c")).arg("-o").arg(&library).output().unwrap();
    assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
    let rom=dir.join("Contract.gb");std::fs::write(&rom,b"fixture").unwrap();
    let core=crate::libretro::Core::load(&library,&rom,&dir,&dir,&[]).unwrap();
    // The Android player runs the core once to negotiate timing/geometry before normal playback.
    // This core supplies its green picture there and duplicates it on every following run.
    let (initial,..)=core.run(0);
    let mut image=video::startup(initial,None);
    let overlay=vec![0;32*32];
    gfx.present(&overlay,(32,32),(0,0,32,32),image.as_deref().map(|p|(p,Area::new(0,0,32,32))),true,&[],None,((32,32),(0,0,32,32)));
    assert!(read().chunks_exact(4).all(|p|p[..3]==[0,255,0]),"the bootstrap picture must be visible before the core starts sending only NULL duplicates");
    for (buttons,colour) in [(0,[0,255,0]),(1<<5,[255,0,0]),(1<<4,[0,0,255]),(1<<8,[0,0,255]),(1<<7,[0,255,0])] {
        for repeat in 0..8 {
            let (frame,..)=core.run(if repeat==0 {buttons}else{0});
            if let Some(frame)=frame {
                image=Some(video::image(frame));
            }
            let image=image.as_ref().expect("the core supplied a picture");
            gfx.present(&overlay,(32,32),(0,0,32,32),Some((image,Area::new(0,0,32,32))),repeat%2==0,&[],None,((32,32),(0,0,32,32)));
            assert!(read().chunks_exact(4).all(|p|p[..3]==colour),"format change, padded row, NULL duplicate or a run without a callback must retain the intended scene; buttons={buttons}, repeat={repeat}");
        }
    }
    // A picture produced after restoring a moment supersedes the earlier bootstrap image.
    let (before_resume,..)=core.run(1<<5);
    let (after_resume,..)=core.run(1<<4);
    let resumed=video::startup(before_resume,after_resume).unwrap();
    gfx.present(&overlay,(32,32),(0,0,32,32),Some((&resumed,Area::new(0,0,32,32))),false,&[],None,((32,32),(0,0,32,32)));
    assert!(read().chunks_exact(4).all(|p|p[..3]==[0,0,255]),"Continue must show the newer scene rather than the bootstrap picture");
    drop(core);drop(gfx);drop(context);
}

// A Continue must be checked across processes: a same-process save/load leaves caches
// (ClownMDEmu's converted CRAM palette, for example) populated and can hide a black picture.
fn cold_continue(library: &std::path::Path, rom: &std::path::Path, dir: &std::path::Path, frames: usize, buttons: u16, colour: Option<u32>) {
    std::fs::create_dir_all(dir).unwrap();
    let request = serde_json::json!({"core": library, "rom": rom, "frames": frames, "buttons": buttons, "colour": colour});
    std::fs::write(dir.join("request.json"), request.to_string()).unwrap();
    for phase in ["save", "resume"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_video_tests::continue_in_a_fresh_player_process", "--ignored", "--nocapture"])
            .env("POCKETUI_TEST_CONTINUE_DIR", dir).env("POCKETUI_TEST_CONTINUE_PHASE", phase).output().unwrap();
        assert!(output.status.success(), "{phase}: {}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
#[ignore = "child process of the cold Continue tests; needs their request file"]
fn continue_in_a_fresh_player_process() {
    use std::path::Path;
    let Some(dir) = std::env::var_os("POCKETUI_TEST_CONTINUE_DIR") else { return };
    let dir = std::path::PathBuf::from(dir);
    let request: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("request.json")).unwrap()).unwrap();
    let library = Path::new(request["core"].as_str().unwrap());
    let rom = Path::new(request["rom"].as_str().unwrap());
    let core_id = crate::core_options::core_id(library);
    let mut core = crate::libretro::Core::load(library, rom, &dir, &dir, &[]).unwrap();
    let (initial, ..) = core.run(0); // Android's timing/geometry bootstrap.
    core.apply_state_policy(&core_id);
    let base = dir.join("game.pstate");
    if std::env::var("POCKETUI_TEST_CONTINUE_PHASE").unwrap() == "save" {
        let mut last = initial;
        for _ in 0..request["frames"].as_u64().unwrap() {
            let (frame, ..) = core.run(request["buttons"].as_u64().unwrap() as u16);
            if frame.is_some() { last = frame; }
        }
        assert!(last.unwrap().to_rgb().iter().any(|b| *b != 0), "the test must save a visible scene");
        crate::state_file::write(&crate::slots::auto_path(&base), &core.save_state().unwrap()).unwrap();
        return;
    }
    assert_eq!(core.resume(&crate::slots::auto_path(&base)), crate::libretro::Resume::Resumed);
    let (after, ..) = core.run(0);
    assert!(after.as_ref().expect("the next run must redraw the restored scene").to_rgb().iter().any(|b| *b != 0),
        "the first frame after unserialize must not be black for this visible saved scene");
    let image = video::startup(initial, after).unwrap();
    if let Some(expected) = request["colour"].as_u64() {
        assert!(image.px.iter().all(|p| *p == expected as u32), "Continue must show the loaded scene, not the bootstrap image");
    }
    let Some((mut gfx, context, _fbo)) = compositor() else { panic!("Continue regression requires the headless GLES compositor") };
    gfx.present(&vec![0;32*32], (32,32), (0,0,32,32), Some((&image,Area::new(0,0,32,32))), true, &[], None, ((32,32),(0,0,32,32)));
    let pixels = read();
    assert!(pixels.chunks_exact(4).any(|p| p[..3] != [0,0,0]), "the next presented frame must be visible");
    if let Some(expected) = request["colour"].as_u64() {
        let rgb = [(expected >> 16) as u8, (expected >> 8) as u8, expected as u8];
        assert!(pixels.chunks_exact(4).all(|p| p[..3] == rgb));
    }
    let earlier = base.with_extension("earlier0");
    video::moment(&earlier, &core.save_state().unwrap(), Some(&image)).unwrap();
    assert_eq!(crate::image::load_png(&crate::slots::preview_path(&earlier)).unwrap().px, image.px,
        "Earlier must keep the same restored picture that was presented");
    drop(gfx); drop(context);
}

#[test]
fn continue_redraws_the_loaded_scene_in_every_software_format_and_earlier() {
    let Some((gfx, context, _fbo)) = compositor() else { return };
    drop(gfx); drop(context);
    let dir = crate::testutil::TestDir::new("android-cold-continue");
    let library = dir.join("video_contract_libretro.so");
    let output = std::process::Command::new("cc").args(["-shared", "-fPIC", "-std=c11", "-O1", "-Wall", "-Wextra", "-Werror"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/video_contract.c")).arg("-o").arg(&library).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let rom = dir.join("Continue.gb"); std::fs::write(&rom, b"fixture").unwrap();
    for (format, buttons, colour) in [("rgb565", 1 << 7, 0x00ff00), ("rgb1555", 1 << 5, 0xff0000), ("xrgb8888", 1 << 4, 0x0000ff)] {
        cold_continue(&library, &rom, &dir.join(format), 3, buttons, Some(colour));
    }
}

#[test]
#[ignore = "real-core Continue regression: POCKETUI_TEST_CORE and POCKETUI_TEST_ROM name a software core and a visible game"]
fn a_real_core_continue_presents_a_visible_frame_and_earlier() {
    let (Some(library), Some(rom)) = (std::env::var_os("POCKETUI_TEST_CORE"), std::env::var_os("POCKETUI_TEST_ROM")) else {
        eprintln!("real-core Continue test needs POCKETUI_TEST_CORE and POCKETUI_TEST_ROM");
        return;
    };
    let library = std::path::PathBuf::from(library);
    let rom = std::path::PathBuf::from(rom);
    let dir = crate::testutil::TestDir::new("real-core-cold-continue");
    cold_continue(&library, &rom, &dir, 180, 0, None);
}

const LOOKS: [&str; 7] = ["clean", "smooth", "soft", "bloom", "lcd", "scanlines", "oldtv"];
fn linux_variant(i: usize) -> crate::video_program::Variant {
    let (sharpness, effect) = crate::look::Look::from_value(LOOKS[i]).and_then(|l| l.canonical()).unwrap();
    (crate::video::Sharpness::parse(sharpness), crate::video::Effect::parse(effect), false)
}

/// Android's player draws every Look it offers with pocket-core's own picture pass (`video_program`, `shaders/video.frag`):
/// the same game picture at the same size comes out of the player's compositor and Linux's renderer with the same pixels
/// (Clean, Smooth, Old TV, Scanlines). Until 18.11.2026 the player had a one-line shader of its own: Old TV and Scanlines a
/// sine, Smooth plain bilinear (REPORT.md #1 of the Android audit).
#[test]
fn every_android_look_draws_linux_s_pixels() {
    let _lock = crate::hw_render::TEST_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    // A small picture with hard edges, diagonals and both bright and dark parts: every effect changes it visibly.
    let (w, h) = (8u32, 8u32);
    let px: Vec<u32> = (0..w * h).map(|i| { let (x, y) = (i % w, i / w); if (x + y) % 3 == 0 { 0xff_f0_20 } else if x > y { 0x20_60_e0 } else { 0x10_10_10 } }).collect();
    let image = Image { w, h, px: px.clone() };
    let overlay = vec![0; 32 * 32];
    let mut android = Vec::new();
    {
        let Some((mut gfx, context, _fbo)) = compositor() else { return };
        for look in 0..LOOKS.len() {
            gfx.variant = linux_variant(look);
            gfx.present(&overlay, (32, 32), (0, 0, 32, 32), Some((&image, Area::new(0, 0, 32, 32))), true, &[], None, ((32, 32), (0, 0, 32, 32)));
            // GL's rows run bottom to top; Linux's offscreen read gives them top to bottom.
            let rows: Vec<u8> = read().chunks_exact(32 * 4).rev().flatten().copied().collect();
            android.push(rows);
        }
        drop(gfx);
        drop(context);
    }
    let (gl, _headless) = crate::headless_gl::context().unwrap();
    let mut linux = crate::video::Renderer::new(gl).unwrap();
    let data: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    linux.upload(&crate::libretro::Frame { width: w, height: h, pitch: (w * 4) as usize, format: crate::libretro::PIXEL_XRGB8888, data });
    for (look, drawn) in android.iter().enumerate() {
        let (sharpness, effect, _) = linux_variant(look);
        let settings = crate::video::VideoSettings { scaling: crate::video::Scaling::Fullscreen, sharpness, effect, ..Default::default() };
        let want = linux.render_offscreen(32, 32, 0.0, &settings);
        let worst = drawn.chunks_exact(4).zip(want.chunks_exact(4)).map(|(a, b)| (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap()).max().unwrap();
        assert!(worst <= 2, "Look {}: Android's pixels differ from Linux's by up to {worst}", LOOKS[look]);
        // And the Look does something: Old TV and Scanlines are not Clean.
        if look > 0 {
            assert_ne!(drawn, &android[0], "Look {} draws like Clean", LOOKS[look]);
        }
    }
}

/// The player has no picture shader of its own any more: the picture pass is `video_program`'s.
#[test]
fn the_player_has_no_picture_shader_of_its_own() {
    let src = include_str!("../android/src/player_graphics.rs");
    assert!(src.contains("video_program::Programs") && src.contains("video_program::prepare_texture"));
    for own in ["sourceHeight", "effect==", "uniform int effect", "sin(uv.y"] {
        assert!(!src.contains(own), "player_graphics.rs draws an effect of its own ({own})");
    }
}

/// A game from the Handheld view: the player's pause menu reads and writes the view's own `game-settings` files, as
/// pocket-core's does on Linux, so the view's This game page and the next start show the change with no translation back.
#[test]
fn the_player_s_picture_rows_are_the_handheld_view_s_settings() {
    let view = crate::testutil::TestDir::new("player-view-settings");
    let rom = std::path::Path::new("/storage/emulated/0/ROMs/gba/Golden Sun.gba");
    std::fs::create_dir_all(view.join("game-settings")).unwrap();
    std::fs::write(view.join("game-settings/gba.json"), r#"{"frame":"pocket","frame_pocket":"gold","scaling":"native"}"#).unwrap();
    let handed = crate::options::GameSettings::load(&view, "gba", rom).effective();
    let mut s = picture::Settings::load(&view, "gba", rom, handed);
    assert!(!s.for_game(), "the console's file holds the values: Keep for says the console");
    // The GBA SP-style frame and its finish are drawn as Linux draws them, and the row says Console.
    assert_eq!(s.frame_style("gba"), crate::video::VideoSettings::from_values(&crate::options::GameSettings::load(&view, "gba", rom).effective()).fixed_style("gba"));
    assert_eq!(s.frame, 1);
    // Look → Old TV in the pause menu.
    s.set_values(&[("sharpness", "crisp".into()), ("effect", "crt".into())]).unwrap();
    let linux = crate::options::GameSettings::load(&view, "gba", rom);
    assert_eq!((linux.values.get("effect").map(String::as_str), linux.level), (Some("crt"), crate::options::Level::Console));
    assert_eq!(linux.values.get("frame_pocket").map(String::as_str), Some("gold"), "what the menu didn't change stays");
    assert_eq!(s.variant("gba"), (crate::video::Sharpness::Crisp, crate::video::Effect::Crt, false));
    // Keep for → this game: the game's own file, the console's unchanged.
    s.change(3, 1).unwrap();
    assert!(view.join("game-settings/gba/Golden Sun.json").exists() && s.for_game());
    assert!(!view.join("picture").exists(), "the player keeps no files of its own for a view's game");
}

/// The player's own files from before (indexes in `picture/games/<key>.json` and `picture/systems/<system>.json`, as the
/// old `picture.rs` wrote them) are taken over once as Linux's values in `game-settings`; a Mobile game starts with them.
#[test]
fn old_player_picture_files_are_taken_over_once() {
    let data = crate::testutil::TestDir::new("player-old-picture");
    std::fs::create_dir_all(data.join("picture/games")).unwrap();
    std::fs::create_dir_all(data.join("picture/systems")).unwrap();
    // Bytes as the old `Settings::save` wrote them (serde_json, its key order).
    std::fs::write(data.join("picture/games/0123456789abcdef.json"), br#"{"effect":2,"frame":1,"over_game":1,"scaling":2,"resolution":2,"earlier":0}"#).unwrap();
    std::fs::write(data.join("picture/systems/snes.json"), br#"{"effect":3,"frame":0,"over_game":0,"scaling":1}"#).unwrap();
    picture::migrate_old(&data, "snes", "0123456789abcdef");
    assert!(!data.join("picture/games/0123456789abcdef.json").exists() && !data.join("picture/systems/snes.json").exists(), "the old files go");
    let s = picture::Settings::load(&data, "snes", std::path::Path::new("0123456789abcdef"), picture::mobile_start());
    assert_eq!((s.scaling, s.frame, s.over_game, s.resolution, s.earlier), (2, 1, true, Some(2), Some(false)));
    assert_eq!(s.variant("snes"), (crate::video::Sharpness::Crisp, crate::video::Effect::Crt, false), "CRT became Old TV");
    assert!(s.for_game());
    let console: std::collections::BTreeMap<String, String> = crate::fsutil::read_json_or_backup(&data.join("game-settings/snes.json")).unwrap();
    assert_eq!((console["effect"].as_str(), console["sharpness"].as_str(), console["scaling"].as_str()), ("scanlines", "crisp", "native"));
    // Once: a second run finds nothing to take over and changes nothing.
    picture::migrate_old(&data, "snes", "0123456789abcdef");
    assert_eq!(picture::Settings::load(&data, "snes", std::path::Path::new("0123456789abcdef"), Default::default()).variant("snes").1, crate::video::Effect::Crt);
}

//! The Android software-core picture through its production GLES compositor.
#[path = "../android/src/player_graphics.rs"]
mod graphics;
#[path = "../android/src/player_video.rs"]
mod video;
use glow::HasContext;
use crate::{image::Image, touch::Area};

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
        gfx.present(&overlay,(32,32),(0,0,32,32),Some((&image,rect)),dirty,&[]);
        assert!(read().chunks_exact(4).all(|p|p[..3]==[64,128,192]),"a cached or changed touch overlay must leave the game visible");
    }
    drop(gfx); drop(context);
}

#[test]
fn android_picture_returns_after_pause_and_a_cancelled_crt_transition() {
    let Some((mut gfx, context, _fbo)) = compositor() else {return};
    let image=Image{w:3,h:2,px:vec![0xff_80_40;6]};let transparent=vec![0;32*32];let sheet=vec![0x10_20_30;32*32];let rect=Area::new(0,0,32,32);
    gfx.present(&sheet,(32,32),(0,0,32,32),None,true,&[]);
    gfx.present(&transparent,(32,32),(0,0,32,32),Some((&image,rect)),true,&[]);
    assert!(read().chunks_exact(4).all(|p|p[..3]==[255,128,64]));
    let quads=crate::crt_on::plan(std::time::Duration::from_millis(30),(0.,0.,32.,32.));
    gfx.present(&transparent,(32,32),(0,0,32,32),Some((&image,rect)),false,&quads);
    gfx.present(&transparent,(32,32),(0,0,32,32),Some((&image,rect)),false,&[]);
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
    gfx.present(&overlay,(32,32),(0,0,32,32),image.as_deref().map(|p|(p,Area::new(0,0,32,32))),true,&[]);
    assert!(read().chunks_exact(4).all(|p|p[..3]==[0,255,0]),"the bootstrap picture must be visible before the core starts sending only NULL duplicates");
    for (buttons,colour) in [(0,[0,255,0]),(1<<5,[255,0,0]),(1<<4,[0,0,255]),(1<<8,[0,0,255]),(1<<7,[0,255,0])] {
        for repeat in 0..8 {
            let (frame,..)=core.run(if repeat==0 {buttons}else{0});
            if let Some(frame)=frame {
                image=Some(video::image(frame));
            }
            let image=image.as_ref().expect("the core supplied a picture");
            gfx.present(&overlay,(32,32),(0,0,32,32),Some((image,Area::new(0,0,32,32))),repeat%2==0,&[]);
            assert!(read().chunks_exact(4).all(|p|p[..3]==colour),"format change, padded row, NULL duplicate or a run without a callback must retain the intended scene; buttons={buttons}, repeat={repeat}");
        }
    }
    // A picture produced after restoring a moment supersedes the earlier bootstrap image.
    let (before_resume,..)=core.run(1<<5);
    let (after_resume,..)=core.run(1<<4);
    let resumed=video::startup(before_resume,after_resume).unwrap();
    gfx.present(&overlay,(32,32),(0,0,32,32),Some((&resumed,Area::new(0,0,32,32))),false,&[]);
    assert!(read().chunks_exact(4).all(|p|p[..3]==[0,0,255]),"Continue must show the newer scene rather than the bootstrap picture");
    drop(core);drop(gfx);drop(context);
}

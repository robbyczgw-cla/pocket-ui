//! Exercise the Android core's real negotiation on the host, without creating a GPU context.
#[allow(dead_code)]
#[path = "../src/platform/android_hw_render.rs"]
mod hw;
use std::ffi::{c_char, c_void};
static EGL_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[repr(C)]
struct Request {
    kind: u32, reset: Option<unsafe extern "C" fn()>,
    framebuffer: Option<unsafe extern "C" fn() -> usize>,
    proc_address: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
    depth: bool, stencil: bool, bottom_left: bool, major: u32, minor: u32,
    cache: bool, destroy: Option<unsafe extern "C" fn()>, debug: bool,
}
#[test]
fn saturn_can_request_gles31_and_desktop_contexts_are_refused() {
    let _lock = EGL_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut r=Request {kind:5,reset:None,framebuffer:None,proc_address:None,depth:true,stencil:true,bottom_left:true,major:3,minor:1,cache:false,destroy:None,debug:false};
    unsafe {
        assert_eq!(hw::environment(14,(&mut r as *mut Request).cast()),Some(true),"YabaSanshiro needs GLES 3.1 before its first frame");
        assert!(r.framebuffer.is_some()&&r.proc_address.is_some());
        r.kind=3;assert_eq!(hw::environment(14,(&mut r as *mut Request).cast()),Some(false));
        r.kind=5;r.major=4;assert_eq!(hw::environment(14,(&mut r as *mut Request).cast()),Some(false));
        assert_eq!(hw::environment(14,std::ptr::null_mut()),Some(false));
    }
    hw::reset();
}

#[test]
fn a_new_gles_scene_stays_visible_when_the_core_leaves_no_read_buffer() {
    use glow::HasContext;
    let _lock = EGL_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let (gl, headless) = match crate::headless_gl::context() {
        Ok(context) => context,
        Err(error) if std::env::var_os("POCKET_REQUIRE_GL").is_some() => panic!("no headless GLES: {error}"),
        Err(error) => { eprintln!("GLES readback skipped: {error}; POCKET_REQUIRE_GL=1 requires it"); return; }
    };
    eprintln!("Android readback through {}", headless.renderer);
    hw::reset();
    let mut r=Request {kind:4,reset:None,framebuffer:None,proc_address:None,depth:false,stencil:false,bottom_left:false,major:3,minor:0,cache:false,destroy:None,debug:false};
    unsafe { assert_eq!(hw::environment(14,(&mut r as *mut Request).cast()),Some(true)); }
    let core = hw::HwContext::create(8, 8).unwrap().unwrap();
    core.core_current();
    unsafe {
        let fbo = std::num::NonZeroU32::new(r.framebuffer.unwrap()() as u32).map(glow::NativeFramebuffer);
        gl.bind_framebuffer(glow::FRAMEBUFFER, fbo);
        gl.clear_color(0.25, 0.5, 0.75, 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        // A core's scene passes may leave arbitrary GL state behind. Its visible colour
        // attachment still contains the picture when it no longer selects it for reads.
        for selected in [glow::COLOR_ATTACHMENT0, glow::NONE, glow::COLOR_ATTACHMENT1] {
            gl.read_buffer(selected);
            assert_eq!(gl.get_error(), glow::NO_ERROR);
            assert!(hw::video_refresh(usize::MAX as *const c_void, 8, 8));
            let picture = core.capture().unwrap();
            let error = gl.get_error();
            assert_eq!(error, glow::NO_ERROR, "the frame must remain readable after a scene pass");
            assert!(picture.to_rgb().chunks_exact(3).all(|p| p[0].abs_diff(64)<=1 && p[1].abs_diff(128)<=1 && p[2].abs_diff(191)<=1), "the new scene must retain its colours instead of becoming black");
            assert_eq!(gl.get_parameter_i32(glow::READ_BUFFER), selected as i32, "the core keeps its own read-buffer state");
        }
    }
    assert_eq!(hw::read_buffer_overrides(), 2, "only the two changed selections needed an override");
    drop(core);
    hw::reset();
}

#[allow(dead_code)]
#[path = "../android/src/play_cards.rs"]
mod cards;
#[test]
fn sega_touch_pad_offers_six_saturn_buttons_and_dreamcast_triggers() {
    for (system, expected) in [("saturn",vec![(0,"A"),(8,"B"),(10,"C"),(1,"X"),(9,"Y"),(11,"Z"),(12,"L"),(13,"R")]),("dreamcast",vec![(0,"A"),(8,"B"),(1,"X"),(9,"Y"),(12,"L"),(13,"R")])] {
        for (bit, label) in expected {
            assert!(cards::touch_bits(system).contains(&bit),"{system}: touch {label} is missing");
            assert_eq!(cards::touch_label(system,bit as usize),label,"{system}: the touch label must match the core");
        }
        assert!(!cards::touch_bits(system).contains(&2),"Neither console has Select");
    }
}
#[test]
fn a_dreamcast_cdi_on_the_sd_card_is_on_the_shelf() {
    use pocket_ui::document_library::{scan,Document};
    let games=scan(vec![Document {uri:"content://sd/game".into(),id:"game".into(),authority:"sd".into(),relative:"dreamcast/Test.cdi".into(),root_name:String::new(),sheet:String::new(),size:900<<20}]);
    assert_eq!(games.len(),1,"A CDI supported by Flycast must not disappear from the shelf");
    assert_eq!(games[0].system_id,"dreamcast");
    assert!(pocket_ui::document_library::takes("dreamcast").contains(&"cdi"));
}

#[test]
fn sega_controls_are_named_before_the_first_game() {
    for system in ["saturn","dreamcast"] {
        let lines=cards::control_lines(system,"",&[],false);
        assert!(!lines.iter().any(|s|s.contains("Play with the D-pad")),"{system}: the bundled core's verified pad should already be explained");
    }
}

#[test]
fn dreamcast_touch_arrows_move_the_stick_without_swallowing_its_trigger() {
    let (axes, consumed) = cards::touch_analog("dreamcast",(1<<4)|(1<<7)|(1<<13))
        .expect("Dreamcast games such as Sonic need the analog stick when using touch arrows");
    assert_eq!(axes,[32767,-32767,0,0]);
    assert_eq!(consumed & (1<<13),0,"R remains the Dreamcast trigger, never a C arrow");
    assert_eq!(cards::touch_analog("dreamcast",0).unwrap().0,[0;4],"Lifting the finger releases the stick");
    assert_eq!(cards::touch_analog("n64",1<<13).unwrap().0,[0,0,32767,0],"The N64 C arrow keeps its right-stick mapping");
    assert!(cards::touch_analog("saturn",1<<7).is_none(),"The Saturn digital pad stays digital");
}


// Test the actual pad painter as well as the controls card's table.
use cards as play_cards;
#[path = "../android/src/player_touch.rs"]
mod painted_touch;
#[test]
fn painted_sega_buttons_reach_every_console_button_shown_on_the_controls_card() {
    for system in ["saturn", "dreamcast"] {
        for (w,h) in [(1280,720),(720,1280)] {
            let mut pixels=vec![0;w*h];
            let mut canvas=crate::gfx::Canvas::new(&mut pixels,w as u32,h as u32);
            let (areas,_,_)=painted_touch::touch_controls(&mut canvas,&mut crate::text::Text::builtin(),system,(w as i32,h as i32),1.0,None,0);
            for bit in cards::touch_bits(system) {
                let (area,_)=areas.iter().find(|(_,b)|*b==u16::from(bit)).unwrap_or_else(||panic!("{system}: the painted {} button is missing",cards::touch_label(system,bit as usize)));
                assert!(area.w>0 && area.h>0);
                let x=(area.x+area.w/2).clamp(0,w as i32-1) as usize;
                let y=(area.y+area.h/2).clamp(0,h as i32-1) as usize;
                assert_ne!(pixels[y*w+x],0,"{system}: an active button must be visible");
            }
            assert!(!areas.iter().any(|(_,b)|*b==2),"Sega consoles must not show Select");
            if system=="dreamcast" {assert!(!areas.iter().any(|(_,b)|[10,11].contains(b)),"Dreamcast L/R are analog-trigger bits, not L1/R1");}
        }
    }
}

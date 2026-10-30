//! Frames of Cozy's old-TV power-on (`crt_on`) without a device: a made-up game picture (`picture_shots`' SNES scene
//! style, drawn here) through the real renderer and the real overlay (`Overlay::draw_solid`) on a headless GLES 3
//! context, at the given moments, one PNG per frame and the draw time of the effect's pass.
//!
//!     cargo run --release --example crt_on_shots -- <out-dir> [width height] [ms …]

use std::path::Path;
use std::time::{Duration, Instant};

use pocket_ui::libretro::{Frame, PIXEL_XRGB8888};
use pocket_ui::overlay::Overlay;
use pocket_ui::video::{Renderer, VideoSettings};

fn picture() -> Frame {
    let (w, h) = (256u32, 224u32);
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let sky = 0x40 + (y * 0x80 / h);
            let (r, g, b) = if y > 160 { (0x30 + (x / 4 % 2) * 0x40, 0x90, 0x30) } else { (0x30, sky, 0xe0) };
            let tile = if (x / 16 + y / 16) % 2 == 0 && y < 120 && x > 90 && x < 170 { 0xff } else { 0 };
            data.extend_from_slice(&[(b | tile) as u8, (g | tile) as u8, (r | tile) as u8, 0xff]);
        }
    }
    Frame { width: w, height: h, pitch: (w * 4) as usize, format: PIXEL_XRGB8888, data }
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: crt_on_shots <out-dir> [width height] [ms …]");
        std::process::exit(2);
    };
    let nums: Vec<u64> = args[1..].iter().filter_map(|a| a.parse().ok()).collect();
    let ((w, h), times) = if nums.len() >= 2 { ((nums[0] as u32, nums[1] as u32), nums[2..].to_vec()) } else { ((1240, 1080), vec![]) };
    let times = if times.is_empty() { vec![0, 20, 50, 80, 110, 140, 170, 200, 240, 270, 299, 300] } else { times };
    std::fs::create_dir_all(out).expect("out dir");
    let (gl, hl) = pocket_ui::headless_gl::context().unwrap_or_else(|e| panic!("no headless GL: {e}"));
    eprintln!("renderer: {}", hl.renderer);
    let mut r = Renderer::new(gl).expect("renderer");
    r.set_frame_source("snes", Path::new("/nonexistent"));
    r.upload(&picture());
    let mut overlay = Overlay::new(r.gl()).expect("overlay");
    let settings = VideoSettings::default();
    let placement = r.placement(w, h, 4.0 / 3.0, &settings).map(|(r, _)| r);
    for ms in times {
        let mut spent = Duration::ZERO;
        let rgba = r.render_offscreen_over(w, h, 4.0 / 3.0, &settings, &mut |gl| {
            let area = placement.map_or((0.0, 0.0, w as f32, h as f32), |r| (r.x as f32, r.y as f32, r.w as f32, r.h as f32));
            let quads = pocket_ui::crt_on::plan(Duration::from_millis(ms), area);
            let t0 = Instant::now();
            overlay.draw_solid(gl, w, h, &quads);
            unsafe { glow::HasContext::finish(gl) };
            spent = t0.elapsed();
        });
        write_png(&Path::new(out).join(format!("crt-{ms:03}.png")), w, h, &rgba);
        println!("{ms:>3} ms: effect pass {:.2} ms (llvmpipe, CPU)", spent.as_secs_f32() * 1000.0);
    }
}

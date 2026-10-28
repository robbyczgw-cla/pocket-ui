//! Over the game without a device: paints what pocket-core's HUD draws for one level of Over the game (`over_game`:
//! off, fps, sensors, pixel) where the HUD puts it, at the HUD's opacity, over a background. The FPS line has made-up
//! numbers; Pixel Path's strip is the one of a made-up case (`pixel_live::demo`: gba, snes, n64, runahead,
//! compromised, pal, gb, nds).
//!
//!     cargo run --release --example over_game_shots -- <level> <out.png> [case] [background.png]
//!
//! The background is a 1240×1080 screenshot of the game with nothing over it (the virtual device's `vdev.sh shot`);
//! without one the picture's place (the case's) is drawn in grey, so the placement can be seen. The numbers are made
//! up, not the screenshot's: the virtual device's own line is in `vdev.sh shot` with Over the game on that level.

use std::path::Path;

use pocket_ui::over_game::Level;
use pocket_ui::pixel_live::{self, KEY};
use pocket_ui::text::Text;

/// What the FPS line says on a calm 60 fps game, and with the sensors of a Mini V2 under load.
const FPS_LINE: &str = "60 FPS   CPU 23%";
const SENSORS_LINE: &str = "60 FPS   CPU 23%   62 °C   Fan 45%   4.8 W";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(level), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: over_game_shots <off|fps|sensors|pixel> <out.png> [case] [background.png]");
        std::process::exit(2);
    };
    let Some(level) = Level::ALL.into_iter().find(|l| l.value() == level) else {
        eprintln!("no level {level}: off, fps, sensors or pixel");
        std::process::exit(2);
    };
    let case = args.get(3).map_or("gba", String::as_str);
    let Some(strip) = pixel_live::demo(case) else {
        eprintln!("no case {case}");
        std::process::exit(2);
    };
    let (vw, vh) = strip.view;
    let mut px: Vec<u32> = match args.get(4).and_then(|p| pocket_ui::image::load_png(Path::new(p))) {
        Some(img) if img.w as i32 == vw && img.h as i32 == vh => img.px,
        Some(img) => {
            eprintln!("background is {}×{}, not {vw}×{vh}", img.w, img.h);
            std::process::exit(1);
        }
        None => {
            let mut px = vec![0u32; (vw * vh) as usize];
            let (x, y, w, h) = strip.picture;
            for row in y.max(0)..(y + h).min(vh) {
                for col in x.max(0)..(x + w).min(vw) {
                    // A quiet checker, so the picture's edges show.
                    px[(row * vw + col) as usize] = if (row / 40 + col / 40) % 2 == 0 { 0x4a5a6a } else { 0x56687a };
                }
            }
            px
        }
    };
    let mut text = Text::builtin();
    // One piece at most, as the HUD draws it: (x, y, w, h, pixels, opacity).
    let piece = match level {
        Level::Off => None,
        Level::Fps | Level::Sensors => {
            let line = if level.sensors() { SENSORS_LINE } else { FPS_LINE };
            let (w, h, buf) = pocket_ui::overlay::paint_pill(&mut text, line);
            Some((pocket_ui::overlay::MARGIN, pocket_ui::overlay::MARGIN, w, h, buf, pocket_ui::overlay::ALPHA))
        }
        Level::PixelPath => {
            let at = pixel_live::place(&strip, |s| pixel_live::measure(&mut text, &strip, s));
            let (w, h, buf) = pixel_live::paint(&mut text, &strip, at.style);
            Some((at.x, at.y, w, h, buf, at.style.alpha()))
        }
    };
    if let Some((x0, y0, w, h, buf, alpha)) = &piece {
        let mix = |a: u32, b: u32, shift: u32| -> u32 {
            let (a, b) = ((a >> shift) & 0xff, (b >> shift) & 0xff);
            ((a as f32 * (1.0 - alpha) + b as f32 * alpha).round() as u32) << shift
        };
        for row in 0..*h {
            for col in 0..*w {
                let (x, y) = (x0 + col, y0 + row);
                let s = buf[(row * w + col) as usize];
                if s == KEY || x < 0 || y < 0 || x >= vw || y >= vh {
                    continue;
                }
                let d = &mut px[(y * vw + x) as usize];
                *d = mix(*d, s, 16) | mix(*d, s, 8) | mix(*d, s, 0);
            }
        }
    }
    let rgb: Vec<u8> = px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let file = std::fs::File::create(out).expect("create output");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), vw as u32, vh as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&rgb)).expect("write png");
    // What the HUD does when its piece changes (twice a second at most): measure, place, paint.
    let t0 = std::time::Instant::now();
    for _ in 0..200 {
        match level {
            Level::Off => {}
            Level::Fps => drop(std::hint::black_box(pocket_ui::overlay::paint_pill(&mut text, FPS_LINE))),
            Level::Sensors => drop(std::hint::black_box(pocket_ui::overlay::paint_pill(&mut text, SENSORS_LINE))),
            Level::PixelPath => {
                let at = pixel_live::place(&strip, |s| pixel_live::measure(&mut text, &strip, s));
                std::hint::black_box(pixel_live::paint(&mut text, &strip, at.style));
            }
        }
    }
    println!("{}: paint {:.0} µs a change", level.label(), t0.elapsed().as_secs_f64() * 1e6 / 200.0);
    match &piece {
        None => println!("off: nothing drawn"),
        Some((x, y, w, h, _, alpha)) => println!("{}: at {x},{y} {w}×{h}, opacity {alpha}", level.label()),
    }
}

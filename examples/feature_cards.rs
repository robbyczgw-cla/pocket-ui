//! Paints every feature card (with its preview) to PNG files, dark and light, and prints how long each took:
//! `cargo run --release --example feature_cards -- <out dir>`. For looking at the cards without the app.

use pocket_ui::features::{self, Cards, Mode, Switches};
use pocket_ui::gfx::Canvas;
use pocket_ui::text::Text;

fn write_png(buf: &[u32], w: u32, h: u32, out: &std::path::Path) {
    let rgb: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out).expect("create")), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&rgb)).expect("write png");
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/feature-cards".into()));
    std::fs::create_dir_all(&out).expect("out dir");
    let mut text = Text::new();
    let (w, h) = (1240u32, 1080u32);
    let mut buf = vec![0u32; (w * h) as usize];
    for (light, name) in [(0u8, "dark"), (255, "light")] {
        pocket_ui::palette::set_blend(light);
        let list = features::tour_list();
        let mut cards = Cards::new(Mode::Tour, list.clone(), Switches::default(), Vec::new());
        for (k, i) in list.iter().enumerate() {
            cards.show(k);
            let t0 = std::time::Instant::now();
            cards.paint(&mut Canvas::new(&mut buf, w, h), &mut text);
            let first = t0.elapsed();
            let t1 = std::time::Instant::now();
            cards.paint(&mut Canvas::new(&mut buf, w, h), &mut text);
            println!(
                "{name} {:<22} first paint {:>6.1} ms, again {:>5.1} ms",
                features::FEATURES[*i].id,
                first.as_secs_f64() * 1e3,
                t1.elapsed().as_secs_f64() * 1e3
            );
            write_png(&buf, w, h, &out.join(format!("{name}-{:02}-{}.png", k, features::FEATURES[*i].id)));
        }
        // The preview alone, and for comparison what the general `Image::resized` would take to scale one.
        for (_, f) in features::available() {
            let t0 = std::time::Instant::now();
            let _ = features::render_preview(f, &mut text, light == 255);
            let all = t0.elapsed();
            let big = pocket_ui::image::Image {
                w,
                h,
                px: vec![0x123456; (w * h) as usize],
            };
            let t1 = std::time::Instant::now();
            let _ = big.resized(features::PREVIEW.0, features::PREVIEW.1);
            println!(
                "{name} {:<22} preview {:>5.1} ms (Image::resized alone: {:>4.1} ms)",
                f.id,
                all.as_secs_f64() * 1e3,
                t1.elapsed().as_secs_f64() * 1e3
            );
        }
        for sel in 0..3 {
            features::paint_profiles(&mut Canvas::new(&mut buf, w, h), &mut text, sel);
            write_png(&buf, w, h, &out.join(format!("{name}-profiles-{sel}.png")));
        }
    }
}

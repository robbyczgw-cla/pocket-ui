//! The demo's frames of a photo being taken over a running game (`album_eject`), painted by the same code pocket-core
//! paints it with:
//!
//!     cargo run --example album_eject_frames -- <frames.rgb> <w>x<h> <device.png> <x>,<y>,<scale> <photo.jpg> <first> <out dir>
//!
//! `frames.rgb`: the game's frames one after another as packed RGB (`w` x `h` each, a headless run of the real game);
//! `device.png`: a screen of the device as pocket-core shows a game (its frame around it), the game drawn into it at
//! `x`,`y` times `scale`; `photo.jpg`: the album's photo; `first`: the frame at which the moment starts (60 per second).
//! Writes `out dir/NNNN.png`, one per game frame.

use std::io::Read;

use pocket_ui::gfx::Canvas;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    // An 8th argument: each frame given stands for that many (a preview of every 20th frame: `20`).
    let step: usize = a.get(7).and_then(|v| v.parse().ok()).unwrap_or(1);
    let [frames, size, device, at, photo, first, out] = &a[..a.len().min(7)] else {
        eprintln!("usage: album_eject_frames <frames.rgb> <w>x<h> <device.png> <x>,<y>,<scale> <photo.jpg> <first> <out dir>");
        std::process::exit(2);
    };
    let (gw, gh) = size.split_once('x').map(|(w, h)| (w.parse::<usize>().unwrap(), h.parse::<usize>().unwrap())).unwrap();
    let at: Vec<i32> = at.split(',').map(|v| v.parse().unwrap()).collect();
    let (gx, gy, k) = (at[0], at[1], at[2]);
    let device = pocket_ui::image::load_png(std::path::Path::new(device)).expect("device picture");
    let photo = pocket_ui::image::load_jpeg(std::path::Path::new(photo)).expect("photo");
    let first: usize = first.parse().unwrap();
    std::fs::create_dir_all(out).unwrap();
    let mut raw = Vec::new();
    std::fs::File::open(frames).unwrap().read_to_end(&mut raw).unwrap();
    let n = raw.len() / (gw * gh * 3);
    let (w, h) = (device.w as usize, device.h as usize);
    for i in 0..n {
        let mut buf = device.px.clone();
        let f = &raw[i * gw * gh * 3..][..gw * gh * 3];
        for y in 0..gh * k as usize {
            for x in 0..gw * k as usize {
                let s = ((y / k as usize) * gw + x / k as usize) * 3;
                let (dx, dy) = (gx as usize + x, gy as usize + y);
                if dx < w && dy < h {
                    buf[dy * w + dx] = (f[s] as u32) << 16 | (f[s + 1] as u32) << 8 | f[s + 2] as u32;
                }
            }
        }
        if i * step >= first {
            let t = (i * step - first) as f32 / 60.0;
            if t <= pocket_ui::album_eject::LENGTH.as_secs_f32() {
                let mut c = Canvas::new(&mut buf, w as u32, h as u32);
                pocket_ui::album_eject::paint(&mut c, &photo, t, (0, 0, w as i32, h as i32));
            }
        }
        let rgb: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        let file = std::fs::File::create(format!("{out}/{i:04}.png")).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_compression(png::Compression::Fast);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    }
    println!("{n} frames");
}

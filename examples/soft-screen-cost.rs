//! CPU-only before/after cost, no window/device state. Build with --features brick.
//! --snapshots DIR writes golden PNGs from the frozen old renderer, never from the new renderer.
#[cfg(feature = "brick")]
#[path = "../tests/common/soft_screen_reference.rs"]
mod reference;

#[cfg(feature = "brick")]
fn main() {
    use pocket_ui::{
        display::Look,
        libretro::{PIXEL_RGB565, PIXEL_XRGB8888},
        soft_screen,
    };
    use std::{hint::black_box, time::Instant};
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--snapshots") {
        let dir = std::path::Path::new(&args[2]);
        std::fs::create_dir_all(dir).unwrap();
        let frame = reference::frame(PIXEL_XRGB8888, 16, 13);
        for (name, warmth, night) in [("neutral", 0, 0), ("warm2", 2, 0), ("warm4", 4, 0), ("cool4", -4, 0), ("night", 4, 3800)] {
            let mut dst = vec![0; 67 * 53];
            reference::render(
                &frame,
                &mut dst,
                67,
                53,
                Look {
                    soft: true,
                    warmth,
                    night,
                    contrast: 0,
                },
            );
            let rgb: Vec<u8> = dst.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
            let mut enc = png::Encoder::new(std::fs::File::create(dir.join(format!("{name}.png"))).unwrap(), 67, 53);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&rgb).unwrap();
        }
        return;
    }
    let frames: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(120);
    println!("format,warmth,renderer,round,frames,ms_per_frame");
    for format in [PIXEL_XRGB8888, PIXEL_RGB565] {
        let frame = reference::frame(format, 240, 160);
        let mut dst = vec![0u32; 1024 * 768];
        for warmth in [0, 2, 4] {
            let look = Look {
                soft: true,
                warmth,
                ..Look::default()
            };
            let t = look.tables(); // Cached when a look changes, as in SoftScreen::set_look.
            let mut expected = vec![0; dst.len()];
            reference::render(&frame, &mut expected, 1024, 768, look);
            if warmth != 0 {
                soft_screen::blit_tinted(&frame, &mut dst, 1024, 768, &t);
            } else {
                soft_screen::blit(&frame, &mut dst, 1024, 768);
            }
            assert_eq!(dst, expected, "before/after pixel difference must be 0 LSB");
            for round in 0..3 {
                for old in [true, false] {
                    let mut render = || {
                        if old {
                            // The old hot path cached these tables too; exclude table construction from timing.
                            reference::blit(black_box(&frame), black_box(&mut dst), 1024, 768);
                            if warmth != 0 {
                                for p in &mut dst {
                                    *p = (t[0][(*p >> 16 & 255) as usize] as u32) << 16
                                        | (t[1][(*p >> 8 & 255) as usize] as u32) << 8
                                        | t[2][(*p & 255) as usize] as u32;
                                }
                            }
                        } else if warmth != 0 {
                            soft_screen::blit_tinted(black_box(&frame), black_box(&mut dst), 1024, 768, black_box(&t));
                        } else {
                            soft_screen::blit(black_box(&frame), black_box(&mut dst), 1024, 768);
                        }
                        black_box(&dst);
                    };
                    for _ in 0..10 {
                        render();
                    }
                    let start = Instant::now();
                    for _ in 0..frames {
                        render();
                    }
                    println!(
                        "{format},{warmth},{},{round},{frames},{:.6}",
                        if old { "before" } else { "after" },
                        start.elapsed().as_secs_f64() * 1000. / frames as f64
                    );
                }
            }
        }
    }
}

#[cfg(not(feature = "brick"))]
fn main() {
    eprintln!("Build with --features brick");
}

// Also runnable without the UI binaries: cargo test --features brick --example soft-screen-cost.
#[cfg(all(feature = "brick", test))]
#[path = "../tests/soft_screen_colour.rs"]
mod colour_tests;

//! Player-visible colour and placement must stay identical to the old software path.
#![cfg(feature = "brick")]
#[path = "common/soft_screen_reference.rs"]
mod reference;
use pocket_ui::{
    display::Look,
    libretro::{PIXEL_0RGB1555, PIXEL_RGB565, PIXEL_XRGB8888},
    soft_screen,
};

fn render(frame: &pocket_ui::libretro::Frame, w: usize, h: usize, look: Look) -> Vec<u32> {
    let mut dst = vec![0xdeadbeef; w * h];
    if soft_screen::tinted(&look) {
        soft_screen::blit_tinted(frame, &mut dst, w, h, &look.tables());
    } else {
        soft_screen::blit(frame, &mut dst, w, h);
    }
    dst
}

#[test]
fn colours_and_placement_match_the_old_picture_in_every_core_pixel_format() {
    for format in [PIXEL_XRGB8888, PIXEL_RGB565, PIXEL_0RGB1555] {
        for (sw, sh, dw, dh) in [
            (240, 160, 1024, 768),
            (160, 144, 1024, 768),
            (256, 224, 1024, 768),
            (19, 13, 67, 53),
            (89, 71, 67, 53),
            (97, 31, 67, 53),
            (0, 0, 13, 11),
            (1, 1, 0, 0),
        ] {
            let frame = reference::frame(format, sw, sh);
            for (warmth, night, soft) in [
                (0, 0, true),
                (2, 0, true),
                (4, 0, true),
                (-10, 0, true),
                (10, 0, true),
                (0, 3800, true),
                (4, 4400, true),
                (-4, 3200, true),
                (4, 3800, false),
            ] {
                let look = Look {
                    soft,
                    warmth,
                    night,
                    contrast: 7,
                }; // Contrast is gamma-only, even when nonzero.
                let mut old = vec![0xdeadbeef; dw * dh];
                reference::render(&frame, &mut old, dw, dh, look);
                assert_eq!(render(&frame, dw, dh, look), old, "format {format}, {sw}x{sh} -> {dw}x{dh}, {look:?}");
            }
        }
    }
}

#[test]
fn every_16_bit_colour_keeps_its_exact_rounding() {
    for format in [PIXEL_RGB565, PIXEL_0RGB1555] {
        let frame = pocket_ui::libretro::Frame {
            width: 256,
            height: 256,
            pitch: 512,
            format,
            data: (0..=u16::MAX).flat_map(u16::to_le_bytes).collect(),
        };
        for warmth in [-10, -4, 0, 2, 4, 10] {
            let look = Look {
                soft: true,
                warmth,
                ..Look::default()
            };
            let mut old = vec![0; 512 * 512];
            reference::render(&frame, &mut old, 512, 512, look);
            assert_eq!(render(&frame, 512, 512, look), old, "format {format}, warmth {warmth}");
        }
    }
}

#[test]
fn screen_colour_snapshots_stay_pixel_exact() {
    let frame = reference::frame(PIXEL_XRGB8888, 16, 13);
    for (name, warmth, night) in [("neutral", 0, 0), ("warm2", 2, 0), ("warm4", 4, 0), ("cool4", -4, 0), ("night", 4, 3800)] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/snapshots/soft-screen")
            .join(format!("{name}.png"));
        let mut reader = png::Decoder::new(std::fs::File::open(path).unwrap()).read_info().unwrap();
        let mut golden = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut golden).unwrap();
        assert_eq!((info.width, info.height), (67, 53));
        assert_eq!(info.color_type, png::ColorType::Rgb);
        let pixels = render(
            &frame,
            67,
            53,
            Look {
                soft: true,
                warmth,
                night,
                contrast: 0,
            },
        );
        let actual: Vec<u8> = pixels.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        assert_eq!(actual, &golden[..info.buffer_size()], "{name}: RGB difference must be 0 LSB");
    }
}

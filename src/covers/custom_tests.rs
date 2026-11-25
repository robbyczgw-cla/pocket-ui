//! A cover chosen on the device (Android's photo picker): what is taken, how it is scaled, what is refused and why, and
//! the way back to the automatic cover.

use super::*;
use crate::testutil::TestDir;

fn jpeg(w: u16, h: u16) -> Vec<u8> {
    let px: Vec<u8> = (0..w as usize * h as usize).flat_map(|i| [(i % 251) as u8, 0x80, 0x20]).collect();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 85).encode(&px, w, h, jpeg_encoder::ColorType::Rgb).unwrap();
    out
}

fn png_of(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&vec![0xc0; (w * h * 4) as usize]).unwrap();
    }
    out
}

fn size_of(png: &[u8]) -> (u32, u32) {
    let img = crate::image::read_png(io::Cursor::new(png)).expect("the cover reads back");
    (img.w, img.h)
}

#[test]
fn a_photo_becomes_a_png_cover_of_at_most_1024_pixels_a_side() {
    let cover = from_picture(&jpeg(2048, 1536)).expect("a camera-sized JPEG is taken");
    assert!(valid_png(&cover), "what the card keeps is the PNG the web companion would send");
    assert_eq!(size_of(&cover), (1024, 768), "scaled down, the shape kept");
    let tall = from_picture(&jpeg(600, 3000)).unwrap();
    assert_eq!(size_of(&tall).1, 1024);
}

#[test]
fn a_small_picture_keeps_its_size() {
    let cover = from_picture(&png_of(300, 400)).expect("a small PNG is taken");
    assert!(valid_png(&cover));
    assert_eq!(size_of(&cover), (300, 400));
    assert_eq!(size_of(&from_picture(&jpeg(120, 160)).unwrap()), (120, 160));
}

#[test]
fn a_picture_over_8_mb_is_refused_with_its_reason() {
    let mut big = jpeg(64, 64);
    big.resize(MAX_BYTES as usize + 1, 0);
    assert_eq!(from_picture(&big), Err(Refused::TooBig));
    assert!(Refused::TooBig.message().contains("8 MB"));
}

#[test]
fn a_file_of_another_type_is_refused() {
    let mut webp = b"RIFF\x24\0\0\0WEBPVP8 ".to_vec();
    webp.resize(64, 0);
    let mut heic = b"\0\0\0\x18ftypheic".to_vec();
    heic.resize(64, 0);
    for (what, bytes) in [("empty", Vec::new()), ("GIF", b"GIF89a\x01\0\x01\0\0\0\0;".to_vec()), ("WebP", webp), ("HEIC", heic), ("text", b"not a picture".to_vec())] {
        assert_eq!(from_picture(&bytes), Err(Refused::NotAPicture), "{what}");
    }
    assert!(Refused::NotAPicture.message().contains("PNG or JPEG"));
}

#[test]
fn a_broken_picture_is_refused() {
    let whole = png_of(64, 64);
    assert_eq!(from_picture(&whole[..whole.len() / 2]), Err(Refused::NotAPicture), "a PNG cut off");
    let photo = jpeg(256, 256);
    assert_eq!(from_picture(&photo[..40]), Err(Refused::NotAPicture), "a JPEG cut off in its header");
}

#[test]
fn a_picture_with_too_many_pixels_is_refused_before_it_is_read() {
    // Only the header: 9000 x 9000 would be 324 MB as 0RGB; it is refused without a byte allocated for it.
    let mut head = PNG_SIGNATURE.to_vec();
    head.extend_from_slice(b"\0\0\0\rIHDR");
    head.extend_from_slice(&9000u32.to_be_bytes());
    head.extend_from_slice(&9000u32.to_be_bytes());
    head.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(from_picture(&head), Err(Refused::TooManyPixels));
}

#[test]
fn a_chosen_cover_shows_and_reset_brings_back_the_automatic_one() {
    let data = TestDir::new("cover-pick");
    let rom = data.join("roms/gba/Advance Wars (USA).gba");
    let automatic = crate::covers::cover_path(&data, "gba", &rom);
    std::fs::create_dir_all(automatic.parent().unwrap()).unwrap();
    std::fs::write(&automatic, png_of(4, 4)).unwrap();

    let chosen = from_picture(&jpeg(1200, 1200)).unwrap();
    apply(&data, "gba", &rom, Some(&chosen)).unwrap();
    assert_eq!(crate::covers::active_cover_path(&data, "gba", &rom), path(&data, "gba", &rom), "the chosen cover is the one shown");
    assert_eq!(std::fs::read(path(&data, "gba", &rom)).unwrap(), chosen);

    apply(&data, "gba", &rom, None).unwrap();
    assert!(existing(&data, "gba", &rom).is_none());
    assert_eq!(crate::covers::active_cover_path(&data, "gba", &rom), automatic, "reset: the usual cover again");
    let kept = std::fs::read_dir(data.join("covers").join(FOLDER).join("backups")).unwrap().count();
    assert_eq!(kept, 1, "the reset cover is kept as a copy");
}

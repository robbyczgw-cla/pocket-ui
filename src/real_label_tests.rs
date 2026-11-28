//! What a player sees of a real label: it is the game's own (not the cover), it sits where the label is and nowhere
//! else, it is as coarse as the pixel art round it, a disc stays round with its hole, and nothing is read in a frame.

use super::*;
use crate::cabinet_art::Look;
use crate::media::{self, DiscKind, Game};
use crate::testutil::TestDir;

const RED: u32 = 0xd02020;
const BLUE: u32 = 0x2040c0;
const YELLOW: u32 = 0xe0c020;

fn look() -> Look {
    Look { color: 0x304050, band: 0x8090a0, plastic: 0x505050, tape: false }
}

fn text() -> crate::text::Text {
    crate::text::Text::new()
}

/// A picture of a whole cartridge as ScreenScraper's support-2D gives it: cut out (a transparent margin round it), the
/// template's shape `k` times, grey, its label red.
fn cart_photo(g: Game, k: i32) -> Rgba {
    let ((cw, ch), (lx, ly, lw, lh)) = media::label_cell(g).unwrap();
    let m = 7;
    let (w, h) = (cw * k + 2 * m, ch * k + 2 * m);
    let mut px = vec![0u32; (w * h) as usize];
    for y in 0..ch * k {
        for x in 0..cw * k {
            let on = x >= lx * k && x < (lx + lw) * k && y >= ly * k && y < (ly + lh) * k;
            px[((y + m) * w + x + m) as usize] = 0xff00_0000 | if on { RED } else { 0x7a7a7a };
        }
    }
    Rgba { w: w as u32, h: h as u32, px }
}

/// A flat label scan: blue on the left half, yellow on the right, on a white scanner bed.
fn label_scan(w: u32, h: u32) -> Rgba {
    let m = 5;
    let (fw, fh) = (w + 2 * m, h + 2 * m);
    let px = (0..fh)
        .flat_map(|y| (0..fw).map(move |x| (x, y)))
        .map(|(x, y)| {
            let inside = x >= m && x < m + w && y >= m && y < m + h;
            0xff00_0000 | if !inside { 0xffffff } else if x < m + w / 2 { BLUE } else { YELLOW }
        })
        .collect();
    Rgba { w: fw, h: fh, px }
}

fn near(a: u32, b: u32, tol: i32) -> bool {
    let d = |s: u32| ((a >> s & 0xff) as i32 - (b >> s & 0xff) as i32).abs();
    d(16) <= tol && d(8) <= tol && d(0) <= tol
}

fn png_file(path: &Path, img: &Rgba) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.w, img.h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = img.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]).collect();
        enc.write_header().unwrap().write_image_data(&data).unwrap();
    }
    std::fs::write(path, out).unwrap();
}

#[test]
fn the_thing_is_found_in_a_cut_out_picture_and_a_label_scan_is_kept_whole() {
    let photo = cart_photo(Game::MegaDrive, 2);
    let ((cw, ch), _) = media::label_cell(Game::MegaDrive).unwrap();
    assert_eq!(content_box(&photo), Some((7, 7, cw as u32 * 2, ch as u32 * 2)));
    // A label scan is the label: its own white edge stays.
    assert_eq!(content_box(&label_scan(60, 30)), Some((0, 0, 70, 40)));
    // Nothing in it: no label (a broken download never prints a blank).
    assert_eq!(content_box(&Rgba { w: 40, h: 40, px: vec![0; 1600] }), None);
}

#[test]
fn a_whole_cartridge_s_picture_gives_its_label_from_where_the_label_is() {
    for g in [Game::MegaDrive, Game::Nes, Game::Snes, Game::Gba, Game::N64, Game::Gb, Game::MasterSystem] {
        let ((_, _), (_, _, lw, lh)) = media::label_cell(g).unwrap();
        let target = Target::Cart { size: media::label_cell(g).unwrap().0, label: media::label_cell(g).unwrap().1 };
        let label = prepare(&cart_photo(g, 3), Face::Support, target).unwrap();
        assert_eq!((label.w as i32, label.h as i32), (lw * media::LABEL_DETAIL, lh * media::LABEL_DETAIL), "{g:?}: the label is made finer than the template");
        let red = label.px.iter().filter(|&&p| near(p, RED, 40)).count();
        assert!(red as f32 >= label.px.len() as f32 * 0.9, "{g:?}: the label's place, not the shell ({red} of {} red)", label.px.len());
    }
}

#[test]
fn a_flat_label_scan_is_the_label_whole_on_the_label() {
    let g = Game::Snes;
    let (size, label) = media::label_cell(g).unwrap();
    let made = prepare(&label_scan(label.2 as u32 * 4, label.3 as u32 * 4), Face::SupportTexture, Target::Cart { size, label }).unwrap();
    let (w, h) = (made.w, made.h);
    assert!(near(made.px[(h / 2 * w + 24) as usize], BLUE, 30), "its left half where the label's left is");
    assert!(near(made.px[(h / 2 * w + w - 24) as usize], YELLOW, 30), "its right half on the right");
    // Its own white edge is part of it: kept, at the label's rim.
    assert!(near(made.px[(h / 2 * w) as usize], 0xffffff, 12), "the label's own edge stays");
}

#[test]
fn a_label_shaped_unlike_its_place_is_printed_whole_never_stretched() {
    // A Genesis label (about 6:5, ScreenScraper's support-texture) on the Mega Drive's wide label recess (about 2:1).
    let g = Game::MegaDrive;
    let (size, label) = media::label_cell(g).unwrap();
    let (sw, sh) = (240, 200);
    // Black round its art, as the real one: a band of black at its rim, the art blue and yellow inside.
    let mut scan = label_scan(sw, sh);
    for y in 0..scan.h {
        for x in 0..scan.w {
            let rim = x < 5 + 12 || x >= 5 + sw - 12 || y < 5 + 12 || y >= 5 + sh - 12;
            let inside = x >= 5 && x < 5 + sw && y >= 5 && y < 5 + sh;
            if rim && inside {
                scan.px[(y * scan.w + x) as usize] = 0xff10_1010;
            }
        }
    }
    let made = prepare(&scan, Face::SupportTexture, Target::Cart { size, label }).unwrap();
    let (w, h) = (made.w as i32, made.h as i32);
    let is = |x: i32, y: i32, c: u32| near(made.px[(y * w + x) as usize], c, 30);
    // Its whole height is there: the art's top and bottom rows inside the black rim.
    let art_rows: Vec<i32> = (0..h).filter(|&y| (0..w).any(|x| is(x, y, BLUE) || is(x, y, YELLOW))).collect();
    assert!(art_rows.len() as i32 >= h * 3 / 4, "the label is whole, not cut to a band ({} of {h} rows)", art_rows.len());
    // Its width in proportion: the art's columns as many as its rows make them at 240:200.
    let art_cols = (0..w).filter(|&x| (0..h).any(|y| is(x, y, BLUE) || is(x, y, YELLOW))).count() as f32;
    let ratio = art_cols / art_rows.len() as f32;
    assert!((ratio - 216.0 / 176.0).abs() < 0.2, "not stretched: {ratio} wide a row");
    // The rest of the place is the label's own black, not grey and not white.
    assert!(is(0, h / 2, 0x101010) && is(w - 1, h / 2, 0x101010), "its edge colour fills the sides");
}

#[test]
fn a_real_label_keeps_its_own_full_colours() {
    // Owner 26.11.2026: a muted, posterised label looked paler than the cover's art it replaced.
    let g = Game::Snes;
    let (size, label) = media::label_cell(g).unwrap();
    let made = prepare(&label_scan(label.2 as u32 * 8, label.3 as u32 * 8), Face::SupportTexture, Target::Cart { size, label }).unwrap();
    let mid = (made.h / 2 * made.w) as usize;
    assert_eq!(made.px[mid + 20], BLUE, "the blue as it is");
    assert_eq!(made.px[mid + made.w as usize - 20], YELLOW, "the yellow as it is");
}

#[test]
fn every_drawn_cartridge_carries_the_label_only_on_its_label_in_its_own_colour() {
    let mut t = text();
    let green = 0x10c060;
    for system in ["snes", "n64", "nes", "gb", "gbc", "gba", "mastersystem", "gamegear", "pcengine", "neogeo", "atarilynx", "wonderswan", "ngpc"] {
        let g = Game::of(system, "Game (Europe).zip");
        let Some(Target::Cart { size, label: (lx, ly, lw, lh) }) = Target::of(system, "Game (Europe).zip") else { panic!("{system}: no label target") };
        let d = media::LABEL_DETAIL;
        let real = Image { w: (lw * d) as u32, h: (lh * d) as u32, px: vec![green; (lw * d * lh * d) as usize] };
        for h in [120, 300] {
            let plain = media::game_pic(&mut t, g, h, look(), "Sonic", None, None).unwrap();
            let with = media::game_pic_labelled(&mut t, g, h, look(), "Sonic", None, None, Some(&real)).unwrap();
            assert_eq!((plain.w, plain.h), (with.w, with.h));
            let f = h as f32 / size.1 as f32;
            let (mut on_label, mut real_seen) = (0, 0);
            for y in 0..with.h {
                for x in 0..with.w {
                    let (tx, ty) = ((x as f32 / f) as i32, (y as f32 / f) as i32);
                    let inside = tx >= lx - 1 && tx <= lx + lw && ty >= ly - 1 && ty <= ly + lh;
                    let (a, b) = (plain.px[(y * with.w + x) as usize], with.px[(y * with.w + x) as usize]);
                    if !inside {
                        assert_eq!(a, b, "{system} {h}: ({x},{y}) is off the label and keeps the cartridge's art");
                        continue;
                    }
                    on_label += 1;
                    if a != b {
                        assert_eq!(b & 0xffffff, green, "{system} {h}: ({x},{y}) is the label's own colour");
                        real_seen += 1;
                    }
                }
            }
            // The label's rounded corners (a GBA's, an N64's top) stay plastic; the rest carries the label.
            assert!(real_seen as f32 >= on_label as f32 * 0.5, "{system} {h}: {real_seen} of {on_label} label pixels show the real label");
        }
    }
}

#[test]
fn a_label_of_the_wrong_size_is_not_printed_the_cover_stays() {
    let mut t = text();
    let wrong = Image { w: 5, h: 5, px: vec![RED; 25] };
    let plain = media::game_pic(&mut t, Game::Snes, 200, look(), "x", None, None).unwrap();
    let with = media::game_pic_labelled(&mut t, Game::Snes, 200, look(), "x", None, None, Some(&wrong)).unwrap();
    assert_eq!(plain.px, with.px);
}

#[test]
fn a_real_disc_is_round_its_hole_and_hub_stay_and_its_face_is_printed_between() {
    let mut t = text();
    for kind in DiscKind::ALL {
        let Some(d) = media::disc_diameter(kind) else {
            assert_eq!(kind, DiscKind::Umd, "only the UMD has no round print");
            continue;
        };
        let dd = d * media::LABEL_DETAIL;
        let face = Image { w: dd as u32, h: dd as u32, px: vec![RED; (dd * dd) as usize] };
        let h = 240;
        let plain = media::disc_pic(&mut t, kind, h, look(), "Game", None);
        let with = media::disc_pic_labelled(&mut t, kind, h, look(), "Game", None, Some(&face));
        let (w, cx, cy) = (with.w, with.w / 2, with.h / 2);
        let at = |p: &crate::cabinet_art::Pic, x: i32, y: i32| p.px[(y * w + x) as usize];
        assert_eq!(at(&with, cx, cy) >> 24, 0, "{kind:?}: the hole stays");
        assert_eq!(at(&with, 0, 0) >> 24, 0, "{kind:?}: round, nothing at the corner");
        assert_eq!(at(&with, cx, cy) , at(&plain, cx, cy), "{kind:?}");
        // Halfway out across the print: the real face.
        let print_r = (1..h / 2).filter(|r| near(at(&with, cx + r, cy) & 0xffffff, RED, 24)).collect::<Vec<_>>();
        assert!(print_r.len() as i32 >= h / 12, "{kind:?}: the face is printed on the disc ({} px of its radius)", print_r.len());
        // The hub just outside the hole is the disc's own (no print over the clear plastic).
        let hub = print_r[0] - 2;
        assert_eq!(at(&with, cx + hub, cy), at(&plain, cx + hub, cy), "{kind:?}: the hub stays at {hub}");
        // The alpha (the disc's outline) is the plain disc's everywhere.
        assert!(with.px.iter().zip(&plain.px).all(|(a, b)| a >> 24 == b >> 24), "{kind:?}: the same round outline");
    }
}

#[test]
fn what_takes_a_label_and_what_does_not() {
    assert!(matches!(Target::of("snes", "Super Metroid (USA, Europe).zip"), Some(Target::Cart { .. })));
    // Owner 26.11.2026: their real labels look plainer than the cover's art on them.
    for (system, rom) in [("megadrive", "Sonic (USA, Europe).zip"), ("genesis", "Sonic (USA).md"), ("nds", "Mario Kart DS (Europe).nds"), ("3ds", "Game.3ds")] {
        assert_eq!(Target::of(system, rom), None, "{system}: keeps the cover's art on its label");
    }
    assert!(matches!(Target::of("psx", "Ridge Racer (Europe).chd"), Some(Target::Disc { .. })));
    assert_eq!(Target::of("psp", "Game.iso"), None, "a UMD's paper label keeps the cover");
    assert_eq!(Target::of("pico8", "celeste.p8.png"), None, "a memory card has no label to find");
    assert_eq!(Target::of("nes", "x.nes").map(|t| t.faces()), Some([Face::SupportTexture, Face::Support]));
}

#[test]
fn a_label_is_made_on_its_thread_and_a_game_without_one_is_left_alone() {
    let dir = TestDir::new("real-label");
    let rom = dir.join("roms/snes/Super Metroid (Europe).sfc");
    let target = Target::of("snes", "Super Metroid (Europe).sfc").unwrap();
    png_file(&crate::screenscraper::path(&dir, Face::Support, "snes", &rom), &cart_photo(Game::Snes, 2));
    let mut labels = Labels::new(&dir, None);
    // The frame asks and goes on: nothing read in it.
    assert!(labels.get("snes", &rom, target).is_none());
    let until = Instant::now() + Duration::from_secs(10);
    let made = loop {
        if let Some(l) = labels.get("snes", &rom, target) {
            break l;
        }
        assert!(Instant::now() < until, "the label never came");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(made.px.iter().filter(|&&p| near(p, RED, 40)).count() as f32 > made.px.len() as f32 * 0.9);
    // Another game without a file: nothing, and asked again only after a while.
    let other = dir.join("roms/snes/Columns (Europe).sfc");
    assert!(labels.get("snes", &other, target).is_none());
    std::thread::sleep(Duration::from_millis(100));
    assert!(labels.get("snes", &other, target).is_none());
    // Snapshots read at once.
    let mut sync = Labels::new(&dir, None);
    sync.sync = true;
    assert!(sync.get("snes", &rom, target).is_some());
}

#[test]
fn a_broken_or_foreign_file_gives_no_label() {
    let dir = TestDir::new("real-label-bad");
    let rom = dir.join("roms/snes/Game (USA).sfc");
    let file = crate::screenscraper::path(&dir, Face::SupportTexture, "snes", &rom);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"NOMEDIA").unwrap();
    let target = Target::of("snes", "Game (USA).sfc").unwrap();
    assert!(make(&file, Face::SupportTexture, target).is_none());
    let mut sync = Labels::new(&dir, None);
    sync.sync = true;
    assert!(sync.get("snes", &rom, target).is_none());
}

#[test]
fn a_huge_narrow_disc_picture_is_made_at_the_disc_s_size_without_squaring_it() {
    // Review 26.11. (P1): an 8192 x 512 picture passed `image::size_ok` and was squared at full size (half a gigabyte).
    let (w, h) = (8192u32, 512u32);
    let img = Rgba { w, h, px: (0..w * h).map(|i| 0xff00_0000 | if (i % w) < w / 2 { RED } else { BLUE }).collect() };
    let d = media::disc_diameter(DiscKind::Ps1).unwrap();
    let t0 = Instant::now();
    let face = prepare(&img, Face::Support, Target::Disc { diameter: d }).unwrap();
    assert_eq!((face.w as i32, face.h as i32), (d * media::LABEL_DETAIL, d * media::LABEL_DETAIL));
    // Its middle row is the picture (red, then blue); above and below the band the square is empty (the dark under it).
    let mid = (face.h / 2 * face.w) as usize;
    assert!(near(face.px[mid + 2], RED, 30) && near(face.px[mid + face.w as usize - 3], BLUE, 30));
    assert!(near(face.px[2 * face.w as usize + face.w as usize / 2], 0x101010, 12), "nothing where the picture is not");
    assert!(t0.elapsed() < Duration::from_secs(2), "sampled, not walked pixel by pixel: {:?}", t0.elapsed());
}

#[test]
fn at_most_keep_labels_stay_made_the_least_recently_asked_for_goes() {
    let mut made = HashMap::new();
    let target = Target::Disc { diameter: 4 };
    let t0 = Instant::now();
    for i in 0..KEEP + 10 {
        let at = t0 + Duration::from_millis(i as u64);
        keep(&mut made, ("snes".into(), PathBuf::from(format!("g{i}")), target), State::Ready(Arc::new(Image { w: 1, h: 1, px: vec![0] }), at));
    }
    assert_eq!(made.len(), KEEP);
    assert!(!made.contains_key(&("snes".to_string(), PathBuf::from("g0"), target)), "the oldest went");
    assert!(made.contains_key(&("snes".to_string(), PathBuf::from(format!("g{}", KEEP + 9)), target)), "the newest stays");
    // Missing and loading entries are not labels: they never push one out.
    keep(&mut made, ("snes".into(), PathBuf::from("none"), target), State::Missing(t0));
    assert_eq!(made.values().filter(|s| matches!(s, State::Ready(..))).count(), KEEP);
}

#[test]
fn after_a_run_the_label_scan_takes_the_place_of_the_cartridge_s_photo() {
    // Review 26.11. (P2): a label cut from support-2D stayed for good after the run brought the support-texture scan.
    let dir = TestDir::new("real-label-forget");
    let rom = dir.join("roms/snes/Super Metroid (Europe).sfc");
    let target = Target::of("snes", "Super Metroid (Europe).sfc").unwrap();
    png_file(&crate::screenscraper::path(&dir, Face::Support, "snes", &rom), &cart_photo(Game::Snes, 2));
    let mut sync = Labels::new(&dir, None);
    sync.sync = true;
    let red = sync.get("snes", &rom, target).unwrap();
    assert!(near(red.px[red.px.len() / 2], RED, 40));
    let Target::Cart { label: (_, _, lw, lh), .. } = target else { unreachable!() };
    png_file(&crate::screenscraper::path(&dir, Face::SupportTexture, "snes", &rom), &label_scan(lw as u32 * 4, lh as u32 * 4));
    assert!(Arc::ptr_eq(&red, &sync.get("snes", &rom, target).unwrap()), "kept until told");
    sync.forget();
    let scan = sync.get("snes", &rom, target).unwrap();
    assert!(near(scan.px[(scan.h / 2 * scan.w + 24) as usize], BLUE, 30), "the scan now");
}

use super::*;
use std::path::PathBuf;

const A: Button = Button::East;
const B: Button = Button::South;

fn pages(n: usize) -> Vec<Kept> {
    (0..n).map(|i| Kept::Jpeg(PathBuf::from(format!("/m/{:03}.jpg", i + 1)), 700, 1000)).collect()
}

/// Loading on a slow Wi-Fi: the amount so far shows and the bar's bright part moves as bytes come.
#[test]
fn loading_shows_how_much_came_and_moves_with_it() {
    assert_eq!((megabytes(1_450_000), megabytes(300_000), megabytes(0)), ("1.4 MB".to_string(), "0.3 MB".to_string(), "0.0 MB".to_string()));
    let (w, h) = (1024u32, 768u32);
    let paint = |loaded: u64| {
        let mut v = ManualView::loading("Game");
        v.loaded = loaded;
        let mut px = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut px, w, h);
        let mut text = Text::builtin();
        text.set_translate(false);
        v.paint(&mut c, &mut text, &mut HitMap::default());
        px
    };
    let (a, b) = (paint(100_000), paint(900_000));
    assert!(a != b, "the screen changes as bytes come");
}

#[test]
fn l_and_r_turn_the_page_and_stop_at_the_covers() {
    let mut v = ManualView::new("Game", pages(3));
    assert_eq!(v.press(Button::L1, A, B), Action::Nothing, "no page before the first");
    assert_eq!(v.press(Button::R1, A, B), Action::Moved);
    assert_eq!(v.press(Button::Right, A, B), Action::Moved, "Right turns too while the page is whole");
    assert_eq!(v.page(), 2);
    assert_eq!(v.press(Button::R1, A, B), Action::Nothing, "no page after the last");
    assert_eq!(v.press(Button::Left, A, B), Action::Moved);
    assert_eq!(v.page(), 1);
    assert_eq!(v.press(B, A, B), Action::Close);
}

#[test]
fn a_zooms_in_twice_and_back_to_the_whole_page_and_the_d_pad_moves_the_part() {
    let mut v = ManualView::new("Game", pages(2));
    assert_eq!(v.level(), 1);
    assert_eq!(v.press(Button::Down, A, B), Action::Nothing, "a whole page has nothing to move");
    v.press(A, A, B);
    assert_eq!(v.level(), 2);
    // Zoomed: Left/Right move the part, they don't turn the page.
    let page = v.page();
    let before = v.layout(700, 1000, 1240, 1000).0;
    assert_eq!(v.press(Button::Right, A, B), Action::Moved);
    assert_eq!(v.page(), page);
    assert!(v.layout(700, 1000, 1240, 1000).0 .0 > before.0, "moved right");
    assert_eq!(v.press(Button::Down, A, B), Action::Moved);
    v.press(A, A, B);
    assert_eq!(v.level(), 3);
    v.press(A, A, B);
    assert_eq!(v.level(), 1, "and whole again");
    // L1/R1 still turn the page while zoomed; the next page starts at its top.
    v.press(A, A, B);
    v.press(Button::Down, A, B);
    assert_eq!(v.press(Button::R1, A, B), Action::Moved);
    assert_eq!(v.layout(700, 1000, 1240, 1000).0 .1, 0, "a new page starts at its top");
}

/// 0.8 check video on the Brick (27.11.2026): B in the A zoom closed the whole manual, while on the game box's real back
/// B only leaves the zoom. B leaves the zoom first, at both levels, on the page it was on; B on the whole page closes.
#[test]
fn b_in_the_zoom_leaves_the_zoom_first_then_the_manual() {
    for zooms in 1..=2 {
        let mut v = ManualView::new("Game", pages(3));
        v.press(Button::R1, A, B);
        for _ in 0..zooms { v.press(A, A, B); }
        assert_eq!(v.level(), 1 + zooms as u32);
        assert_eq!(v.press(B, A, B), Action::Moved, "{zooms}x zoom: B leaves the zoom, the manual stays");
        assert_eq!((v.level(), v.page()), (1, 1), "the whole page, the same page");
        assert!(v.hints().contains(&("B", "Back")));
        assert_eq!(v.press(B, A, B), Action::Close, "B on the whole page closes the manual");
    }
}

#[test]
fn the_whole_page_fits_the_screen_and_zoom_shows_a_part_of_it_larger() {
    let v = ManualView::new("Game", pages(1));
    for (w, room) in [(640, 400), (1024, 680), (1240, 1000), (1920, 980)] {
        let (part, (x, y, dw, dh)) = v.layout(700, 1000, w, room);
        assert_eq!(part, (0, 0, 700, 1000), "{w}: the whole page");
        assert!(dw <= w && dh <= room && x >= 0 && y >= 0, "{w}: inside the screen");
        assert!(dh == room || dw == w, "{w}: as large as it goes");
        let ratio = dw as f32 / dh as f32;
        assert!((ratio - 0.7).abs() < 0.02, "{w}: not stretched ({ratio})");
    }
    let mut z = ManualView::new("Game", pages(1));
    z.press(A, A, B);
    let (part, (_, _, dw, dh)) = z.layout(700, 1000, 1240, 1000);
    assert!(part.3 <= 500 + 1 && part.2 <= 700, "2x shows at most half the page's height: {part:?}");
    assert!(dh <= 1000 && dw <= 1240);
    // A double page in landscape (Metroid Fusion's 1400 x 1050 spreads): the same rules.
    let (part, (_, _, dw, dh)) = v.layout(1400, 1050, 1024, 680);
    assert_eq!(part, (0, 0, 1400, 1050));
    assert!(dw <= 1024 && dh <= 680);
}

#[test]
fn it_decodes_this_page_first_then_the_next_and_keeps_only_three() {
    let mut v = ManualView::new("Game", pages(5));
    v.press(Button::R1, A, B);
    v.press(Button::R1, A, B);
    let want: Vec<usize> = v.wanted().into_iter().map(|(i, _)| i).collect();
    assert_eq!(want, [2, 3, 1]);
    let img = Arc::new(Image { w: 2, h: 2, px: vec![0; 4] });
    for i in 0..5 {
        v.decoded(i, img.clone());
    }
    assert_eq!(v.wanted(), Vec::new());
    assert_eq!(v.decoded.len(), KEEP, "no more than three pages in memory");
    v.press(Button::R1, A, B);
    v.press(Button::R1, A, B);
    assert_eq!(v.wanted().into_iter().map(|(i, _)| i).collect::<Vec<_>>(), [4], "page 5: only it is missing");
}

/// What the screen shows has something readable in every state: loading, none, a page not decoded yet, a page that is
/// not a picture: never an empty gap.
#[test]
fn every_state_shows_words_never_an_empty_screen() {
    let (w, h) = (1024u32, 768u32);
    let paint = |v: &ManualView| {
        let mut px = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut px, w, h);
        let mut text = Text::builtin();
        text.set_translate(false);
        v.paint(&mut c, &mut text, &mut HitMap::default());
        // Bright pixels above the hint bar: the words (the background is near black).
        let top = kit::hint_bar_top(h as i32) as usize;
        px[..top * w as usize].iter().filter(|&&p| (p >> 16 & 0xff) > 0x80).count()
    };
    // 26.11.2026: the title was painted in the paper's ink, dark on the dark page; it is bright now.
    let title_px = {
        let mut px = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut px, w, h);
        let mut text = Text::builtin();
        text.set_translate(false);
        ManualView::loading("WWWWWWWWWW").paint(&mut c, &mut text, &mut HitMap::default());
        let room = kit::hint_bar_top(h as i32) as usize;
        px[(room / 2 - 60) * w as usize..(room / 2) * w as usize].iter().filter(|&&p| (p & 0xff) > 0xc0).count()
    };
    assert!(title_px > 100, "the game's name is bright on the dark page ({title_px})");
    let mut failed = ManualView::loading("Game");
    // 26.11.2026: the page opened offline asked for page 1 of none and panicked (index out of bounds).
    assert!(failed.wanted().is_empty() && ManualView::new("Game", Vec::new()).wanted().is_empty(), "no pages: nothing to decode");
    assert!(paint(&failed) > 200, "loading");
    failed.failed("No manual for this game");
    assert!(paint(&failed) > 200, "none");
    assert!(paint(&ManualView::new("Game", pages(2))) > 100, "a page not decoded yet: its number");
    assert!(paint(&ManualView::new("Game", vec![Kept::Other])) > 200, "a page that is not a picture");
    assert!(paint(&ManualView::new("Game", Vec::new())) > 200, "no pages at all");
}

#[test]
fn a_decoded_page_is_painted_whole_and_large() {
    let (w, h) = (1024u32, 768u32);
    let mut v = ManualView::new("Game", pages(1));
    v.decoded(0, Arc::new(Image { w: 70, h: 100, px: vec![0xd02080; 7000] }));
    let mut px = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut px, w, h);
    let mut text = Text::builtin();
    v.paint(&mut c, &mut text, &mut HitMap::default());
    let magenta = px.iter().filter(|&&p| p == 0xd02080).count();
    let room = kit::hint_bar_top(h as i32) as usize;
    // Nothing over the page: inside the page's own rectangle every pixel is the page's (the number, the zoom level and
    // the hints stay outside it), whole and zoomed in.
    for level in 0..3 {
        let mut z = ManualView::new("Game", pages(3));
        z.decoded(0, Arc::new(Image { w: 70, h: 100, px: vec![0xd02080; 7000] }));
        for _ in 0..level {
            z.press(A, A, B);
        }
        let mut px = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut px, w, h);
        z.paint(&mut c, &mut Text::builtin(), &mut HitMap::default());
        let (_, (x, y, dw, dh)) = z.layout(70, 100, w as i32, room as i32);
        let covered = (y.max(0)..(y + dh).min(room as i32))
            .flat_map(|yy| (x.max(0)..(x + dw).min(w as i32)).map(move |xx| (xx, yy)))
            .filter(|&(xx, yy)| px[yy as usize * w as usize + xx as usize] != 0xd02080)
            .count();
        assert_eq!(covered, 0, "{}x: something painted over the page", z.level());
        // And the page number is on the screen, on the hint bar's line.
        let below = px[room * w as usize..].iter().filter(|&&p| (p >> 16 & 0xff) > 0x80 && p != 0xd02080).count();
        assert!(below > 100, "{}x: the page number and the hints under the page", z.level());
    }
    assert!(magenta > room * room * 7 / 10 * 8 / 10, "the page as high as the room above the hints ({magenta})");
}

#[test]
fn a_large_scan_is_decoded_smaller_a_small_one_as_it_is() {
    let t = crate::testutil::TestDir::new("manual-decode");
    for (side, want) in [(600u16, 600u32), (3000, 1500)] {
        let px: Vec<u8> = (0..side as usize * side as usize).flat_map(|_| [10, 200, 30]).collect();
        let mut jpg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpg, 70).encode(&px, side, side, jpeg_encoder::ColorType::Rgb).unwrap();
        let p = t.join(format!("{side}.jpg"));
        std::fs::write(&p, jpg).unwrap();
        let img = decode(&p).unwrap();
        assert_eq!((img.w, img.h), (want, want));
        assert!(img.w <= MAX_EDGE);
    }
}

/// A kept page's picture through every fault: missing, empty, cut, flipped bytes. Never a panic; the page then shows
/// its number (nothing decoded), never a broken picture.
#[test]
fn file_source_rule_manual_page_picture() {
    let t = crate::testutil::TestDir::new("manual-page-faults");
    let px: Vec<u8> = (0..64 * 64).flat_map(|_| [200u8, 20, 20]).collect();
    let mut jpg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpg, 80).encode(&px, 64, 64, jpeg_encoder::ColorType::Rgb).unwrap();
    let p = t.join("001.jpg");
    assert!(decode(&p).is_none(), "missing");
    std::fs::write(&p, b"").unwrap();
    assert!(decode(&p).is_none(), "empty");
    for i in (0..jpg.len()).step_by(31) {
        std::fs::write(&p, &jpg[..i]).unwrap();
        let _ = decode(&p);
        let mut flipped = jpg.clone();
        flipped[i] ^= 0x5a;
        std::fs::write(&p, &flipped).unwrap();
        let _ = decode(&p);
    }
    std::fs::write(&p, &jpg).unwrap();
    assert_eq!(decode(&p).map(|i| (i.w, i.h)), Some((64, 64)), "the whole file decodes");
}

/// Sol 26.11.2026 (P1): a page whose JPEG header claims a giant picture was decoded first and checked after; the
/// decoder sizes its buffers from the header. Now the header is checked before anything is decoded.
#[test]
fn a_page_claiming_a_giant_picture_is_not_decoded() {
    let t = crate::testutil::TestDir::new("manual-giant");
    let px: Vec<u8> = (0..16 * 16).flat_map(|_| [1u8, 2, 3]).collect();
    let mut jpg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpg, 80).encode(&px, 16, 16, jpeg_encoder::ColorType::Rgb).unwrap();
    // The frame header (SOF0: FF C0, length, precision, height, width): 60000 x 60000.
    let sof = jpg.windows(2).position(|w| w == [0xff, 0xc0]).unwrap();
    jpg[sof + 5..sof + 9].copy_from_slice(&[0xea, 0x60, 0xea, 0x60]);
    let p = t.join("001.jpg");
    std::fs::write(&p, &jpg).unwrap();
    let started = std::time::Instant::now();
    assert!(decode(&p).is_none());
    assert!(started.elapsed() < std::time::Duration::from_secs(1), "refused from its header");
}

/// Sol 26.11.2026 (P1/P2): a page that did not decode was asked for again at once, for ever. Now it is marked: not
/// asked for again, and it says it can't be shown.
#[test]
fn a_page_that_does_not_decode_is_not_asked_for_again() {
    let mut v = ManualView::new("Game", pages(3));
    assert_eq!(v.wanted().first().map(|(i, _)| *i), Some(0));
    v.broken_page(0);
    assert!(v.wanted().iter().all(|(i, _)| *i != 0), "not asked for again");
    let (w, h) = (1024u32, 768u32);
    let mut px = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut px, w, h);
    let mut text = Text::builtin();
    text.set_translate(false);
    v.paint(&mut c, &mut text, &mut HitMap::default());
    let room = kit::hint_bar_top(h as i32) as usize;
    assert!(px[..room * w as usize].iter().filter(|&&p| (p >> 16 & 0xff) > 0x80).count() > 200, "it says so");
}

/// Android, 27.11.2026: the manual turned only with L1/R1. A swipe or a tap on a half of the screen turns it too, one page per
/// swipe however long; at the covers nothing happens, and the pad's buttons are as they were.
#[test]
fn a_swipe_or_a_tap_on_either_half_turns_the_page() {
    use crate::touch::{Phase, Recognizer};
    let w = 1240;
    let mut v = ManualView::new("Game", pages(4));
    // A real swipe through the recognizer: right to left across most of the screen.
    let swipe = |v: &mut ManualView, from: f32, to: f32| {
        let mut rec = Recognizer::default();
        let t = std::time::Instant::now();
        v.finger_down();
        rec.feed(Phase::Down, from, 500.0, t, ManualView::swipe_pitch(w as u32));
        let mut moved = 0;
        for n in 1..=20 {
            let x = from + (to - from) * n as f32 / 20.0;
            if let Some(g) = rec.feed(Phase::Move, x, 500.0, t, ManualView::swipe_pitch(w as u32)) {
                if v.touch(g, w) == Action::Moved { moved += 1; }
            }
        }
        rec.feed(Phase::Up, to, 500.0, t, ManualView::swipe_pitch(w as u32));
        moved
    };
    assert_eq!(swipe(&mut v, 1100.0, 100.0), 1, "one page for one long swipe");
    assert_eq!(v.page(), 1);
    assert_eq!(swipe(&mut v, 100.0, 1100.0), 1);
    assert_eq!(v.page(), 0, "to the right: the page before");
    assert_eq!(swipe(&mut v, 600.0, 640.0), 0, "a wobble turns nothing");
    assert_eq!(v.touch(Gesture::Tap { x: 1000.0, y: 400.0 }, w), Action::Moved);
    assert_eq!(v.touch(Gesture::Tap { x: 1000.0, y: 400.0 }, w), Action::Moved);
    assert_eq!(v.page(), 2, "taps on the right half turn on");
    assert_eq!(v.touch(Gesture::Tap { x: 100.0, y: 400.0 }, w), Action::Moved);
    assert_eq!(v.page(), 1, "on the left half back");
    v.finger_down();
    assert_eq!(v.touch(Gesture::Scroll { steps: 2, vertical: true }, w), Action::Nothing, "up and down turn nothing");
    let mut first = ManualView::new("Game", pages(2));
    assert_eq!(first.touch(Gesture::Tap { x: 100.0, y: 400.0 }, w), Action::Nothing, "no page before the first");
    let mut loading = ManualView::loading("Game");
    assert_eq!(loading.touch(Gesture::Tap { x: 1000.0, y: 400.0 }, w), Action::Nothing, "nothing to turn while it loads");
    // The pad as before.
    assert_eq!(v.press(Button::R1, A, B), Action::Moved);
    assert_eq!(v.page(), 2);
}

//! Small pixel-art pictures of the interface (assets/ui-art, sources and licences in assets/ui-art/SOURCES.md):
//! the vignettes above an empty page's words, the handhelds on About, first start and the manual's Controls page,
//! the Settings categories' icons and the Year in review's stickers. Compiled into the binary like the system
//! pictures (`system_art`) and decoded the first time one is painted, then kept: a page that never shows one
//! never decodes it, and a frame that does only copies pixels.

use std::sync::OnceLock;

use crate::device::Model;
use crate::gfx::Canvas;
use crate::system_art::{decode, Sprite};

/// Every picture, by what it is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// An empty wooden shelf with a plant: no games on the card.
    NoGames,
    /// A star resting on a cushion: no favourites yet.
    NoFavorites,
    /// An open blank notebook and a pencil: no play history yet.
    History,
    /// A small radio asleep under a moon: no network.
    Offline,
    /// A bell and an hourglass on a reception desk: a hotel's network asks to wait or sign in.
    Hotel,
    /// An instant camera and a blank photo: no screenshots.
    Screenshots,
    /// A magnifying glass beside an empty box: nothing matches.
    Search,
    /// A market stall with empty shelves: nothing in the store.
    Store,
    /// A handheld asleep under a blanket: nothing played yet.
    NothingPlayed,
    Device(Model),
    Icon(Icon),
    Sticker(Sticker),
}

/// The Settings categories (`SETTINGS_GROUPS` in main.rs, in that order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Look,
    Playing,
    Games,
    Connections,
    System,
}

/// What a Year (or Month) in review card can be marked with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sticker {
    /// A game marked completed.
    Completed,
    /// 100 hours played in the period.
    HundredHours,
    /// Ten games or more on one console.
    TenGames,
    /// A week or more of days in a row.
    Streak,
    /// A game played again after a long time.
    CameBack,
}

const FILES: &[(Art, &[u8])] = &[
    (Art::NoGames, include_bytes!("../assets/ui-art/empty-no-games.png")),
    (Art::NoFavorites, include_bytes!("../assets/ui-art/empty-no-favorites.png")),
    (Art::History, include_bytes!("../assets/ui-art/empty-history.png")),
    (Art::Offline, include_bytes!("../assets/ui-art/empty-offline.png")),
    (Art::Hotel, include_bytes!("../assets/ui-art/empty-hotel.png")),
    (Art::Screenshots, include_bytes!("../assets/ui-art/empty-screenshots.png")),
    (Art::Search, include_bytes!("../assets/ui-art/empty-search.png")),
    (Art::Store, include_bytes!("../assets/ui-art/empty-store.png")),
    (Art::NothingPlayed, include_bytes!("../assets/ui-art/empty-nothing-played.png")),
    (Art::Device(Model::MiniV2), include_bytes!("../assets/ui-art/device-miniv2.png")),
    (Art::Device(Model::MiniV1), include_bytes!("../assets/ui-art/device-miniv1.png")),
    (Art::Device(Model::Pocket5), include_bytes!("../assets/ui-art/device-rp5.png")),
    (Art::Device(Model::Flip2), include_bytes!("../assets/ui-art/device-flip2.png")),
    (Art::Device(Model::Pocket6), include_bytes!("../assets/ui-art/device-rp6.png")),
    (Art::Device(Model::Nova), include_bytes!("../assets/ui-art/device-nova.png")),
    (Art::Icon(Icon::Look), include_bytes!("../assets/ui-art/icon-look.png")),
    (Art::Icon(Icon::Playing), include_bytes!("../assets/ui-art/icon-playing.png")),
    (Art::Icon(Icon::Games), include_bytes!("../assets/ui-art/icon-games.png")),
    (Art::Icon(Icon::Connections), include_bytes!("../assets/ui-art/icon-connections.png")),
    (Art::Icon(Icon::System), include_bytes!("../assets/ui-art/icon-system.png")),
    (Art::Sticker(Sticker::Completed), include_bytes!("../assets/ui-art/sticker-completed.png")),
    (Art::Sticker(Sticker::HundredHours), include_bytes!("../assets/ui-art/sticker-hundred-hours.png")),
    (Art::Sticker(Sticker::TenGames), include_bytes!("../assets/ui-art/sticker-ten-games.png")),
    (Art::Sticker(Sticker::Streak), include_bytes!("../assets/ui-art/sticker-streak.png")),
    (Art::Sticker(Sticker::CameBack), include_bytes!("../assets/ui-art/sticker-came-back.png")),
];

/// The decoded picture, decoded on first use and kept.
pub fn get(art: Art) -> Option<&'static Sprite> {
    static DECODED: [OnceLock<Option<Sprite>>; FILES.len()] = [const { OnceLock::new() }; FILES.len()];
    let i = FILES.iter().position(|(a, _)| *a == art)?;
    DECODED[i].get_or_init(|| decode(FILES[i].1)).as_ref()
}

/// The Mini V2's controls in the picture's pixels (`device-miniv2.png`; assets/ui-art/anchors.json, written by
/// tools/draw_devices.py with the picture). Face buttons are by where they sit: north, east, south, west.
pub mod anchors {
    pub const L: (i32, i32) = (38, 3);
    pub const R: (i32, i32) = (206, 3);
    pub const SELECT: (i32, i32) = (23, 14);
    pub const START: (i32, i32) = (221, 14);
    pub const DPAD: (i32, i32) = (35, 34);
    pub const NORTH: (i32, i32) = (209, 24);
    pub const EAST: (i32, i32) = (219, 34);
    pub const SOUTH: (i32, i32) = (209, 44);
    pub const WEST: (i32, i32) = (199, 34);
    pub const LSTICK: (i32, i32) = (38, 73);
    pub const RSTICK: (i32, i32) = (206, 73);
    pub const HOME: (i32, i32) = (23, 103);
    pub const BACK: (i32, i32) = (221, 103);
}

/// Width and height of `art` painted `k` times as large; (0, 0) when it is missing.
pub fn size(art: Art, k: i32) -> (i32, i32) {
    get(art).map_or((0, 0), |s| (s.w as i32 * k, s.h as i32 * k))
}

/// `art` at (x, y), each of its pixels a `k` x `k` square (whole numbers keep pixel art crisp).
pub fn paint(c: &mut Canvas, art: Art, x: i32, y: i32, k: i32) {
    if let Some(s) = get(art) {
        blit(c, s, x, y, k, None);
    }
}

/// An icon (white on clear) at (x, y), `k` times as large, in `color`.
pub fn paint_tinted(c: &mut Canvas, art: Art, x: i32, y: i32, k: i32, color: u32) {
    if let Some(s) = get(art) {
        blit(c, s, x, y, k, Some(color));
    }
}

fn blit(c: &mut Canvas, s: &Sprite, x: i32, y: i32, k: i32, tint: Option<u32>) {
    let k = k.max(1);
    for sy in 0..s.h as i32 {
        let y0 = y + sy * k;
        if y0 + k <= 0 || y0 >= c.h {
            continue;
        }
        let mut sx = 0;
        while sx < s.w as i32 {
            let p = s.px[(sy * s.w as i32 + sx) as usize];
            if p >> 24 == 0 {
                sx += 1;
                continue;
            }
            // A run of the same colour is one rectangle.
            let mut end = sx + 1;
            while end < s.w as i32 && s.px[(sy * s.w as i32 + end) as usize] == p {
                end += 1;
            }
            c.rect(x + sx * k, y0, (end - sx) * k, k, tint.unwrap_or(p & 0x00ff_ffff));
            sx = end;
        }
    }
}

/// How large an empty page's picture is painted: twice its pixels, once on a screen too low for that.
pub fn empty_scale(screen_h: i32) -> i32 {
    if screen_h >= 900 {
        2
    } else {
        1
    }
}

/// An empty page's picture above its words. The words' heading has its baseline at `title_base` (type `title_px`);
/// the picture is painted at `x` (its left edge, or its middle when `centered`), its bottom 28 px above the heading's
/// capitals, and no higher than `top`. Returns how far the words must move down to make room (0 when they already
/// sit low enough), so the caller adds it to their y.
pub fn above_words(c: &mut Canvas, art: Art, x: i32, centered: bool, top: i32, title_base: i32, title_px: u32) -> i32 {
    let k = empty_scale(c.h);
    let (w, h) = size(art, k);
    if h == 0 {
        return 0;
    }
    let caps = title_base - crate::type_scale::px(title_px) as i32 * 3 / 4;
    let y = (caps - 28 - h).max(top);
    let x = if centered { x - w / 2 } else { x };
    paint(c, art, x, y, k);
    (y + h + 28 - caps).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_decodes_with_on_off_transparency() {
        for (art, _) in FILES {
            let s = get(*art).unwrap_or_else(|| panic!("{art:?} does not decode"));
            assert!(s.w > 0 && s.h > 0, "{art:?}");
            assert!(s.px.iter().all(|p| p >> 24 == 0 || p >> 24 == 0xff), "{art:?}: only clear or solid pixels");
            assert!(s.px.iter().any(|p| p >> 24 == 0) && s.px.iter().any(|p| p >> 24 == 0xff), "{art:?}: a picture on clear");
        }
    }

    #[test]
    fn every_device_has_a_picture_and_icons_are_one_colour() {
        for m in [Model::MiniV2, Model::MiniV1, Model::Pocket5, Model::Flip2, Model::Pocket6, Model::Nova] {
            assert!(get(Art::Device(m)).is_some(), "{m:?}");
        }
        for i in [Icon::Look, Icon::Playing, Icon::Games, Icon::Connections, Icon::System] {
            let s = get(Art::Icon(i)).unwrap();
            assert_eq!((s.w, s.h), (16, 16), "{i:?}");
            assert!(s.px.iter().all(|p| *p == 0 || *p == 0xffff_ffff), "{i:?}: white on clear, so it takes the row's colour");
        }
    }

    #[test]
    fn the_anchors_sit_on_the_controls_of_the_picture() {
        let s = get(Art::Device(Model::MiniV2)).unwrap();
        let solid = |(x, y): (i32, i32)| s.px[(y * s.w as i32 + x) as usize] >> 24 != 0;
        use anchors::*;
        for a in [L, R, SELECT, START, DPAD, NORTH, EAST, SOUTH, WEST, LSTICK, RSTICK, HOME, BACK] {
            assert!(solid(a), "{a:?} is on the picture");
        }
        let json = include_str!("../assets/ui-art/anchors.json");
        for (key, a) in [("\"select\"", SELECT), ("\"home\"", HOME), ("\"X\"", NORTH), ("\"B\"", SOUTH), ("\"rstick\"", RSTICK)] {
            let at = json.find(key).unwrap();
            let nums: String = json[at..].chars().skip_while(|c| *c != '[').take_while(|c| *c != ']').filter(|c| c.is_ascii_digit() || *c == ',').collect();
            assert_eq!(nums, format!("{},{}", a.0, a.1), "{key} matches anchors.json");
        }
    }

    #[test]
    fn the_words_move_down_only_as_far_as_the_picture_needs() {
        let mut buf = vec![0u32; 1240 * 1080];
        let mut c = Canvas::new(&mut buf, 1240, 1080);
        let (_, h) = size(Art::NoGames, 2);
        // Heading at 300: the picture can't fit above it under the top line, so the words move.
        let dy = above_words(&mut c, Art::NoGames, 72, false, 120, 300, 50);
        assert_eq!(dy, 120 + h + 28 - (300 - crate::type_scale::px(50) as i32 * 3 / 4));
        assert!(c.buf.iter().any(|p| *p != 0), "painted");
        // Heading low enough: nothing moves.
        assert_eq!(above_words(&mut c, Art::NoGames, 620, true, 120, 700, 50), 0);
    }

    #[test]
    fn painting_a_vignette_is_cheap() {
        let mut buf = vec![0u32; 1240 * 1080];
        let mut c = Canvas::new(&mut buf, 1240, 1080);
        paint(&mut c, Art::NoGames, 0, 0, 2);
        let t = std::time::Instant::now();
        for _ in 0..100 {
            paint(&mut c, Art::NoGames, 72, 120, 2);
        }
        // About 20 µs a time on the build machine; a frame has 16 ms.
        assert!(t.elapsed().as_micros() / 100 < 1500, "{:?}", t.elapsed() / 100);
    }
}

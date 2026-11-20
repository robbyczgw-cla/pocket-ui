//! The window on the room's wall (Settings → Look → Cozy extras → Window): the sky as it is outside now (morning, day,
//! evening, night by the device's clock) and something for the season on its sill (tulips, a sunflower, a pumpkin, a
//! snow globe). It hangs only on free wall: in the Shelf look beside the row where the row leaves the shelf free, in the
//! Cabinet look on the wall over the top board, left of the TV, where the cases out of the shelf pass in front of it.
//!
//! Pure apart from the pictures it decodes once: [`now`] reads the clock, [`Window::at`] makes the picture with the
//! piece on its sill and says where the sill and the piece are (for `scene_check`), [`shelf_spot`] and
//! [`cabinet_spot`] find where it hangs. The pictures are PixelLab pixel art (`assets/media/props/window_*.png`,
//! `sill_*.png`, prompts in `pixellab-assets.json`): one frame, its four skies painted behind the glass.

use crate::cabinet_art::Pic;

/// The sky behind the glass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sky {
    Morning,
    Day,
    Evening,
    Night,
}

/// What stands on the sill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    /// March to May: tulips.
    Spring,
    /// June to August: a sunflower.
    Summer,
    /// September to November: a pumpkin.
    Autumn,
    /// December to February: a snow globe.
    Winter,
}

/// The window's size against the props on the shelves (a little larger: it is on the wall behind them).
pub const SCALE: f32 = 1.2;
/// The piece on the sill against the window.
const PIECE: f32 = 0.75;

/// The sky at `minute` of the day: morning from 05:30, day from 10:00, evening from 17:30, night from 21:30.
pub fn sky_at(minute: f32) -> Sky {
    let m = minute.rem_euclid(1440.0);
    if m < 330.0 || m >= 1290.0 {
        Sky::Night
    } else if m < 600.0 {
        Sky::Morning
    } else if m < 1050.0 {
        Sky::Day
    } else {
        Sky::Evening
    }
}

/// The season of `month` (1 to 12), as the northern half of the world has it.
pub fn season_of(month: u32) -> Season {
    match month {
        3..=5 => Season::Spring,
        6..=8 => Season::Summer,
        9..=11 => Season::Autumn,
        _ => Season::Winter,
    }
}

/// The month (1 to 12) of the day `unix` seconds after 1970 (already in local time).
pub fn month_of(unix: i64) -> u32 {
    // Howard Hinnant's days_from_civil, backwards.
    let z = unix.div_euclid(86_400) + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    (if mp < 10 { mp + 3 } else { mp - 9 }) as u32
}

/// The sky and the season now on this device's clock; `POCKETUI_ROOM_TIME=HH:MM` and `POCKETUI_ROOM_MONTH=1..12` set
/// them outright (snapshots, trying it without waiting).
#[cfg(any(target_os = "linux", target_os = "emscripten", target_os = "android"))]
pub fn now() -> (Sky, Season) {
    let sky = sky_at(crate::room_light::minute_now());
    let month = std::env::var("POCKETUI_ROOM_MONTH").ok().and_then(|m| m.trim().parse::<u32>().ok()).filter(|m| (1..=12).contains(m));
    (sky, season_of(month.unwrap_or_else(local_month)))
}

#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
fn local_month() -> u32 {
    let unix = crate::appearance::shifted_unix_now();
    let off = {
        let _zone = crate::timezone::zone_lock();
        crate::appearance::utc_offset_now(unix)
    };
    month_of(unix + off as i64)
}

#[cfg(all(target_os = "android", not(feature = "handheld")))]
fn local_month() -> u32 {
    let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    month_of(unix)
}

/// For the caches of shelf pictures: changes when the sky or the season does, or the window is switched.
pub fn key(on: bool, (sky, season): (Sky, Season)) -> u64 {
    if !on {
        return 0;
    }
    0x5717_0000 | (sky as u64) << 4 | season as u64
}

fn decoded(png: &'static [u8]) -> Pic {
    crate::system_art::decode(png).map_or_else(|| Pic::new(1, 1, 0), |s| Pic::of(s.w as i32, s.h as i32, s.px))
}

/// The frame with `sky` behind its glass, at one pixel a pixel, decoded once.
fn frame(sky: Sky) -> &'static Pic {
    static PICS: [std::sync::OnceLock<Pic>; 4] = [const { std::sync::OnceLock::new() }; 4];
    PICS[sky as usize].get_or_init(|| {
        decoded(match sky {
            Sky::Morning => include_bytes!("../assets/media/props/window_morning.png"),
            Sky::Day => include_bytes!("../assets/media/props/window_day.png"),
            Sky::Evening => include_bytes!("../assets/media/props/window_evening.png"),
            Sky::Night => include_bytes!("../assets/media/props/window_night.png"),
        })
    })
}

/// The piece for `season`, at one pixel a pixel, decoded once.
fn piece(season: Season) -> &'static Pic {
    static PICS: [std::sync::OnceLock<Pic>; 4] = [const { std::sync::OnceLock::new() }; 4];
    PICS[season as usize].get_or_init(|| {
        decoded(match season {
            Season::Spring => include_bytes!("../assets/media/props/sill_spring.png"),
            Season::Summer => include_bytes!("../assets/media/props/sill_summer.png"),
            Season::Autumn => include_bytes!("../assets/media/props/sill_autumn.png"),
            Season::Winter => include_bytes!("../assets/media/props/sill_winter.png"),
        })
    })
}

/// The sill's top row in a frame: the lowest row where the picture widens to its full width (the sill reaches out
/// past the frame on both sides).
fn sill_row(p: &Pic) -> i32 {
    let width = |y: i32| (0..p.w).filter(|&x| p.px[(y * p.w + x) as usize] >> 24 > 0x80).count() as i32;
    let full = (0..p.h).map(width).max().unwrap_or(0);
    (1..p.h).rev().find(|&y| width(y) >= full - 1 && width(y - 1) < full - 2).unwrap_or(p.h - 1)
}

/// The window as painted: the picture (frame, sky and the piece on the sill), the sill's top and the piece's box in
/// it (x, y, w, h), and the frame's box (the part that hangs on the wall).
#[derive(Clone)]
pub struct Window {
    pub pic: Pic,
    pub sill_y: i32,
    pub sill: (i32, i32),
    pub piece: (i32, i32, i32, i32),
    pub frame: (i32, i32, i32, i32),
}

impl Window {
    /// The window at scale `k` (screen pixels a template pixel), the piece standing on its sill near the right end.
    pub fn at(sky: Sky, season: Season, k: f32) -> Window {
        let f = crate::media::rescaled(frame(sky), k);
        let p = crate::media::rescaled(piece(season), k * PIECE);
        let sill_y = (sill_row(frame(sky)) as f32 * k).round() as i32;
        // The piece's top may stand above the frame's top only if it were taller than the window: it never is.
        let lift = (p.h - sill_y).max(0);
        let mut pic = Pic::new(f.w, f.h + lift, 0);
        blit(&mut pic, &f, 0, lift);
        let px = f.w - p.w - (f.w / 12).max(1);
        let py = lift + sill_y - p.h;
        blit(&mut pic, &p, px, py);
        // The sill reaches over the frame's whole width.
        Window { sill_y: lift + sill_y, sill: (0, f.w), piece: (px, py, p.w, p.h), frame: (0, lift, f.w, f.h), pic }
    }
}

fn blit(dst: &mut Pic, src: &Pic, x0: i32, y0: i32) {
    for y in 0..src.h {
        for x in 0..src.w {
            let c = src.px[(y * src.w + x) as usize];
            let (dx, dy) = (x0 + x, y0 + y);
            if c >> 24 > 0x80 && dx >= 0 && dy >= 0 && dx < dst.w && dy < dst.h {
                dst.px[(dy * dst.w + dx) as usize] = c | 0xff00_0000;
            }
        }
    }
}

/// Something standing on the shelf's board under where the window could hang: its left and right edge and its top.
pub type Below = (i32, i32, i32);

/// Where a window `w` x `h` hangs in the Shelf look's free part of the shelf, from `from` to `to` along it, in a tile
/// `room` high down to the board, with the things standing there (`below`), keeping `gap` from everything: its top left,
/// or None where it doesn't fit whole. It hangs in the upper part of the wall, centred in the free part; over something
/// standing on the board only with `gap` above it, else beside it.
pub fn shelf_spot(from: i32, to: i32, room: i32, below: &[Below], (w, h): (i32, i32), gap: i32) -> Option<(i32, i32)> {
    if to - from < w + 2 * gap || room < h + 2 * gap {
        return None;
    }
    // Hung like a picture: a third of the free height above it, two thirds under it.
    let pref_y = gap.max((room - h) / 3);
    let fits = |x: i32, y: i32| {
        y >= gap / 2 && y + h <= room - gap && below.iter().all(|&(b0, b1, top)| x + w + gap <= b0 || x >= b1 + gap || y + h + gap <= top)
    };
    let centre = from + (to - from - w) / 2;
    // Tried in order: centred at its place, then along the free part from the middle out, then higher.
    let mut xs: Vec<i32> = vec![centre];
    let mut d = 2;
    while centre - d >= from + gap || centre + d + w <= to - gap {
        if centre + d + w <= to - gap {
            xs.push(centre + d);
        }
        if centre - d >= from + gap {
            xs.push(centre - d);
        }
        d += 2;
    }
    for y in [pref_y, gap] {
        if let Some(&x) = xs.iter().find(|&&x| fits(x, y)) {
            return Some((x, y));
        }
    }
    None
}

/// Where a window `w` x `h` hangs on the Cabinet's wall over the top board: right of `left`, at `gap` left of `right` (the
/// TV's or its console's left edge), in the showcase from `top` down to the board at `board`, centred in its height:
/// its top left, or None where it doesn't fit whole. The same on every shelf: it depends on the cabinet's frame only.
pub fn cabinet_spot(left: i32, right: i32, top: i32, board: i32, (w, h): (i32, i32), gap: i32) -> Option<(i32, i32)> {
    let x = right - gap - w;
    if x < left + gap || board - top < h + 2 * gap {
        return None;
    }
    Some((x, top + (board - top - h) / 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sky_follows_the_clock() {
        let at = |h: u32, m: u32| sky_at((h * 60 + m) as f32);
        assert_eq!(at(0, 0), Sky::Night);
        assert_eq!(at(5, 29), Sky::Night);
        assert_eq!(at(5, 30), Sky::Morning);
        assert_eq!(at(9, 59), Sky::Morning);
        assert_eq!(at(10, 0), Sky::Day);
        assert_eq!(at(17, 29), Sky::Day);
        assert_eq!(at(17, 30), Sky::Evening);
        assert_eq!(at(21, 29), Sky::Evening);
        assert_eq!(at(21, 30), Sky::Night);
        assert_eq!(sky_at(1440.0 + 720.0), Sky::Day);
    }

    #[test]
    fn the_sill_follows_the_season() {
        let seasons: Vec<Season> = (1..=12).map(season_of).collect();
        use Season::*;
        assert_eq!(seasons, [Winter, Winter, Spring, Spring, Spring, Summer, Summer, Summer, Autumn, Autumn, Autumn, Winter]);
    }

    #[test]
    fn months_from_days() {
        assert_eq!(month_of(0), 1, "1970-01-01");
        assert_eq!(month_of(1_794_139_200), 11, "2026-11-08");
        assert_eq!(month_of(1_709_164_800), 2, "2024-02-29");
        assert_eq!(month_of(1_709_251_200), 3, "2024-03-01");
        assert_eq!(month_of(1_798_675_199), 12, "2026-12-31 23:59:59");
        assert_eq!(month_of(-1), 12, "1969-12-31");
    }

    #[test]
    fn keys_change_with_sky_season_and_switch() {
        let all: std::collections::HashSet<u64> = [Sky::Morning, Sky::Day, Sky::Evening, Sky::Night]
            .into_iter()
            .flat_map(|s| [Season::Spring, Season::Summer, Season::Autumn, Season::Winter].into_iter().map(move |z| key(true, (s, z))))
            .chain([key(false, (Sky::Day, Season::Spring))])
            .collect();
        assert_eq!(all.len(), 17);
    }

    #[test]
    fn the_piece_stands_on_the_sill_inside_the_picture() {
        for sky in [Sky::Morning, Sky::Day, Sky::Evening, Sky::Night] {
            for season in [Season::Spring, Season::Summer, Season::Autumn, Season::Winter] {
                for k in [1.0, 1.25, 1.7, 2.04, 2.5, 3.0] {
                    let w = Window::at(sky, season, k);
                    let (px, py, pw, ph) = w.piece;
                    assert_eq!(py + ph, w.sill_y, "{sky:?} {season:?} {k}: the piece's foot on the sill");
                    assert!(px >= w.sill.0 && px + pw <= w.sill.1, "{sky:?} {season:?} {k}: on the sill's length");
                    assert!(px >= 0 && py >= 0 && px + pw <= w.pic.w && py + ph <= w.pic.h);
                    // The sill is low in the frame: under the glass.
                    let (_, fy, _, fh) = w.frame;
                    assert!(w.sill_y > fy + fh * 3 / 4 && w.sill_y < fy + fh, "{k}: sill at {} in {fy}..{}", w.sill_y, fy + fh);
                    // The piece is smaller than the window.
                    assert!(ph < fh && pw < w.pic.w / 2);
                }
            }
        }
    }

    #[test]
    fn on_the_shelf_it_hangs_only_where_it_fits_whole_and_clear_of_what_stands_there() {
        let (w, h, gap) = (60, 60, 8);
        // Plenty of free shelf: centred, in the upper part.
        let (x, y) = shelf_spot(100, 500, 200, &[], (w, h), gap).unwrap();
        assert_eq!(x, 100 + (400 - w) / 2);
        assert!(y >= gap && y + h <= 200 - gap);
        // Too narrow or too low: none.
        assert_eq!(shelf_spot(100, 100 + w + gap, 200, &[], (w, h), gap), None);
        assert_eq!(shelf_spot(100, 500, h + gap, &[], (w, h), gap), None);
        // A lamp under the middle, too tall to hang over: beside it.
        let lamp = (270, 330, 80);
        let (x, y) = shelf_spot(100, 500, 200, &[lamp], (w, h), gap).unwrap();
        assert!(x + w + gap <= lamp.0 || x >= lamp.1 + gap || y + h + gap <= lamp.2, "{x},{y}");
        // Something short under the middle: over it.
        let low = (270, 330, 170);
        let (x, y) = shelf_spot(100, 500, 200, &[low], (w, h), gap).unwrap();
        assert_eq!(x, 100 + (400 - w) / 2);
        assert!(y + h + gap <= 170);
        // Every answer, for many shelves: inside the free part, clear of everything.
        for from in (0..300).step_by(37) {
            for room in [80, 120, 160, 240] {
                for b in [(from + 20, from + 60, room - 50), (from + 150, from + 170, room / 3)] {
                    if let Some((x, y)) = shelf_spot(from, from + 260, room, &[b], (w, h), gap) {
                        assert!(x >= from + gap && x + w <= from + 260 - gap);
                        assert!(y >= gap / 2 && y + h <= room - gap);
                        assert!(x + w + gap <= b.0 || x >= b.1 + gap || y + h + gap <= b.2);
                    }
                }
            }
        }
    }

    #[test]
    fn in_the_cabinet_it_hangs_left_of_the_tv_in_the_showcase() {
        let (x, y) = cabinet_spot(30, 900, 180, 360, (80, 80), 12).unwrap();
        assert_eq!(x + 80 + 12, 900);
        assert_eq!(y, 180 + (180 - 80) / 2);
        assert_eq!(cabinet_spot(30, 120, 180, 360, (80, 80), 12), None, "no room left of the TV");
        assert_eq!(cabinet_spot(30, 900, 180, 270, (80, 80), 12), None, "the showcase too low");
    }
}

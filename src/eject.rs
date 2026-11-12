//! Cozy's way back from a game: after Quit the last picture of the game switches off like
//! an old TV, it pulls together to a bright line and to a dot (`crt_on` backwards); a handheld's display (Game Boy,
//! GBA, DS, PSP: no TV) simply goes dark. Then the shelf fades in with the console still up and the launch animation
//! plays backwards (`insert_scene`): the cartridge, card or disc comes out of the console and goes back into its case,
//! which shuts and stands in its place, a little open while Continue waits (`paint_ajar_game`).
//!
//! Only with Animations on Cozy and Motion on Full; the old-TV part only with the old-TV switch-on (`Extra::CrtOn`).
//! Nothing waits for it: the shelf is live from its first frame, the effect is painted over it, and any button ends it
//! at once and does what it does on the shelf. Quit itself is not touched: pocket-core ends as before.
//!
//! [`moment`] says what shows `since` the game ended; [`paint_tv_off`] and [`paint_lcd_off`] draw the first part over
//! the whole screen from the picture the pause menu grabbed (`QuickMenu::frame`, taken over, not copied).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cabinet::{self, FormFactor};
use crate::gfx::Canvas;
use crate::image::Image;

/// The old TV switching off: the picture to a line, the line to a dot, the dot gone.
pub const TV_OFF: Duration = Duration::from_millis(380);
/// A handheld's display going dark.
pub const LCD_OFF: Duration = Duration::from_millis(240);
/// From black to the shelf, the console still up: the launch fade backwards.
pub const FADE: Duration = crate::insert_scene::FADE;
/// The launch animation runs back from where its fade began (`cabinet::FADE_AT_COZY`) to A, this much faster.
pub const BACK_SPEED: f32 = 1.3;

/// Back in its place, an open case's game comes a little out of its side in this long, on its own clock (not the
/// shelf's turn: restarting that right after the way back was a jump, 11.11.).
pub const AJAR_IN: Duration = Duration::from_millis(240);

/// Parts of [`TV_OFF`]: the picture is a line here, a dot here.
const LINE_AT: f32 = 0.42;
const DOT_AT: f32 = 0.72;
/// The line's thickness at 720 px of screen height, as `crt_on`'s.
const LINE_PX: f32 = 3.0;

/// What the game was shown on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// A console on a TV: the picture pulls together to a line and a dot.
    Tv,
    /// A handheld's own display: it goes dark.
    Lcd,
}

impl Screen {
    /// Game Boy, GBA, Game Gear, DS and PSP games had no TV.
    pub fn of(ff: FormFactor) -> Screen {
        match ff {
            FormFactor::HandheldCart | FormFactor::CardBox | FormFactor::UmdCase => Screen::Lcd,
            _ => Screen::Tv,
        }
    }

    fn off_len(self) -> Duration {
        match self {
            Screen::Tv => TV_OFF,
            Screen::Lcd => LCD_OFF,
        }
    }
}

/// What the way back shows at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Moment {
    /// The last picture switching off, this far (0 to 1).
    Off(f32),
    /// The shelf with the launch animation at `scene` after A over it (`Scene::paint`), under black of `black` (1 black,
    /// 0 none).
    Shelf { black: f32, scene: Duration },
    /// Over: the shelf alone.
    Done,
}

/// One way back: what the game was shown on, whether its last picture switches off first (there is one, and on a TV the
/// old-TV switch-on is on; else it starts from black), and whether its console shuts a lid or tray over a disc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Way {
    pub screen: Screen,
    pub off: bool,
    pub lid: bool,
}

impl Way {
    /// The launch animation's moment it starts from: the medium in and, with a lid or tray, that shut.
    pub fn start(self) -> Duration {
        if self.lid {
            cabinet::IN_AT_COZY + crate::insert_rig::SHUT_AFTER + Duration::from_millis(60)
        } else {
            cabinet::FADE_AT_COZY
        }
    }

    fn off_len(self) -> Duration {
        if self.off {
            self.screen.off_len()
        } else {
            Duration::ZERO
        }
    }

    /// How long it all takes.
    pub fn len(self) -> Duration {
        self.off_len() + FADE + self.start().div_f32(BACK_SPEED)
    }

    /// What shows `since` the game ended.
    pub fn moment(self, since: Duration) -> Moment {
        let first = self.off_len();
        if since < first {
            return Moment::Off(since.as_secs_f32() / first.as_secs_f32());
        }
        let s = since - first;
        if s < FADE {
            return Moment::Shelf { black: 1.0 - s.as_secs_f32() / FADE.as_secs_f32(), scene: self.start() };
        }
        let back = (s - FADE).mul_f32(BACK_SPEED);
        match self.start().checked_sub(back) {
            Some(scene) if !scene.is_zero() => Moment::Shelf { black: 0.0, scene },
            _ => Moment::Done,
        }
    }

    /// The medium is fully back before the case closes: both timelines start opening at 120 ms.
    /// The earlier carry threshold (0.3) still leaves the cartridge sticking out of its case.
    pub fn case_shut_at(self) -> Duration {
        let shut = cabinet::INSERT_COZY.mul_f32(0.06);
        self.off_len() + FADE + self.start().saturating_sub(shut).div_f32(BACK_SPEED)
    }

}

/// Android's player and launcher are two processes: at Save & quit the player leaves the game's last picture and where
/// it stood for the launcher's way back (`android/src/eject.rs`), in `last-picture.json` beside the saves. The picture
/// is the preview the auto state was written with (`slots::preview_path`), nothing is drawn or copied for it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LastPicture {
    /// The game's key in the player (`ui::game_key`).
    pub key: String,
    /// The game's last picture (a PNG), when there is one.
    pub picture: Option<PathBuf>,
    /// Where the game's picture stood in the player's window, in its pixels: (x, y, w, h) from the top left.
    pub rect: Option<(i32, i32, i32, i32)>,
    /// The player's window, in pixels.
    pub window: (u32, u32),
    /// The game's moment was saved (Save & quit; not "Return to library" after a failed load).
    pub saved: bool,
    /// When, in ms since 1970: a note older than [`LastPicture::FRESH`] is from another time and not shown.
    pub at_ms: u64,
}

impl LastPicture {
    /// The launcher is back about a second after Save & quit; a note this old belongs to an earlier time.
    pub const FRESH: Duration = Duration::from_secs(10);

    pub fn path(data: &Path) -> PathBuf {
        data.join("last-picture.json")
    }

    pub fn write(&self, data: &Path) -> std::io::Result<()> {
        crate::fsutil::write_atomic(&Self::path(data), serde_json::to_string(self).unwrap_or_default().as_bytes())
    }

    /// The note, read once: it is gone afterwards, whatever it says. None when missing, broken or stale at `now_ms`.
    pub fn take(data: &Path, now_ms: u64) -> Option<LastPicture> {
        let path = Self::path(data);
        let text = std::fs::read_to_string(&path).ok()?;
        // Not shown when it can't be taken away: it would come again at the next return.
        std::fs::remove_file(&path).ok()?;
        let note: LastPicture = serde_json::from_str(&text).ok()?;
        let age = now_ms.checked_sub(note.at_ms)?;
        (age <= Self::FRESH.as_millis() as u64).then_some(note)
    }

    /// Where the picture stood on a launcher canvas at `offset` in a window `window` big: the same window (nothing
    /// turned meanwhile) and a rectangle, else None (the whole canvas).
    pub fn area_in(&self, window: (u32, u32), offset: (i32, i32)) -> Option<(i32, i32, i32, i32)> {
        let (x, y, w, h) = self.rect.filter(|r| r.2 > 0 && r.3 > 0 && window == self.window)?;
        Some((x - offset.0, y - offset.1, w, h))
    }
}

/// The old TV switching off at `p` (0 to 1), as rectangles over a screen `w` x `h`: where the squeezed picture is and how
/// white it has gone, and the light (the line, then the dot) with its brightness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Off {
    pub picture: Option<((f32, f32, f32, f32), f32)>,
    pub light: Option<((f32, f32, f32, f32), f32)>,
}

pub fn tv_off(p: f32, w: f32, h: f32) -> Off {
    let line = (LINE_PX * h / 720.0).max(2.0);
    let (cx, cy) = (w / 2.0, h / 2.0);
    if p < LINE_AT {
        // Squeezed to the middle, faster and faster, and whiter as it goes: the beam's last sweep.
        let q = p / LINE_AT;
        let e = q * q;
        let hh = h + (line - h) * e;
        let white = 0.9 * q.powf(1.5);
        let rect = (0.0, cy - hh / 2.0, w, hh);
        let light = (q > 0.7).then(|| ((0.0, cy - line / 2.0, w, line), (q - 0.7) / 0.3));
        return Off { picture: Some((rect, white)), light };
    }
    if p < DOT_AT {
        // The line pulls in from both sides to a dot.
        let q = (p - LINE_AT) / (DOT_AT - LINE_AT);
        let dot = line * 3.2;
        let lw = w + (dot - w) * (1.0 - (1.0 - q).powi(2));
        // Thicker to a round dot only at the end, so what is lit only ever shrinks.
        let lh = line + (dot - line) * q.powi(4);
        return Off { picture: None, light: Some(((cx - lw / 2.0, cy - lh / 2.0, lw, lh), 1.0)) };
    }
    if p < 1.0 {
        // The dot glows on a moment and goes out.
        let q = (p - DOT_AT) / (1.0 - DOT_AT);
        let d = line * 3.2 * (1.0 - 0.4 * q);
        return Off { picture: None, light: Some(((cx - d / 2.0, cy - d / 2.0, d, d), (1.0 - q).powi(2))) };
    }
    Off { picture: None, light: None }
}

/// Where the game's picture stood on a `w` x `h` screen, from its rectangle `rect` in the grabbed picture `size` (pocket-core's
/// game rectangle, `preview::GameRect`); the whole screen without one.
pub fn area_on(w: i32, h: i32, size: (u32, u32), rect: Option<(u32, u32, u32, u32)>) -> (i32, i32, i32, i32) {
    let Some((rx, ry, rw, rh)) = rect.filter(|r| r.2 > 0 && r.3 > 0 && size.0 > 0 && size.1 > 0) else { return (0, 0, w, h) };
    let (kx, ky) = (w as f32 / size.0 as f32, h as f32 / size.1 as f32);
    ((rx as f32 * kx) as i32, (ry as f32 * ky) as i32, ((rw as f32 * kx) as i32).max(1), ((rh as f32 * ky) as i32).max(1))
}

/// `frame`'s part `src` (the game's rectangle in the grabbed picture; all of it when None) into (x, y, w, h).
fn blit(c: &mut Canvas, frame: &Image, src: Option<(u32, u32, u32, u32)>, (x, y, w, h): (i32, i32, i32, i32)) {
    match src {
        Some(r) => c.blit_region(frame, r, x, y, w, h),
        None => c.blit_scaled(frame, x, y, w, h),
    }
}

/// Paints the old TV switching off at `p` over the whole of `c` (black round it), from `frame`, the game's last picture
/// (its part `src`, the game without borders; the pause menu's grab is used as it is, not copied), which stood at `area`
/// on the screen.
pub fn paint_tv_off(c: &mut Canvas, frame: Option<&Image>, src: Option<(u32, u32, u32, u32)>, area: (i32, i32, i32, i32), p: f32) {
    c.clear(0);
    let (ax, ay, aw, ah) = area;
    let Off { picture, light } = tv_off(p, aw as f32, ah as f32);
    let light = light.map(|((x, y, w, h), a)| ((x + ax as f32, y + ay as f32, w, h), a));
    if let Some(((x, y, w, h), white)) = picture {
        let (x, y, w, h) = (x as i32 + ax, y.round() as i32 + ay, w as i32, (h.round() as i32).max(1));
        match frame {
            Some(f) => blit(c, f, src, (x, y, w, h)),
            None => c.rect(x, y, w, h, 0x404040),
        }
        if white > 0.0 {
            brighten(c, (x, y, w, h), white);
        }
    }
    if let Some(((x, y, w, h), a)) = light {
        // The dot: round, with a halo that goes with it.
        if w <= h * 1.5 {
            let r = w / 2.0;
            glow_disc(c, (x + r, y + h / 2.0), r, r * 5.0, a);
            return;
        }
        // A soft glow round the line, then the line itself.
        let glow = (h * 3.0).max(6.0);
        for i in (1..=3).rev() {
            let g = glow * i as f32 / 3.0;
            let alpha = (a * 0.22 / i as f32 * 255.0) as u8;
            c.shade_rect((x - g) as i32, (y - g) as i32, (w + 2.0 * g) as i32, (h + 2.0 * g) as i32, 0xd8e6ff, alpha);
        }
        c.shade_rect(x as i32, y.round() as i32, (w.round() as i32).max(1), (h.round() as i32).max(1), 0xffffff, (a * 255.0) as u8);
    }
}

/// The beam's last sweep over `rect`: what is lit goes towards white by `k`, black much less (the black round a game's
/// picture stays dark instead of turning into grey bars).
fn brighten(c: &mut Canvas, (x, y, w, h): (i32, i32, i32, i32), k: f32) {
    for row in y.max(0)..(y + h).min(c.h) {
        for col in x.max(0)..(x + w).min(c.w) {
            let i = (row * c.w + col) as usize;
            let p = c.buf[i];
            let luma = ((p >> 16 & 0xff) * 3 + (p >> 8 & 0xff) * 6 + (p & 0xff)) as f32 / 2550.0;
            c.buf[i] = crate::gfx::mix(p, 0xffffff, (k * (0.25 + 0.75 * luma) * 255.0) as u32);
        }
    }
}

/// A round light at `centre`, `r` across its core, its halo out to `halo`, at brightness `a`.
fn glow_disc(c: &mut Canvas, (cx, cy): (f32, f32), r: f32, halo: f32, a: f32) {
    let (x0, x1) = (((cx - halo).floor() as i32).max(0), ((cx + halo).ceil() as i32).min(c.w));
    let (y0, y1) = (((cy - halo).floor() as i32).max(0), ((cy + halo).ceil() as i32).min(c.h));
    for y in y0..y1 {
        for x in x0..x1 {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let (toward, k) = if d <= r {
                (0xffffff, a)
            } else if d < halo {
                let t = 1.0 - (d - r) / (halo - r);
                (0xd8e6ff, a * 0.55 * t * t)
            } else {
                continue;
            };
            let i = (y * c.w + x) as usize;
            c.buf[i] = crate::gfx::mix(c.buf[i], toward, (k * 255.0) as u32);
        }
    }
}

/// How dark a handheld's display has gone at `p` (0 to 1): fast at first, as an LCD's light goes.
pub fn lcd_dark(p: f32) -> f32 {
    1.0 - (1.0 - p.clamp(0.0, 1.0)).powi(2)
}

/// Paints a handheld's display going dark at `p` over the whole of `c`, its picture (`frame`'s part `src`) at `area`.
pub fn paint_lcd_off(c: &mut Canvas, frame: Option<&Image>, src: Option<(u32, u32, u32, u32)>, area: (i32, i32, i32, i32), p: f32) {
    c.clear(0);
    let (x, y, w, h) = area;
    if let Some(f) = frame {
        blit(c, f, src, (x, y, w, h));
    }
    c.darken_rect(x, y, w, h, (lcd_dark(p) * 255.0) as u8);
}

/// Black over the shelf by `black` (1 black).
pub fn paint_black(c: &mut Canvas, black: f32) {
    c.fade((black.clamp(0.0, 1.0) * 255.0) as u8);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_tv_game_goes_to_a_line_then_a_dot_then_dark() {
        let (w, h) = (1280.0, 720.0);
        let early = tv_off(0.1, w, h);
        let ((_, _, _, ph), white) = early.picture.expect("the picture still shows");
        assert!(ph > h * 0.8 && white < 0.2, "at first nearly the whole picture: {ph} {white}");
        let line = tv_off(0.5, w, h);
        let ((_, ly, lw, lh), a) = line.light.expect("a line");
        assert!(line.picture.is_none() && lw > w * 0.3 && lh <= 8.0 && a == 1.0, "a bright thin line: {lw}x{lh}");
        assert!((ly + lh / 2.0 - h / 2.0).abs() < 1.0, "in the middle");
        let dot = tv_off(0.8, w, h);
        let ((_, _, dw, dh), _) = dot.light.expect("a dot");
        assert!(dw < 12.0 && dh < 12.0, "a dot: {dw}x{dh}");
        assert_eq!(tv_off(1.0, w, h), Off { picture: None, light: None });
    }

    #[test]
    fn the_picture_only_shrinks_while_it_switches_off() {
        let mut last = f32::MAX;
        for i in 0..100 {
            let o = tv_off(i as f32 / 100.0, 640.0, 480.0);
            let size = o.picture.map(|((_, _, w, h), _)| w * h).or(o.light.map(|((_, _, w, h), _)| w * h)).unwrap_or(0.0);
            assert!(size <= last + 0.5, "{i}: {size} > {last}");
            last = size;
        }
    }

    #[test]
    fn a_handheld_has_no_tv_its_display_goes_dark() {
        assert_eq!(Screen::of(FormFactor::HandheldCart), Screen::Lcd, "GB, GBC, GBA");
        assert_eq!(Screen::of(FormFactor::CardBox), Screen::Lcd, "DS");
        assert_eq!(Screen::of(FormFactor::ConsoleCart), Screen::Tv, "SNES");
        assert_eq!(Screen::of(FormFactor::JewelCase), Screen::Tv, "PlayStation");
        assert!(lcd_dark(0.0) == 0.0 && lcd_dark(1.0) == 1.0 && lcd_dark(0.5) > 0.5);
    }

    #[test]
    fn the_game_rectangle_is_where_the_picture_switches_off() {
        assert_eq!(area_on(1240, 1080, (1240, 1080), None), (0, 0, 1240, 1080), "no rectangle: the whole screen");
        assert_eq!(area_on(1240, 1080, (620, 540), Some((0, 37, 620, 465))), (0, 74, 1240, 930), "scaled to the screen");
        // Only the game's part of the grabbed picture is painted, at the game's place.
        let img = Image { w: 4, h: 3, px: (0..12).collect() };
        let mut buf = vec![0u32; 8 * 6];
        paint_lcd_off(&mut Canvas::new(&mut buf, 8, 6), Some(&img), Some((1, 1, 2, 2)), (2, 2, 4, 2), 0.0);
        assert_eq!(&buf[2 * 8..3 * 8], &[0, 0, 5, 5, 6, 6, 0, 0]);
        assert!(buf[..2 * 8].iter().all(|p| *p == 0), "black round it");
    }

    #[test]
    fn the_way_back_runs_the_launch_backwards_and_ends() {
        for (screen, off, lid) in [(Screen::Tv, true, false), (Screen::Tv, false, false), (Screen::Tv, true, true), (Screen::Lcd, true, false), (Screen::Lcd, false, false)] {
            let way = Way { screen, off, lid };
            let first = way.off_len();
            assert!(matches!(way.moment(Duration::ZERO), Moment::Off(_)) == !first.is_zero());
            // Then black to the shelf with the medium in, then the scene going back to A.
            assert_eq!(way.moment(first), Moment::Shelf { black: 1.0, scene: way.start() });
            let mut last = way.start();
            let mut t = first + FADE;
            while t < way.len() {
                match way.moment(t) {
                    Moment::Shelf { black, scene } => {
                        assert!(black == 0.0 && scene <= last, "{way:?} {t:?}: backwards only");
                        last = scene;
                    }
                    m => panic!("{way:?} {t:?}: {m:?}"),
                }
                t += ms(16);
            }
            assert!(last < ms(30), "{way:?}: back to A ({last:?})");
            assert_eq!(way.moment(way.len()), Moment::Done);
            assert!(way.case_shut_at() < way.len() && way.case_shut_at() > first + FADE);
        }
        // A lid or tray starts shut: the way back begins after it closed in the launch.
        let lid = Way { screen: Screen::Tv, off: true, lid: true };
        assert!(crate::insert_rig::pose(crate::insert_rig::Kind::Lid, lid.start()).open < 0.01);
        // Short enough not to stand in the way: about two seconds on a TV, less on a handheld.
        assert!(lid.len() < ms(2100) && Way { screen: Screen::Lcd, off: true, lid: false }.len() < lid.len());
    }

    #[test]
    fn android_s_last_picture_is_read_once_fresh_and_placed_where_it_stood() {
        let dir = tempfile::tempdir().unwrap();
        let note = LastPicture { key: "0123456789abcdef".into(), picture: Some(dir.path().join("p.png")), rect: Some((240, 0, 1440, 1080)), window: (1920, 1080), saved: true, at_ms: 1_000_000 };
        note.write(dir.path()).unwrap();
        assert_eq!(LastPicture::take(dir.path(), 1_000_900), Some(note.clone()), "back a second later");
        assert!(LastPicture::take(dir.path(), 1_000_900).is_none(), "read once: a later return from Home shows nothing");
        note.write(dir.path()).unwrap();
        assert!(LastPicture::take(dir.path(), 1_000_000 + 60_000).is_none(), "a minute old: from another time");
        assert!(!LastPicture::path(dir.path()).exists(), "and gone");
        std::fs::write(LastPicture::path(dir.path()), b"{broken").unwrap();
        assert!(LastPicture::take(dir.path(), 1_000_000).is_none() && !LastPicture::path(dir.path()).exists());
        // On the launcher's canvas, the safe area 80 px in from the left: the same place on the glass.
        assert_eq!(note.area_in((1920, 1080), (80, 0)), Some((160, 0, 1440, 1080)));
        assert_eq!(note.area_in((1080, 1920), (0, 0)), None, "turned meanwhile: the whole canvas");
        assert_eq!(LastPicture { rect: None, ..note }.area_in((1920, 1080), (0, 0)), None);
    }

    #[test]
    fn painting_the_off_stays_on_the_screen_and_goes_black() {
        let frame = Image { w: 256, h: 224, px: vec![0x3366cc; 256 * 224] };
        for (w, h) in [(640, 480), (1240, 1080), (1280, 720)] {
            let mut buf = vec![0u32; (w * h) as usize];
            for i in 0..=20 {
                let area = area_on(w as i32, h as i32, (w, h), Some((w / 8, 0, w * 3 / 4, h)));
                paint_tv_off(&mut Canvas::new(&mut buf, w, h), Some(&frame), None, area, i as f32 / 20.0);
                assert!(buf.iter().enumerate().all(|(n, p)| (n as u32 % w) >= w / 8 - 40 || *p == 0), "{w}x{h}: only over the game");
                paint_lcd_off(&mut Canvas::new(&mut buf, w, h), None, None, area, i as f32 / 20.0);
            }
            let all = (0, 0, w as i32, h as i32);
            paint_tv_off(&mut Canvas::new(&mut buf, w, h), Some(&frame), None, all, 1.0);
            assert!(buf.iter().all(|p| *p == 0), "{w}x{h}: dark at the end");
            paint_lcd_off(&mut Canvas::new(&mut buf, w, h), Some(&frame), Some((10, 10, 100, 100)), all, 1.0);
            assert!(buf.iter().all(|p| *p == 0), "{w}x{h}: a handheld's display dark at the end");
            paint_tv_off(&mut Canvas::new(&mut buf, w, h), Some(&frame), None, all, 0.5);
            let lit = buf.iter().filter(|p| (**p & 0xff) > 0xe0).count();
            assert!(lit > 0 && lit < (w * h / 20) as usize, "{w}x{h}: a line, {lit} px");
        }
    }
}

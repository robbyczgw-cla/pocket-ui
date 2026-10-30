//! Part of the pocket-ui binary: the CRT TV on the Cabinet's top board (`pocket_ui::cabinet_tv`), showing the selected
//! game's screenshot. It stands at the board's right end, in the showcase; the case out of the shelf keeps clear of
//! it (`paint_out` reads [`TvState::left`]). Its size follows the screen, not the case, so it doesn't change as the
//! selection moves: about a fifth of the screen's height, a whole number of screen pixels a template pixel where it is
//! two or more, left out where its glass would be under 60 px high or the showcase has no room for it.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use pocket_ui::cabinet_art::{self as art, Pic};
use pocket_ui::cabinet_tv::{self, Body, Loader, Want};
use pocket_ui::gfx::Canvas;

use super::App;

/// The TV's height as a share of the screen's.
const SHARE: f32 = 0.21;
/// From the cabinet's right inner wall.
const GAP: i32 = 30;

#[derive(Default)]
pub(crate) struct TvState {
    loader: Option<Loader>,
    /// The TV's body at a scale (hundredths) and the screen's position on it.
    body: Option<(i32, Rc<Body>)>,
    /// What the glass shows, the picture before it while the crossfade runs, and when it began.
    shown: Option<(Want, Option<Rc<Pic>>)>,
    /// Files changed at the same paths: ask the worker again, keeping the old picture until its replacement arrives.
    refresh: bool,
    prev: Option<Option<Rc<Pic>>>,
    since: Option<Instant>,
    /// The TV with its screen as painted last (the same picture, at rest, is not made again).
    frame: Option<(usize, usize, u32, Rc<Pic>)>,
    /// The TV's left edge this frame (None: no TV): the case out of the shelf stays left of it.
    pub(crate) left: Option<i32>,
}

impl TvState {
    pub(crate) fn pictures_changed(&mut self) {
        if let Some(loader) = &mut self.loader {
            loader.invalidate();
        }
        self.refresh = true;
    }

    /// Drain replies even when a page hides the Cabinet; a completed job must no longer keep frames running.
    pub(crate) fn poll(&mut self) -> bool {
        self.loader.as_mut().is_some_and(Loader::poll)
    }

    fn select(&mut self, want: Want, data_dir: &Path, inline: bool, now: Instant) {
        let loader = self.loader.get_or_insert_with(|| Loader::new(data_dir));
        loader.poll();
        if inline && loader.get(&want).is_none() {
            loader.settle(&want);
        }
        if self.refresh || self.shown.as_ref().is_none_or(|(w, _)| *w != want) {
            if let Some(pic) = loader.get(&want) {
                let before = self.shown.take().map(|(_, p)| p);
                let fade = before.is_some() && pocket_ui::motion::allowed(pocket_ui::motion::Kind::Fade) && !inline;
                self.prev = if fade { before } else { None };
                self.since = fade.then_some(now);
                self.shown = Some((want, pic));
                self.refresh = false;
            }
        }
    }

    /// A screen is being made or crossfading: the frames go on.
    pub(crate) fn busy(&self, now: Instant) -> bool {
        self.loader.as_ref().is_some_and(Loader::waiting) || self.since.is_some_and(|t| now.duration_since(t) < cabinet_tv::FADE)
    }
}

/// The TV's scale for a screen `w` x `h` with `room` px of showcase above the board: None where it is left out.
pub(crate) fn tv_scale(w: i32, h: i32, room: i32) -> Option<f32> {
    let t = cabinet_tv::template();
    let want = (h as f32 * SHARE).min(room as f32 - 12.0);
    let f = want / t.body.h as f32;
    let f = if f >= 2.0 { f.floor() } else { f };
    // Never more than a quarter of the width: the case out of the shelf needs the rest.
    cabinet_tv::scale_for(w / 4, room, f)
}

impl App {
    /// The TV for the selected entry (`system`, `rom`: empty for what is not a game; `cover` its box art), standing on
    /// the top board whose top is at `board_y`; `room` the showcase's height. Sets `left` for `paint_out`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_cabinet_tv(&mut self, c: &mut Canvas, system: &str, rom: &Path, cover: Option<PathBuf>, board_y: i32, room: i32, now: Instant) {
        self.cabinet.tv.left = None;
        // The look step's sample cabinet has no TV: it shows how the cases look. Nor with its Cozy extra off.
        if self.sample_cover.is_some() || !pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::CabinetTv) {
            return;
        }
        let Some(f) = tv_scale(c.w, c.h, room) else { return };
        let fkey = (f * 100.0).round() as i32;
        if self.cabinet.tv.body.as_ref().is_none_or(|(k, _)| *k != fkey) {
            self.cabinet.tv.body = Some((fkey, Rc::new(cabinet_tv::body(f))));
            self.cabinet.tv.frame = None;
        }
        let body = self.cabinet.tv.body.as_ref().map(|(_, b)| b.clone()).unwrap();
        let (_, _, gw, gh) = body.glass;
        let want = Want { system: system.to_string(), rom: rom.to_path_buf(), cover, w: gw, h: gh };
        let inline = self.cabinet.inline;
        let data_dir = self.data_dir.clone();
        let tv = &mut self.cabinet.tv;
        // The new game's picture once it is made; until then the last one stays on the glass.
        tv.select(want, &data_dir, inline, now);
        let a = match tv.since {
            Some(t) if now.duration_since(t) < cabinet_tv::FADE => {
                let x = now.duration_since(t).as_secs_f32() / cabinet_tv::FADE.as_secs_f32();
                (x * x * (3.0 - 2.0 * x) * 255.0) as u32
            }
            _ => {
                tv.since = None;
                tv.prev = None;
                255
            }
        };
        let ptr = |p: &Option<Rc<Pic>>| p.as_ref().map_or(0, |p| Rc::as_ptr(p) as usize);
        let cur = tv.shown.as_ref().map(|(_, p)| p.clone()).unwrap_or(None);
        let prev = tv.prev.clone().unwrap_or(None);
        let key = (ptr(&cur), ptr(&prev), a);
        let pic = match &tv.frame {
            Some((k1, k2, k3, p)) if (*k1, *k2, *k3) == key => p.clone(),
            _ => {
                let mut p = body.pic.clone();
                if let Some(old) = &prev {
                    cabinet_tv::show(&mut p, body.glass, &body.mask, old, 255);
                }
                if let Some(new) = &cur {
                    cabinet_tv::show(&mut p, body.glass, &body.mask, new, a);
                }
                let p = Rc::new(p);
                tv.frame = Some((key.0, key.1, key.2, p.clone()));
                p
            }
        };
        let x = c.w - super::app_cabinet::CAB_X - art::WALL - GAP - pic.w;
        let y = board_y - pic.h;
        tv.left = Some(x);
        art::soft_shadow(c, x + 10, y + 12, pic.w - 4, pic.h - 12, 18, 0.45, (0, c.h));
        art::blit(c, &pic, x, y, 256, (0, c.h));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_snap(dir: &Path, want: &Want, rgb: [u8; 3]) {
        let file = pocket_ui::game_pictures::path(dir, pocket_ui::game_pictures::Kind::Snap, &want.system, &want.rom);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let mut encoder = png::Encoder::new(std::fs::File::create(file).unwrap(), 4, 4);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&rgb.repeat(16)).unwrap();
    }

    #[test]
    fn replacing_a_picture_at_the_same_path_refreshes_the_selected_tv() {
        let dir = tempfile::tempdir().unwrap();
        let want = Want { system: "snes".into(), rom: PathBuf::from("Game.sfc"), cover: None, w: 90, h: 70 };
        write_snap(dir.path(), &want, [230, 10, 10]);
        let mut tv = TvState::default();
        tv.select(want.clone(), dir.path(), true, Instant::now());
        let old = tv.shown.as_ref().unwrap().1.clone().unwrap();
        let center = 35 * 90 + 45;
        assert!((old.px[center] >> 16 & 0xff) > (old.px[center] & 0xff) * 3);

        // The initial inline selection can still have its original worker reply queued. Its generation must not
        // refill the decoded cache; replacing the file wins without changing Want or starting a second worker.
        write_snap(dir.path(), &want, [10, 10, 230]);
        tv.pictures_changed();
        assert!(Rc::ptr_eq(&old, tv.shown.as_ref().unwrap().1.as_ref().unwrap()), "keep the old screen while loading");
        let started = Instant::now();
        while tv.refresh && started.elapsed() < std::time::Duration::from_secs(3) {
            tv.select(want.clone(), dir.path(), false, Instant::now());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!tv.refresh, "the real loader delivered the replacement");
        let new = tv.shown.as_ref().unwrap().1.as_ref().unwrap();
        assert!(!Rc::ptr_eq(&old, new));
        assert!((new.px[center] & 0xff) > (new.px[center] >> 16 & 0xff) * 3, "the same path now shows the new blue picture");
    }

    #[test]
    fn a_hidden_tv_finishes_waiting_and_has_its_picture_when_the_cabinet_returns() {
        let dir = tempfile::tempdir().unwrap();
        let want = Want { system: "snes".into(), rom: PathBuf::from("Game.sfc"), cover: None, w: 90, h: 70 };
        write_snap(dir.path(), &want, [20, 210, 20]);
        let mut tv = TvState::default();
        tv.select(want.clone(), dir.path(), false, Instant::now());
        assert!(tv.busy(Instant::now()));
        assert!(tv.shown.is_none(), "the initial frame asked the real worker");

        // Settings, a card or another look is painted now. Only the common event-loop poll runs; there is no
        // paint_cabinet_tv/select call to consume the answer. Finishing must stop the redraw timer anyway.
        let started = Instant::now();
        let mut repaint = false;
        while tv.busy(Instant::now()) && started.elapsed() < std::time::Duration::from_secs(3) {
            repaint |= tv.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(repaint, "the completed picture requests one frame");
        assert!(!tv.busy(Instant::now()), "a hidden TV no longer keeps the UI animating");
        assert!(tv.shown.is_none(), "the hidden Cabinet has not painted");

        // Returning to the Cabinet uses the already completed result on its first frame, without another wait.
        tv.select(want, dir.path(), false, Instant::now());
        assert!(tv.shown.as_ref().unwrap().1.is_some());
        assert!(!tv.busy(Instant::now()));
    }

    #[test]
    fn the_tv_fits_every_screen_or_is_left_out() {
        // The Mini V2, 1280 x 720, 640 x 480 (laid out at 1280 x 960, `scaled_layout`) with a roomy and with a tight
        // showcase.
        for (w, h, room) in [(1240, 1080, 420), (1280, 720, 260), (1280, 960, 300), (1280, 960, 60)] {
            match tv_scale(w, h, room) {
                Some(f) => {
                    let t = cabinet_tv::template();
                    let (tw, th) = (t.body.w as f32 * f, t.body.h as f32 * f);
                    assert!(th <= room as f32 && tw <= w as f32 / 4.0 + 0.5, "{w}x{h} room {room}: {tw}x{th}");
                    assert!(t.glass.3 as f32 * f >= 60.0);
                }
                None => assert!(room < 120, "{w}x{h} room {room}: a TV was expected"),
            }
        }
        assert!(tv_scale(1240, 1080, 420).is_some_and(|f| f.fract() == 0.0), "whole pixels on the Mini V2");
    }
}

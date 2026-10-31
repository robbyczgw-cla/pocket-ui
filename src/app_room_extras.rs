//! Part of the pocket-ui binary: the App's side of `pocket_ui::room_light` (light by time of day) and
//! `pocket_ui::room_deco` (decorations that grow with play) on the Shelf and the Cabinet, each switched by its row in
//! Settings → Look → Cozy extras (`cozy_extras::Extra::TimeOfDayLight`, `Extra::Decorations`).
//!
//! No work per frame: the light is read from the clock at most every [`LOOK_AGAIN`], the milestones from the library
//! when its case stickers changed or every [`LOOK_AGAIN`]; when either changed, the wood and the Shelf look's tiles are
//! made again once (`CabinetUi::room_changed`). While a Shelf or the Cabinet is on screen the app wakes once a minute
//! to look (`room_tick`) and asks for a frame only when something changed; with Motion off it doesn't wake by itself.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::cabinet_art::Pic;
use pocket_ui::cozy_extras::{self, Extra};
use pocket_ui::gfx::Canvas;
use pocket_ui::media::Prop;
use pocket_ui::palette::*;
use pocket_ui::room_deco::Earned;
use pocket_ui::room_light::{self, Light};

use super::{App, Screen, Theme};

/// How long a reading of the clock or the library holds.
const LOOK_AGAIN: Duration = Duration::from_secs(20);

/// What the room's light and decorations were read as, and the framed "100 %" per scale.
#[derive(Default)]
pub(crate) struct Room {
    light: Option<(Instant, Light)>,
    /// Polling is independent of the optional light cache: Decorations can stay on with the light off.
    next_poll: Option<Instant>,
    earned: Option<(Instant, u64, Rc<Earned>)>,
    /// What the wood and tiles in the caches were made for (`Room::key`), and the light they are made in.
    pub(crate) made_for: Option<u64>,
    pub(crate) lit: Light,
    /// The decorations' pictures per (prop, scale × 100), the framed "100 %" with its words on.
    pics: HashMap<(Prop, i32), Rc<Pic>>,
}

impl Room {
    /// Whether the minute poll is due, and its next deadline. A disabled or invalidated light cache must never
    /// make the library's milestone scan run on every input or animation frame.
    fn poll(&mut self, now: Instant) -> (bool, Instant) {
        if let Some(next) = self.next_poll.filter(|next| now < *next) {
            return (false, next);
        }
        let next = now + Duration::from_secs(60);
        self.next_poll = Some(next);
        (true, next)
    }

    /// Light and milestones together, for the caches.
    pub(crate) fn key(light: Light, earned: &Earned) -> u64 {
        light.key().wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ earned.key()
    }
}

/// With Decorations off: the shelves as they always were (the grown plant, nothing earned shown).
fn classic() -> Earned {
    Earned { plant: 2, ..Default::default() }
}

impl App {
    /// The room's light now (neutral with the extra off).
    pub(crate) fn room_light(&mut self) -> Light {
        if !cozy_extras::on(Extra::TimeOfDayLight) {
            return Light::NEUTRAL;
        }
        let now = Instant::now();
        let light_page = pal().void != 0;
        match self.cabinet.room.light {
            Some((at, l)) if now.duration_since(at) < LOOK_AGAIN && self.cabinet.room.made_for.is_some() => l,
            _ => {
                let l = room_light::at(room_light::minute_now(), light_page);
                self.cabinet.room.light = Some((now, l));
                l
            }
        }
    }

    /// What the player has earned, as the shelves show it.
    pub(crate) fn room_earned(&mut self) -> Rc<Earned> {
        if !cozy_extras::on(Extra::Decorations) {
            thread_local!(static CLASSIC: Rc<Earned> = Rc::new(classic()));
            return CLASSIC.with(Rc::clone);
        }
        let now = Instant::now();
        let gen = self.library.case_generation();
        match &self.cabinet.room.earned {
            Some((at, g, e)) if *g == gen && now.duration_since(*at) < LOOK_AGAIN => e.clone(),
            _ => {
                let e = Rc::new(self.library.earned());
                self.cabinet.room.earned = Some((now, gen, e.clone()));
                e
            }
        }
    }

    /// At the start of a Shelf or Cabinet frame: the light and the milestones, the cached wood and tiles made again
    /// when either changed since they were made.
    pub(crate) fn room_sync(&mut self) -> (Light, Rc<Earned>) {
        let (light, earned) = (self.room_light(), self.room_earned());
        let key = Room::key(light, &earned);
        if self.cabinet.room.made_for != Some(key) {
            if self.cabinet.room.made_for.is_some() {
                self.cabinet.room_changed();
            }
            self.cabinet.room.made_for = Some(key);
            self.cabinet.room.lit = light;
        }
        (light, earned)
    }

    /// Once a minute while a Shelf or the Cabinet is on screen with either extra on: a frame when the light or the
    /// milestones changed. None otherwise (and with Motion off: the light follows the next frame painted anyway).
    pub(crate) fn room_tick(&mut self, now: Instant) -> Option<Instant> {
        let shown = matches!(self.theme, Theme::Shelf | Theme::Cabinet)
            && self.screen == Screen::Home
            && self.running.is_none()
            && self.settings_page.is_none()
            && self.sleep.state() == pocket_ui::sleep::State::Awake;
        let on = cozy_extras::on(Extra::TimeOfDayLight) || cozy_extras::on(Extra::Decorations);
        if !shown || !on || pocket_ui::motion::level() == pocket_ui::motion::Motion::Off {
            return None;
        }
        let (due, next) = self.cabinet.room.poll(now);
        if !due {
            return Some(next);
        }
        let before = self.cabinet.room.made_for;
        self.cabinet.room.light = None;
        self.cabinet.room.earned = None;
        let (light, earned) = (self.room_light(), self.room_earned());
        if before != Some(Room::key(light, &earned)) {
            self.request_redraw();
        }
        Some(next)
    }

    /// The picture of `prop` at scale `f` (screen pixels a template pixel) with what was earned: the plant at its
    /// stage; the framed "100 %" with its words. Made once.
    pub(crate) fn room_prop_pic(&mut self, prop: Prop, earned: &Earned, f: f32) -> Rc<Pic> {
        pocket_ui::room_props::prop_pic(&mut self.cabinet.room.pics, &mut self.text, prop, earned, f)
    }
}

pub(crate) use pocket_ui::room_props::deco_for;

/// The lamp's glow behind it: a soft warm light round its shade, `strength` 0-255 (`Light::lamp`), inside `clip`
/// (x0, y0, x1, y1: the shelf's compartment, so it never spills onto the board above). The light's shape is made once
/// per lamp size and strength (`glow_mask`) and painted into a picture that is kept (the Cabinet's back wall,
/// `lit_wall`; the Shelf look's tile), never per frame.
pub(crate) fn lamp_glow(c: &mut Canvas, lamp: &Pic, x: i32, y: i32, strength: u8, clip: (i32, i32, i32, i32)) {
    if strength == 0 {
        return;
    }
    let mask = glow_mask(lamp.w, lamp.h, strength);
    let (mw, mh) = (lamp.w * 4, lamp.h * 3 / 2);
    let (left, top) = (x + lamp.w / 2 - mw / 2, y + lamp.h / 4 - mh / 2);
    let (x0, x1) = (left.max(clip.0).max(0), (left + mw).min(clip.2).min(c.w));
    let (y0, y1) = (top.max(clip.1).max(0), (top + mh).min(clip.3).min(c.h));
    let (gr, gg, gb) = (room_light::LAMP_GLOW >> 16 & 0xff, room_light::LAMP_GLOW >> 8 & 0xff, room_light::LAMP_GLOW & 0xff);
    for yy in y0..y1 {
        let m = &mask[((yy - top) * mw) as usize..][..mw as usize];
        let row = &mut c.buf[(yy * c.w) as usize..][..c.w as usize];
        for xx in x0..x1 {
            let a = m[(xx - left) as usize] as u32;
            if a == 0 {
                continue;
            }
            let d = row[xx as usize];
            let mixc = |dc: u32, gc: u32| (dc * (256 - a) + gc * a) >> 8;
            // The alpha byte stays (the glow is also painted into the wood's pictures, `lit_wall`).
            row[xx as usize] = d & 0xff00_0000 | mixc(d >> 16 & 0xff, gr) << 16 | mixc(d >> 8 & 0xff, gg) << 8 | mixc(d & 0xff, gb);
        }
    }
}

/// The glow's shape for a lamp `w` x `h`: an ellipse twice the lamp's width each side and three quarters of its height
/// up and down, in eight soft steps, strongest by the shade; 0-82 (a third of the light at most). Kept per size and
/// strength (a handful at most).
fn glow_mask(w: i32, h: i32, strength: u8) -> Rc<Vec<u8>> {
    thread_local!(static MASKS: std::cell::RefCell<HashMap<(i32, i32, u8), Rc<Vec<u8>>>> = Default::default());
    MASKS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(v) = m.get(&(w, h, strength)) {
            return v.clone();
        }
        let (mw, mh) = (w * 4, h * 3 / 2);
        let (rx, ry) = (mw as f32 / 2.0, mh as f32 / 2.0);
        let max = strength as f32 / 255.0 * 0.32;
        let mut v = vec![0u8; (mw * mh) as usize];
        for y in 0..mh {
            let dy = (y as f32 + 0.5 - ry) / ry;
            for x in 0..mw {
                let dx = (x as f32 + 0.5 - rx) / rx;
                let d = dx * dx + dy * dy;
                if d < 1.0 {
                    let step = ((1.0 - d) * 8.0).ceil() / 8.0;
                    v[(y * mw + x) as usize] = (max * step * step * 256.0) as u8;
                }
            }
        }
        if m.len() > 8 {
            m.clear();
        }
        let v = Rc::new(v);
        m.insert((w, h, strength), v.clone());
        v
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decorations_poll_once_a_minute_even_with_the_light_off() {
        let mut room = Room::default();
        let start = Instant::now();
        let next = start + Duration::from_secs(60);
        assert_eq!(room.poll(start), (true, next));
        // Time-of-day light off leaves this cache empty. At 60 Hz, none of the intervening animation/input
        // turns may trigger another full-library milestone scan or postpone the existing wake-up.
        for frame in 1..3600 {
            assert!(room.light.is_none());
            assert_eq!(room.poll(start + Duration::from_secs_f64(frame as f64 / 60.0)), (false, next));
        }
        assert_eq!(room.poll(next), (true, next + Duration::from_secs(60)));
        // Rebuilding the light for a palette change also leaves the polling deadline alone.
        room.light = Some((next, Light::NEUTRAL));
        assert_eq!(room.poll(next + Duration::from_secs(1)), (false, next + Duration::from_secs(60)));
    }

    #[test]
    fn the_glow_stays_in_its_compartment() {
        let mut buf = vec![0u32; 200 * 120];
        let mut c = Canvas::new(&mut buf, 200, 120);
        let lamp = Pic::new(30, 50, 0xff00_0000);
        lamp_glow(&mut c, &lamp, 80, 60, 255, (20, 40, 180, 110));
        for y in 0..120 {
            for x in 0..200 {
                let lit = buf[y * 200 + x] != 0;
                if lit {
                    assert!((20..180).contains(&x) && (40..110).contains(&y), "{x},{y}");
                }
            }
        }
        assert!(buf.iter().filter(|p| **p != 0).count() > 500);
    }
}

//! The start of a game as a picture and a schedule, for pocket-ui on Linux (`app_cabinet`) and the Android launcher
//! (`android/src/insert.rs`): the console comes up, the case opens, the medium leaves it and goes in, by its console's
//! real mechanism where there is a rig (`insert_rig`), the plain way otherwise. Each host makes the case's front and the
//! medium with its own caches and says where the console stands ([`Stage`]); [`Scene::paint`] draws one moment,
//! [`cues`] are the sounds with their waits and [`hold`] how long the game's window keeps back.

use std::rc::Rc;
use std::time::Duration;

use crate::cabinet::{self, FormFactor};
use crate::cabinet_art::{self as art, Look, Pic};
use crate::gfx::{mix, Canvas};
use crate::insert_rig as ir;
use crate::media::{Game, Medium};
use crate::system_art::Sprite;
use crate::ui_sound::{self, Sound};

/// The launch fade: from the shelf to black, and the game's window comes.
pub const FADE: Duration = Duration::from_millis(250);
/// A sound goes a little before its moment: the output takes about that long to play what it is given.
const SOUND_LEAD: Duration = Duration::from_millis(10);

/// How long after A the game's window may come up: Cozy holds it until the medium is in and the fade has run, the quick
/// way doesn't hold it at all.
pub fn hold(cozy: bool) -> Option<Duration> {
    cozy.then(|| cabinet::fade_delay(true) + FADE)
}

/// When a game's window may come, for a host that starts it itself at that moment (Android's `PlayerActivity`): when the
/// hold has run or at the first skip, once. `insert_scene::hold`'s rules.
#[derive(Clone, Copy, Debug)]
pub struct HandOver {
    hold: Option<Duration>,
    released: bool,
    skipped: bool,
}

impl HandOver {
    pub fn new(cozy: bool) -> HandOver {
        HandOver { hold: hold(cozy), released: false, skipped: false }
    }

    /// A tap or any button: the picture ends and the game comes now. True the first time.
    pub fn skip(&mut self) -> bool {
        let first = !self.skipped;
        self.skipped = true;
        self.hold = None;
        first
    }

    pub fn skipped(&self) -> bool {
        self.skipped
    }

    pub fn released(&self) -> bool {
        self.released
    }

    /// True once, `elapsed` after A when the game may come.
    pub fn take_release(&mut self, elapsed: Duration) -> bool {
        if self.released || self.hold.is_some_and(|hold| elapsed < hold) {
            return false;
        }
        self.released = true;
        true
    }
}

/// One sound of a start: `after` from A, `db` against the UI volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    pub sound: Sound,
    pub after: Duration,
    pub db: f32,
}

/// The sounds of a start, all queued at A with their waits: the sound thread plays them on time. Quick: a quiet click as
/// it goes in (`cabinet::IN_AT`). Cozy: the case opens, and the game goes into the console with its own sound, seated as
/// it is in (`IN_AT_COZY`). A rig's disc (`disc_rig`): down on its spindle, a pause, and the lid or tray shuts with the
/// rest of the sound.
pub fn cues(cozy: bool, medium: Medium, disc_rig: bool) -> Vec<Cue> {
    let in_at = cabinet::in_at(cozy).saturating_sub(SOUND_LEAD);
    let at = |sound: Sound, moment: Duration, db: f32| Cue { sound, after: moment.saturating_sub(ui_sound::hit_at(sound)), db };
    if !cozy {
        return vec![at(Sound::Insert, in_at, ui_sound::QUICK_DB)];
    }
    let opening = Cue { sound: ui_sound::opening(medium), after: Duration::ZERO, db: 0.0 };
    if disc_rig {
        vec![opening, at(Sound::DiscDrop, in_at, 0.0), at(Sound::DiscShut, in_at + ir::SHUT_AFTER, 0.0)]
    } else {
        vec![opening, at(ui_sound::going_in(medium), in_at, 0.0)]
    }
}

/// Where the console stands on this screen. `k` is screen pixels per pixel of pocket-ui's handheld layout, on which the
/// animation's measures are made: 1 on Linux.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// The floor under the console.
    pub floor: i32,
    pub k: f32,
}

/// A disc's radius over the drive of a console `sw` pixels wide: a 12 cm disc a sixth of the console, the GameCube's
/// 8 cm one and a UMD smaller by their real size.
pub fn over_drive(game: Game, sw: f32, k: f32) -> f32 {
    (sw * 0.16).clamp(46.0 * k, 90.0 * k) * game.mm().1 / 120.0
}

/// A rig's pictures, ready to paint: the console's at screen size, what moves at its own (drawn through a map).
pub struct Rigged {
    pub rig: ir::Rig,
    pub base: Pic,
    pub over: Option<Pic>,
    pub top: Option<Pic>,
    pub under: Option<Pic>,
    /// Screen pixels per pixel of the rig's art.
    pub scale: f32,
}

impl Rigged {
    /// `blocks`: screen pixels per pixel of the art (`ir::SCALE` on Linux).
    pub fn load(system: &str, game: Game, blocks: i32) -> Option<Rigged> {
        let f = ir::files(system, game)?;
        let pic = |data: &[u8]| {
            let s = crate::system_art::decode(data)?;
            Some(Pic { w: s.w as i32, h: s.h as i32, px: s.px })
        };
        let (mut base, over) = (enlarged(pic(&f.base)?, blocks), f.over.and_then(|d| pic(&d)).map(|p| enlarged(p, blocks)));
        // What `over` covers whole is never seen on the base: each frame only copies a console's pixels once.
        if let Some(o) = over.as_ref().filter(|o| (o.w, o.h) == (base.w, base.h)) {
            for (b, t) in base.px.iter_mut().zip(&o.px) {
                if t >> 24 == 255 {
                    *b = 0;
                }
            }
        }
        Some(Rigged { rig: f.rig, base, over, top: f.top.and_then(|d| pic(&d)), under: f.under.and_then(|d| pic(&d)), scale: blocks as f32 })
    }
}

/// `p` `n` times as large, each pixel a block.
fn enlarged(p: Pic, n: i32) -> Pic {
    let (w, h) = (p.w * n, p.h * n);
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let row = &p.px[((y / n) * p.w) as usize..][..p.w as usize];
        for x in 0..w {
            px.push(row[(x / n) as usize]);
        }
    }
    Pic { w, h, px }
}

/// The case going into the console while the game starts.
pub struct Scene {
    pub system: String,
    pub ff: FormFactor,
    pub medium: Medium,
    pub look: Look,
    pub title: String,
    /// What comes out of the case (`media::Game::of`: the system's own cartridge).
    pub kind: Game,
    /// The case's front has the game's box art (not a title card): the label gets it.
    pub has_cover: bool,
    pub front: Rc<Pic>,
    pub from: (i32, i32, i32, i32),
    /// The cartridge or card that comes out of the case, made on the first frame of the animation (after the game's
    /// process is started, so nothing is added before it) from the cover on the case's front; None for a disc.
    pub game: Option<Rc<Pic>>,
    /// Cozy or Quick when it started: its length and when the cartridge is in.
    pub cozy: bool,
    /// Playful animations: the disc at its size over the drive, made once (each frame only scales it a little).
    pub at_drive: Option<Rc<Pic>>,
    /// Cozy + Playful, a console with its own mechanism (`insert_rig`): its pictures, decoded on the first frame and gone with the animation.
    pub rig: Option<Rc<Rigged>>,
    /// It has a rig: asked once, on the first frame.
    pub rig_asked: bool,
}

impl Scene {
    /// How far the launch fade is, `elapsed` after A, 0-1: it waits for the medium to go in, and a rig's lid or tray shuts
    /// first, so its fade keeps its end and runs after the clunk.
    pub fn fade_at(&self, elapsed: Duration) -> f32 {
        let p = (elapsed.saturating_sub(cabinet::fade_delay(self.cozy)).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
        self.rig_fade(p)
    }

    /// The fade `p` as it shows with this scene's rig.
    pub fn rig_fade(&self, p: f32) -> f32 {
        if self.rig.is_some() {
            ((p - 0.55) / 0.45).clamp(0.0, 1.0)
        } else {
            p
        }
    }

    /// Asks for the console's own mechanism, once: Cozy + Playful only.
    pub fn load_rig(&mut self, blocks: i32) {
        if !self.rig_asked {
            self.rig = (self.cozy && crate::playful::on()).then(|| Rigged::load(&self.system, self.kind, blocks)).flatten().map(Rc::new);
            self.rig_asked = true;
        }
    }

    /// One moment of it, `elapsed` after A, over what is on `c`. `sprite` is the console's picture (`fitted` to the stage)
    /// for the plain animation.
    pub fn paint(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, stage: Stage) {
        if self.rig.is_some() {
            return self.paint_rig(c, elapsed, stage);
        }
        self.paint_plain(c, elapsed, sprite, stage);
    }

    /// Every system without a rig, and every system in Quick: the console comes up from below and the medium goes in.
    fn paint_plain(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, Stage { floor, k }: Stage) {
        let px = |n: i32| (n as f32 * k) as i32;
        let (system, ff, medium, front, from) = (self.system.clone(), self.ff, self.medium, self.front.clone(), self.from);
        let tint = self.look.color;
        let game = self.kind;
        let game_pic = self.game.clone();
        let st = cabinet::insert_at(elapsed, self.cozy);
        // Playful animations: each medium goes in its own way (`playful::Loader`), timed to its sound.
        let playful = crate::playful::on();
        let loader = crate::playful::Loader::of(&system, game);
        let keys = if playful {
            let t = (elapsed.as_secs_f32() / cabinet::insert_len(self.cozy).as_secs_f32()).min(1.0);
            let since_in = elapsed.as_secs_f32() - cabinet::in_at(self.cozy).as_secs_f32();
            crate::playful::keys(loader, t, since_in, if self.cozy { crate::motion::COZY_SLOWER } else { 1.0 })
        } else {
            crate::playful::Keys::default()
        };
        art::dim(c, 0, c.h, 256 - (st.dim * 256.0) as u32);
        // The console, up from below the screen.
        let (sw, sh) = sprite.as_ref().map_or((px(360), px(120)), |s| (s.w as i32, s.h as i32));
        let base = floor - px(10);
        let sx = (c.w - sw) / 2;
        let sy_final = base - sh;
        let sy = sy_final + ((1.0 - st.rise) * (sh + px(160)) as f32) as i32;
        if let Some(sp) = &sprite {
            c.blit_sprite(sp, sx, sy, ((1.0 - st.rise) * 200.0) as u8);
        } else {
            c.round_rect(sx, sy, sw, sh, px(16), mix(0x2c2c30, 0x000000, ((1.0 - st.rise) * 200.0) as u32));
        }
        // The slot: a handheld's at its top edge, a console's on its top, a disc drive's in its middle.
        let slot_y = sy
            + match ff {
                FormFactor::HandheldCart | FormFactor::CardBox => sh / 14,
                FormFactor::ConsoleCart => sh / 3,
                _ => sh * 2 / 5,
            };
        let slot_cx = c.w / 2;
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let (fx, fy, fw, fh) = (from.0 as f32, from.1 as f32, from.2 as f32, from.3 as f32);
        // The game slides out of its case (from behind it, to its right), the case fades, the game goes to the console
        // and in.
        let case_fade = 256 - (((st.carry - 0.3) / 0.4).clamp(0.0, 1.0) * 256.0) as u32;
        let out = (st.carry / 0.35).min(1.0);
        let fly = ((st.carry - 0.35) / 0.65).clamp(0.0, 1.0);
        let clip_bottom = if fly > 0.5 { slot_y } else { c.h };
        use crate::playful::Loader;
        let draw_game: Box<dyn Fn(&mut Canvas)> = if game.is_round() && playful && game_pic.is_some() && matches!(loader, Loader::Tray | Loader::Lid) {
            let r0 = game.size_beside(medium, from.3).1 as f32 / 2.0;
            let r1 = over_drive(game, sw as f32, k);
            // The plastic around the drive, for its tray or lid; the console's top edge, for the lid.
            let body = sprite
                .as_ref()
                .and_then(|sp| {
                    let y = ((slot_y - sy - px(10)).max(0) as u32).min(sp.h.saturating_sub(1));
                    let p = sp.px[(y * sp.w + sp.w / 2) as usize];
                    (p >> 24 > 200).then_some(p & 0xffffff)
                })
                .unwrap_or_else(|| art::plastic(&system));
            // A top-loader's lid where its picture has it, in the colour the picture gives it.
            let (ax, ay, arx, ary) = crate::playful::lid_of(&system);
            let lid = ((sx as f32 + ax * sw as f32) as i32, (sy as f32 + ay * sh as f32) as i32, (arx * sw as f32) as i32, (ary * sh as f32) as i32);
            let lid_top = sprite
                .as_ref()
                .and_then(|sp| {
                    let (x, y) = (((lid.0 - sx).max(0) as u32).min(sp.w - 1), ((lid.1 + lid.3 * 3 / 4 - sy).max(0) as u32).min(sp.h - 1));
                    let p = sp.px[(y * sp.w + x) as usize];
                    (p >> 24 > 200).then_some(p & 0xffffff)
                })
                .unwrap_or(body);
            let start = (fx + fw / 2.0 + out * fw * 0.55, fy + fh / 2.0);
            self.playful_disc(c, (slot_cx, slot_y, body), (lid, lid_top), (r0, r1), (start, fly), (st, keys, loader), k)
        } else if game.is_round() {
            // As large as the real disc beside its case.
            let r0 = game.size_beside(medium, from.3).1 as f32 / 2.0;
            let r1 = over_drive(game, sw as f32, k);
            let start = (fx + fw / 2.0 + out * fw * 0.55, fy + fh / 2.0);
            let end = (slot_cx as f32, slot_y as f32 - r1);
            let r = lerp(r0, r1, fly);
            let (x, y) = (lerp(start.0, end.0, fly), lerp(start.1, end.1, fly) + st.sink * 2.0 * r1);
            if st.rise > 0.4 {
                c.round_rect(slot_cx - r1 as i32 - px(10), slot_y - px(4), 2 * r1 as i32 + px(20), px(9), px(4), 0x0c0c0d);
            }
            if let Some(pic) = game_pic {
                // The printed disc, scaled to its size on the way (a UMD keeps its shell's shape).
                let (h, w) = (2.0 * r, 2.0 * r * pic.w as f32 / pic.h.max(1) as f32);
                let shown = if (h as i32, w as i32) == (pic.h, pic.w) { pic } else { Rc::new(pic.scaled_smooth(w as i32, h as i32)) };
                Box::new(move |c: &mut Canvas| art::blit(c, &shown, (x - w / 2.0) as i32, (y - h / 2.0) as i32, 256, (0, clip_bottom)))
            } else {
                Box::new(move |c: &mut Canvas| {
                    if game == Game::Umd {
                        art::umd(c, x as i32, y as i32, r as i32, tint, clip_bottom);
                    } else {
                        art::disc(c, x as i32, y as i32, r as i32, tint, clip_bottom);
                    }
                })
            }
        } else {
            let Some(pic) = game_pic else { return };
            let (gw, gh) = (pic.w as f32, pic.h as f32);
            // The cartridge shrinks onto the slot and sinks in, partly: it stays out of its slot as it does in the real
            // console. A game card goes in all the way.
            let end_w = (sw as f32 * if game == Game::Card { 0.2 } else { 0.36 }).clamp(if game == Game::Card { 60.0 * k } else { 90.0 * k }, 230.0 * k);
            let end_h = end_w * gh / gw.max(1.0);
            let depth = match (game, ff) {
                (Game::Card, _) => 1.0,
                (_, FormFactor::ConsoleCart) => 0.55,
                (_, FormFactor::HandheldCart) => 0.65,
                _ => 0.85,
            };
            let start = (fx + fw / 2.0 + out * (fw / 2.0 + gw * 0.45), fy + fh / 2.0);
            let end = (slot_cx as f32, slot_y as f32 - end_h / 2.0);
            let (w, h) = (lerp(gw, end_w, fly), lerp(gh, end_h, fly));
            let (cx, cy) = (lerp(start.0, end.0, fly), lerp(start.1, end.1, fly) + st.sink * end_h * depth);
            if st.rise > 0.4 {
                c.round_rect(slot_cx - end_w as i32 / 2 - px(8), slot_y - px(4), end_w as i32 + px(16), px(9), px(4), 0x0c0c0d);
            }
            if playful {
                // Pushed in, it seats: flatter and wider for a moment with the click, then back out a hair (a game card
                // clicks all the way in and springs back so its edge shows for a moment). Scaled as it is drawn.
                let (sq, lift) = (keys.squash, keys.lift * if loader == Loader::Card { 4.0 } else { 1.0 });
                let (w, h0) = (w * (1.0 + 0.08 * sq), h);
                let h = h0 * (1.0 - 0.14 * sq);
                // The part in the slot stays where it is: the top comes down as it squashes.
                let cy = cy + (h0 - h) / 2.0 - lift * end_h;
                Box::new(move |c: &mut Canvas| art::blit_scaled(c, &pic, (cx - w / 2.0) as i32, (cy - h / 2.0) as i32, w as i32, h as i32, 256, (0, clip_bottom)))
            } else {
                let shown = if fly > 0.0 { Rc::new(pic.scaled(w as i32, h as i32)) } else { pic };
                Box::new(move |c: &mut Canvas| art::blit(c, &shown, (cx - w / 2.0) as i32, (cy - h / 2.0) as i32, 256, (0, clip_bottom)))
            }
        };
        // Behind the case while it comes out, in front once it is on its way.
        if fly == 0.0 {
            draw_game(c);
        }
        if case_fade > 0 {
            art::blit(c, &front, from.0, from.1, case_fade, (0, c.h));
        }
        if fly > 0.0 {
            draw_game(c);
        }
        // It is in: the slot lights up for a moment, in the label's colour lifted towards white.
        if st.glow > 0.0 {
            let light = mix(tint, crate::palette::scheme().lamp, 170);
            art::glow(c, slot_cx, slot_y, (sw / 3).max(px(90)), (sh / 5).max(px(40)), light, st.glow * 0.8);
            art::glow(c, slot_cx, slot_y, (sw / 8).max(px(36)), (sh / 14).max(px(14)), 0xffffff, st.glow * 0.7);
        }
    }

    /// A game going into its console's real mechanism (`insert_rig`, docs/DESIGN.md): the console comes up, the lid or
    /// tray opens, the game leaves its case and goes in behind the right parts, the lid shuts with the sound's clunk.
    fn paint_rig(&mut self, c: &mut Canvas, elapsed: Duration, Stage { floor, k }: Stage) {
        use ir::Mech;
        let Some(pics) = self.rig.clone() else { return };
        let (front, from, kind, medium) = (self.front.clone(), self.from, self.kind, self.medium);
        let Some(pic) = self.game.clone() else { return };
        let rig = pics.rig;
        let scale = pics.scale;
        let st = cabinet::insert_at(elapsed, true);
        let pose = ir::pose(rig.kind(), elapsed);
        art::dim(c, 0, c.h, 256 - (st.dim * 256.0) as u32);
        let (bw, bh) = (pics.base.w, pics.base.h);
        let px = |n: i32| (n as f32 * k) as i32;
        let ox = ((c.w - bw) / 2) as f32;
        let oy = (floor - px(10) - bh) as f32 + (1.0 - st.rise) * (bh + px(160)) as f32 + pose.dip * scale;
        let shade = 256 - ((1.0 - st.rise) * 200.0) as u32;
        let screen = |p: (f32, f32)| (ox + p.0 * scale, oy + p.1 * scale);
        let all = (0, c.h);

        // The medium on its way: face-on at its size beside the case, down to lying in its seat.
        let (fx, fy, fw, fh) = (from.0 as f32, from.1 as f32, from.2 as f32, from.3 as f32);
        let beside = kind.size_beside(medium, from.3);
        let near_h = beside.1 as f32;
        let near_w = if kind.is_round() { near_h * pic.w as f32 / pic.h.max(1) as f32 } else { beside.0 as f32 };
        let start = (fx + fw / 2.0 + pose.out * (fw / 2.0 + near_w * 0.45), fy + fh / 2.0);
        let m = rig.medium_at(start, screen, (near_w, near_h), &pose, scale);
        let draw_medium = |c: &mut Canvas| {
            let (sx, sy) = (pic.w as f32 / 2.0, pic.h as f32 / 2.0);
            let o = (m.at.0 - m.u.0 - m.v.0, m.at.1 - m.u.1 - m.v.1);
            art::blit_affine(c, &pic, o, (m.u.0 / sx, m.u.1 / sx), (m.v.0 / sy, m.v.1 / sy), 256, all);
        };

        // Behind the case while it comes out, in front once it is on its way.
        let case_fade = 256 - (((st.carry - 0.3) / 0.4).clamp(0.0, 1.0) * 256.0) as u32;
        let flying = pose.fly > 0.0;
        if rig.behind && flying {
            draw_medium(c);
        }
        art::blit(c, &pics.base, ox.round() as i32, oy.round() as i32, shade, all);
        // The lid stands behind the disc coming down, and shuts over it; the tray is under it always.
        let lid_later = pose.landed >= 1.0;
        match rig.mech() {
            Mech::Lid(l) if !lid_later => draw_lid(c, l, &pics, (ox, oy), pose.open, shade),
            Mech::Tray(t) => {
                if let Some(tray) = &pics.top {
                    art::blit_affine(c, tray, screen(rig.tray_at(t, pose.open)), (scale, 0.0), (0.0, scale), shade, all);
                }
            }
            _ => {}
        }
        if !flying {
            draw_medium(c);
        }
        if case_fade > 0 {
            art::blit(c, &front, from.0, from.1, case_fade, all);
        }
        // A disc coming down is in front of everything; once it is down the parts in front of it hide it.
        let coming_down = rig.kind() != ir::Kind::Cart && pose.landed < 1.0;
        if flying && !rig.behind {
            // A shadow on the console under it: darker as it gets close.
            if coming_down && pose.fly >= 1.0 {
                let at = screen(rig.seat.at);
                let (rx, ry) = (rig.seat.u.0.hypot(rig.seat.u.1) * scale, rig.seat.v.0.hypot(rig.seat.v.1) * scale);
                art::glow(c, at.0 as i32, at.1 as i32, (rx * 1.08) as i32, (ry * 1.08) as i32, 0x000000, 0.45 * pose.drop);
            }
            if !coming_down && !pose.shut_in {
                draw_medium(c);
            }
        }
        if let Some(over) = &pics.over {
            art::blit(c, over, ox.round() as i32, oy.round() as i32, shade, all);
        }
        if flying && !rig.behind && coming_down {
            draw_medium(c);
        }
        if let Mech::Lid(l) = rig.mech() {
            if lid_later {
                draw_lid(c, l, &pics, (ox, oy), pose.open, shade);
            }
        }
        // The console takes it: its power light comes on.
        if let (Some(led), true) = (rig.led, pose.led > 0.0) {
            let at = screen(led.at);
            art::glow(c, at.0 as i32, at.1 as i32, px(16), px(16), led.color, 0.9 * pose.led);
            art::glow(c, at.0 as i32, at.1 as i32, px(5), px(5), 0xffffff, 0.8 * pose.led);
        }
    }

    /// Playful animations, a disc going into a console with a tray or a lid (`playful::Loader`): the tray slides out
    /// under the drive's slot, the disc comes over it, drops in lying down and rides in as the tray shuts; or the lid
    /// on top flips up, the disc drops in behind the console's top edge and the lid shuts. Draws the tray or lid now
    /// (behind the disc) and gives back what draws the disc. `slot` is (x, y) of the drive's slot, the console's top
    /// edge and its plastic's colour; the disc is made at its size over the drive once (`at_drive`) and
    /// only scaled a little on the way.
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn playful_disc(&mut self, c: &mut Canvas, (slot_x, slot_y, body): (i32, i32, u32), (lid, lid_top): ((i32, i32, i32, i32), u32), (r0, r1): (f32, f32), (start, fly): ((f32, f32), f32), (st, keys, loader): (cabinet::Insert, crate::playful::Keys, crate::playful::Loader), k: f32) -> Box<dyn Fn(&mut Canvas)> {
        let px = |n: i32| (n as f32 * k) as i32;
        let Some(pic) = self.game.clone() else { return Box::new(|_: &mut Canvas| {}) };
        let aspect = pic.w as f32 / pic.h.max(1) as f32;
        let big = match &self.at_drive {
            Some(p) => p.clone(),
            None => {
                let h = (2.0 * r1).round() as i32;
                let w = (h as f32 * aspect).round() as i32;
                let p = Rc::new(if (w, h) == (pic.w, pic.h) { (*pic).clone() } else { pic.scaled_smooth(w, h) });
                self.at_drive = Some(p.clone());
                p
            }
        };
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let r = lerp(r0, r1, fly);
        // Where it goes: over the tray's well or over the lid's opening, then down.
        let src = if fly >= 1.0 { big } else { pic };
        if loader == crate::playful::Loader::Tray {
            let tw = (r1 * 2.5) as i32;
            if st.rise > 0.4 {
                c.round_rect(slot_x - tw / 2 - px(6), slot_y - px(4), tw + px(12), px(9), px(4), 0x0c0c0d);
            }
            let (wy, depth) = art::tray(c, slot_x, slot_y, tw, keys.open, body);
            let well = slot_y as f32 + tw as f32 * 0.42 / 2.0;
            let over = (slot_x as f32, well - r1 * 1.15);
            let landed = keys.drop >= 1.0;
            if landed && depth == 0 {
                return Box::new(|_: &mut Canvas| {});
            }
            let x = lerp(start.0, over.0, fly);
            let y = if landed { wy as f32 } else { lerp(lerp(start.1, over.1, fly), well, keys.drop) };
            // Lying in the tray it is seen at a slant: 0.4 of its height.
            let (w, h) = (2.0 * r * aspect, 2.0 * r * lerp(1.0, 0.4, keys.drop));
            let clip = if landed { (slot_y, c.h) } else { (0, c.h) };
            return Box::new(move |c: &mut Canvas| art::blit_scaled(c, &src, (x - w / 2.0).round() as i32, (y - h / 2.0).round() as i32, w.round() as i32, h.round() as i32, 256, clip));
        }
        // A lid: open behind, the disc over the opening, down into it lying as the opening is drawn, the lid over it.
        let (lx, ly, lrx, lry) = lid;
        art::lid_behind(c, lid, keys.open, lid_top);
        if keys.drop >= 1.0 && keys.open <= 0.0 {
            return Box::new(|_: &mut Canvas| {});
        }
        let over = (lx as f32, ly as f32 - lry as f32 - r1 * 1.1);
        let x = lerp(lerp(start.0, over.0, fly), lx as f32, keys.drop);
        let y = lerp(lerp(start.1, over.1, fly), ly as f32, keys.drop);
        let w = lerp(2.0 * r * aspect, 2.0 * lrx as f32 * 0.92, keys.drop);
        let h = lerp(2.0 * r, 2.0 * lry as f32 * 0.92, keys.drop);
        let open = keys.open;
        Box::new(move |c: &mut Canvas| {
            art::blit_scaled(c, &src, (x - w / 2.0).round() as i32, (y - h / 2.0).round() as i32, w.round() as i32, h.round() as i32, 256, (0, c.h));
            art::lid_over(c, lid, open, lid_top);
        })
    }
}

fn draw_lid(c: &mut Canvas, l: ir::Lid, pics: &Rigged, o: (f32, f32), open: f32, shade: u32) {
    let m = l.map(open);
    let Some(p) = (if m.under { pics.under.as_ref() } else { pics.top.as_ref() }) else { return };
    let s = pics.scale;
    let at = (o.0 + m.o.0 * s, o.1 + m.o.1 * s);
    let (u, v) = ((m.u.0 * s, m.u.1 * s), (m.v.0 * s, m.v.1 * s));
    // Turned away from the light above it, a little darker.
    let shade = shade * (256 - (m.turned * 50.0) as u32) / 256;
    art::blit_affine(c, p, at, u, v, shade, (0, c.h));
}

/// How a phone places a start: the most whole screen pixels per pixel of the art with which a rig, its medium hanging
/// over it included, stays inside `w` x `h`; never below the handhelds' 2, never above 4 (a console, not a poster).
pub fn phone_blocks(rig: Option<&ir::Rig>, base: (i32, i32), (w, h): (i32, i32)) -> i32 {
    let (half, tall) = phone_extent(rig, base);
    let across = ((w - 32) as f32 / (2.0 * half)) as i32;
    let down = ((h - 32) as f32 / tall) as i32;
    across.min(down).clamp(2, 4)
}

/// Half the width and the height a start needs, in art pixels: the console, and the medium over its seat's hover.
fn phone_extent(rig: Option<&ir::Rig>, (bw, bh): (i32, i32)) -> (f32, f32) {
    let (bw, bh) = (bw as f32, bh as f32);
    let Some(rig) = rig else { return (bw / 2.0, bh) };
    let s = rig.seat;
    let (ru, rv) = (s.u.0.hypot(s.u.1), s.v.0.hypot(s.v.1));
    (((s.hover.0 - bw / 2.0).abs() + ru).max(bw / 2.0), bh - (s.hover.1 - rv).min(0.0))
}

/// The floor for a rig `blocks` screen pixels per art pixel: it and the medium over it in the middle of a screen `h` high.
pub fn phone_floor(rig: Option<&ir::Rig>, base: (i32, i32), blocks: i32, h: i32, k: f32) -> i32 {
    let (_, tall) = phone_extent(rig, base);
    let above = tall - base.1 as f32;
    // The console's top edge at `(h - tall) / 2 + above`; the floor is 10 handheld pixels under its bottom (`paint_rig`).
    (h as f32 / 2.0 + (above - tall / 2.0 + base.1 as f32) * blocks as f32 + 10.0 * k) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(system: &str, cozy: bool) -> Scene {
        let kind = Game::of(system, "x.rom");
        let medium = Medium::of(system);
        let look = Look { color: 0x8040c0, band: 0x8040c0, plastic: art::plastic(system) };
        let front = Rc::new(Pic::new(120, 180, 0xff40_6080));
        let game = Rc::new(Pic::new(90, 90, 0xffc0_c0c0));
        Scene { system: system.into(), ff: FormFactor::of(system), medium, look, title: "T".into(), kind, has_cover: false, front, from: (40, 40, 120, 180), game: Some(game), cozy, at_drive: None, rig: None, rig_asked: false }
    }

    fn frame(s: &mut Scene, ms: u64, stage: Stage) -> Vec<u32> {
        let mut buf = vec![0x202020u32; 720 * 1600];
        let sprite = crate::system_art::get(&s.system).map(|p| p.fitted((560.0 * stage.k) as i32, (330.0 * stage.k) as i32));
        s.paint(&mut Canvas::new(&mut buf, 720, 1600), Duration::from_millis(ms), sprite.as_ref(), stage);
        buf
    }

    fn lit(px: &[u32]) -> usize {
        px.iter().filter(|p| (*p & 0xff) > 0x30 || (*p >> 8 & 0xff) > 0x30 || (*p >> 16 & 0xff) > 0x30).count()
    }

    const STAGE: Stage = Stage { floor: 1000, k: 1.0 };

    #[test]
    fn a_system_without_a_rig_shows_the_plain_animation_never_a_gap() {
        for system in ["psp", "3ds", "nes", "pcengine", "nosuchsystem"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert!(s.rig.is_none(), "{system}");
            let (a, b) = (frame(&mut s, 700, STAGE), frame(&mut s, 1100, STAGE));
            assert!(lit(&a) > 3000, "{system}: a console stands there");
            assert!(a != b, "{system}: the game moves");
        }
    }

    #[test]
    fn a_rig_with_cozy_and_never_without() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        for system in ["psx", "ps2", "gamecube", "snes", "gba", "n64", "megadrive", "dreamcast", "saturn", "xbox", "gb", "nds", "wii"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert!(s.rig.is_some(), "{system}");
            assert!(lit(&frame(&mut s, 750, STAGE)) > 3000, "{system}: a console stands there");
            let mut quick = scene(system, false);
            quick.load_rig(2);
            assert!(quick.rig.is_none(), "{system}: Quick never loads a rig");
        }
    }

    #[test]
    fn motion_off_or_a_frozen_game_is_the_quick_way_with_no_hold_and_one_quiet_click() {
        use crate::motion::Motion::{Full, Off, Reduced};
        for (motion, frozen) in [(Off, false), (Reduced, false), (Full, true)] {
            let cozy = cabinet::starts_cozy(true, motion, frozen);
            assert!(!cozy, "{motion:?} {frozen}");
            assert_eq!(hold(cozy), None);
            let mut s = scene("psx", cozy);
            s.load_rig(2);
            assert!(s.rig.is_none());
            assert_eq!(cues(cozy, Medium::of("psx"), false).iter().map(|c| c.sound).collect::<Vec<_>>(), [Sound::Insert]);
        }
        assert!(cabinet::starts_cozy(true, Full, false));
    }

    #[test]
    fn the_window_is_held_until_the_fade_has_run_and_the_fade_ends_black() {
        assert_eq!(hold(true), Some(Duration::from_millis(1600)));
        for system in ["psx", "psp"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert_eq!(s.fade_at(cabinet::IN_AT_COZY), 0.0, "{system}");
            assert_eq!(s.fade_at(hold(true).unwrap()), 1.0, "{system}");
        }
    }

    #[test]
    fn a_skip_brings_the_game_at_once_and_only_once() {
        let at = |ms| Duration::from_millis(ms);
        let mut cozy = HandOver::new(true);
        assert!(!cozy.take_release(at(0)) && !cozy.take_release(at(1599)), "held while the animation plays");
        assert!(cozy.skip() && cozy.skipped());
        assert!(cozy.take_release(at(300)), "a skip ends the hold");
        assert!(!cozy.take_release(at(301)) && !cozy.skip(), "once");
        let mut played = HandOver::new(true);
        assert!(played.take_release(at(1600)) && played.released());
        // Quick, Motion below Full, a frozen game: nothing to wait for.
        assert!(HandOver::new(false).take_release(at(0)));
    }

    #[test]
    fn each_sound_hits_on_its_moment_of_the_picture() {
        let hit = |c: &Cue| c.after + ui_sound::hit_at(c.sound) + SOUND_LEAD;
        let disc = cues(true, Medium::of("psx"), true);
        assert_eq!(disc.iter().map(|c| c.sound).collect::<Vec<_>>(), [Sound::CaseOpen, Sound::DiscDrop, Sound::DiscShut]);
        assert_eq!(hit(&disc[1]), cabinet::IN_AT_COZY);
        assert_eq!(hit(&disc[2]), cabinet::IN_AT_COZY + ir::SHUT_AFTER);
        let cart = cues(true, Medium::of("snes"), false);
        assert_eq!(cart[0].sound, ui_sound::opening(Medium::of("snes")));
        assert_eq!((cart[1].sound, hit(&cart[1])), (Sound::CartIn, cabinet::IN_AT_COZY));
        // The plain animation's disc keeps the whole sound, on the same moment.
        let plain = cues(true, Medium::of("psp"), false);
        assert_eq!(hit(&plain[1]), cabinet::IN_AT_COZY);
        let quick = cues(false, Medium::of("psx"), false);
        assert_eq!((hit(&quick[0]), quick[0].db), (cabinet::IN_AT, ui_sound::QUICK_DB));
    }

    #[test]
    fn a_phone_keeps_every_rig_and_its_medium_on_screen_whatever_way_it_is_held() {
        for system in ["psx", "ps2", "gamecube", "snes", "snes-us", "gba", "gb", "n64", "megadrive", "dreamcast", "saturn", "xbox", "nds", "wii"] {
            let game = if system == "snes-us" { Game::SnesUs } else { Game::of(system, "x.rom") };
            let system = if system == "snes-us" { "snes" } else { system };
            let f = ir::files(system, game).expect(system);
            let b = crate::system_art::decode(&f.base).unwrap();
            let base = (b.w as i32, b.h as i32);
            // The X300's screen, held both ways.
            for (w, h) in [(1216, 2500), (2500, 1120)] {
                let n = phone_blocks(Some(&f.rig), base, (w, h));
                let k = n as f32 / ir::SCALE;
                assert!(n * base.0 <= w, "{system} {n}: wider than {w}");
                let floor = phone_floor(Some(&f.rig), base, n, h, k);
                let top = floor - (10.0 * k) as i32 - base.1 * n;
                let bottom = top + base.1 * n;
                assert!(top >= 0 && bottom <= h, "{system} {n} on {w}x{h}: {top}..{bottom}");
                let s = f.rig.seat;
                let hover_top = top as f32 + (s.hover.1 - s.v.0.hypot(s.v.1)) * n as f32;
                assert!(hover_top >= 0.0, "{system} {n} on {w}x{h}: the medium hangs off the top ({hover_top})");
                let x0 = (w - base.0 * n) / 2;
                let reach = s.u.0.hypot(s.u.1) * n as f32;
                let (left, right) = (x0 as f32 + s.hover.0 * n as f32 - reach, x0 as f32 + s.hover.0 * n as f32 + reach);
                assert!(left >= 0.0 && right <= w as f32, "{system} {n} on {w}x{h}: the medium hangs off the side {left}..{right}");
            }
        }
    }

    #[test]
    fn a_bigger_phone_only_scales_the_handhelds_picture() {
        // At the handhelds' 2 the blocks are the art's old doubling.
        let p = Pic { w: 2, h: 1, px: vec![1, 2] };
        assert_eq!(enlarged(p, 2).px, vec![1, 1, 2, 2, 1, 1, 2, 2]);
    }
}

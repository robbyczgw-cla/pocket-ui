//! Part of the pocket-ui binary: the case stickers (`pocket_ui::case_stickers`) on the pictures of the Cabinet, the
//! Shelf look and the game box. A front or spine with stickers is made where the bare one is made (once per case,
//! size and set of stickers: the stickers are part of `app_cabinet`'s `CaseKey`), never per frame. What is inside the
//! case takes its label from the front without them ([`App::bare_front`]): the stickers stay on the case.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::cabinet_art::Pic;
use pocket_ui::case_stickers::{self, CaseStickers, Kind, Rect};
use pocket_ui::media::{self, Medium};
use pocket_ui::sticker_press::{self, Pose};

use super::App;

/// Fronts with stickers as made last, each with the same front without them. Their lifetime follows the bounded
/// front caches and the active box/launch animation, so a front can never outlive its undecorated source.
#[derive(Default)]
pub(crate) struct BareFronts(Vec<(Rc<Pic>, Rc<Pic>, Vec<Rect>)>);

impl BareFronts {
    pub(crate) fn prune(&mut self) {
        self.0.retain(|(stuck, _, _)| Rc::strong_count(stuck) > 1);
    }

    pub(crate) fn remember(&mut self, stuck: &Rc<Pic>, bare: Pic) {
        self.remember_keeping(stuck, bare, Vec::new());
    }

    /// [`remember`], and what the stickers of that front keep clear of (a title card's printed name and console).
    pub(crate) fn remember_keeping(&mut self, stuck: &Rc<Pic>, bare: Pic, keep: Vec<Rect>) {
        self.prune();
        self.0.push((stuck.clone(), Rc::new(bare), keep));
    }

    pub(crate) fn bare(&self, front: &Rc<Pic>) -> Rc<Pic> {
        self.0.iter().find(|(s, _, _)| Rc::ptr_eq(s, front)).map_or_else(|| front.clone(), |(_, b, _)| b.clone())
    }

    /// What the stickers of `front` keep clear of: empty for a front with a cover, or one this has not made.
    pub(crate) fn keep(&self, front: &Rc<Pic>) -> Vec<Rect> {
        self.0.iter().find(|(s, _, _)| Rc::ptr_eq(s, front)).map_or_else(Vec::new, |(_, _, k)| k.clone())
    }
}

/// The same number for a game every time (FNV-1a of its file): each sticker's tilt and place.
pub(crate) fn seed(rom: &Path) -> u64 {
    rom.as_os_str().as_encoded_bytes().iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// What changed on the case while the Stickers sheet was open, landing (put on) or lifting (taken off) in the game box
/// when the sheet closes (`sticker_press`), where the case is in full view. Ended by any button.
pub(crate) struct Press {
    at: Instant,
    /// Each sticker that is on now and was not (it lands) or was and is not (it lifts), and the game's stickers with it
    /// on: where it goes, whichever way it moves.
    changes: Vec<Change>,
    rom: PathBuf,
    /// The pictures, made once for the front they were made for.
    made: Option<Made>,
}

#[derive(Clone, Copy)]
struct Change {
    kind: Kind,
    on: bool,
    with: CaseStickers,
}

struct Made {
    /// The case's own front (the key: another one, another size, makes these again).
    front: Rc<Pic>,
    /// What the box shows under the stickers: the front without those that land, the front as it is for those that lift.
    base: Rc<Pic>,
    /// Each sticker's turned picture, its top left on the front, the side its tilt leans to, and whether it lands (else lifts).
    stickers: Vec<(Rc<Pic>, (i32, i32), f32, bool)>,
}

/// What the game box paints for a press at one moment: the front under the stickers and the stickers over it.
pub(crate) struct Moving {
    pub base: Rc<Pic>,
    pub stickers: Vec<(Rc<Pic>, (i32, i32), Pose)>,
}

impl App {
    /// The stickers on a game's case (none for what is not a game: a system, Curator's boxes), none on the pictures
    /// with Cozy extras → Stickers on cases off; the card's Stickers sheet still has them (`library.case`).
    pub(crate) fn case_stickers_of(&self, rom: &Path) -> CaseStickers {
        if rom.as_os_str().is_empty() || !pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::Stickers) {
            return CaseStickers::default();
        }
        self.library.case(rom)
    }

    /// `front` (`media::front` at `k`) with the game's stickers on its cover; the bare one is kept for [`bare_front`].
    /// `title` (as printed), `cover` and `console` are what `media::front` was given: a front with no cover is the printed
    /// title card, and the stickers keep off its name.
    pub(crate) fn stuck_front(&mut self, front: Pic, m: Medium, k: f32, rom: &Path, case: CaseStickers, (title, cover, console): (&str, bool, Option<&pocket_ui::system_art::Sprite>)) -> Rc<Pic> {
        if case.is_empty() {
            return Rc::new(front);
        }
        let mut p = front.clone();
        let label = media::label_rect(m, k);
        let keep = case_stickers::kept_clear(&mut self.text, &case, label, title, cover, console);
        case_stickers::on_front_around(&mut self.text, &mut p, &case, label, seed(rom), &keep);
        let stuck = Rc::new(p);
        // The press in the game box (`sticker_moving`) puts its stickers where these are: it needs the same places kept clear.
        self.cabinet.bare_fronts.remember_keeping(&stuck, front, keep);
        stuck
    }

    /// `spine` (`media::spine` at `k`) with the game's first sticker small at its top, above the title.
    pub(crate) fn stuck_spine(&mut self, mut spine: Pic, m: Medium, k: f32, rom: &Path, case: CaseStickers) -> Pic {
        if !case.is_empty() {
            let (_, label_y, _, _) = media::spine_label_rect(m, k);
            let w = spine.w;
            case_stickers::on_spine(&mut self.text, &mut spine, &case, (0, 0, w, label_y), seed(rom));
        }
        spine
    }

    /// The Stickers sheet closed: what is on the case now against what was when it opened (`before`). In the game box
    /// each sticker put on lands on the case and each one taken off lifts away, in full view now that the sheet is not
    /// over it. Only with the playful animations and the stickers shown on the case, and only where the case is on screen
    /// (the box); the pictures are made on the first frame, once.
    pub(crate) fn start_sticker_press(&mut self, rom: &Path, before: CaseStickers, now: CaseStickers) {
        self.cabinet.press = None;
        let boxed = self.card.as_ref().is_some_and(|o| o.card.boxed.is_some());
        if !boxed || !pocket_ui::playful::on() || !pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::Stickers) {
            return;
        }
        let changes: Vec<Change> = Kind::ALL
            .iter()
            .filter(|k| before.has(**k) != now.has(**k))
            .map(|k| Change { kind: *k, on: now.has(*k), with: if now.has(*k) { now } else { before } })
            .collect();
        if changes.is_empty() {
            return;
        }
        let at = pocket_ui::frame_clock::now();
        let landing = changes.iter().any(|c| c.on);
        let run = if landing { sticker_press::PRESS } else { sticker_press::PEEL };
        self.cabinet.press = Some(Press { at, changes, rom: rom.to_path_buf(), made: None });
        pocket_ui::motion::keep_alive(pocket_ui::motion::end_of(at, run));
        if landing {
            // The click when it touches: the Quick launch's seat sound, as quiet, timed to the touch.
            let touch = pocket_ui::motion::span(pocket_ui::motion::Kind::Move, Duration::from_secs_f32(sticker_press::FALL));
            let sound = pocket_ui::ui_sound::Sound::Insert;
            self.play_sound_after(sound, touch.saturating_sub(pocket_ui::ui_sound::hit_at(sound)), pocket_ui::ui_sound::QUICK_DB);
        }
    }

    /// For a snapshot: `POCKETUI_SNAPSHOT_STICKER_MS=<ms>` holds the clock `ms` (animation time) into the press the keys
    /// just started, so the next frame is that moment of it. True when it did.
    pub(crate) fn sticker_snapshot_clock(&mut self) -> bool {
        let Some(ms) = std::env::var("POCKETUI_SNAPSHOT_STICKER_MS").ok().and_then(|v| v.parse::<u64>().ok()) else { return false };
        let Some(press) = self.cabinet.press.as_ref() else { return false };
        let by = pocket_ui::motion::span(pocket_ui::motion::Kind::Move, Duration::from_millis(ms));
        pocket_ui::frame_clock::freeze(press.at + by);
        true
    }

    /// A button was pressed: whatever is moving sits (or is gone) at once.
    pub(crate) fn end_sticker_press(&mut self) {
        self.cabinet.press = None;
    }

    /// The stickers on their way at this frame over the case's `front` (made for the box at `case_h`), if there are any.
    pub(crate) fn sticker_moving(&mut self, m: Medium, front: &Rc<Pic>, case_h: i32) -> Option<Moving> {
        let press = self.cabinet.press.as_ref()?;
        let secs = pocket_ui::playful::secs(press.at, pocket_ui::frame_clock::now());
        let run = if press.changes.iter().any(|c| c.on) { sticker_press::PRESS } else { sticker_press::PEEL };
        if secs >= run.as_secs_f32() || !pocket_ui::playful::on() {
            self.cabinet.press = None;
            return None;
        }
        if press.made.as_ref().is_none_or(|made| !Rc::ptr_eq(&made.front, front)) {
            let (changes, rom) = (press.changes.clone(), press.rom.clone());
            // Exactly the k the box's front was made at (`box_front`), so the places are the case's own.
            let k = case_h as f32 / m.size().front_h.max(1) as f32;
            let label = media::label_rect(m, k);
            let keep = self.cabinet.bare_fronts.keep(front);
            let seed = seed(&rom);
            let mut stickers = Vec::new();
            for c in &changes {
                // A sticker that found no clear place on a title card is not on the case: nothing lands or lifts.
                let Some((pic, x, y)) = case_stickers::placed(&mut self.text, front.w, &c.with, label, seed, &keep, c.kind) else { continue };
                stickers.push((Rc::new(pic), (x, y), case_stickers::lean(seed, c.kind), c.on));
            }
            // The front without the stickers that land (the others as they will sit); one that lifts is already off it.
            let landing: Vec<Kind> = changes.iter().filter(|c| c.on).map(|c| c.kind).collect();
            let base = if landing.is_empty() {
                front.clone()
            } else {
                let mut p = (*self.bare_front(front)).clone();
                let now = changes.iter().find(|c| c.on).map_or_else(CaseStickers::default, |c| c.with);
                case_stickers::on_front_but(&mut self.text, &mut p, &now, label, seed, &keep, &landing);
                Rc::new(p)
            };
            self.cabinet.press.as_mut()?.made = Some(Made { front: front.clone(), base, stickers });
        }
        let press = self.cabinet.press.as_ref()?;
        let made = press.made.as_ref()?;
        let stickers = made
            .stickers
            .iter()
            .map(|(pic, at, side, on)| (pic.clone(), *at, if *on { sticker_press::press(secs, *side) } else { sticker_press::peel(secs, *side) }))
            .collect();
        Some(Moving { base: made.base.clone(), stickers })
    }

    /// The front without its stickers: what the cartridge or card inside takes its label from.
    pub(crate) fn bare_front(&self, front: &Rc<Pic>) -> Rc<Pic> {
        // A sealed front's film first (`app_shrink_wrap`), then its stickers.
        self.cabinet.bare_fronts.bare(&self.cabinet.wrap_fronts.bare(front))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_fronts_keep_their_unstickered_source_until_the_last_owner_drops() {
        let mut cache = BareFronts::default();
        let fronts: Vec<_> = (0..12).map(|i| {
            let stuck = Rc::new(Pic::new(4, 4, 0xffa0_0000 + i));
            cache.remember(&stuck, Pic::new(4, 4, 0xffb0_0000 + i));
            stuck
        }).collect();
        for (i, front) in fronts.iter().enumerate() {
            let bare = cache.bare(front);
            assert!(!Rc::ptr_eq(front, &bare), "a live front must retain its own bare source, including beyond eight");
            assert_eq!(bare.px, vec![0xffb0_0000 + i as u32; 16]);
        }
        let oldest_bare = Rc::downgrade(&cache.bare(&fronts[0]));
        let oldest_front = Rc::downgrade(&fronts[0]);
        drop(fronts);
        cache.prune();
        assert!(cache.0.is_empty(), "inactive fronts and their source pixels are released together");
        assert!(oldest_bare.upgrade().is_none());
        assert!(oldest_front.upgrade().is_none());
    }
}

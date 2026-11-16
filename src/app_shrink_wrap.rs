//! Part of the pocket-ui binary: Sealed new games (`pocket_ui::shrink_wrap`) on the App's pictures. Which games are
//! sealed ([`App::sealed`]), the film made into a case's front and spine where they are made (once per case: `sealed`
//! is part of `app_cabinet`'s `CaseKey`), Split's and the card's cover in its film ([`App::sealed_cover`]), and the
//! first start: the film is torn off for good ([`App::tear_wrap`]) and, with Cozy and Motion Full, rips off the case
//! before the launch's own scene ([`Tear`]).

use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::cabinet_art::Pic;
use pocket_ui::gfx::Canvas;
use pocket_ui::image::Image;
use pocket_ui::shrink_wrap;

use super::{App, Item};

/// Whether sealing is on at all in this run. A device: Cozy extras decides. Snapshots, the headless test driver and
/// unit tests paint every library unsealed (their libraries are made-up games nobody played, and their pictures are
/// about other things) unless `POCKETUI_SEALED=1`; `POCKETUI_SEALED=0` turns it off anywhere.
static BENCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Performance snapshots exercise the default Cozy extras, including new games in film.
pub(crate) fn bench_run() {
    BENCH.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn sealing_in_run(snapshot: bool, driver: bool) -> bool {
    static ENV: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    match *ENV.get_or_init(|| std::env::var("POCKETUI_SEALED").ok().map(|v| v != "0")) {
        Some(on) => on,
        None => BENCH.load(std::sync::atomic::Ordering::Relaxed) || !(snapshot || driver || cfg!(test)),
    }
}

/// Fronts in their film, each with the same front without it and the size of a template pixel it was made at (the
/// tear lays exactly that film over the bare front). Kept as long as the front is (as `app_case_stickers::BareFronts`).
#[derive(Default)]
pub(crate) struct SealedFronts(Vec<(Rc<Pic>, Rc<Pic>, f32)>);

impl SealedFronts {
    pub(crate) fn prune(&mut self) {
        self.0.retain(|(sealed, _, _)| Rc::strong_count(sealed) > 1);
    }

    fn remember(&mut self, sealed: &Rc<Pic>, bare: Rc<Pic>, unit: f32) {
        self.prune();
        self.0.push((sealed.clone(), bare, unit));
    }

    /// The front without its film (the front itself when it has none).
    pub(crate) fn bare(&self, front: &Rc<Pic>) -> Rc<Pic> {
        self.0.iter().find(|(s, _, _)| Rc::ptr_eq(s, front)).map_or_else(|| front.clone(), |(_, b, _)| b.clone())
    }

    fn unit(&self, front: &Rc<Pic>) -> Option<f32> {
        self.0.iter().find(|(s, _, _)| Rc::ptr_eq(s, front)).map(|(_, _, u)| *u)
    }
}

/// The film tearing off a case at its first start: its two halves, made once, flying off where the case stood.
pub(crate) struct Tear {
    at: Instant,
    /// The game: its other pictures (the spine left on the shelf) keep their film until the pieces are gone.
    rom: std::path::PathBuf,
    /// Where the case is painted when the launch's scene begins (its `from`): x, y, width, height.
    rect: (i32, i32, i32, i32),
    halves: (Pic, Pic),
}

impl Tear {
    /// The halves at this moment, over the scene's first frame (which shows the bare case where the sealed one stood).
    pub(crate) fn paint(&self, c: &mut Canvas, now: Instant) {
        let secs = now.saturating_duration_since(self.at).as_secs_f32();
        if secs >= shrink_wrap::TEAR.as_secs_f32() {
            return;
        }
        for (half, first) in [(&self.halves.0, true), (&self.halves.1, false)] {
            shrink_wrap::paint_piece(c, half, shrink_wrap::piece(secs, first), self.rect);
        }
    }

    pub(crate) fn moving(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) < shrink_wrap::TEAR
    }

    /// `rom`'s film is still coming off here.
    pub(crate) fn tearing(&self, rom: &Path, now: Instant) -> bool {
        self.rom == rom && self.moving(now)
    }
}

/// The same number for a game every time: where its film's glare bands sit (`app_case_stickers::seed`).
fn seed(rom: &Path) -> u64 {
    super::app_case_stickers::seed(rom)
}

impl App {
    /// `rom` starts (any look, any way it starts): when it is sealed, its film is off for good from now, saved at once on
    /// the card writer's thread. True when it was sealed.
    pub(crate) fn unseal(&mut self, rom: &Path) -> bool {
        if !self.sealed(rom) {
            return false;
        }
        if let Err(e) = self.library.set_unwrapped_soon(rom, |r| {
            if let Err(e) = r {
                eprintln!("sealed new games: library.json: {e}");
            }
        }) {
            eprintln!("sealed new games: library.json: {e}");
        }
        true
    }

    /// A game never played, still in the shop's film: Cozy extras → Sealed new games on, a game (not a system or a row),
    /// no recorded use, save/state/RTC/memory-card file or known progress (`App::play_evidence`).
    pub(crate) fn sealed(&self, rom: &Path) -> bool {
        // A game's file, never the made-up "ROM" of Curator's boxes or Just Play (`case_info`: "curator/…", "just-play"), which
        // has no play history and stood in film on the home shelf (Air emulator, 14.11.).
        if !rom.is_absolute() || !pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::ShrinkWrap) || !pocket_ui::cozy_extras::Extra::ShrinkWrap.built() {
            return false;
        }
        if !sealing_in_run(super::snapshot_run(), super::driven()) {
            return false;
        }
        // Torn open just now: the rest of the case keeps its film while the pieces fly (nothing jumps at A).
        if self.cabinet.insert.as_ref().is_some_and(|i| i.tearing(rom, pocket_ui::frame_clock::now())) {
            return true;
        }
        !self.play_evidence(rom).ever_played()
    }

    /// A spine `k` times its shelf size, in its film when `sealed`.
    pub(crate) fn sealed_spine(&mut self, mut spine: Pic, k: f32, rom: &Path, sealed: bool) -> Pic {
        if sealed {
            shrink_wrap::wrap(&mut spine, pocket_ui::media::shelf_scale() * k, seed(rom), shrink_wrap::Glare::Spine);
        }
        spine
    }

    /// A front `k` times its shelf size, in its film when `sealed`; the bare one is kept for the tear and the medium's label
    /// (`bare_front`).
    pub(crate) fn sealed_front(&mut self, front: Rc<Pic>, k: f32, rom: &Path, sealed: bool) -> Rc<Pic> {
        if !sealed {
            return front;
        }
        let unit = pocket_ui::media::shelf_scale() * k;
        let mut p = (*front).clone();
        shrink_wrap::wrap(&mut p, unit, seed(rom), shrink_wrap::Glare::Front);
        let wrapped = Rc::new(p);
        self.cabinet.wrap_fronts.remember(&wrapped, front, unit);
        wrapped
    }

    /// The box art of a game `w` x `h` (`cover`), in its film while it is sealed: Split's picture and the card's cover.
    /// Made once per picture and kept while it is shown.
    pub(crate) fn sealed_cover(&mut self, item: Item, w: i32, h: i32) -> Option<Rc<Image>> {
        let img = self.cover(item, w, h)?;
        let Some((_, rom)) = self.item_rom(item) else { return Some(img) };
        if !self.sealed(&rom) {
            return Some(img);
        }
        if let Some((_, sealed)) = self.sealed_covers.iter().find(|(src, _)| Rc::ptr_eq(src, &img)) {
            return Some(sealed.clone());
        }
        // A cover is a printed sheet, not a template: the film's grain as on a case seen at about this size.
        let unit = (h.max(1) as f32 / 120.0).max(1.0);
        let sealed = Rc::new(shrink_wrap::wrap_image(&img, unit, seed(&rom)));
        self.sealed_covers.retain(|(src, _)| Rc::strong_count(src) > 2);
        self.sealed_covers.truncate(3);
        self.sealed_covers.insert(0, (img, sealed.clone()));
        Some(sealed)
    }

    /// The game `rom` starts: a sealed one is unwrapped for good (saved at once, whatever comes of the start), and the
    /// launch's case (`cabinet.insert`, when there is one) tears its film off first with Cozy and Motion Full. Returns how
    /// long the launch's own scene waits for the tear (zero without one). `at`: when A started it.
    pub(crate) fn tear_wrap(&mut self, rom: &Path, cozy: bool, at: Instant) -> Duration {
        if !self.unseal(rom) {
            return Duration::ZERO;
        }
        let full = pocket_ui::motion::level() == pocket_ui::motion::Motion::Full;
        let Some(ins) = self.cabinet.insert.as_ref().filter(|_| cozy && full) else { return Duration::ZERO };
        let front = ins.scene_front();
        let Some(unit) = self.cabinet.wrap_fronts.unit(&front) else { return Duration::ZERO };
        let bare = self.cabinet.wrap_fronts.bare(&front);
        let t0 = Instant::now();
        let film = shrink_wrap::layer(&bare, unit, seed(rom), shrink_wrap::Glare::Front);
        let halves = shrink_wrap::halves(&film, seed(rom));
        self.cabinet.made("film halves", t0);
        let lead = shrink_wrap::LEAD;
        let Some(ins) = self.cabinet.insert.as_mut() else { return Duration::ZERO };
        let rect = ins.scene_from();
        ins.unwrap(bare, at, lead, Tear { at, rom: rom.to_path_buf(), rect, halves });
        // The rip's crack on the rip (`shrink_wrap::RIP` is the sound's own `hit_at`): it starts with the tear.
        self.play_sound_after(pocket_ui::ui_sound::Sound::WrapTear, Duration::ZERO, 0.0);
        lead
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_up_libraries_of_snapshots_and_tests_stay_unsealed_unless_asked() {
        // `POCKETUI_SEALED` is not set under `cargo test`.
        assert!(std::env::var_os("POCKETUI_SEALED").is_none());
        assert!(!sealing_in_run(true, false), "a snapshot");
        assert!(!sealing_in_run(false, true), "the test driver");
        assert!(!sealing_in_run(false, false), "a unit test");
    }

    #[test]
    fn a_sealed_front_keeps_its_bare_front_while_it_lives() {
        let mut s = SealedFronts::default();
        let bare = Rc::new(Pic::new(4, 4, 0xff10_2030));
        let sealed = Rc::new(Pic::new(4, 4, 0xff80_9098));
        s.remember(&sealed, bare.clone(), 2.0);
        assert!(Rc::ptr_eq(&s.bare(&sealed), &bare));
        assert_eq!(s.unit(&sealed), Some(2.0));
        let other = Rc::new(Pic::new(4, 4, 0));
        assert!(Rc::ptr_eq(&s.bare(&other), &other), "a front without film is its own bare front");
        let weak = Rc::downgrade(&bare);
        drop((bare, sealed));
        s.prune();
        assert!(weak.upgrade().is_none(), "released with its front");
    }
}

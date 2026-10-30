//! Part of the pocket-ui binary: the case stickers (`pocket_ui::case_stickers`) on the pictures of the Cabinet, the
//! Shelf look and the game box. A front or spine with stickers is made where the bare one is made (once per case,
//! size and set of stickers: the stickers are part of `app_cabinet`'s `CaseKey`), never per frame. What is inside the
//! case takes its label from the front without them ([`App::bare_front`]): the stickers stay on the case.

use std::path::Path;
use std::rc::Rc;

use pocket_ui::cabinet_art::Pic;
use pocket_ui::case_stickers::{self, CaseStickers};
use pocket_ui::media::{self, Medium};

use super::App;

/// Fronts with stickers as made last, each with the same front without them. Their lifetime follows the bounded
/// front caches and the active box/launch animation, so a front can never outlive its undecorated source.
#[derive(Default)]
pub(crate) struct BareFronts(Vec<(Rc<Pic>, Rc<Pic>)>);

impl BareFronts {
    pub(crate) fn prune(&mut self) {
        self.0.retain(|(stuck, _)| Rc::strong_count(stuck) > 1);
    }

    pub(crate) fn remember(&mut self, stuck: &Rc<Pic>, bare: Pic) {
        self.prune();
        self.0.push((stuck.clone(), Rc::new(bare)));
    }

    pub(crate) fn bare(&self, front: &Rc<Pic>) -> Rc<Pic> {
        self.0.iter().find(|(s, _)| Rc::ptr_eq(s, front)).map_or_else(|| front.clone(), |(_, b)| b.clone())
    }
}

/// The same number for a game every time (FNV-1a of its file): each sticker's tilt and place.
fn seed(rom: &Path) -> u64 {
    rom.as_os_str().as_encoded_bytes().iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

impl App {
    /// The stickers on a game's case (none for what is not a game: a system, Curator's boxes).
    pub(crate) fn case_stickers_of(&self, rom: &Path) -> CaseStickers {
        if rom.as_os_str().is_empty() {
            return CaseStickers::default();
        }
        self.library.case(rom)
    }

    /// `front` (`media::front` at `k`) with the game's stickers on its cover; the bare one is kept for [`bare_front`].
    pub(crate) fn stuck_front(&mut self, front: Pic, m: Medium, k: f32, rom: &Path, case: CaseStickers) -> Rc<Pic> {
        if case.is_empty() {
            return Rc::new(front);
        }
        let mut p = front.clone();
        case_stickers::on_front(&mut self.text, &mut p, &case, media::label_rect(m, k), seed(rom));
        let stuck = Rc::new(p);
        self.cabinet.bare_fronts.remember(&stuck, front);
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

    /// The front without its stickers: what the cartridge or card inside takes its label from.
    pub(crate) fn bare_front(&self, front: &Rc<Pic>) -> Rc<Pic> {
        self.cabinet.bare_fronts.bare(front)
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

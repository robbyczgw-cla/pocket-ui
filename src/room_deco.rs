//! Decorations that grow with your play, on the Shelf and the Cabinet (Settings → Look → Cozy extras): milestones
//! from what the library knows (`library_meta`: time played a game, its status, its case stickers) turn into things on
//! the shelves. The plant grows in four stages with the hours played in all; a trophy stands there after the first
//! completed game (the "Completed" status or sticker); a framed "100 %" after the first "100 %" sticker; a small
//! figurine for every system played for [`FIGURE_HOURS`] hours or more, on that system's own shelf.
//!
//! Pure: [`earned`] reads totals, [`place`] fits pictures into the free part of a shelf (never over a case, never cut
//! at the shelf's end). The pictures are PixelLab pixel art (`assets/media/props`, prompts in `pixellab-assets.json`);
//! the app decodes them once (`media::Prop`).

use crate::library_meta::{GameMeta, Status};

/// Hours in all for the plant's stages 1, 2 and 3 (stage 0 is the sprout it starts as).
pub const PLANT_HOURS: [u64; 3] = [2, 15, 60];
/// Hours on one system for its figurine.
pub const FIGURE_HOURS: u64 = 10;

/// The six figurines; a system gets one by its name (the same one every time).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Figure {
    Robot,
    Knight,
    Dragon,
    Cat,
    Rocket,
    Owl,
}

impl Figure {
    pub const ALL: [Figure; 6] = [Figure::Robot, Figure::Knight, Figure::Dragon, Figure::Cat, Figure::Rocket, Figure::Owl];

    /// The figurine of `system` (its folder name).
    pub fn of(system: &str) -> Figure {
        let h = system.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
        Figure::ALL[(h.wrapping_mul(2654435761) >> 13) as usize % Figure::ALL.len()]
    }
}

/// What the player has earned so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Earned {
    /// 0 sprout, 1 small, 2 grown (the plant as it always was), 3 big with flowers.
    pub plant: u8,
    pub trophy: bool,
    pub hundred: bool,
    /// The systems (folder names) with a figurine, sorted.
    pub figures: Vec<String>,
}

impl Earned {
    /// Everything at once: what a player with a long history sees (the look step's previews, snapshots).
    pub fn all(systems: &[&str]) -> Earned {
        Earned { plant: 3, trophy: true, hundred: true, figures: systems.iter().map(|s| s.to_string()).collect() }
    }

    /// For the caches of shelf pictures.
    pub fn key(&self) -> u64 {
        let mut h = (self.plant as u64) << 2 | (self.trophy as u64) << 1 | self.hundred as u64;
        for s in &self.figures {
            h = s.bytes().fold(h.wrapping_mul(1_000_003), |h, b| h.wrapping_mul(31) ^ b as u64);
        }
        h
    }

    pub fn figure(&self, system: &str) -> Option<Figure> {
        self.figures.binary_search_by(|s| s.as_str().cmp(system)).ok().map(|_| Figure::of(system))
    }
}

/// The system a ROM path is under: the folder after `roms/` (`/storage/roms/snes/Game.sfc` is "snes"), else the ROM's
/// own folder.
pub fn system_of(path: &str) -> &str {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let dirs = &parts[..parts.len().saturating_sub(1)];
    dirs.iter().position(|p| *p == "roms").and_then(|i| dirs.get(i + 1)).or(dirs.last()).copied().unwrap_or("")
}

/// The milestones reached by the games in `games` (path, what the library keeps about it).
pub fn earned<'a>(games: impl IntoIterator<Item = (&'a str, &'a GameMeta)>) -> Earned {
    let mut total = 0u64;
    let mut per: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    let mut e = Earned::default();
    for (path, g) in games {
        total += g.secs;
        if g.secs > 0 {
            *per.entry(system_of(path)).or_default() += g.secs;
        }
        e.trophy |= g.status == Status::Completed || g.case.has(crate::case_stickers::Kind::Completed);
        e.hundred |= g.case.has(crate::case_stickers::Kind::Hundred);
    }
    e.plant = PLANT_HOURS.iter().filter(|h| total >= **h * 3600).count() as u8;
    e.figures = per.into_iter().filter(|(s, secs)| !s.is_empty() && *secs >= FIGURE_HOURS * 3600).map(|(s, _)| s.to_string()).collect();
    e
}

/// Fits pictures `widths` (in order of importance) into the free part of a shelf from `from` to `to`: each `gap`
/// after the one before, the first `gap` after `from`. One that doesn't fit whole is left out and the next tried, so
/// nothing is ever cut at the shelf's end. The x of each one placed, by its index in `widths`.
pub fn place(from: i32, to: i32, gap: i32, widths: &[i32]) -> Vec<(usize, i32)> {
    let mut x = from + gap;
    let mut out = Vec::new();
    for (i, &w) in widths.iter().enumerate() {
        if w > 0 && x + w <= to {
            out.push((i, x));
            x += w + gap;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case_stickers::{CaseStickers, Kind};

    fn game(secs: u64) -> GameMeta {
        GameMeta { secs, ..Default::default() }
    }

    #[test]
    fn a_new_player_has_a_sprout_and_nothing_else() {
        assert_eq!(earned(std::iter::empty()), Earned::default());
        let g = game(600);
        let e = earned([("/storage/roms/snes/A.sfc", &g)]);
        assert_eq!((e.plant, e.trophy, e.hundred, e.figures.len()), (0, false, false, 0));
    }

    #[test]
    fn the_plant_grows_with_the_hours_and_systems_get_figurines() {
        let (a, b, c) = (game(9 * 3600), game(2 * 3600), game(6 * 3600));
        let e = earned([("/storage/roms/snes/A.sfc", &a), ("/storage/roms/snes/B.sfc", &b), ("/storage/roms/gba/C.gba", &c)]);
        assert_eq!(e.plant, 2, "17 h in all");
        assert_eq!(e.figures, vec!["snes".to_string()], "11 h on the SNES, 6 on the GBA");
        assert_eq!(e.figure("snes"), Some(Figure::of("snes")));
        assert_eq!(e.figure("gba"), None);
        let big = game(70 * 3600);
        assert_eq!(earned([("/storage/roms/psx/D.chd", &big)]).plant, 3);
    }

    #[test]
    fn completed_and_hundred_come_from_status_and_stickers() {
        let done = GameMeta { status: Status::Completed, ..Default::default() };
        assert!(earned([("/r/x/a", &done)]).trophy);
        let mut case = CaseStickers::default();
        case.set(Kind::Hundred, true, 2026);
        let full = GameMeta { case, ..Default::default() };
        let e = earned([("/r/x/a", &full)]);
        assert!(e.hundred && !e.trophy);
        let mut case = CaseStickers::default();
        case.set(Kind::Completed, true, 2026);
        assert!(earned([("/r/x/a", &GameMeta { case, ..Default::default() })]).trophy);
    }

    #[test]
    fn systems_from_paths() {
        assert_eq!(system_of("/storage/roms/snes/Game.sfc"), "snes");
        assert_eq!(system_of("/storage/roms/psx/Game/disc1.chd"), "psx");
        assert_eq!(system_of("/media/card/n64/Game.z64"), "n64");
        assert_eq!(system_of("Game.z64"), "");
    }

    #[test]
    fn placing_never_overlaps_or_cuts() {
        let got = place(100, 300, 10, &[50, 200, 40, 60]);
        assert_eq!(got, vec![(0, 110), (2, 170), (3, 220)]);
        let got = place(100, 150, 10, &[50]);
        assert!(got.is_empty(), "60 would end at 160");
        for (i, x) in place(0, 500, 12, &[30, 30, 30]) {
            assert!(x >= 12 && x + 30 <= 500, "{i}");
        }
    }

    #[test]
    fn keys_change_with_milestones_and_figures_spread() {
        let a = Earned::default();
        let b = Earned { trophy: true, ..Default::default() };
        let c = Earned { figures: vec!["snes".into()], ..Default::default() };
        assert_ne!(a.key(), b.key());
        assert_ne!(a.key(), c.key());
        let picked: std::collections::HashSet<Figure> =
            ["nes", "snes", "gb", "gba", "n64", "psx", "megadrive", "mastersystem", "gamegear", "pcengine", "ps2", "psp"].iter().map(|s| Figure::of(s)).collect();
        assert!(picked.len() >= 4, "{picked:?}");
    }
}

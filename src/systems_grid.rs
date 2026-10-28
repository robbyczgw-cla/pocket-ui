//! The systems grid of "Add your games" (first start and Settings > Add games): a tile per system with its
//! console, its ROM folder and how many games it has. The popular systems come first, then any other system that
//! has games, then "All systems", which opens every system ROCKNIX can start (es_systems.cfg). The selected tile's
//! file types and BIOS needs show under the grid.

use std::collections::HashMap;
use std::path::Path;

use crate::library::SysInfo;
use crate::pad::Button;

/// Shown first, in this order (ROCKNIX ids): the systems most people bring games for.
pub const POPULAR: [&str; 13] = ["gb", "gbc", "gba", "nes", "snes", "megadrive", "psx", "n64", "psp", "nds", "ps2", "gamecube", "dreamcast"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    /// Index into `Grid::all`.
    System(usize),
    /// "All systems" (or "Fewer systems" while open).
    More,
}

pub struct Grid {
    /// Every system pocket-ui can start, from es_systems.cfg.
    pub all: Vec<SysInfo>,
    /// Games per system id.
    pub counts: HashMap<String, usize>,
    pub expanded: bool,
    pub sel: usize,
    pub cols: usize,
}

impl Grid {
    pub fn new(all: Vec<SysInfo>, counts: HashMap<String, usize>, cols: usize) -> Self {
        Grid { all, counts, expanded: false, sel: 0, cols: cols.max(1) }
    }

    /// The tiles in order: popular, other systems with games, "All systems", and when open everything else.
    pub fn tiles(&self) -> Vec<Tile> {
        let index = |id: &str| self.all.iter().position(|s| s.id == id);
        let mut first: Vec<usize> = POPULAR.iter().filter_map(|id| index(id)).collect();
        // `all` is sorted by name already.
        let with_games: Vec<usize> = (0..self.all.len()).filter(|i| !first.contains(i) && self.games(&self.all[*i].id) > 0).collect();
        first.extend(with_games);
        let mut tiles: Vec<Tile> = first.iter().map(|i| Tile::System(*i)).collect();
        tiles.push(Tile::More);
        if self.expanded {
            tiles.extend((0..self.all.len()).filter(|i| !first.contains(i)).map(Tile::System));
        }
        tiles
    }

    /// Moves the selection, or opens and closes "All systems" (A). True when the button was used.
    pub fn press(&mut self, b: Button) -> bool {
        let n = self.tiles().len();
        let last = n.saturating_sub(1);
        let cols = self.cols;
        match b {
            Button::Left => self.sel = self.sel.saturating_sub(1),
            Button::Right => self.sel = (self.sel + 1).min(last),
            Button::Up => self.sel = self.sel.checked_sub(cols).unwrap_or(self.sel),
            Button::Down => {
                // Straight down, or the last tile of a shorter last row; nothing below the last row.
                if self.sel + cols <= last {
                    self.sel += cols;
                } else if self.sel / cols < last / cols {
                    self.sel = last;
                }
            }
            Button::L1 => self.sel = self.sel.saturating_sub(cols * 3),
            Button::R1 => self.sel = (self.sel + cols * 3).min(last),
            Button::East if self.selected() == Some(Tile::More) => self.expanded = !self.expanded,
            _ => return false,
        }
        true
    }

    pub fn selected(&self) -> Option<Tile> {
        self.tiles().get(self.sel).copied()
    }

    pub fn games(&self, id: &str) -> usize {
        self.counts.get(id).copied().unwrap_or(0)
    }
}

/// The name on a tile: the one people use where es_systems' full name is too long for it.
pub fn short_name(info: &SysInfo) -> String {
    match info.id.as_str() {
        "nes" => "NES".into(),
        "psx" => "PlayStation".into(),
        "psp" => "PSP".into(),
        "megadrive" => "Mega Drive".into(),
        _ => info.name.clone(),
    }
}

/// What a system needs besides its games.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Bios {
    /// pocket-ui knows the system needs none.
    NotNeeded,
    /// Required files (any one of each group, `hostsys::Bios`): the message naming what is missing, or None
    /// when everything is in roms/bios.
    Needed { missing: Option<String>, what: String },
    /// Not known to pocket-ui: the emulator ROCKNIX uses may want one.
    Unknown,
}

/// The selected tile's details: its file types (".gba .zip .7z") and BIOS needs, looked up in `bios_dir`.
pub fn details(info: &SysInfo, bios_dir: &Path) -> (String, Bios) {
    let exts = info.extensions.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(" ");
    (exts, bios(&info.id, bios_dir))
}

/// The handhelds pocket-core started with (main.rs `HOST_CORES`): their cores need no BIOS file.
const OWN_CORES_WITHOUT_BIOS: [&str; 5] = ["gb", "gbc", "gba", "nes", "snes"];

fn bios(id: &str, bios_dir: &Path) -> Bios {
    if let Some(sys) = crate::hostsys::find(id) {
        // "None needed" only for systems played through on the device; an unchecked one may still want one
        // (Neo Geo's neogeo.zip, say).
        if sys.bios.is_empty() {
            return if sys.verified { Bios::NotNeeded } else { Bios::Unknown };
        }
        let what = sys
            .bios
            .iter()
            .map(|b| {
                let (last, rest) = b.files.split_last().map_or(("", &[][..]), |(l, r)| (*l, r));
                let files = if rest.is_empty() { last.to_string() } else { format!("{} or {last}", rest.join(", ")) };
                format!("{}: {files}", b.label)
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Bios::Needed { missing: sys.missing_bios(bios_dir), what };
    }
    if OWN_CORES_WITHOUT_BIOS.contains(&id) {
        return Bios::NotNeeded;
    }
    // ROCKNIX's PlayStation 2 emulator does not start without a BIOS dump; any name works (SCPH-…, ps2-…).
    if id == "ps2" {
        let found = std::fs::read_dir(bios_dir).into_iter().flatten().flatten().any(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.ends_with(".bin") && (name.starts_with("scph") || name.starts_with("ps2"))
        });
        return Bios::Needed {
            missing: (!found).then(|| "PlayStation 2 needs a BIOS file in roms/bios: a dump of your own console's (.bin)".into()),
            what: "PlayStation 2: a PS2 BIOS file (.bin)".into(),
        };
    }
    Bios::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn info(id: &str, name: &str, exts: &[&str]) -> SysInfo {
        SysInfo {
            id: id.into(),
            name: name.into(),
            folder: format!("roms/{id}"),
            path: format!("/storage/roms/{id}").into(),
            extensions: exts.iter().map(|e| e.to_string()).collect(),
        }
    }

    /// es_systems in its own order (by name), with a few systems that are not popular.
    fn catalog() -> Vec<SysInfo> {
        [
            ("3do", "3DO"),
            ("dreamcast", "Dreamcast"),
            ("gb", "Game Boy"),
            ("gba", "Game Boy Advance"),
            ("gbc", "Game Boy Color"),
            ("gamecube", "GameCube"),
            ("megadrive", "Mega Drive"),
            ("msx", "MSX"),
            ("n64", "Nintendo 64"),
            ("nds", "Nintendo DS"),
            ("nes", "NES"),
            ("pcengine", "PC Engine"),
            ("psx", "PlayStation"),
            ("ps2", "PlayStation 2"),
            ("psp", "PSP"),
            ("snes", "Super Nintendo"),
            ("wonderswan", "WonderSwan"),
        ]
        .iter()
        .map(|(id, name)| info(id, name, &["zip"]))
        .collect()
    }

    fn ids(g: &Grid, tiles: &[Tile]) -> Vec<String> {
        tiles
            .iter()
            .map(|t| match t {
                Tile::System(i) => g.all[*i].id.clone(),
                Tile::More => "+".into(),
            })
            .collect()
    }

    #[test]
    fn popular_systems_come_first_in_their_order_then_others_with_games() {
        let counts = HashMap::from([("pcengine".to_string(), 4), ("gba".to_string(), 2)]);
        let g = Grid::new(catalog(), counts, 4);
        assert_eq!(
            ids(&g, &g.tiles()),
            ["gb", "gbc", "gba", "nes", "snes", "megadrive", "psx", "n64", "psp", "nds", "ps2", "gamecube", "dreamcast", "pcengine", "+"],
            "PC Engine has games, so it is there before All systems; 3DO, MSX and WonderSwan wait behind it"
        );
    }

    #[test]
    fn a_popular_system_rocknix_does_not_have_is_left_out() {
        let all: Vec<SysInfo> = catalog().into_iter().filter(|s| s.id != "gamecube").collect();
        let g = Grid::new(all, HashMap::new(), 4);
        assert!(!ids(&g, &g.tiles()).contains(&"gamecube".to_string()));
    }

    #[test]
    fn all_systems_opens_below_itself_and_closes_again() {
        let mut g = Grid::new(catalog(), HashMap::new(), 4);
        let closed = g.tiles();
        let more = closed.iter().position(|t| *t == Tile::More).unwrap();
        assert_eq!(more, closed.len() - 1);
        // To the tile: Down three rows of four, then Right to the 14th tile.
        g.press(Button::Down);
        g.press(Button::Down);
        g.press(Button::Down);
        g.press(Button::Right);
        assert_eq!(g.selected(), Some(Tile::More));
        assert!(g.press(Button::East));
        assert!(g.expanded);
        assert_eq!(g.selected(), Some(Tile::More), "the selection stays on the tile");
        let open = g.tiles();
        assert_eq!(open.len(), catalog().len() + 1, "every system once, and the tile");
        assert_eq!(ids(&g, &open[more + 1..]), ["3do", "msx", "pcengine", "wonderswan"], "the rest, by name");
        // Further down and back up to it, then closed again.
        g.press(Button::Right);
        assert_eq!(g.selected(), Some(Tile::System(0)), "3DO");
        g.press(Button::Left);
        assert!(g.press(Button::East));
        assert!(!g.expanded);
        assert_eq!(g.tiles(), closed);
        assert_eq!(g.selected(), Some(Tile::More));
    }

    #[test]
    fn the_selection_moves_by_rows_and_stays_in_the_grid() {
        let mut g = Grid::new(catalog(), HashMap::new(), 4);
        g.press(Button::Up);
        g.press(Button::Left);
        assert_eq!(g.sel, 0);
        g.press(Button::Down);
        assert_eq!(g.sel, 4);
        for _ in 0..10 {
            g.press(Button::Down);
        }
        assert_eq!(g.sel, 12, "14 tiles in rows of four: the last row starts at 12, Down stays there");
        // From the end of the row above, Down lands on the short last row's last tile.
        g.sel = 11;
        g.press(Button::Down);
        assert_eq!(g.sel, 13);
        g.press(Button::Right);
        assert_eq!(g.sel, g.tiles().len() - 1, "Right stops at the end");
        assert!(!g.press(Button::South), "B is not the grid's");
    }

    #[test]
    fn details_name_file_types_and_bios() {
        let bios = TestDir::new("grid-bios");

        let gba = info("gba", "Game Boy Advance", &["gba", "zip", "7z"]);
        assert_eq!(details(&gba, &bios), (".gba .zip .7z".to_string(), Bios::NotNeeded), "mGBA skips the BIOS");
        let md = info("megadrive", "Mega Drive", &["md", "bin"]);
        assert_eq!(details(&md, &bios).1, Bios::NotNeeded, "hostsys: no BIOS");

        let cd = info("segacd", "Sega CD", &["cue", "chd"]);
        let (exts, need) = details(&cd, &bios);
        assert_eq!(exts, ".cue .chd");
        assert_eq!(
            need,
            Bios::Needed {
                missing: Some("Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin".into()),
                what: "Sega CD: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin".into()
            },
            "hostsys::missing_bios names the files"
        );
        std::fs::write(bios.join("bios_CD_E.bin"), b"x").unwrap();
        assert!(matches!(details(&cd, &bios).1, Bios::Needed { missing: None, .. }), "one region's file is enough");

        let ps2 = info("ps2", "PlayStation 2", &["iso", "chd"]);
        assert!(matches!(details(&ps2, &bios).1, Bios::Needed { missing: Some(_), .. }), "PS2 never starts without one");
        std::fs::write(bios.join("SCPH-70012.bin"), b"x").unwrap();
        assert!(matches!(details(&ps2, &bios).1, Bios::Needed { missing: None, .. }));

        let msx = info("msx", "MSX", &["rom"]);
        assert_eq!(details(&msx, &bios).1, Bios::Unknown, "not one pocket-ui knows");
        let neogeo = info("neogeo", "Neo Geo", &["zip"]);
        assert_eq!(details(&neogeo, &bios).1, Bios::Unknown, "wired in hostsys but not played through: no promise");
    }
}

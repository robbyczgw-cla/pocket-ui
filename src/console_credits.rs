//! Credits & links for the console pages: `assets/data/console-credits.json`, compiled into the binary and keyed by
//! the same ids as `consoles` (ROCKNIX system ids, aliases through `consoles::canonical`).
//!
//! The file is built by `scripts/build-console-credits.py`, which checks every fact against its source (Wikidata, or
//! the console's English Wikipedia infobox and lead) and drops what it can't find there. This module shows the
//! values as they are and only works out the links: which related consoles and which consoles with the same chip
//! are in the player's library. See docs/DEVELOPING.md, "Console credits".

use std::sync::OnceLock;

use serde::Deserialize;

const DATA: &str = include_str!("../assets/data/console-credits.json");

/// The ROCKNIX system ids (es_systems.cfg of ROCKNIX 20261001). A family link may only point at one of these; the
/// build script keeps the same list (`ROCKNIX`), and the tests hold the file to it.
pub const ROCKNIX_SYSTEMS: &[&str] = &[
    "3do",
    "3ds",
    "amiga",
    "amigacd32",
    "amstradcpc",
    "arcade",
    "arduboy",
    "atari2600",
    "atari5200",
    "atari7800",
    "atari800",
    "atarijaguar",
    "atarilynx",
    "atarist",
    "atomiswave",
    "bbcmicro",
    "bk",
    "c128",
    "c16",
    "c64",
    "cdi",
    "channelf",
    "chip-8",
    "colecovision",
    "cps1",
    "cps2",
    "cps3",
    "daphne",
    "doom",
    "dreamcast",
    "easyrpg",
    "famicom",
    "fbn",
    "fds",
    "gameandwatch",
    "gamecube",
    "gamegear",
    "gb",
    "gba",
    "gbah",
    "gbav",
    "gbc",
    "gbch",
    "gbh",
    "genesis",
    "genh",
    "ggh",
    "gp32",
    "heroic",
    "idtech",
    "imageviewer",
    "intellivision",
    "ios",
    "j2me",
    "macintosh",
    "mame",
    "mastersystem",
    "megacd",
    "megadrive-japan",
    "megadrive",
    "megadriveh",
    "megaduck",
    "moonlight",
    "moto",
    "mplayer",
    "msx",
    "msx2",
    "music",
    "n64",
    "n64dd",
    "naomi",
    "nds",
    "ndsiware",
    "neocd",
    "neogeo",
    "nes",
    "nesh",
    "ngp",
    "ngpc",
    "odyssey2",
    "openbor",
    "palm",
    "pc-8800",
    "pc-9800",
    "pc",
    "pcengine",
    "pcenginecd",
    "pcfx",
    "pet",
    "pico-8",
    "pokemini",
    "ports",
    "ps2",
    "ps3",
    "psp",
    "pspockets",
    "psvita",
    "psx",
    "satellaview",
    "saturn",
    "scummvm",
    "scv",
    "sega32x",
    "segacd",
    "segamodel3",
    "sfc",
    "sg-1000",
    "snes",
    "snesh",
    "snesmsu1",
    "st-v",
    "steam",
    "sufami",
    "supergrafx",
    "supervision",
    "tg16",
    "tg16cd",
    "tic-80",
    "tools",
    "triforce",
    "uzebox",
    "vectrex",
    "vic20",
    "videopac",
    "vircon32",
    "virtualboy",
    "wasm4",
    "wii",
    "wiiu",
    "wiiware",
    "windows",
    "wonderswan",
    "wonderswancolor",
    "x1",
    "x68000",
    "xbox",
    "zmachine",
    "zx81",
    "zxspectrum",
];

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    license: Option<String>,
    systems: Vec<Credits>,
}

/// One console's credits as the file has them.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct Credits {
    pub id: String,
    pub qid: String,
    /// Two sentences written for pocket-ui from the facts below.
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub people: Vec<Person>,
    #[serde(default)]
    pub makers: Vec<Maker>,
    #[serde(default)]
    pub chips: Vec<Chip>,
    #[serde(default)]
    pub family: Vec<Relative>,
    #[serde(default)]
    pub best_selling: Vec<BestSeller>,
    /// Developer names as gamedb has them (Wikidata labels): the games of the library by these are "by the makers".
    #[serde(default)]
    pub game_makers: Vec<String>,
    pub sources: Sources,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Person {
    pub name: String,
    /// Our words for what the source says they did.
    pub role: String,
    pub source: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Maker {
    /// "Developer", "Manufacturer", "Prototype", "Co-developer".
    pub role: String,
    /// As the page shows it ("Nintendo R&D1").
    pub name: String,
    pub source: String,
    /// The Wikidata label ("Nintendo Research & Development 1"), where the maker has an item.
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Chip {
    /// "CPU", "Second CPU", "Graphics", "Sound" …
    pub role: String,
    pub name: String,
    pub source: String,
    /// The chip's Wikidata item: two consoles with the same one have the same chip.
    #[serde(default)]
    pub qid: Option<String>,
    #[serde(default)]
    pub maker: Option<String>,
    /// "made" (Wikidata's manufacturer) or "designed" (its developer).
    #[serde(default)]
    pub maker_by: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    Before,
    After,
    Model,
    Family,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Relative {
    pub relation: Relation,
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub qid: Option<String>,
    /// The ROCKNIX system that plays this console's games, when it is one of its own.
    #[serde(default)]
    pub system: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct BestSeller {
    pub title: String,
    #[serde(default)]
    pub figure: Option<String>,
    pub source: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Sources {
    pub wikidata: String,
    pub wikipedia: String,
}

fn file() -> &'static File {
    static FILE: OnceLock<File> = OnceLock::new();
    FILE.get_or_init(|| match serde_json::from_str(DATA) {
        Ok(f) => f,
        Err(e) => {
            // Checked by the tests; a broken file means no credits, never a crash.
            eprintln!("console-credits.json: {e}");
            File {
                license: None,
                systems: Vec::new(),
            }
        }
    })
}

pub fn all() -> &'static [Credits] {
    &file().systems
}

pub fn license() -> Option<&'static str> {
    file().license.as_deref()
}

/// The credits of a ROCKNIX system, aliases included.
pub fn for_system(system: &str) -> Option<&'static Credits> {
    let id = crate::consoles::canonical(system);
    all().iter().find(|c| c.id == id)
}

/// Other ROCKNIX folders for the same add-on that `consoles::ALIASES` doesn't have (the add-ons without a console page).
const SAME_FOLDER: &[(&str, &str)] = &[("tg16cd", "pcenginecd")];

/// Whether a library folder (a ROCKNIX system id) plays the games of `target`, a link's system: the same id, an
/// alias of the console pages ("genesis" for "megadrive") or another folder of the same add-on ("tg16cd").
pub fn folder_plays(folder: &str, target: &str) -> bool {
    let canon = |s: &str| -> String {
        let s = crate::consoles::canonical(s);
        SAME_FOLDER.iter().find(|(a, _)| *a == s).map_or(s, |(_, b)| b).to_string()
    };
    canon(folder) == canon(target)
}

impl Relation {
    /// The row label on the page.
    pub fn label(self) -> &'static str {
        match self {
            Relation::Before => "Before",
            Relation::After => "After",
            Relation::Model => "Models",
            Relation::Family => "Related",
        }
    }

    /// The word at the end of a link to it: "Game Boy Color · after".
    pub fn word(self) -> &'static str {
        match self {
            Relation::Before => "before",
            Relation::After => "after",
            Relation::Model => "a model",
            Relation::Family => "related",
        }
    }
}

impl Chip {
    /// "Sharp SM83 · made by Sharp", "R3000 · designed by MIPS Technologies", "Zilog Z80".
    pub fn text(&self) -> String {
        match (&self.maker, self.maker_by.as_deref()) {
            (Some(m), Some("designed")) => format!("{} · designed by {m}", self.name),
            (Some(m), _) => format!("{} · made by {m}", self.name),
            (None, _) => self.name.clone(),
        }
    }
}

impl BestSeller {
    /// "Pokémon Red, Blue, and Yellow, 46 million".
    pub fn text(&self) -> String {
        match &self.figure {
            Some(f) => format!("{}, {f}", self.title),
            None => self.title.clone(),
        }
    }
}

impl Credits {
    /// Where a fact's source is, as the page names it: "Wikidata" or "Wikipedia".
    pub fn source_name(source: &str) -> &'static str {
        if source.starts_with("wikidata:") {
            "Wikidata"
        } else {
            "Wikipedia"
        }
    }

    /// The other consoles of the file with one or more of this console's chips: (console id, the shared chips as
    /// indices into `chips`), in the order of the chips and then of the file. Only chips with a Wikidata item count:
    /// the same item is the same chip, a similar name is not.
    pub fn same_chips(&self) -> Vec<(&'static str, Vec<usize>)> {
        let mut out: Vec<(&'static str, Vec<usize>)> = Vec::new();
        for (i, chip) in self.chips.iter().enumerate() {
            let Some(q) = &chip.qid else { continue };
            for other in all() {
                if other.id == self.id || !other.chips.iter().any(|c| c.qid.as_ref() == Some(q)) {
                    continue;
                }
                match out.iter_mut().find(|(id, _)| *id == other.id) {
                    Some((_, chips)) => chips.push(i),
                    None => out.push((other.id.as_str(), vec![i])),
                }
            }
        }
        out
    }

    /// The names of the makers whose games are "by the makers" ("Nintendo R&D1" for "Nintendo Research & Development 1").
    pub fn makers_text(&self) -> String {
        let names: Vec<&str> = self
            .game_makers
            .iter()
            .map(|g| {
                self.makers
                    .iter()
                    .find(|m| m.label.as_deref() == Some(g.as_str()) || m.name == *g)
                    .map_or(g.as_str(), |m| m.name.as_str())
            })
            .collect();
        match names.as_slice() {
            [] => String::new(),
            [one] => one.to_string(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consoles;

    #[test]
    fn the_file_loads_and_has_credits_for_every_console_page() {
        let f: File = serde_json::from_str(DATA).expect("console-credits.json parses");
        assert_eq!(f.systems.len(), consoles::all().len());
        assert!(license().is_some_and(|l| l.contains("Wikidata") && l.contains("Wikipedia")));
        for c in consoles::all() {
            let cr = for_system(&c.id).unwrap_or_else(|| panic!("{}: no credits", c.id));
            assert!(cr.summary.is_some(), "{}: summary", c.id);
            assert!(!cr.makers.is_empty(), "{}: makers", c.id);
            assert!(!cr.chips.is_empty(), "{}: chips", c.id);
            assert!(cr.sources.wikidata.starts_with("https://www.wikidata.org/wiki/Q"), "{}", c.id);
            assert!(cr.sources.wikipedia.starts_with("https://en.wikipedia.org/wiki/"), "{}", c.id);
            assert!(cr.sources.wikidata.ends_with(&cr.qid), "{}", c.id);
        }
        assert_eq!(
            for_system("genesis").map(|c| c.id.as_str()),
            Some("megadrive"),
            "aliases as for the console page"
        );
        assert!(for_system("pcenginecd").is_none(), "an add-on without a sheet has no credits");
        assert_eq!(for_system("megacd").map(|c| c.id.as_str()), Some("segacd"));
    }

    #[test]
    fn every_fact_names_a_source_the_build_script_checks() {
        let ok = |s: &str| s == "lead" || s.starts_with("infobox:") || (s.starts_with("wikidata:") && s.len() > 9);
        for c in all() {
            let sources = c
                .people
                .iter()
                .map(|p| &p.source)
                .chain(c.makers.iter().map(|m| &m.source))
                .chain(c.chips.iter().map(|m| &m.source))
                .chain(c.family.iter().map(|m| &m.source))
                .chain(c.best_selling.iter().map(|m| &m.source));
            for s in sources {
                assert!(ok(s), "{}: source {s:?}", c.id);
            }
            for p in &c.people {
                assert!(!p.name.is_empty() && !p.role.is_empty(), "{}", c.id);
            }
        }
    }

    #[test]
    fn links_point_at_rocknix_systems_and_never_at_the_console_itself() {
        let mut ids: Vec<&str> = ROCKNIX_SYSTEMS.to_vec();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ROCKNIX_SYSTEMS.len(), "no id twice");
        for c in all() {
            assert!(consoles::for_system(&c.id).is_some(), "{}: a console of consoles.json", c.id);
            for r in &c.family {
                if let Some(s) = &r.system {
                    assert!(ROCKNIX_SYSTEMS.contains(&s.as_str()), "{}: {} -> {s} is no ROCKNIX system", c.id, r.name);
                    assert_ne!(consoles::canonical(s), c.id, "{}: {} links to itself", c.id, r.name);
                }
            }
            for (other, chips) in c.same_chips() {
                assert!(!chips.is_empty());
                assert!(for_system(other).is_some(), "{}: same chip as {other}, which has credits", c.id);
                assert_ne!(other, c.id);
            }
        }
    }

    #[test]
    fn family_links_to_consoles_with_a_page_are_their_page_ids() {
        // A relative that has a console page names that page's id, not an alias, so a link opens it directly.
        for c in all() {
            for r in c.family.iter().filter_map(|r| r.system.as_deref()) {
                assert_eq!(consoles::canonical(r), r, "{}: {r}", c.id);
            }
        }
        let gb = for_system("gb").unwrap();
        let after: Vec<&str> = gb
            .family
            .iter()
            .filter(|r| r.relation == Relation::After)
            .filter_map(|r| r.system.as_deref())
            .collect();
        assert_eq!(after, ["gbc"]);
    }

    #[test]
    fn the_same_chip_links_the_game_boys_and_the_z80_consoles() {
        let ids = |id: &str| for_system(id).unwrap().same_chips().into_iter().map(|(o, _)| o).collect::<Vec<_>>();
        assert_eq!(ids("gb"), ["gba", "gbc"], "Sharp SM83, in the file's order");
        assert!(
            ids("gamegear").contains(&"mastersystem") && ids("gamegear").contains(&"megadrive"),
            "{:?}",
            ids("gamegear")
        );
        assert!(ids("sg-1000").contains(&"gamegear"), "the SN76489: {:?}", ids("sg-1000"));
        assert!(ids("nds").contains(&"gba"), "ARM7TDMI");
        assert!(ids("n64").is_empty(), "nothing shares the VR4300 or the RCP");
        // A chip without an item never links, even when the names match.
        assert!(for_system("dreamcast").unwrap().same_chips().is_empty());
        // The Game Gear shares two chips with the Master System: one link, both named.
        let gg = for_system("gamegear").unwrap();
        let sms = gg.same_chips().into_iter().find(|(o, _)| *o == "mastersystem").unwrap().1;
        let names: Vec<&str> = sms.iter().map(|&i| gg.chips[i].name.as_str()).collect();
        assert_eq!(names, ["Zilog Z80", "Texas Instruments SN76489"]);
    }

    #[test]
    fn a_link_finds_the_folders_that_play_its_games() {
        assert!(folder_plays("gbc", "gbc"));
        assert!(folder_plays("genesis", "megadrive") && folder_plays("megadrive-japan", "megadrive"));
        assert!(folder_plays("megacd", "segacd") && folder_plays("segacd", "segacd"));
        assert!(folder_plays("tg16cd", "pcenginecd"));
        assert!(folder_plays("famicom", "nes"));
        assert!(!folder_plays("gbc", "gb") && !folder_plays("segacd", "megadrive") && !folder_plays("ports", "psx"));
    }

    #[test]
    fn game_makers_are_among_the_makers_and_read_by_their_short_names() {
        for c in all() {
            for g in &c.game_makers {
                assert!(c.makers.iter().any(|m| m.label.as_deref() == Some(g.as_str()) || m.name == *g), "{}: {g}", c.id);
            }
        }
        assert_eq!(for_system("gb").unwrap().makers_text(), "Nintendo R&D1");
        assert_eq!(for_system("pcengine").unwrap().makers_text(), "NEC and Hudson Soft");
        assert_eq!(for_system("psx").unwrap().makers_text(), "Sony Computer Entertainment");
    }

    #[test]
    fn texts_read_as_the_page_shows_them() {
        let gb = for_system("gb").unwrap();
        assert_eq!(gb.chips[0].text(), "Sharp SM83 · made by Sharp");
        assert_eq!(for_system("psx").unwrap().chips[0].text(), "R3000 · designed by MIPS Technologies");
        assert_eq!(for_system("gamegear").unwrap().chips[0].text(), "Zilog Z80");
        assert_eq!(gb.best_selling[0].text(), "Pokémon Red, Blue, and Yellow, 46 million");
        assert_eq!(
            for_system("pcengine").unwrap().best_selling[0].text(),
            "Bonk's Adventure",
            "no figure, none made up"
        );
        assert_eq!(Credits::source_name("wikidata:P880"), "Wikidata");
        assert_eq!(Credits::source_name("infobox:cpu"), "Wikipedia");
        for c in all() {
            for t in c.chips.iter().map(Chip::text).chain(c.best_selling.iter().map(BestSeller::text)) {
                let l = t.to_lowercase();
                assert!(!l.contains("null") && !l.contains("none") && !t.trim().is_empty(), "{}: {t}", c.id);
            }
        }
    }
}

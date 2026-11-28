//! Consoles that are one machine sold under regional names: Mega Drive and Genesis, SNES and Super Famicom, NES and
//! Famicom, PC Engine and TurboGrafx-16, their CD add-ons, Mega-CD and Sega CD. ROCKNIX and dArkOS give each name a
//! folder of its own (`megadrive`, `genesis`, `megadrive-japan`; `snes`, `sfc`; `nes`, `famicom`; `pcengine`, `tg16`,
//! dArkOS's `turbografx`; `pcenginecd`, `tg16cd`, `turbografxcd`; `segacd`, `megacd`), and pocket-ui showed two
//! shelves for one console. Since 0.8 they are one shelf (`library::load_with`), named as the player chooses under
//! Settings → Look → Cozy extras, like the GBA console (Alex, 25.11.2026).
//!
//! Nothing moves on the card: every game keeps its folder, and with it its system (`own_system`): the start
//! (`-P genesis`, the emulator chosen for that folder), the saves, states, covers (`covers/genesis/`), pictures and
//! achievements stay where they were. A merged shelf carries the other folders' facts as its parts
//! (`library::Part`); `learn` tells this module where they are, so a cover or a state of a game in `genesis/` is looked
//! for under `genesis`, whichever shelf id a caller passes.
//!
//! The name starts by the interface's language and the device's time zone: English in an American time zone gets the
//! American names (Genesis, SNES with the US console, TurboGrafx-16, NES, Sega CD), every other start the others (Mega
//! Drive, SNES, PC Engine, NES, Mega-CD). Famicom and Super Famicom only when chosen. The CD add-ons follow their
//! console's choice. The pictures stay the console's (no logos); the SNES's US choice puts every game into the US
//! console at the start (`snes_us_look`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// One name a pair can be shown by.
#[derive(Debug, PartialEq, Eq)]
pub struct Name {
    /// Saved in settings.json (`console_names`).
    pub id: &'static str,
    /// What the Cozy extras row shows.
    pub label: &'static str,
    /// A spine, a chip, a row with little room (`system_names::short`).
    pub short: &'static str,
    /// The system's page, its card and the game card (the shelf's full name).
    pub long: &'static str,
}

/// Consoles that are one shelf.
#[derive(Debug)]
pub struct Pair {
    /// Its key in settings.json and its row's name.
    pub key: &'static str,
    /// Its system ids, the main one (where an upload goes, the merged shelf's id) first.
    pub ids: &'static [&'static str],
    /// The folder names on a card that are one of `ids` (dArkOS keeps `turbografx16` in `turbografx`).
    pub folders: &'static [(&'static str, &'static str)],
    /// The names to choose from, the one for every start but an American one first.
    pub names: &'static [Name],
    /// The American start's name (an index into `names`).
    pub american: usize,
    /// A CD add-on follows its console's choice (the same index); it has no row of its own.
    pub follows: Option<&'static str>,
    /// Its row under Cozy extras and the line under it (empty for a follower).
    pub row: &'static str,
    pub blurb: &'static str,
}

const fn name(id: &'static str, label: &'static str, short: &'static str, long: &'static str) -> Name {
    Name { id, label, short, long }
}

/// Every pair. Hacks folders (`megadriveh`, `snesh`) and the add-ons with one name (32X, Satellaview) stay shelves of their own.
pub const PAIRS: &[Pair] = &[
    Pair {
        key: "megadrive",
        ids: &["megadrive", "genesis", "megadrive-japan"],
        folders: &[("megadrive", "megadrive"), ("genesis", "genesis"), ("megadrive-japan", "megadrive-japan")],
        names: &[name("megadrive", "Mega Drive", "Mega Drive", "Mega Drive"), name("genesis", "Genesis", "Genesis", "Genesis")],
        american: 1,
        follows: None,
        row: "Mega Drive / Genesis",
        blurb: "Name the Sega console: Mega Drive or Genesis. One shelf holds the games of all its folders; the Mega-CD follows. Saves and pictures stay with each game.",
    },
    Pair {
        key: "snes",
        ids: &["snes", "sfc"],
        folders: &[("snes", "snes"), ("sfc", "sfc")],
        names: &[
            name("snes", "SNES", "SNES", "Super Nintendo"),
            name("snes-us", "SNES (US console)", "SNES", "Super Nintendo"),
            name("sfc", "Super Famicom", "Super Famicom", "Super Famicom"),
        ],
        american: 1,
        follows: None,
        row: "SNES / Super Famicom",
        blurb: "Name the Super Nintendo: SNES, SNES with the US console at launch, or Super Famicom. One shelf holds the games of all its folders. Saves and pictures stay with each game.",
    },
    Pair {
        key: "nes",
        ids: &["nes", "famicom", "fc"],
        folders: &[("nes", "nes"), ("famicom", "famicom"), ("fc", "fc")],
        names: &[name("nes", "NES", "NES", "Nintendo Entertainment System"), name("famicom", "Famicom", "Famicom", "Famicom")],
        american: 0,
        follows: None,
        row: "NES / Famicom",
        blurb: "Name the Nintendo console: NES or Famicom. One shelf holds the games of all its folders. Saves and pictures stay with each game.",
    },
    Pair {
        key: "pcengine",
        ids: &["pcengine", "tg16", "turbografx16"],
        folders: &[("pcengine", "pcengine"), ("tg16", "tg16"), ("turbografx", "turbografx16"), ("turbografx16", "turbografx16")],
        names: &[name("pcengine", "PC Engine", "PC Engine", "PC Engine"), name("tg16", "TurboGrafx-16", "TurboGrafx-16", "TurboGrafx-16")],
        american: 1,
        follows: None,
        row: "PC Engine / TurboGrafx-16",
        blurb: "Name the NEC console: PC Engine or TurboGrafx-16. One shelf holds the games of all its folders; the CD follows. Saves and pictures stay with each game.",
    },
    Pair {
        key: "pcenginecd",
        ids: &["pcenginecd", "tg16cd", "turbografxcd", "tg-cd"],
        folders: &[("pcenginecd", "pcenginecd"), ("tg16cd", "tg16cd"), ("turbografxcd", "turbografxcd"), ("tg-cd", "tg-cd")],
        names: &[name("pcenginecd", "PC Engine CD", "PC Engine CD", "PC Engine CD"), name("tg16cd", "TurboGrafx-CD", "TurboGrafx-CD", "TurboGrafx-CD")],
        american: 1,
        follows: Some("pcengine"),
        row: "",
        blurb: "",
    },
    Pair {
        key: "segacd",
        ids: &["segacd", "megacd"],
        folders: &[("segacd", "segacd"), ("megacd", "megacd")],
        names: &[name("megacd", "Mega-CD", "Mega-CD", "Mega-CD"), name("segacd", "Sega CD", "Sega CD", "Sega CD")],
        american: 1,
        follows: Some("megadrive"),
        row: "",
        blurb: "",
    },
];

/// The pair system `id` is one of.
pub fn pair_of(id: &str) -> Option<&'static Pair> {
    PAIRS.iter().find(|p| p.ids.contains(&id))
}

/// The pair with settings key `key`.
pub fn pair(key: &str) -> Option<&'static Pair> {
    PAIRS.iter().find(|p| p.key == key)
}

/// The pairs with a row of their own under Cozy extras (the CD add-ons follow theirs).
pub fn rows() -> impl Iterator<Item = &'static Pair> {
    PAIRS.iter().filter(|p| p.follows.is_none())
}

/// The shelf a system's games stand on: the pair's main id ("genesis" is "megadrive"), else the id itself.
pub fn shelf_id(id: &str) -> &str {
    pair_of(id).map_or(id, |p| p.ids[0])
}

/// Two system ids are one shelf: the same id, or two of one pair. A list of systems has one shelf per pair
/// (`library::join_region_pairs`), so a stored id ("genesis" in Recent) finds the shelf it is on now.
pub fn same_shelf(a: &str, b: &str) -> bool {
    a == b || (pair_of(a).is_some() && shelf_id(a) == shelf_id(b))
}

// ---- where a game's folder is -----------------------------------------------------------------------------------

/// The folders of merged shelves' parts: (folder, its system id). Only ever added to: a folder stays the system's it
/// is on this card, and tests on their own folders can't take another test's.
static PARTS: Mutex<Vec<(PathBuf, String)>> = Mutex::new(Vec::new());

/// The folders `system` (a part of a merged shelf) reads its games from (`library::learn_names`).
pub fn learn(system: &str, folders: &[PathBuf]) {
    let Ok(mut parts) = PARTS.lock() else { return };
    for f in folders {
        if !parts.iter().any(|(d, s)| d == f && s == system) {
            parts.push((f.clone(), system.to_string()));
        }
    }
}

/// The system a game of `shelf` at `path` is: the part whose folder it is in ("genesis" for a game in `genesis/` on
/// the merged Mega Drive shelf), else `shelf`. What its states, covers, pictures and start are kept under.
pub fn own_system<'a>(shelf: &'a str, path: &Path) -> &'a str {
    if pair_of(shelf).is_none() {
        return shelf;
    }
    let Ok(parts) = PARTS.lock() else { return shelf };
    // The deepest folder that holds it (a part's folder inside another's would be odd, but then it is that one's).
    let found = parts.iter().filter(|(dir, id)| same_shelf(id, shelf) && path.starts_with(dir)).max_by_key(|(dir, _)| dir.as_os_str().len());
    match found.and_then(|(_, id)| pair_of(shelf)?.ids.iter().copied().find(|i| i == id)) {
        Some(id) => id,
        None => shelf,
    }
}

/// The system a folder name on a card is, when it is one of a pair's (`turbografx` is dArkOS's `turbografx16`).
pub fn folder_system(folder: &str) -> Option<&'static str> {
    let lower = folder.to_ascii_lowercase();
    PAIRS.iter().flat_map(|p| p.folders.iter()).find(|(f, _)| *f == lower).map(|(_, id)| *id)
}

// ---- the names -------------------------------------------------------------------------------------------------

/// The player's choices, pair key → name id; absent: the start's own (`american`).
static CHOSEN: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
/// The device starts with the American names (`set_american`).
static AMERICAN: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// `for_this_thread`'s choices and start.
    static THREAD: std::cell::RefCell<Option<(BTreeMap<String, String>, bool)>> = const { std::cell::RefCell::new(None) };
}

/// Whether the start's names are the American ones: English, and a time zone in America (the US's and Canada's, not the
/// ones south of them, where the consoles were sold under the other names). A device in UTC, or without a zone, isn't.
pub fn american_start(lang: crate::i18n::Lang, zone: &str) -> bool {
    const SOUTH: &[&str] = &[
        "America/Sao_Paulo", "America/Argentina", "America/Buenos_Aires", "America/Santiago", "America/Bogota", "America/Lima",
        "America/Caracas", "America/Montevideo", "America/Asuncion", "America/La_Paz", "America/Guayaquil", "America/Fortaleza",
        "America/Recife", "America/Bahia", "America/Manaus", "America/Belem", "America/Cuiaba", "America/Campo_Grande",
        "America/Porto_Velho", "America/Boa_Vista", "America/Rio_Branco", "America/Maceio", "America/Araguaina", "America/Santarem",
        "America/Noronha", "America/Punta_Arenas", "America/Cayenne", "America/Paramaribo", "America/Guyana",
    ];
    lang == crate::i18n::Lang::En
        && (zone.starts_with("America/") || zone.starts_with("US/") || zone.starts_with("Canada/"))
        && !SOUTH.iter().any(|s| zone == *s || zone.starts_with(&format!("{s}/")))
}

/// The start's names for this device (`american_start`, worked out once at the start).
pub fn set_american(on: bool) {
    AMERICAN.store(on, Ordering::Relaxed);
}

/// The choices saved in settings.json (an unknown pair or name is left out, the start's name stays).
pub fn set_chosen(saved: &BTreeMap<String, String>) {
    let clean: BTreeMap<String, String> = saved.iter().filter(|(k, v)| pair(k).is_some_and(|p| p.follows.is_none() && p.names.iter().any(|n| n.id == *v))).map(|(k, v)| (k.clone(), v.clone())).collect();
    let own = THREAD.with(|t| t.borrow_mut().as_mut().map(|(c, _)| *c = clean.clone()).is_some());
    if !own {
        if let Ok(mut c) = CHOSEN.lock() {
            *c = clean;
        }
    }
}

/// What settings.json keeps: the rows the player changed.
pub fn chosen() -> BTreeMap<String, String> {
    THREAD.with(|t| t.borrow().as_ref().map(|(c, _)| c.clone())).unwrap_or_else(|| CHOSEN.lock().map(|c| c.clone()).unwrap_or_default())
}

fn american() -> bool {
    THREAD.with(|t| t.borrow().as_ref().map(|(_, a)| *a)).unwrap_or_else(|| AMERICAN.load(Ordering::Relaxed))
}

/// The index of the name pair `p` shows now.
fn index(p: &Pair) -> usize {
    let lead = p.follows.and_then(pair).unwrap_or(p);
    let saved = chosen().get(lead.key).and_then(|id| lead.names.iter().position(|n| n.id == id));
    let at = saved.unwrap_or(if american() { lead.american } else { 0 });
    // A follower has a name per name of its console's, except the SNES's two looks of one name.
    at.min(p.names.len() - 1)
}

/// The name pair `p` shows now.
pub fn name_of(p: &Pair) -> &'static Name {
    &p.names[index(p)]
}

/// The name system `id` is shown by now, when it is one of a pair's.
pub fn current(id: &str) -> Option<&'static Name> {
    pair_of(id).map(name_of)
}

/// The next (`step` 1) or previous (-1) name of the row `p`, used at once; the caller saves the settings.
pub fn step(p: &Pair, step: i32) {
    let now = index(p) as i32;
    let next = (now + step).rem_euclid(p.names.len() as i32) as usize;
    let mut c = chosen();
    c.insert(p.key.to_string(), p.names[next].id.to_string());
    set_chosen(&c);
}

/// Every SNES game goes into the US console at the start (the "SNES (US console)" name); otherwise a US cartridge does
/// and the others go into the Super Famicom-shaped one, as before.
pub fn snes_us_look() -> bool {
    pair("snes").is_some_and(|p| name_of(p).id == "snes-us")
}

/// Choices and a start for this thread only, until the guard is dropped (tests side by side in one process).
#[must_use = "the thread's choices go back when the guard is dropped"]
pub fn for_this_thread(chosen: &[(&str, &str)], american: bool) -> ThreadNames {
    let mine = (chosen.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), american);
    ThreadNames(THREAD.with(|t| t.replace(Some(mine))))
}

/// See `for_this_thread`.
pub struct ThreadNames(Option<(BTreeMap<String, String>, bool)>);

impl Drop for ThreadNames {
    fn drop(&mut self) {
        let before = self.0.take();
        THREAD.with(|t| *t.borrow_mut() = before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    /// The pairs ROCKNIX's and dArkOS's es_systems.cfg list as folders of their own (route-bases H700-20261101,
    /// SM8250-20261001, dArkOS RG353M): each is one shelf.
    #[test]
    fn every_regional_folder_of_rocknix_and_darkos_is_one_shelf_with_its_console() {
        for (a, b) in [
            ("genesis", "megadrive"), ("megadrive-japan", "megadrive"), ("sfc", "snes"), ("famicom", "nes"), ("fc", "nes"),
            ("tg16", "pcengine"), ("turbografx16", "pcengine"), ("tg16cd", "pcenginecd"), ("turbografxcd", "pcenginecd"), ("megacd", "segacd"),
        ] {
            assert_eq!(shelf_id(a), b, "{a}");
            assert!(same_shelf(a, b) && same_shelf(b, a), "{a} {b}");
        }
        for other in ["megadriveh", "snesh", "nesh", "sega32x", "satellaview", "sufami", "fds", "gba", "supergrafx"] {
            assert_eq!(shelf_id(other), other, "{other} stays a shelf of its own");
            assert!(!same_shelf(other, "megadrive") && !same_shelf(other, "snes") && !same_shelf(other, "nes"), "{other}");
        }
        assert_eq!(folder_system("turbografx"), Some("turbografx16"), "dArkOS's folder of the TurboGrafx-16");
        assert_eq!(folder_system("Genesis"), Some("genesis"));
        assert_eq!(folder_system("gba"), None);
    }

    #[test]
    fn english_in_american_time_zones_starts_with_the_american_names_every_other_start_with_the_others() {
        let names = |american: bool| {
            let _t = for_this_thread(&[], american);
            ["megadrive", "genesis", "snes", "sfc", "nes", "famicom", "pcengine", "tg16", "pcenginecd", "segacd", "megacd"].map(|id| current(id).unwrap().short)
        };
        assert_eq!(names(true), ["Genesis", "Genesis", "SNES", "SNES", "NES", "NES", "TurboGrafx-16", "TurboGrafx-16", "TurboGrafx-CD", "Sega CD", "Sega CD"]);
        assert_eq!(names(false), ["Mega Drive", "Mega Drive", "SNES", "SNES", "NES", "NES", "PC Engine", "PC Engine", "PC Engine CD", "Mega-CD", "Mega-CD"]);
        {
            let _t = for_this_thread(&[], true);
            assert!(snes_us_look(), "the American SNES is the US console");
        }
        let _t = for_this_thread(&[], false);
        assert!(!snes_us_look());
        assert!(american_start(Lang::En, "America/Chicago") && american_start(Lang::En, "America/Toronto") && american_start(Lang::En, "US/Pacific"));
        for (lang, zone) in [(Lang::De, "America/Chicago"), (Lang::En, "Europe/London"), (Lang::En, "UTC"), (Lang::En, ""), (Lang::En, "America/Sao_Paulo"), (Lang::En, "America/Argentina/Cordoba"), (Lang::Es, "America/Mexico_City")] {
            assert!(!american_start(lang, zone), "{lang:?} {zone}");
        }
    }

    #[test]
    fn a_chosen_name_wins_over_the_start_and_the_cd_follows_its_console() {
        let _t = for_this_thread(&[("megadrive", "genesis"), ("pcengine", "tg16"), ("nes", "famicom"), ("snes", "sfc")], false);
        assert_eq!(current("megadrive").unwrap().long, "Genesis");
        assert_eq!(current("segacd").unwrap().short, "Sega CD", "Genesis brings the Sega CD");
        assert_eq!(current("tg16cd").unwrap().short, "TurboGrafx-CD");
        assert_eq!(current("famicom").unwrap().long, "Famicom", "Famicom only when chosen");
        assert_eq!(current("snes").unwrap().long, "Super Famicom");
        // Round and round, saved as the name's id.
        step(pair("megadrive").unwrap(), 1);
        assert_eq!(current("genesis").unwrap().short, "Mega Drive");
        assert_eq!(chosen().get("megadrive").map(String::as_str), Some("megadrive"));
        step(pair("snes").unwrap(), -1);
        assert_eq!(name_of(pair("snes").unwrap()).id, "snes-us");
        assert!(snes_us_look());
        // A name or a row this version doesn't know is dropped; the start's name stays.
        set_chosen(&[("megadrive".to_string(), "future".to_string()), ("segacd".to_string(), "segacd".to_string()), ("x".to_string(), "y".to_string())].into());
        assert!(chosen().is_empty());
        assert_eq!(current("megadrive").unwrap().short, "Mega Drive");
        assert_eq!(rows().map(|p| p.key).collect::<Vec<_>>(), ["megadrive", "snes", "nes", "pcengine"], "the CD add-ons follow, no rows");
    }

    #[test]
    fn a_game_keeps_the_system_of_its_folder_on_a_merged_shelf() {
        // Paths only: nothing is read.
        let top = PathBuf::from("/region-pairs-test");
        let roms = top.join("roms");
        learn("genesis", &[roms.join("genesis")]);
        learn("megadrive-japan", &[roms.join("megadrive-japan")]);
        assert_eq!(own_system("megadrive", &roms.join("genesis/Sonic.md")), "genesis");
        assert_eq!(own_system("genesis", &roms.join("genesis/sub/Sonic.md")), "genesis");
        assert_eq!(own_system("megadrive", &roms.join("megadrive-japan/Puyo.md")), "megadrive-japan");
        assert_eq!(own_system("megadrive", &roms.join("megadrive/Sonic.md")), "megadrive");
        assert_eq!(own_system("snes", &roms.join("genesis/Sonic.md")), "snes", "another pair's folder is no part of this shelf");
        assert_eq!(own_system("gba", &roms.join("genesis/x.gba")), "gba");
        // A folder only read as the console's (Android's `genesis`, another card's "Sega Genesis") stays the shelf's.
        assert_eq!(own_system("megadrive", &top.join("other/genesis/Sonic.md")), "megadrive");
    }
}

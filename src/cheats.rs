//! Cheats for pocket-core games, from libretro-database's `.cht` files (the ones RetroArch uses; ROCKNIX
//! ships them in `/usr/share/libretro-database/cht` and mounts them, with the user's additions in
//! `/storage/database`, at `/tmp/database/cht`).
//!
//! * `find` matches a game to its files by system folder and name: region and other tags in brackets
//!   are ignored, a region the game names has to agree with the file's.
//! * `CheatMenu` is the pause menu's Cheats page: a list with On/Off per cheat. What is on is stored per
//!   game in `<data>/cheats/<system>/<game>.json` the moment it changes, and pocket-core is told (`cheats`
//!   over its command FIFO) to read it again.
//! * Entries that give no code but a memory address and a value (RetroArch's newer cht format, `cheatN_address`)
//!   are not applied: `retro_cheat_set` only takes codes, and a memory write would need each core's memory map
//!   (`docs/HOST.md`, Cheats). They are counted, and the Cheats page says so.
//! * `active_codes` is what pocket-core hands to the core with `retro_cheat_reset` / `retro_cheat_set`,
//!   at the start of a game and after every change.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::control;
use crate::emu_menu::Row;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cheat {
    pub desc: String,
    /// As the core wants it: several codes joined by `+`.
    pub code: String,
}

/// Where the database may be, first match wins. `POCKET_CHEATS_DB` is for tests.
pub fn db_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(p) = std::env::var_os("POCKET_CHEATS_DB") {
        roots.push(PathBuf::from(p));
    }
    roots.push(PathBuf::from("/tmp/database/cht"));
    roots.push(PathBuf::from("/usr/share/libretro-database/cht"));
    roots
}

/// The database folder for a ROCKNIX system name (libretro-database names them by manufacturer and console).
pub fn folder(system: &str) -> Option<&'static str> {
    Some(match system {
        "gb" | "gbh" => "Nintendo - Game Boy",
        "gbc" | "gbch" => "Nintendo - Game Boy Color",
        "gba" | "gbah" | "gbav" => "Nintendo - Game Boy Advance",
        "nes" | "nesh" | "famicom" => "Nintendo - Nintendo Entertainment System",
        "fds" => "Nintendo - Family Computer Disk System",
        "snes" | "snesh" | "sfc" | "snesmsu1" => "Nintendo - Super Nintendo Entertainment System",
        "n64" => "Nintendo - Nintendo 64",
        "nds" => "Nintendo - Nintendo DS",
        "megadrive" | "megadrive-japan" | "genesis" | "genh" | "megadriveh" => "Sega - Mega Drive - Genesis",
        "mastersystem" | "sms" => "Sega - Master System - Mark III",
        "gamegear" | "ggh" => "Sega - Game Gear",
        "sega32x" => "Sega - 32X",
        "segacd" | "megacd" => "Sega - Mega-CD - Sega CD",
        "pcengine" | "tg16" => "NEC - PC Engine - TurboGrafx 16",
        "pcenginecd" | "tg16cd" => "NEC - PC Engine CD - TurboGrafx-CD",
        "supergrafx" => "NEC - PC Engine SuperGrafx",
        "atari2600" => "Atari - 2600",
        "atari7800" => "Atari - 7800",
        "atarilynx" => "Atari - Lynx",
        "psx" => "Sony - PlayStation",
        _ => return None,
    })
}

// ---- parsing -------------------------------------------------------------------------------------------

/// `&eacute;` and friends: the database was written for HTML.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "eacute" => Some('é'),
            "egrave" => Some('è'),
            "aacute" => Some('á'),
            "agrave" => Some('à'),
            "iacute" => Some('í'),
            "oacute" => Some('ó'),
            "uacute" => Some('ú'),
            "ntilde" => Some('ñ'),
            "ouml" => Some('ö'),
            "uuml" => Some('ü'),
            "auml" => Some('ä'),
            "ccedil" => Some('ç'),
            n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// What one `.cht` file holds.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Parsed {
    /// Entries with a code, in file order.
    pub cheats: Vec<Cheat>,
    /// Entries pocket-core can't apply: no code, but a memory address and a value to write (`cheatN_address`,
    /// `cheatN_value`, `cheatN_cheat_type` above 0). Each as a text that tells equal ones in several files apart.
    pub unsupported: Vec<String>,
}

#[derive(Default)]
struct Raw {
    desc: String,
    code: String,
    address: String,
    value: String,
    kind: String,
}

/// The cheats of one `.cht` file (`cheat0_desc`, `cheat0_code`, `cheat0_enable` …), in file order, and the
/// entries that can't be applied. Entries with neither a code nor a memory write are left out: section titles and
/// empty placeholders, and the rumble triggers of the "(Rumbles)" files (`cheat_type` 0), which are no cheats.
pub fn parse_cht_full(text: &str) -> Parsed {
    let mut found: std::collections::BTreeMap<usize, Raw> = Default::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim().trim_matches('"').trim());
        let Some(rest) = key.strip_prefix("cheat") else { continue };
        let Some((n, field)) = rest.split_once('_') else { continue };
        let Ok(n) = n.parse::<usize>() else { continue };
        let entry = found.entry(n).or_default();
        match field {
            "desc" => entry.desc = unescape(value),
            "code" => entry.code = value.to_string(),
            "address" => entry.address = value.to_string(),
            "value" => entry.value = value.to_string(),
            "cheat_type" => entry.kind = value.to_string(),
            _ => {}
        }
    }
    let mut out = Parsed::default();
    for raw in found.into_values() {
        if !raw.code.is_empty() {
            out.cheats.push(Cheat {
                desc: if raw.desc.is_empty() { raw.code.clone() } else { raw.desc },
                code: raw.code,
            });
        } else if !raw.address.is_empty() && !raw.value.is_empty() && raw.kind.parse::<u32>().is_ok_and(|k| k > 0) {
            out.unsupported.push(format!("{}\u{1}{}\u{1}{}\u{1}{}", raw.desc, raw.address, raw.value, raw.kind));
        }
    }
    out
}

/// The cheats of one `.cht` file that have a code.
pub fn parse_cht(text: &str) -> Vec<Cheat> {
    parse_cht_full(text).cheats
}

// ---- matching a game to its files --------------------------------------------------------------------

/// The groups in round or square brackets of a name, lower case, without the brackets.
fn tags(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                if depth == 1 {
                    cur.clear();
                    continue;
                }
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push(cur.to_lowercase());
                    continue;
                }
            }
            _ => {}
        }
        if depth > 0 {
            cur.push(c);
        }
    }
    out
}

/// The name without brackets, lower case, letters and digits only; "&" counts as "and" and a leading
/// "The" is dropped, so "Legend of X, The" and "The Legend of X" are one name.
fn base(name: &str) -> String {
    let mut plain = String::new();
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    let mut s = plain.to_lowercase().replace('&', " and ");
    if let Some(i) = s.rfind(", the").filter(|i| !s[i + 5..].starts_with(|c: char| c.is_alphanumeric())) {
        // "Name, The - Subtitle" and "Name, The"
        s.replace_range(i..i + 5, "");
        s.insert_str(0, "the ");
    }
    let s: String = s.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let mut words: Vec<&str> = s.split_whitespace().collect();
    if words.first() == Some(&"the") {
        words.remove(0);
    }
    words.concat()
}

/// Region words of a tag list: "usa, europe" and "(u)" both mean what they say.
fn regions(tags: &[String]) -> BTreeSet<&'static str> {
    const NAMES: [(&str, &str); 14] = [
        ("usa", "usa"),
        ("u", "usa"),
        ("europe", "europe"),
        ("e", "europe"),
        ("japan", "japan"),
        ("j", "japan"),
        ("world", "world"),
        ("germany", "germany"),
        ("france", "france"),
        ("spain", "spain"),
        ("italy", "italy"),
        ("australia", "australia"),
        ("korea", "korea"),
        ("brazil", "brazil"),
    ];
    let mut out = BTreeSet::new();
    for t in tags {
        for word in t.split(|c: char| c == ',' || c.is_whitespace()).filter(|w| !w.is_empty()) {
            if let Some((_, r)) = NAMES.iter().find(|(n, _)| *n == word) {
                out.insert(*r);
            }
        }
    }
    out
}

/// What a file adds to its name besides regions ("Game Genie", "Action Replay", an author): shown when two
/// files offer the same cheat name.
fn kind(tags: &[String]) -> Option<String> {
    tags.iter()
        .find(|t| {
            regions(std::slice::from_ref(t)).is_empty()
                && t.chars().any(char::is_alphabetic)
                && !["rev", "v1", "sgb", "beta", "proto"].iter().any(|p| t.starts_with(p))
        })
        .map(|t| {
            t.split_whitespace()
                .map(|w| {
                    let mut c = w.chars();
                    c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
}

/// The cheats for `rom` in the database under `roots`. Several files can match (a Game Genie and an Action
/// Replay list, regions); their cheats are put together, identical codes once, in the order of the files
/// (the plain name first).
pub fn find(system: &str, rom: &Path, roots: &[PathBuf]) -> Vec<Cheat> {
    find_all(system, rom, roots).0
}

/// `find`, and how many different entries of those files can't be applied (`Parsed::unsupported`).
pub fn find_all(system: &str, rom: &Path, roots: &[PathBuf]) -> (Vec<Cheat>, usize) {
    let Some(folder) = folder(system) else { return (Vec::new(), 0) };
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let want = base(stem);
    if want.is_empty() {
        return (Vec::new(), 0);
    }
    let rom_regions = regions(&tags(stem));
    for root in roots {
        let Ok(dir) = std::fs::read_dir(root.join(folder)) else { continue };
        // (sort key, file, its tags)
        let mut hits: Vec<(PathBuf, Vec<String>)> = dir
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("cht")))
            .filter_map(|e| {
                let path = e.path();
                let name = path.file_stem()?.to_str()?.to_string();
                (base(&name) == want).then(|| (path, tags(&name)))
            })
            .collect();
        if hits.is_empty() {
            continue;
        }
        // A game that names its region only takes files for that region (or for none).
        if !rom_regions.is_empty() {
            let fits = |t: &Vec<String>| {
                let r = regions(t);
                r.is_empty() || !r.is_disjoint(&rom_regions)
            };
            if hits.iter().any(|(_, t)| fits(t)) {
                hits.retain(|(_, t)| fits(t));
            }
        }
        // Fewest tags first: the plain name, then the variants.
        hits.sort_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| a.0.cmp(&b.0)));
        let mut all: Vec<Cheat> = Vec::new();
        let mut unsupported: BTreeSet<String> = BTreeSet::new();
        for (path, t) in hits {
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let label = kind(&t);
            let parsed = parse_cht_full(&String::from_utf8_lossy(&bytes));
            unsupported.extend(parsed.unsupported);
            for mut c in parsed.cheats {
                if all.iter().any(|x| x.code.eq_ignore_ascii_case(&c.code)) {
                    continue;
                }
                if all.iter().any(|x| x.desc == c.desc) {
                    if let Some(l) = &label {
                        c.desc = format!("{} [{l}]", c.desc);
                    }
                }
                all.push(c);
            }
        }
        return (all, unsupported.len());
    }
    (Vec::new(), 0)
}

// ---- what is on, per game ----------------------------------------------------------------------------

fn store_path(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
    data_dir.join("cheats").join(system).join(format!("{stem}.json"))
}

/// A name for each cheat that is unique within the list (a second "Infinite Lives" becomes "Infinite Lives #2").
fn keys(cheats: &[Cheat]) -> Vec<String> {
    let mut seen: std::collections::HashMap<&str, usize> = Default::default();
    cheats
        .iter()
        .map(|c| {
            let n = seen.entry(c.desc.as_str()).or_insert(0);
            *n += 1;
            if *n == 1 {
                c.desc.clone()
            } else {
                format!("{} #{n}", c.desc)
            }
        })
        .collect()
}

fn load_on(data_dir: &Path, system: &str, rom: &Path) -> BTreeSet<String> {
    crate::fsutil::read_json_or_backup::<serde_json::Value>(&store_path(data_dir, system, rom))
        .and_then(|v| v["on"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()))
        .unwrap_or_default()
}

fn save_on(path: &Path, on: &BTreeSet<String>) -> bool {
    if on.is_empty() {
        // Nothing on: no file, as if the game never had cheats.
        return crate::fsutil::remove_json(path);
    }
    crate::fsutil::write_json(path, &serde_json::json!({ "on": on })).is_ok()
}

/// The codes pocket-core gives the core: the cheats this game has switched on, in database order.
pub fn active_codes(data_dir: &Path, system: &str, rom: &Path) -> Vec<String> {
    active_codes_in(data_dir, system, rom, &db_roots())
}

pub fn active_codes_in(data_dir: &Path, system: &str, rom: &Path, roots: &[PathBuf]) -> Vec<String> {
    let on = load_on(data_dir, system, rom);
    if on.is_empty() {
        return Vec::new();
    }
    let cheats = find(system, rom, roots);
    keys(&cheats)
        .into_iter()
        .zip(cheats)
        .filter(|(k, _)| on.contains(k))
        .map(|(_, c)| c.code)
        .collect()
}

/// Gives the core the cheats this game has switched on (none: clears whatever it had). Returns how many.
pub fn apply(core: &crate::libretro::Core, data_dir: &Path, system: &str, rom: &Path) -> usize {
    let codes = active_codes(data_dir, system, rom);
    core.set_cheats(&codes);
    codes.len()
}

// ---- the pause menu's page -----------------------------------------------------------------------------

/// The page's line for entries that give no code. Plain words: what, why it is not on the list, that nothing is lost.
fn unsupported_row(n: usize) -> Row {
    if n == 1 {
        Row {
            name: "1 cheat uses a format pocket-core can't apply yet".into(),
            value: String::new(),
            desc: "It sets a value in memory instead of giving a code. It is in the cheat file, but pocket-core can't use it yet.".into(),
        }
    } else {
        Row {
            name: format!("{n} cheats use a format pocket-core can't apply yet"),
            value: String::new(),
            desc: "They set a value in memory instead of giving a code. They are in the cheat file, but pocket-core can't use them yet.".into(),
        }
    }
}

pub struct CheatMenu {
    cheats: Vec<Cheat>,
    /// Entries of the game's files that give no code (`Parsed::unsupported`): said on the page, never applied.
    unsupported: usize,
    keys: Vec<String>,
    on: BTreeSet<String>,
    store: PathBuf,
    notify: bool,
}

impl CheatMenu {
    /// None when the database has nothing for this game (not even cheats pocket-core can't apply).
    pub fn load(data_dir: &Path, system: &str, rom: &Path) -> Option<CheatMenu> {
        Self::load_in(data_dir, system, rom, &db_roots(), true)
    }

    pub fn load_in(data_dir: &Path, system: &str, rom: &Path, roots: &[PathBuf], notify: bool) -> Option<CheatMenu> {
        let (cheats, unsupported) = find_all(system, rom, roots);
        if cheats.is_empty() && unsupported == 0 {
            return None;
        }
        let keys = keys(&cheats);
        // What the store remembers, minus cheats the database no longer has.
        let on = load_on(data_dir, system, rom).into_iter().filter(|k| keys.contains(k)).collect();
        Some(CheatMenu {
            cheats,
            unsupported,
            keys,
            on,
            store: store_path(data_dir, system, rom),
            notify,
        })
    }

    /// The cheats, then "All off" (when there are cheats), then the line about those that can't be applied.
    pub fn count(&self) -> usize {
        self.cheats.len() + usize::from(!self.cheats.is_empty()) + usize::from(self.unsupported > 0)
    }

    /// How many entries of the game's files pocket-core can't apply (the last line of the page says so).
    pub fn unsupported(&self) -> usize {
        self.unsupported
    }

    pub fn enabled(&self) -> usize {
        self.on.len()
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = self
            .cheats
            .iter()
            .zip(&self.keys)
            .map(|(c, k)| {
                let code = if c.code.len() > 60 {
                    format!("{}…", &c.code[..c.code.char_indices().nth(60).map_or(c.code.len(), |(i, _)| i)])
                } else {
                    c.code.clone()
                };
                Row {
                    name: c.desc.clone(),
                    value: if self.on.contains(k) { "On" } else { "Off" }.into(),
                    desc: format!("Code {code}\nA turns it on or off. What is on is remembered for this game."),
                }
            })
            .collect();
        if !self.cheats.is_empty() {
            rows.push(Row {
                name: "All off".into(),
                value: String::new(),
                desc: format!("{} on. Switches every cheat of this game off.", self.on.len()),
            });
        }
        if self.unsupported > 0 {
            rows.push(unsupported_row(self.unsupported));
        }
        rows
    }

    /// A on a line (or Left/Right): flips a cheat, or switches all off on the last line. Stores the
    /// change and tells pocket-core. False when nothing changed.
    pub fn press(&mut self, sel: usize) -> bool {
        if sel < self.cheats.len() {
            let k = self.keys[sel].clone();
            if !self.on.remove(&k) {
                self.on.insert(k);
            }
        } else if sel > self.cheats.len() || self.cheats.is_empty() || self.on.is_empty() {
            // The line about cheats pocket-core can't apply, or All off with nothing on: nothing to change.
            return false;
        } else {
            self.on.clear();
        }
        save_on(&self.store, &self.on);
        if self.notify {
            control::pocket_core("cheats");
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn scratch(tag: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("cheats-{tag}"))
    }

    fn db(dir: &Path, folder: &str, files: &[(&str, &str)]) {
        std::fs::create_dir_all(dir.join(folder)).unwrap();
        for (name, body) in files {
            std::fs::write(dir.join(folder).join(name), body).unwrap();
        }
    }

    const SAMPLE: &str = "cheats = 3\n\ncheat0_desc = \"Infinite Lives\"\ncheat0_code = \"7E1F8A03+7E039203\"\ncheat0_enable = false\n\n\
cheat1_desc = \"Pok&eacute;mon &amp; Friends\"\ncheat1_code = \"01FF57D8\"\ncheat1_enable = false\n\ncheat2_desc = \"Empty\"\ncheat2_code = \"\"\ncheat2_enable = false\n";

    #[test]
    fn a_cht_file_is_parsed_with_entities_and_without_empty_codes() {
        let c = parse_cht(SAMPLE);
        assert_eq!(c.len(), 2);
        assert_eq!(
            c[0],
            Cheat {
                desc: "Infinite Lives".into(),
                code: "7E1F8A03+7E039203".into()
            }
        );
        assert_eq!(c[1].desc, "Pokémon & Friends");
        assert!(parse_cht("junk\ncheat_x = 1\n").is_empty());
    }

    #[test]
    fn names_match_without_brackets_case_or_punctuation() {
        assert_eq!(base("Contra III - The Alien Wars (USA)"), base("contra iii - the alien wars"));
        assert_eq!(
            base("Legend of Zelda, The - A Link to the Past (USA)"),
            base("The Legend of Zelda - A Link to the Past")
        );
        assert_ne!(base("Mega Man Zero"), base("Mega Man Zero 2"));
        assert_eq!(base("Mega Man & Bass"), base("Mega Man and Bass"));
        assert_eq!(tags("Foo (USA, Europe) [!] (Rev 1)"), ["usa, europe", "!", "rev 1"]);
        assert_eq!(regions(&tags("Foo (U)(Venom)")).into_iter().collect::<Vec<_>>(), ["usa"]);
    }

    #[test]
    fn region_and_variants_are_chosen_and_merged() {
        let dir = scratch("find");
        db(
            &dir,
            "Nintendo - Super Nintendo Entertainment System",
            &[
                ("Game (USA).cht", "cheat0_desc = \"Lives\"\ncheat0_code = \"AAAA\"\n"),
                ("Game (Europe).cht", "cheat0_desc = \"Lives\"\ncheat0_code = \"EEEE\"\n"),
                (
                    "Game (USA) (Game Genie).cht",
                    "cheat0_desc = \"Lives\"\ncheat0_code = \"GGGG-GGGG\"\ncheat1_desc = \"Same\"\ncheat1_code = \"aaaa\"\n",
                ),
                ("Other Game (USA).cht", "cheat0_desc = \"x\"\ncheat0_code = \"X\"\n"),
            ],
        );
        let roots = [dir.to_path_buf()];
        let usa = find("snes", Path::new("/roms/snes/Game (USA).sfc"), &roots);
        assert_eq!(
            usa.iter().map(|c| c.code.as_str()).collect::<Vec<_>>(),
            ["AAAA", "GGGG-GGGG"],
            "plain file first, same code (any case) once, no Europe"
        );
        assert_eq!(usa[1].desc, "Lives [Game Genie]", "two files, one name: the second says where it is from");
        let eu = find("snes", Path::new("/roms/snes/Game (E).smc"), &roots);
        assert_eq!(eu.iter().map(|c| c.code.as_str()).collect::<Vec<_>>(), ["EEEE"]);
        let none = find("snes", Path::new("/roms/snes/Game.smc"), &roots);
        assert_eq!(none.len(), 3, "no region named: all of them");
        assert!(find("snes", Path::new("/roms/snes/Nope.smc"), &roots).is_empty());
        assert!(find("n64x", Path::new("/roms/x/Game (USA).sfc"), &roots).is_empty(), "unknown system");
    }

    #[test]
    fn the_menu_remembers_per_game_and_hands_over_the_codes_in_order() {
        let (dbdir, data) = (scratch("db"), scratch("data"));
        db(
            &dbdir,
            "Nintendo - Game Boy",
            &[(
                "Tetris (World).cht",
                "cheat0_desc = \"A\"\ncheat0_code = \"01\"\ncheat1_desc = \"B\"\ncheat1_code = \"02\"\ncheat2_desc = \"B\"\ncheat2_code = \"03\"\n",
            )],
        );
        let roots = [dbdir.to_path_buf()];
        let rom = Path::new("/roms/gb/Tetris (World).gb");
        let mut m = CheatMenu::load_in(&data, "gb", rom, &roots, false).expect("cheats exist");
        assert_eq!(m.count(), 4);
        assert_eq!(m.rows().iter().map(|r| r.value.as_str()).collect::<Vec<_>>(), ["Off", "Off", "Off", ""]);
        assert!(!m.press(3), "All off with nothing on changes nothing");
        assert!(m.press(2) && m.press(0));
        assert_eq!(m.rows()[2].value, "On");
        // A second menu (the next visit, or pocket-core) sees the same.
        assert_eq!(
            active_codes_in(&data, "gb", rom, &roots),
            ["01", "03"],
            "the duplicate name has its own key (B #2)"
        );
        let m2 = CheatMenu::load_in(&data, "gb", rom, &roots, false).unwrap();
        assert_eq!(m2.enabled(), 2);
        // Another game of the same console isn't affected.
        assert!(active_codes_in(&data, "gb", Path::new("/roms/gb/Other.gb"), &roots).is_empty());
        assert!(m.press(2), "switch one off again");
        assert_eq!(active_codes_in(&data, "gb", rom, &roots), ["01"]);
        assert!(m.press(3), "All off");
        assert!(active_codes_in(&data, "gb", rom, &roots).is_empty());
        assert!(!store_path(&data, "gb", rom).exists(), "nothing on: no file left");
    }

    #[test]
    fn a_game_without_cheats_has_no_page() {
        let (dbdir, data) = (scratch("db2"), scratch("data2"));
        assert!(CheatMenu::load_in(&data, "gb", Path::new("/roms/gb/X.gb"), &[dbdir.to_path_buf()], false).is_none());
    }

    #[test]
    fn long_codes_are_shortened_in_the_description() {
        let (dbdir, data) = (scratch("db3"), scratch("data3"));
        let code = "01FF57D8+".repeat(30);
        db(
            &dbdir,
            "Nintendo - Game Boy",
            &[("G.cht", &format!("cheat0_desc = \"Big\"\ncheat0_code = \"{code}\"\n"))],
        );
        let m = CheatMenu::load_in(&data, "gb", Path::new("/r/G.gb"), &[dbdir.to_path_buf()], false).unwrap();
        assert!(m.rows()[0].desc.lines().next().unwrap().ends_with('…'));
    }
    /// RetroArch's newer format: a memory write (cheat_type 1 sets a value), no code.
    const MEMORY: &str = "cheats = 4\n\
cheat0_desc = \"Infinite lives\"\ncheat0_code = \"\"\ncheat0_address = \"114\"\ncheat0_value = \"9\"\ncheat0_cheat_type = \"1\"\ncheat0_handler = \"1\"\ncheat0_enable = \"false\"\n\
cheat1_desc = \"Infinite energy\"\ncheat1_address = \"115\"\ncheat1_value = \"255\"\ncheat1_cheat_type = \"1\"\ncheat1_handler = \"1\"\n\
cheat2_desc = \"Level skip\"\ncheat2_code = \"7E1F8A03\"\ncheat2_address = \"9\"\ncheat2_value = \"1\"\ncheat2_cheat_type = \"1\"\n\
cheat3_desc = \"Section title\"\ncheat3_code = \"\"\n";

    /// A "(Rumbles)" file: addresses and values, but cheat_type 0 and rumble settings. Not cheats at all.
    const RUMBLE: &str = "cheats = 1\ncheat0_address = \"2602\"\ncheat0_address_bit_position = \"0\"\ncheat0_cheat_type = \"0\"\ncheat0_code = \"\"\n\
cheat0_desc = \"Rumble on energy loss\"\ncheat0_handler = \"1\"\ncheat0_rumble_type = \"0\"\ncheat0_value = \"1\"\n";

    #[test]
    fn entries_without_a_code_are_counted_not_lost_and_not_mistaken_for_cheats() {
        let p = parse_cht_full(MEMORY);
        assert_eq!(p.cheats.iter().map(|c| c.desc.as_str()).collect::<Vec<_>>(), ["Level skip"], "a code wins over an address");
        assert_eq!(p.unsupported.len(), 2, "the two memory writes; the title with nothing in it is no cheat");
        assert_eq!(parse_cht(MEMORY), p.cheats, "parse_cht is the codes as before");
        let r = parse_cht_full(RUMBLE);
        assert!(r.cheats.is_empty() && r.unsupported.is_empty(), "a rumble trigger is not a cheat that failed");
        assert_eq!(parse_cht_full(SAMPLE).unsupported.len(), 0, "code-based files are as they were");
        assert_eq!(parse_cht_full(SAMPLE).cheats.len(), 2);
        assert_eq!(parse_cht_full("cheat0_address = \"1\"\ncheat0_cheat_type = \"2\"\n").unsupported.len(), 0, "no value: not a write");
    }

    #[test]
    fn the_page_lists_codes_and_says_how_many_entries_it_cannot_apply() {
        let (dbdir, data) = (scratch("db4"), scratch("data4"));
        db(
            &dbdir,
            "Nintendo - Nintendo Entertainment System",
            &[
                ("Micro Mages.cht", MEMORY),
                ("Micro Mages (Game Genie).cht", "cheat0_desc = \"Lives\"\ncheat0_code = \"SXIOPO\"\n"),
                ("Micro Mages (Rumbles).cht", RUMBLE),
                // The same two writes again in a variant file: counted once.
                ("Micro Mages (Beta).cht", MEMORY),
            ],
        );
        let rom = Path::new("/roms/nes/Micro Mages.nes");
        let mut m = CheatMenu::load_in(&data, "nes", rom, std::slice::from_ref(&dbdir.to_path_buf()), false).expect("a page");
        assert_eq!(m.unsupported(), 2);
        let rows = m.rows();
        assert_eq!(
            rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["Level skip", "Lives", "All off", "2 cheats use a format pocket-core can't apply yet"]
        );
        assert_eq!(m.count(), rows.len(), "the note can be selected, so its description can be read");
        assert!(!m.press(3), "pressing the note changes nothing");
        assert!(m.press(0) && m.press(2), "a code cheat on, then All off");
        assert_eq!(active_codes_in(&data, "nes", rom, std::slice::from_ref(&dbdir.to_path_buf())), Vec::<String>::new());
        // Codes only: the page is as before, with no note.
        db(&dbdir, "Nintendo - Game Boy", &[("Plain.cht", "cheat0_desc = \"A\"\ncheat0_code = \"01\"\n")]);
        let plain = CheatMenu::load_in(&data, "gb", Path::new("/r/Plain.gb"), std::slice::from_ref(&dbdir.to_path_buf()), false).unwrap();
        assert_eq!((plain.unsupported(), plain.rows().len()), (0, 2));
    }

    #[test]
    fn a_game_whose_cheats_are_all_memory_writes_still_gets_a_page_that_says_so() {
        let (dbdir, data) = (scratch("db5"), scratch("data5"));
        db(
            &dbdir,
            "Atari - Lynx",
            &[("Ms. Pac-Man.cht", "cheat0_desc = \"Infinite Lives\"\ncheat0_address = \"4298\"\ncheat0_value = \"6\"\ncheat0_cheat_type = \"1\"\n")],
        );
        let rom = Path::new("/roms/atarilynx/Ms. Pac-Man.lnx");
        let mut m = CheatMenu::load_in(&data, "atarilynx", rom, std::slice::from_ref(&dbdir.to_path_buf()), false).expect("not a silently missing page");
        let rows = m.rows();
        assert_eq!(rows.len(), 1, "no All off without cheats");
        assert_eq!(rows[0].name, "1 cheat uses a format pocket-core can't apply yet");
        assert!(rows[0].desc.contains("can't use it yet"));
        assert_eq!((m.count(), m.enabled()), (1, 0));
        assert!(!m.press(0), "nothing to switch");
        assert!(active_codes_in(&data, "atarilynx", rom, std::slice::from_ref(&dbdir.to_path_buf())).is_empty());
        assert!(!store_path(&data, "atarilynx", rom).exists());
    }
}

//! Console fact sheets: `assets/data/consoles.json`, compiled into the binary and keyed by ROCKNIX system id.
//!
//! The file is verified data with a source list per console. It is shown as it is: nothing here adds, rounds
//! or corrects a fact, and a field that is missing (or `null`) is simply left out of every screen. `lines.*`
//! are ready-made English strings for the shelf's short facts; the other fields feed the console page
//! (`console_page`). Edit the file only with a source for the change (docs/DEVELOPING.md).

use std::sync::OnceLock;

use serde::{Deserialize, Deserializer};

const DATA: &str = include_str!("../assets/data/consoles.json");

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    license: Option<String>,
    systems: Vec<Console>,
}

/// One console as the file describes it.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct Console {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "strings")]
    pub aka: Vec<String>,
    #[serde(default, deserialize_with = "text")]
    pub maker: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub kind: Option<String>,
    #[serde(default)]
    pub generation: Option<u32>,
    /// (region, date) in the file's order; dates are "YYYY", "YYYY-MM" or "YYYY-MM-DD".
    #[serde(default, deserialize_with = "ordered")]
    pub released: Vec<(String, String)>,
    #[serde(default, deserialize_with = "text")]
    pub discontinued: Option<String>,
    #[serde(default)]
    pub units_sold: Option<Units>,
    /// (region, price) in the file's order.
    #[serde(default, deserialize_with = "ordered")]
    pub launch_price: Vec<(String, String)>,
    #[serde(default, deserialize_with = "text")]
    pub cpu: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub gpu: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub memory: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub media: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub display: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub predecessor: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub successor: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub summary: Option<String>,
    #[serde(default)]
    pub lines: Lines,
    #[serde(default, deserialize_with = "strings")]
    pub sources: Vec<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Units {
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default, deserialize_with = "text")]
    pub as_of: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub note: Option<String>,
}

/// The shelf's ready-made strings.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct Lines {
    #[serde(default, deserialize_with = "text")]
    pub headline: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub released: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub ended: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub ended_note: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub sold: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub inside: Option<String>,
}

/// A string field; `null`, a missing field and an empty string are all "not there".
fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v: Option<String> = Option::deserialize(d)?;
    Ok(v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
}

fn strings<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let v: Option<Vec<Option<String>>> = Option::deserialize(d)?;
    Ok(v.unwrap_or_default()
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

/// A JSON object of strings as (key, value) pairs in the file's order (serde_json's own map would sort them).
fn ordered<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(String, String)>, D::Error> {
    struct Pairs;
    impl<'de> serde::de::Visitor<'de> for Pairs {
        type Value = Vec<(String, String)>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an object of strings or null")
        }
        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Self::Value, D2::Error> {
            d.deserialize_map(Pairs)
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, mut m: M) -> Result<Self::Value, M::Error> {
            let mut out = Vec::new();
            while let Some((k, v)) = m.next_entry::<String, Option<String>>()? {
                if let Some(v) = v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
                    out.push((k, v));
                }
            }
            Ok(out)
        }
    }
    d.deserialize_option(Pairs)
}

fn file() -> &'static File {
    static FILE: OnceLock<File> = OnceLock::new();
    FILE.get_or_init(|| match serde_json::from_str(DATA) {
        Ok(f) => f,
        Err(e) => {
            // Checked by the tests; a broken file means no console pages, never a crash.
            eprintln!("consoles.json: {e}");
            File {
                license: None,
                systems: Vec::new(),
            }
        }
    })
}

/// Every console of the file.
pub fn all() -> &'static [Console] {
    &file().systems
}

/// Where the facts come from and under which licences, as the file says.
pub fn license() -> Option<&'static str> {
    file().license.as_deref()
}

/// ROCKNIX ids that are the same console as one of the file (other names, the hack folders, an add-on's other folder).
/// Systems without a sheet of their own (PC Engine CD, SuperGrafx, the black-and-white Neo Geo Pocket) and later models
/// with other hardware (WonderSwan Color, New 3DS) are other consoles: no page.
const ALIASES: &[(&str, &str)] = &[
    ("famicom", "nes"),
    ("nesh", "nes"),
    ("sfc", "snes"),
    ("snesh", "snes"),
    ("gbh", "gb"),
    ("gbch", "gbc"),
    ("gbah", "gba"),
    ("genesis", "megadrive"),
    ("megadrive-japan", "megadrive"),
    ("megadriveh", "megadrive"),
    ("genh", "megadrive"),
    ("ggh", "gamegear"),
    ("megacd", "segacd"),
    ("tg16", "pcengine"),
    ("wiiware", "wii"),
];

/// The file's id for a ROCKNIX system id.
pub fn canonical(system: &str) -> &str {
    ALIASES.iter().find(|(alias, _)| *alias == system).map_or(system, |(_, id)| id)
}

/// The fact sheet of a ROCKNIX system, aliases included; None for systems the file doesn't describe.
pub fn for_system(system: &str) -> Option<&'static Console> {
    let id = canonical(system);
    all().iter().find(|c| c.id == id)
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "27 November 1998" from "1998-11-27", "September 1977" from "1977-09", "1992" from "1992". Anything else
/// is returned as it is.
pub fn readable_date(s: &str) -> String {
    let parts: Vec<&str> = s.split('-').collect();
    let num = |p: &str, len: usize| {
        (p.len() == len && p.chars().all(|c| c.is_ascii_digit()))
            .then(|| p.parse::<u32>().ok())
            .flatten()
    };
    match parts.as_slice() {
        [y] if num(y, 4).is_some() => y.to_string(),
        [y, m] => match (num(y, 4), num(m, 2)) {
            (Some(y), Some(m @ 1..=12)) => format!("{} {y}", MONTHS[m as usize - 1]),
            _ => s.to_string(),
        },
        [y, m, d] => match (num(y, 4), num(m, 2), num(d, 2)) {
            (Some(y), Some(m @ 1..=12), Some(d @ 1..=31)) => format!("{d} {} {y}", MONTHS[m as usize - 1]),
            _ => s.to_string(),
        },
        _ => s.to_string(),
    }
}

/// Whether `s` is a date `readable_date` understands.
pub fn is_date(s: &str) -> bool {
    readable_date(s) != s || (s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()))
}

/// The region codes of the file, in words.
fn region_word(code: &str) -> Option<&'static str> {
    Some(match code {
        "JP" => "Japan",
        "NA" => "North America",
        "EU" => "Europe",
        "AU" => "Australia",
        "UK" => "United Kingdom",
        "IE" => "Ireland",
        "IT" => "Italy",
        "KR" | "KOR" => "South Korea",
        "BR" => "Brazil",
        "CHL" => "Chile",
        "CN" => "China",
        _ => return None,
    })
}

/// "Europe, Australia" for "EU/AU"; a code the table doesn't know stays as it is.
pub fn region_name(key: &str) -> String {
    key.split('/')
        .map(|c| region_word(c.trim()).map_or_else(|| c.trim().to_string(), str::to_string))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The short form of a region key for narrow places: "UK, IE, IT, AU".
pub fn region_codes(key: &str) -> String {
    key.split('/').map(str::trim).collect::<Vec<_>>().join(", ")
}

impl Console {
    /// The release dates, earliest first (the file's order for the same date).
    pub fn releases(&self) -> Vec<(String, String)> {
        let mut r = self.released.clone();
        r.sort_by(|a, b| a.1.cmp(&b.1));
        r
    }

    /// The small print under "Sold" on the console page: the date the number is from (when it is a date) and
    /// the file's note about it.
    pub fn sold_note(&self) -> Option<String> {
        let u = self.units_sold.as_ref()?;
        let as_of = u.as_of.as_deref().filter(|a| is_date(a)).map(|a| format!("As of {}.", readable_date(a)));
        let parts: Vec<String> = as_of.into_iter().chain(u.note.clone()).collect();
        (!parts.is_empty()).then(|| parts.join(" "))
    }

    /// The hardware rows of the console page, those that are there.
    pub fn hardware(&self) -> Vec<(&'static str, String)> {
        [
            ("CPU", &self.cpu),
            ("Graphics", &self.gpu),
            ("Memory", &self.memory),
            ("Media", &self.media),
            ("Display", &self.display),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.clone().map(|v| (k, v)))
        .collect()
    }

    /// Names of the places the facts come from, for the credit line: "Wikipedia · Wikidata · Nintendo".
    pub fn source_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Vec::new();
        for s in &self.sources {
            let name = source_name(s);
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }
}

/// Who published a source, by its address.
pub fn source_name(url: &str) -> &'static str {
    let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or_default();
    let host = host.strip_prefix("www.").unwrap_or(host);
    match host {
        h if h.ends_with("wikipedia.org") => "Wikipedia",
        "wikidata.org" => "Wikidata",
        "web.archive.org" => "Internet Archive",
        h if h.starts_with("nintendo.") => "Nintendo",
        "segaretro.org" => "Sega Retro",
        "necretro.org" => "NEC Retro",
        "playstation.com" => "PlayStation",
        "sonyinteractive.com" => "Sony Interactive",
        "theguardian.com" => "The Guardian",
        _ => "other sources",
    }
}

// ---- "On this Pocket": what plays the system here ------------------------------------------------------------

/// The name people know a libretro core by.
pub fn core_name(core: &str) -> String {
    let core = core.strip_suffix("_libretro").unwrap_or(core);
    match core {
        "gambatte" => "Gambatte",
        "mgba" => "mGBA",
        "gpsp" => "gpSP",
        "nestopia" => "Nestopia",
        "fceumm" => "FCEUmm",
        "mesen" => "Mesen",
        "quicknes" => "QuickNES",
        "snes9x" | "snes9x2010" | "snes9x2005_plus" => "Snes9x",
        "bsnes" => "bsnes",
        "genesis_plus_gx" => "Genesis Plus GX",
        "picodrive" => "PicoDrive",
        "gearsystem" => "Gearsystem",
        "smsplus" => "SMS Plus",
        "beetle_pce_fast" | "mednafen_pce_fast" => "Beetle PCE Fast",
        "beetle_supergrafx" => "Beetle SuperGrafx",
        "beetle_vb" => "Beetle VB",
        "beetle_ngp" => "Beetle NeoPop",
        "beetle_wswan" | "mednafen_wswan" => "Beetle WonderSwan",
        "beetle_psx" | "beetle_psx_hw" => "Beetle PSX",
        "beetle_saturn" => "Beetle Saturn",
        "handy" => "Handy",
        "stella" | "stella2014" => "Stella",
        "prosystem" => "ProSystem",
        "pokemini" => "PokeMini",
        "pcsx_rearmed" => "PCSX ReARMed",
        "swanstation" => "SwanStation",
        "duckstation" => "DuckStation",
        "fbneo" => "FinalBurn Neo",
        "mame" | "mame2003_plus" => "MAME",
        "mupen64plus_next" => "Mupen64Plus-Next",
        "parallel_n64" => "ParaLLEl N64",
        "ppsspp" => "PPSSPP",
        "flycast" => "Flycast",
        "melondsds" => "melonDS DS",
        "melonds" => "melonDS",
        "desmume" | "desmume2015" => "DeSmuME",
        "yabasanshiro" => "YabaSanshiro",
        "yabause" => "Yabause",
        "kronos" => "Kronos",
        "dolphin" => "Dolphin",
        "citra" => "Citra",
        "pcsx2" | "lrps2" => "PCSX2",
        "play" => "Play!",
        "dosbox_pure" => "DOSBox Pure",
        "scummvm" => "ScummVM",
        "fake08" => "FAKE-08",
        "tic80" => "TIC-80",
        "gearboy" => "Gearboy",
        "sameboy" => "SameBoy",
        "gearcoleco" => "Gearcoleco",
        other => return other.to_string(),
    }
    .to_string()
}

/// The name of a standalone emulator as ROCKNIX's es_systems.cfg calls it ("armsx2", "dolphin", "flycast-sa").
pub fn emulator_name(emulator: &str) -> String {
    let e = emulator
        .strip_suffix("-sa")
        .or_else(|| emulator.strip_suffix("sa").filter(|e| e.ends_with('-')))
        .unwrap_or(emulator);
    match e {
        "armsx2" => "ARMSX2".to_string(),
        "pcsx2" => "PCSX2".to_string(),
        "dolphin" => "Dolphin".to_string(),
        "azahar" => "Azahar".to_string(),
        "lime3ds" => "Lime3DS".to_string(),
        "citra" => "Citra".to_string(),
        "flycast" => "Flycast".to_string(),
        "ppsspp" => "PPSSPP".to_string(),
        "melonds" => "melonDS".to_string(),
        "drastic" => "DraStic".to_string(),
        "yabasanshiro" => "YabaSanshiro".to_string(),
        "mupen64plus" => "Mupen64Plus".to_string(),
        "duckstation" => "DuckStation".to_string(),
        "scummvm" => "ScummVM".to_string(),
        "dosbox" | "dosbox-x" => "DOSBox".to_string(),
        "pico8" | "pico-8" => "PICO-8".to_string(),
        other => core_name(other),
    }
}

/// "Flycast on pocket-core", "Stella in RetroArch", "ARMSX2 (standalone)"; None when es_systems.cfg names nothing.
pub fn runner_text(emulator: &str, core: &str) -> Option<String> {
    match (emulator, core) {
        ("", "") => None,
        ("pocket-core", core) => Some(format!("{} on pocket-core", core_name(core))),
        ("retroarch", "") => Some("RetroArch".to_string()),
        ("retroarch", core) => Some(format!("{} in RetroArch", core_name(core))),
        ("", core) => Some(core_name(core)),
        (emulator, _) => Some(format!("{} (standalone)", emulator_name(emulator))),
    }
}

/// What the BIOS folder has for a system that pocket-core needs a BIOS for (hostsys); None when it needs none.
pub fn bios_status(system: &str, bios_dir: &std::path::Path) -> Option<String> {
    let s = crate::hostsys::find(system)?;
    if s.bios.is_empty() {
        return None;
    }
    let parts: Vec<String> = s
        .bios
        .iter()
        .map(|b| match b.files.iter().find(|f| bios_dir.join(f).is_file()) {
            Some(f) => format!("{f} found"),
            None if s.default_without_bios => "None found; the standalone emulator doesn't need one".to_string(),
            None => {
                let (last, rest) = b.files.split_last().unwrap_or((&"", &[]));
                let names = if rest.is_empty() {
                    last.to_string()
                } else {
                    format!("{} or {last}", rest.join(", "))
                };
                format!("Missing: {names} in roms/bios")
            }
        })
        .collect();
    Some(parts.join("; "))
}

// ---- the shelf's short facts ---------------------------------------------------------------------------------

/// What the shelf knows about the system on this device, for "Your games" and "Runs on".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Here {
    pub games: usize,
    /// The game of this system played last.
    pub last: Option<String>,
    /// `runner_text`.
    pub runs_on: Option<String>,
}

/// One label/value row of the short facts. `values` are alternatives, the fullest first: the first one that
/// fits the room is shown. `keep` orders what goes when there is no room for every row (lowest first).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: &'static str,
    pub values: Vec<String>,
    pub keep: u8,
}

/// "1 game", "12 games".
pub fn games_text(n: usize) -> String {
    if n == 1 {
        "1 game".to_string()
    } else {
        format!("{n} games")
    }
}

/// The short facts under the system's name on the shelf, in the order they are shown: Released, Ended, Sold,
/// Inside (the file's ready-made lines), Your games, Runs on. Rows without a value are left out.
pub fn shelf_rows(c: &Console, here: &Here) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut push = |label: &'static str, values: Vec<String>, keep: u8| {
        let values: Vec<String> = values.into_iter().filter(|v| !v.trim().is_empty()).collect();
        if !values.is_empty() {
            rows.push(Row { label, values, keep });
        }
    };
    let l = &c.lines;
    push("Released", l.released.clone().into_iter().collect(), 6);
    // The note says more ("1995 in North America and Europe, 2003 in Japan") where it fits.
    push("Ended", l.ended_note.clone().into_iter().chain(l.ended.clone()).collect(), 3);
    push("Sold", l.sold.clone().into_iter().collect(), 4);
    push("Inside", l.inside.clone().into_iter().collect(), 1);
    rows.extend(here_rows(here));
    rows
}

/// The short facts of this device alone, Your games and Runs on: the end of `shelf_rows`, and all there is for a
/// system without a fact sheet (Sega CD on the first-day run, 2026-10-26, left the room under its name empty).
pub fn here_rows(here: &Here) -> Vec<Row> {
    let count = games_text(here.games);
    let mut yours = Vec::new();
    if let Some(last) = &here.last {
        yours.push(format!("{count} · last: {last}"));
        yours.push(format!("{} · last: {last}", here.games));
    }
    yours.push(count);
    let mut rows = vec![Row {
        label: "Your games",
        values: yours,
        keep: 5,
    }];
    // "Runs on", as the page of that name: "On this Pocket", the console page's heading, has no room here in German.
    if let Some(runs_on) = here.runs_on.clone().filter(|r| !r.trim().is_empty()) {
        rows.push(Row {
            label: "Runs on",
            values: vec![runs_on],
            keep: 2,
        });
    }
    rows
}

/// The rows that fit into `room` pixels at `row_h` each: all of them when they do, else the ones to keep most,
/// still in their order.
pub fn rows_that_fit(rows: &[Row], room: i32, row_h: i32) -> Vec<Row> {
    let n = if row_h <= 0 { 0 } else { (room / row_h).max(0) as usize };
    if rows.len() <= n {
        return rows.to_vec();
    }
    let mut by_keep: Vec<usize> = (0..rows.len()).collect();
    by_keep.sort_by_key(|&i| std::cmp::Reverse(rows[i].keep));
    let mut kept: Vec<usize> = by_keep.into_iter().take(n).collect();
    kept.sort_unstable();
    kept.into_iter().map(|i| rows[i].clone()).collect()
}

/// The value to show of a row in `max` pixels: the first alternative that fits at `px` or else at `small`, else the
/// last one shortened with an ellipsis at `small`. Returns (text, size). `width` measures a text at a size.
pub fn pick_value(values: &[String], max: i32, px: u32, small: u32, mut width: impl FnMut(&str, u32) -> i32) -> (String, u32) {
    for v in values {
        for size in [px, small] {
            if width(v, size) <= max {
                return (v.clone(), size);
            }
        }
    }
    let last = values.last().cloned().unwrap_or_default();
    let mut chars: Vec<char> = last.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate = chars.iter().collect::<String>().trim_end().to_string() + "…";
        if width(&candidate, small) <= max {
            return (candidate, small);
        }
    }
    (String::new(), small)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// Every id the file has, and the ROCKNIX ids of the library that should find one.
    const IDS: &[&str] = &[
        "3do",
        "3ds",
        "atari2600",
        "atari7800",
        "atarijaguar",
        "atarilynx",
        "colecovision",
        "dreamcast",
        "fds",
        "gamecube",
        "gamegear",
        "gb",
        "gba",
        "gbc",
        "intellivision",
        "mastersystem",
        "megadrive",
        "n64",
        "nds",
        "neocd",
        "neogeo",
        "nes",
        "ngpc",
        "pcengine",
        "pokemini",
        "ps2",
        "psp",
        "psx",
        "saturn",
        "sega32x",
        "segacd",
        "sg-1000",
        "snes",
        "supergrafx",
        "vectrex",
        "virtualboy",
        "wii",
        "wonderswan",
        "xbox",
    ];

    #[test]
    fn the_file_loads_with_all_39_consoles() {
        let f: File = serde_json::from_str(DATA).expect("consoles.json parses");
        assert_eq!(f.systems.len(), 39);
        assert_eq!(all().len(), 39);
        assert!(license().is_some_and(|l| l.contains("Wikidata")));
        for c in all() {
            assert!(!c.name.is_empty(), "{}", c.id);
            assert!(c.lines.headline.is_some(), "{}: headline", c.id);
            assert!(c.summary.is_some(), "{}: summary", c.id);
            assert!(!c.sources.is_empty(), "{}: sources", c.id);
            assert!(!c.released.is_empty(), "{}: released", c.id);
        }
    }

    #[test]
    fn every_system_id_resolves_to_its_own_sheet() {
        for id in IDS {
            assert_eq!(for_system(id).map(|c| c.id.as_str()), Some(*id), "{id}");
        }
        let mut ids: Vec<&str> = all().iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, IDS, "no console without a test, none twice");
    }

    #[test]
    fn rocknix_aliases_find_the_same_console_and_unknown_systems_find_none() {
        for (alias, id) in [
            ("genesis", "megadrive"),
            ("megadrive-japan", "megadrive"),
            ("tg16", "pcengine"),
            ("famicom", "nes"),
            ("sfc", "snes"),
            ("gbh", "gb"),
            ("gbch", "gbc"),
            ("gbah", "gba"),
            ("ggh", "gamegear"),
            ("wiiware", "wii"),
            ("megacd", "segacd"),
        ] {
            assert_eq!(for_system(alias).map(|c| c.id.as_str()), Some(id), "{alias}");
        }
        for other in [
            "pcenginecd",
            "ngp",
            "wonderswancolor",
            "n3ds",
            "ports",
            "pico-8",
            "scummvm",
            "pc",
            "",
            "NES",
        ] {
            assert!(for_system(other).is_none(), "{other} has no page");
        }
        for (alias, _) in ALIASES {
            assert!(for_system(alias).is_some(), "{alias} points at a console of the file");
        }
    }

    #[test]
    fn values_are_kept_exactly_as_the_file_has_them() {
        let dc = for_system("dreamcast").unwrap();
        assert_eq!(dc.lines.sold.as_deref(), Some("8.2 million by March 2001 (Sega)"));
        assert_eq!(dc.cpu.as_deref(), Some("Hitachi SH-4, 200 MHz"));
        assert_eq!(
            dc.launch_price,
            vec![("JP".into(), "¥29,800".into()), ("NA".into(), "US$199".into()), ("UK".into(), "£200".into())],
            "in the file's order"
        );
        let nes = for_system("nes").unwrap();
        assert_eq!(nes.lines.ended_note.as_deref(), Some("1995 in North America and Europe, 2003 in Japan"));
    }

    #[test]
    fn missing_facts_are_none_not_text() {
        let gbc = for_system("gbc").unwrap();
        assert!(gbc.units_sold.is_none(), "no figure of its own");
        assert!(gbc.lines.sold.is_some(), "but a line saying it is counted with the Game Boy");
        assert!(gbc.gpu.is_none());
        let nds = for_system("nds").unwrap();
        assert!(nds.discontinued.is_none() && nds.lines.ended.is_none());
        let sg = for_system("sg-1000").unwrap();
        assert!(sg.units_sold.is_none() && sg.lines.sold.is_none());
        assert!(sg.sold_note().is_none());
        assert!(for_system("dreamcast").unwrap().display.is_none());
        assert!(!for_system("dreamcast").unwrap().hardware().iter().any(|(k, _)| *k == "Display"));
    }

    #[test]
    fn dates_read_as_words() {
        assert_eq!(readable_date("1998-11-27"), "27 November 1998");
        assert_eq!(readable_date("1977-09"), "September 1977");
        assert_eq!(readable_date("1992"), "1992");
        assert_eq!(
            readable_date("2016-03-31 (last published Nintendo figure)"),
            "2016-03-31 (last published Nintendo figure)"
        );
        assert_eq!(readable_date("1999-13"), "1999-13");
        assert!(is_date("1992") && is_date("2001-03-31") && !is_date("undated estimate"));
        for c in all() {
            for (_, d) in &c.released {
                assert!(is_date(d), "{}: {d}", c.id);
            }
            if let Some(d) = &c.discontinued {
                assert!(is_date(d), "{}: {d}", c.id);
            }
        }
    }

    #[test]
    fn regions_read_as_words_and_releases_come_in_date_order() {
        assert_eq!(region_name("NA"), "North America");
        assert_eq!(region_name("EU/AU"), "Europe, Australia");
        assert_eq!(region_name("CHL/BR"), "Chile, Brazil");
        assert_eq!(region_codes("UK/IE/IT/AU"), "UK, IE, IT, AU");
        assert_eq!(region_name("XX"), "XX");
        let sms = for_system("mastersystem").unwrap();
        let order: Vec<String> = sms.releases().into_iter().map(|r| r.0).collect();
        assert_eq!(order, ["NA", "EU", "JP", "KR", "BR"]);
        for c in all() {
            for (k, _) in c.released.iter().chain(&c.launch_price) {
                for code in k.split('/') {
                    assert!(region_word(code).is_some(), "{}: region {code} has a name", c.id);
                }
            }
        }
    }

    #[test]
    fn the_sold_note_starts_with_its_date_when_it_has_one() {
        let dc = for_system("dreamcast").unwrap().sold_note().unwrap();
        assert!(dc.starts_with("As of 31 March 2001. Sega Annual Report 2001"), "{dc}");
        let sat = for_system("saturn").unwrap().sold_note().unwrap();
        assert!(sat.starts_with("Worldwide total"), "no date: just the note: {sat}");
        let ps2 = for_system("ps2").unwrap().sold_note().unwrap();
        assert!(!ps2.contains("As of"), "an undated figure gets no date: {ps2}");
    }

    #[test]
    fn sources_are_named_after_their_publisher() {
        assert_eq!(
            for_system("dreamcast").unwrap().source_names(),
            ["Wikipedia", "Sega Retro", "Internet Archive", "Wikidata"]
        );
        for c in all() {
            assert!(!c.source_names().contains(&"other sources"), "{}: {:?}", c.id, c.sources);
        }
    }

    #[test]
    fn runners_read_like_the_mockup() {
        assert_eq!(runner_text("pocket-core", "flycast").as_deref(), Some("Flycast on pocket-core"));
        assert_eq!(runner_text("retroarch", "gearsystem").as_deref(), Some("Gearsystem in RetroArch"));
        assert_eq!(runner_text("armsx2", "armsx2").as_deref(), Some("ARMSX2 (standalone)"));
        assert_eq!(runner_text("flycast-sa", "flycast-sa").as_deref(), Some("Flycast (standalone)"));
        assert_eq!(runner_text("", ""), None);
    }

    #[test]
    fn bios_status_only_for_systems_that_need_one() {
        let dir = TestDir::new("consoles-bios");
        assert_eq!(bios_status("dreamcast", &dir), None, "Flycast's own BIOS: nothing to say");
        assert_eq!(bios_status("nes", &dir), None);
        let missing = bios_status("saturn", &dir).unwrap();
        assert!(missing.contains("standalone"), "{missing}");
        std::fs::write(dir.join("sega_101.bin"), b"x").unwrap();
        assert_eq!(bios_status("saturn", &dir).as_deref(), Some("sega_101.bin found"));
        assert!(bios_status("segacd", &dir).unwrap().starts_with("Missing: bios_CD_U.bin"));
    }

    fn here() -> Here {
        Here {
            games: 1,
            last: Some("Sonic Adventure 2".into()),
            runs_on: Some("Flycast on pocket-core".into()),
        }
    }

    #[test]
    fn shelf_rows_follow_the_mockup_and_leave_out_what_is_missing() {
        let labels = |id: &str| shelf_rows(for_system(id).unwrap(), &here()).into_iter().map(|r| r.label).collect::<Vec<_>>();
        assert_eq!(labels("dreamcast"), ["Released", "Ended", "Sold", "Inside", "Your games", "Runs on"]);
        assert_eq!(labels("nds"), ["Released", "Sold", "Inside", "Your games", "Runs on"], "no end date");
        assert_eq!(labels("sg-1000"), ["Released", "Ended", "Inside", "Your games", "Runs on"], "no units");
        assert!(
            labels("gbc").contains(&"Sold"),
            "the Game Boy Color's line says it is counted with the Game Boy"
        );
        let rows = shelf_rows(for_system("nes").unwrap(), &here());
        let ended = rows.iter().find(|r| r.label == "Ended").unwrap();
        assert_eq!(
            ended.values,
            ["1995 in North America and Europe, 2003 in Japan", "1995"],
            "the note first, the year where it doesn't fit"
        );
        let dc = shelf_rows(for_system("dreamcast").unwrap(), &here());
        assert_eq!(dc[4].values[0], "1 game · last: Sonic Adventure 2");
        let none = shelf_rows(
            for_system("dreamcast").unwrap(),
            &Here {
                games: 3,
                last: None,
                runs_on: None,
            },
        );
        assert_eq!(none.iter().find(|r| r.label == "Your games").unwrap().values, ["3 games"]);
        assert!(!none.iter().any(|r| r.label == "Runs on"));
    }

    /// A system the fact sheets don't cover still has its own rows: Split shows them under its name, never an empty room.
    #[test]
    fn a_system_without_a_sheet_keeps_your_games_and_runs_on() {
        assert!(for_system("pcenginecd").is_none(), "pick another system without a fact sheet for this test");
        let here = Here {
            games: 1,
            last: None,
            runs_on: Some("RetroArch · Genesis Plus GX".into()),
        };
        let rows = here_rows(&here);
        let labels: Vec<&str> = rows.iter().map(|r| r.label).collect();
        assert_eq!(labels, ["Your games", "Runs on"]);
        assert_eq!(rows[0].values, ["1 game"]);
        let bare = here_rows(&Here {
            games: 2,
            last: None,
            runs_on: Some(" ".into()),
        });
        assert_eq!(bare.iter().map(|r| r.label).collect::<Vec<_>>(), ["Your games"], "a blank Runs on is left out");
    }

    #[test]
    fn no_row_and_no_page_text_ever_says_none_or_null() {
        for c in all() {
            let text: Vec<String> = shelf_rows(c, &Here::default())
                .into_iter()
                .flat_map(|r| r.values)
                .chain(c.hardware().into_iter().map(|h| h.1))
                .chain(c.releases().into_iter().map(|r| readable_date(&r.1)))
                .chain(c.sold_note())
                .chain(c.lines.headline.clone())
                .collect();
            for t in text {
                let lower = t.to_lowercase();
                assert!(!t.is_empty(), "{}: empty value", c.id);
                assert!(lower != "none" && !lower.contains("null") && !lower.contains("undefined"), "{}: {t}", c.id);
            }
        }
    }

    #[test]
    fn rows_that_do_not_fit_go_by_what_matters_least() {
        let rows = shelf_rows(for_system("dreamcast").unwrap(), &here());
        assert_eq!(rows_that_fit(&rows, 6 * 46, 46).len(), 6, "all six in the Split look's room");
        let four: Vec<&str> = rows_that_fit(&rows, 4 * 46 + 20, 46).iter().map(|r| r.label).collect();
        assert_eq!(
            four,
            ["Released", "Ended", "Sold", "Your games"],
            "Inside and Runs on go first, the order stays"
        );
        assert!(rows_that_fit(&rows, 30, 46).is_empty());
        assert!(rows_that_fit(&rows, 500, 0).is_empty());
    }

    #[test]
    fn a_value_too_long_takes_the_shorter_alternative_then_the_smaller_size() {
        let w = |s: &str, px: u32| s.chars().count() as i32 * px as i32 / 2;
        let values = vec!["1995 in North America and Europe, 2003 in Japan".to_string(), "1995".to_string()];
        assert_eq!(pick_value(&values, 2000, 28, 24, w), (values[0].clone(), 28));
        assert_eq!(
            pick_value(&values, 600, 28, 24, w),
            (values[0].clone(), 24),
            "the note at the smaller size first"
        );
        assert_eq!(pick_value(&values, 300, 28, 24, w), ("1995".to_string(), 28));
        let one = vec!["Emotion Engine 295 MHz · GS".to_string()];
        let (t, px) = pick_value(&one, 200, 28, 24, w);
        assert!(t.ends_with('…') && w(&t, 24) <= 200 && px == 24, "{t}");
    }
}

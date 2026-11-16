//! Phone games (J2ME) at their own screen size. FreeJ2ME-Plus runs every game at one size, its core option
//! `freej2me_resolution` (240×320 unless changed), and has no way to find a game's own: a 176×208 Nokia game then
//! draws into the top-left corner of a 240×320 phone screen and the rest stays empty (RG35XX Plus, 14.11.2026).
//!
//! Before a FreeJ2ME start pocket-ui finds the game's size (`detect`) and sets it as that core option, in
//! RetroArch's options file for the game when the player keeps one, else in the core's (`set_resolution`).
//! Runs on › Screen size (`emu_choice::RA_SETTINGS`, per game or console in system.cfg) can fix one size instead
//! of Automatic. Where the size comes from, first match wins:
//! 1. a size in the file name ("Snake (176x208).jar"), as phone game collections name their files;
//! 2. the game's manifest or a `.jad` beside it: `Nokia-MIDlet-Original-Display-Size` and the other screen-size keys;
//! 3. full-screen pictures in the jar: two or more of one phone screen size, or one when nothing else fits;
//! 4. else 240×320, FreeJ2ME's own default.
//!
//! The picture then fills the screen like other handhelds' (`integer`): by whole numbers when at least twice the size
//! fits (176×208 on 640×480: 352×416), else as large as fits in its shape, through a RetroArch override for the game.
//! An Integer scaling chosen in Runs on wins over this.
use std::path::{Path, PathBuf};

/// The system.cfg key (`j2me["<game>"].j2mesize`, `j2me.j2mesize`).
pub const KEY: &str = "j2mesize";
/// Its value for "find it" (also when unset: `emu_choice::Cfg::get` reads "auto" as unset).
pub const AUTO: &str = "auto";
/// FreeJ2ME's own size, for games that say nothing.
pub const DEFAULT: (u32, u32) = (240, 320);

/// The sizes `freej2me_resolution` takes in ROCKNIX's FreeJ2ME-Plus (`strings freej2me_libretro.so`, RG35XX Plus,
/// 14.11.2026). RetroArch drops a value not in the core's list and the game gets 240×320 again, so every size
/// pocket-ui writes is one of these.
pub const SIZES: &[(u32, u32)] = &[
    (96, 65),
    (101, 64),
    (101, 80),
    (128, 128),
    (130, 130),
    (120, 160),
    (128, 160),
    (132, 176),
    (208, 173),
    (176, 208),
    (176, 220),
    (220, 176),
    (208, 208),
    (180, 320),
    (320, 180),
    (240, 240),
    (208, 320),
    (240, 320),
    (320, 240),
    (240, 400),
    (400, 240),
    (240, 432),
    (240, 480),
    (360, 360),
    (352, 416),
    (360, 640),
    (640, 360),
    (640, 480),
    (480, 800),
    (800, 480),
];

/// RetroArch's per-core config folder on ROCKNIX (`rgui_config_directory`), FreeJ2ME-Plus's under it: the core's
/// options `FreeJ2ME-Plus.opt`, a game's own `<game>.opt` (`game_specific_options`), a game's override `<game>.cfg`.
pub const CONFIG: &str = "/storage/.config/retroarch/config/FreeJ2ME-Plus";
const CORE_OPT: &str = "FreeJ2ME-Plus.opt";
const OPTION: &str = "freej2me_resolution";
/// RetroArch's own config: the screen it fills (`video_fullscreen_x`/`_y`, set by ROCKNIX per device).
pub const RETROARCH_CFG: &str = "/storage/.config/retroarch/retroarch.cfg";
/// The first line of an override pocket-ui wrote; one without it is the player's and is never touched.
const OURS: &str = "# pocket-ui:";

/// Where a game's size came from, for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    Name,
    Manifest,
    Jad,
    Pictures,
    Default,
    /// Fixed in Runs on › Screen size.
    Chosen,
}

/// "176x208", "176,208", "176 x 208", "176×208", "176*208" → (176, 208); None for anything else.
fn pair(text: &str) -> Option<(u32, u32)> {
    let mut nums = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>().ok());
    let (w, h) = (nums.next()??, nums.next()??);
    (nums.next().is_none() && (64..=1024).contains(&w) && (64..=1024).contains(&h)).then_some((w, h))
}

/// A size in a file name: two numbers of two to four digits joined by an x or ×, standing on their own ("Bounce
/// 128x160.jar", "Game_240x320_nokia.jar"); never one inside a longer number or word ("v1.2x3").
pub fn from_name(name: &str) -> Option<(u32, u32)> {
    let b: Vec<char> = name.chars().collect();
    let digits = |from: usize, step: isize| {
        let mut i = from as isize;
        let mut n = 0;
        while i >= 0 && (i as usize) < b.len() && b[i as usize].is_ascii_digit() {
            n += 1;
            i += step;
        }
        n
    };
    for (i, c) in b.iter().enumerate() {
        if !matches!(c, 'x' | 'X' | '×') || i == 0 {
            continue;
        }
        let (left, right) = (digits(i - 1, -1), digits(i + 1, 1));
        if !(2..=4).contains(&left) || !(2..=4).contains(&right) {
            continue;
        }
        let edge = |at: isize| at < 0 || at as usize >= b.len() || !b[at as usize].is_alphanumeric() || b[at as usize] == '_';
        if !edge(i as isize - 1 - left as isize) || !edge((i + 1 + right) as isize) {
            continue;
        }
        let text: String = b[i - left..i + 1 + right].iter().collect();
        if let Some(size) = pair(&text) {
            return Some(size);
        }
    }
    None
}

/// The `Key: value` lines of a manifest or `.jad`, continuation lines (starting with a space) joined.
fn attributes(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(more) = line.strip_prefix(' ') {
            if let Some(last) = out.last_mut() {
                last.1.push_str(more);
            }
        } else if let Some((k, v)) = line.split_once(':') {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

/// The game's screen size as its manifest or `.jad` states it. Nokia's `Nokia-MIDlet-Original-Display-Size` (the size
/// the game was made for; `…-Target-Display-Size` is only what a bigger phone should scale it to) first, then any
/// other key naming a screen, display or canvas size or resolution, then a width/height pair of keys
/// ("LGE-MIDlet-Width"/"LGE-MIDlet-Height").
pub fn from_manifest(text: &str) -> Option<(u32, u32)> {
    let attrs = attributes(text);
    let lower = |k: &str| k.to_ascii_lowercase();
    if let Some(s) = attrs.iter().find(|(k, _)| lower(k) == "nokia-midlet-original-display-size").and_then(|(_, v)| pair(v)) {
        return Some(s);
    }
    let names_size = |k: &str| {
        let k = lower(k).replace(['-', '_', ' '], "");
        !k.contains("target") && ["displaysize", "screensize", "canvassize", "resolution"].iter().any(|n| k.contains(n))
    };
    if let Some(s) = attrs.iter().filter(|(k, _)| names_size(k)).find_map(|(_, v)| pair(v)) {
        return Some(s);
    }
    attrs.iter().find_map(|(k, w)| {
        let lk = lower(k);
        let prefix = lk.strip_suffix("width")?;
        if lk.contains("target") || lk.contains("icon") {
            return None;
        }
        let h = attrs.iter().find(|(k2, _)| lower(k2) == format!("{prefix}height"))?;
        let (w, h) = (w.trim().parse::<u32>().ok()?, h.1.trim().parse::<u32>().ok()?);
        ((64..=1024).contains(&w) && (64..=1024).contains(&h)).then_some((w, h))
    })
}

/// The screen size the jar's pictures point to: sizes of whole phone screens (in `SIZES`, not square: square ones are
/// textures and tiles), the most frequent, the larger on a tie. A 360×640 touch game has 360×640 backgrounds.
pub fn from_pictures(sizes: &[(u32, u32)]) -> Option<(u32, u32)> {
    let screen = |s: &(u32, u32)| s.0 != s.1 && s.0.max(s.1) >= 160 && SIZES.contains(s);
    let mut best: Option<((u32, u32), usize)> = None;
    for s in sizes.iter().filter(|s| screen(s)) {
        let n = sizes.iter().filter(|t| *t == s).count();
        let better = match best {
            None => true,
            Some((b, bn)) => n > bn || (n == bn && s.0 * s.1 > b.0 * b.1),
        };
        if better {
            best = Some((*s, n));
        }
    }
    best.map(|(s, _)| s)
}

/// A PNG's width and height from its first 24 bytes.
fn png_size(head: &[u8]) -> Option<(u32, u32)> {
    if !head.starts_with(b"\x89PNG\r\n\x1a\n") || head.get(12..16)? != b"IHDR" {
        return None;
    }
    let be = |at: usize| Some(u32::from_be_bytes(head.get(at..at + 4)?.try_into().ok()?));
    Some((be(16)?, be(20)?))
}

/// The size as FreeJ2ME-Plus takes it: the size itself when the core lists it, else the nearest listed size of the
/// same shape within a tenth on each side (176×204 → 176×208); None when none is near.
pub fn snap(size: (u32, u32)) -> Option<(u32, u32)> {
    if SIZES.contains(&size) {
        return Some(size);
    }
    let near = |a: u32, b: u32| a.abs_diff(b) * 10 <= b;
    SIZES
        .iter()
        .filter(|s| (s.0 >= s.1) == (size.0 >= size.1) && near(size.0, s.0) && near(size.1, s.1))
        .min_by_key(|s| s.0.abs_diff(size.0) + s.1.abs_diff(size.1))
        .copied()
}

/// The largest jar read to look inside (phone games are a few hundred KB, the biggest a few MB).
const MAX_JAR: u64 = 16 << 20;

/// The game's screen size and where it came from (see the module text). Reads the jar's directory, its manifest and
/// the first bytes of its pictures; a few milliseconds for a phone game.
pub fn detect(jar: &Path) -> ((u32, u32), From) {
    let name = jar.file_stem().and_then(|n| n.to_str()).unwrap_or("");
    if let Some(s) = from_name(name).and_then(snap) {
        return (s, From::Name);
    }
    if let Some(s) = manifest(jar).and_then(|m| from_manifest(&m)).and_then(snap) {
        return (s, From::Manifest);
    }
    let bytes = crate::archive::read_bounded(jar, MAX_JAR).unwrap_or_default();
    if let Some(s) = jad(jar).and_then(|t| from_manifest(&t)).and_then(snap) {
        return (s, From::Jad);
    }
    let pictures: Vec<(u32, u32)> = crate::archive::heads(&bytes, |n| n.to_ascii_lowercase().ends_with(".png"), 24)
        .iter()
        .filter_map(|(_, h)| png_size(h))
        .collect();
    if let Some(s) = from_pictures(&pictures) {
        return (s, From::Pictures);
    }
    (DEFAULT, From::Default)
}

/// The `.jad` beside the jar (same name, any case of the extension), as text.
fn jad(jar: &Path) -> Option<String> {
    let stem = jar.file_stem()?.to_str()?;
    let dir = jar.parent()?;
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let p = e.path();
        let ok = p.file_stem().and_then(|s| s.to_str()) == Some(stem)
            && p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("jad"));
        ok.then(|| std::fs::read(&p).ok().map(|b| String::from_utf8_lossy(&b).into_owned())).flatten()
    })
}

/// `key = "value"` set in RetroArch config text: the line replaced where it is, else added at the end.
fn with_line(text: &str, key: &str, value: &str) -> String {
    let line = format!("{key} = \"{value}\"");
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            if l.split('=').next().map(str::trim) == Some(key) {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        out.push(line);
    }
    out.join("\n") + "\n"
}

/// The game's options file (`<game>.opt`, RetroArch names it after the content file without its extension).
fn game_opt(dir: &Path, rom: &Path) -> Option<PathBuf> {
    Some(dir.join(format!("{}.opt", rom.file_stem()?.to_str()?)))
}

/// The game's override (`<game>.cfg`).
pub fn game_override(dir: &Path, rom: &Path) -> Option<PathBuf> {
    Some(dir.join(format!("{}.cfg", rom.file_stem()?.to_str()?)))
}

/// Sets `freej2me_resolution` for this start: in the game's own options file when the player keeps one (RetroArch
/// reads only that one then), else in the core's, which every phone game without its own file reads. Every other
/// option stays as it is; a file that already says this size is not written.
pub fn set_resolution(dir: &Path, rom: &Path, size: (u32, u32)) -> std::io::Result<()> {
    let file = game_opt(dir, rom).filter(|f| f.exists()).unwrap_or_else(|| dir.join(CORE_OPT));
    let now = std::fs::read_to_string(&file).unwrap_or_default();
    let next = with_line(&now, OPTION, &format!("{}x{}", size.0, size.1));
    if next == now {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    crate::fsutil::write_atomic(&file, next.as_bytes())
}

/// FreeJ2ME's name for the game's own folders (`MIDletLoader.suitename`): the first MIDlet's name from `MIDlet-1`
/// ("3D Bomberman, i.png, BomberMan3D" → "3D Bomberman"), colons taken out.
pub fn suite_name(manifest: &str) -> Option<String> {
    let (_, v) = attributes(manifest).into_iter().find(|(k, _)| k == "MIDlet-1")?;
    let name = v.split([',', '/']).next()?.trim().replace(':', "");
    (!name.is_empty() && !name.contains("..") && !name.contains('/')).then_some(name)
}

/// FreeJ2ME's own settings for the game, `<game folder>/freej2me/config/<suite>/game.conf` (the save folder RetroArch
/// gives it, the content's on ROCKNIX). Written at its first start, and from then on its size wins over the core option
/// (RG35XX Plus, 14.11.2026: a game.conf of 240×320 kept 3D Bomberman at 240×320 with the option at 176×208).
pub fn game_conf(rom: &Path, suite: &str) -> Option<PathBuf> {
    Some(rom.parent()?.join("freej2me").join("config").join(suite).join("game.conf"))
}

/// The size in the game's game.conf, when there is one (FreeJ2ME writes it with the size of the first start). Its other
/// lines stay; without a game.conf nothing is written, the core option is what FreeJ2ME takes then.
pub fn set_game_conf(file: &Path, size: (u32, u32)) -> std::io::Result<()> {
    let Ok(now) = std::fs::read_to_string(file) else { return Ok(()) };
    let mut out: Vec<String> = now
        .lines()
        .filter(|l| !l.starts_with("scrwidth:") && !l.starts_with("scrheight:"))
        .map(str::to_string)
        .collect();
    out.push(format!("scrheight:{}", size.1));
    out.push(format!("scrwidth:{}", size.0));
    out.sort();
    let next = out.join("\n") + "\n";
    if next == now {
        return Ok(());
    }
    crate::fsutil::write_atomic(file, next.as_bytes())
}

/// The jar's manifest, as text.
fn manifest(jar: &Path) -> Option<String> {
    let bytes = crate::archive::read_bounded(jar, MAX_JAR).ok()?;
    let m = crate::archive::heads(&bytes, |n| n.eq_ignore_ascii_case("META-INF/MANIFEST.MF"), 64 << 10);
    m.first().map(|(_, m)| String::from_utf8_lossy(m).into_owned())
}

/// Whole-number scaling for a `size` picture on a `screen`: when at least twice the size fits, every phone pixel is
/// then the same number of screen pixels (176×208 on 640×480: ×2, 352×416). Less than that (240×320 on 480 lines)
/// stays as large as fits; ×1 would leave a third of the screen empty.
pub fn integer(size: (u32, u32), screen: (u32, u32)) -> bool {
    (screen.0 / size.0).min(screen.1 / size.1) >= 2
}

/// Where RetroArch puts a `size` picture on a `screen`: x, y, width, height. Integer as `integer` says, else as large
/// as fits in the picture's shape (`aspect_ratio_index` "core provided"), centred.
pub fn placement(size: (u32, u32), screen: (u32, u32)) -> (u32, u32, u32, u32) {
    let (w, h) = if integer(size, screen) {
        let n = (screen.0 / size.0).min(screen.1 / size.1);
        (size.0 * n, size.1 * n)
    } else if size.0 as u64 * screen.1 as u64 >= size.1 as u64 * screen.0 as u64 {
        (screen.0, (screen.0 as u64 * size.1 as u64 / size.0 as u64) as u32)
    } else {
        ((screen.1 as u64 * size.0 as u64 / size.1 as u64) as u32, screen.1)
    };
    ((screen.0 - w) / 2, (screen.1 - h) / 2, w, h)
}

/// The override for whole-number scaling, marked as pocket-ui's.
pub const INTEGER_OVERRIDE: &str = "# pocket-ui: phone game at whole-number scale (its screen size fits twice or more)\n\
video_scale_integer = \"true\"\n";

/// The game's override as `integer` asks: written when on, taken away when off, and only ever pocket-ui's own (an
/// override the player saved in RetroArch's menu stays as it is).
pub fn apply_integer(dir: &Path, rom: &Path, on: bool) -> std::io::Result<()> {
    let Some(file) = game_override(dir, rom) else { return Ok(()) };
    let now = std::fs::read_to_string(&file).ok();
    let ours = now.as_deref().is_some_and(|t| t.starts_with(OURS));
    match (on, now.is_some()) {
        (true, false) => {
            std::fs::create_dir_all(dir)?;
            crate::fsutil::write_atomic(&file, INTEGER_OVERRIDE.as_bytes())
        }
        (true, true) if ours && now.as_deref() != Some(INTEGER_OVERRIDE) => crate::fsutil::write_atomic(&file, INTEGER_OVERRIDE.as_bytes()),
        (false, true) if ours => std::fs::remove_file(&file),
        _ => Ok(()),
    }
}

/// The screen RetroArch fills, from its config (ROCKNIX sets it per device); None when it doesn't say.
pub fn screen(retroarch_cfg: &str) -> Option<(u32, u32)> {
    let get = |key: &str| {
        retroarch_cfg.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').parse::<u32>().ok()).flatten()
        })
    };
    Some((get("video_fullscreen_x").filter(|w| *w > 0)?, get("video_fullscreen_y").filter(|h| *h > 0)?))
}

/// The size for this start: the one fixed in Runs on › Screen size, else `detect`'s.
pub fn chosen(value: &str, rom: &Path) -> ((u32, u32), From) {
    match pair(value).and_then(snap) {
        Some(s) if value != AUTO => (s, From::Chosen),
        _ => detect(rom),
    }
}

/// Before a FreeJ2ME start: the game's size as the core option, and whole-number scaling where it fits unless Runs on
/// sets Integer scaling for the game or console.
pub fn before_start(system: &str, core: &str, rom: &Path) {
    if !core.starts_with("freej2me") || crate::proc::mock() {
        return;
    }
    use crate::emu_choice::{self as ec, Level};
    let cfg = ec::Cfg::read(Path::new(ec::SYSTEM_CFG));
    let file = rom.file_name().and_then(|f| f.to_str());
    let Some(setting) = ec::RA_SETTINGS.iter().find(|s| s.key == KEY) else { return };
    let value = ec::effective_value(&cfg, system, file, setting);
    let t = std::time::Instant::now();
    let (size, from) = chosen(&value, rom);
    let dir = Path::new(CONFIG);
    if let Err(e) = set_resolution(dir, rom, size) {
        eprintln!("j2me: screen size for {}: {e}", rom.display());
    }
    if let Some(conf) = manifest(rom).as_deref().and_then(suite_name).and_then(|n| game_conf(rom, &n)) {
        if let Err(e) = set_game_conf(&conf, size) {
            eprintln!("j2me: screen size in {}: {e}", conf.display());
        }
    }
    // The player's own Integer scaling (game, console or global, as setsettings.sh reads it) wins.
    let theirs = file.is_some_and(|f| cfg.get(&ec::key(system, Some(f), Level::Game, "integerscale")).is_some())
        || cfg.get(&ec::key(system, None, Level::Console, "integerscale")).is_some()
        || ec::below_console(&cfg, "integerscale").is_some();
    let screen = screen(&std::fs::read_to_string(RETROARCH_CFG).unwrap_or_default());
    let on = !theirs && screen.is_some_and(|s| integer(size, s));
    if let Err(e) = apply_integer(dir, rom, on) {
        eprintln!("j2me: scaling override for {}: {e}", rom.display());
    }
    println!(
        "j2me: {} at {}x{} ({from:?}, {} ms), whole-number scale {on}",
        rom.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
        size.0,
        size.1,
        t.elapsed().as_millis()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A jar (zip) with these entries, deflated like a real one.
    fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
            let at = out.len() as u32;
            let crc = crc32fast::hash(data);
            let head = |sig: &[u8], v: &mut Vec<u8>| {
                v.extend_from_slice(sig);
            };
            head(b"PK\x03\x04", &mut out);
            out.extend_from_slice(&[20, 0, 0, 0, 8, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&packed);
            head(b"PK\x01\x02", &mut central);
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&at.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_at = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    /// A PNG's first bytes (signature and IHDR) with this size, then some picture data.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 3, 0, 0, 0]);
        v.extend(std::iter::repeat_n(7u8, 4000));
        v
    }

    fn write_jar(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, jar(entries)).unwrap();
        p
    }

    /// The 3D Bomberman case (RG35XX Plus, 14.11.): a Nokia 176×208 game ran at 240×320 in the corner.
    #[test]
    fn a_nokia_game_runs_at_the_size_its_manifest_names_not_240x320() {
        let dir = crate::testutil::TestDir::new("j2me-size-nokia");
        let manifest = b"Manifest-Version: 1.0\r\nMIDlet-Name: Bomber\r\nMicroEdition-Profile: MIDP-1.0\r\n\
Nokia-MIDlet-Original-Display-Size: 176,208\r\nNokia-MIDlet-Target-Display-Size: 352,416\r\n";
        let t1 = png(128, 128);
        let jar = write_jar(&dir, "Bomber.jar", &[("META-INF/MANIFEST.MF", manifest), ("t1.png", &t1)]);
        assert_eq!(detect(&jar), ((176, 208), From::Manifest), "the original size, never the target size");
    }

    /// The Diamond Rush case: a touch game with no size key and 360×640 backgrounds; square tiles don't count.
    #[test]
    fn a_game_without_a_size_key_gets_its_full_screen_pictures_size() {
        let dir = crate::testutil::TestDir::new("j2me-size-pictures");
        let manifest = b"Manifest-Version: 1.0\nMIDlet-Name: Diamonds\nMIDlet-Touch-Support: true\n";
        let (bg, tile, wide) = (png(360, 640), png(40, 40), png(240, 40));
        let jar = write_jar(
            &dir,
            "Diamonds.jar",
            &[("META-INF/MANIFEST.MF", manifest), ("background.png", &bg), ("gameBackground.png", &bg), ("d1.png", &tile), ("d2.png", &tile), ("bar.png", &wide)],
        );
        assert_eq!(detect(&jar), ((360, 640), From::Pictures));
    }

    /// The Bounce Tales case (RG35XX Plus, 14.11.): no size key, a wide 422×208 scrolling background and big sprite
    /// sheets. The game lays itself out for the screen it gets: at 240×320 the whole menu shows, at 176×208 it is cut.
    #[test]
    fn a_wide_background_is_no_screen_and_a_game_that_fits_any_size_keeps_240x320() {
        let dir = crate::testutil::TestDir::new("j2me-size-wide");
        let manifest = b"MIDlet-Name: Ball\r\nMIDlet-1: Ball, /icon.png, RMIDlet\r\nMicroEdition-Profile: MIDP-2.0\r\nNokia-Platform: Nokia*\r\n";
        let (wide, sheet, tall) = (png(422, 208), png(364, 341), png(170, 224));
        let jar = write_jar(&dir, "Ball (2008)(v2.0.10).jar", &[("META-INF/MANIFEST.MF", manifest), ("aa.png", &wide), ("ab.png", &sheet), ("ac.png", &tall)]);
        assert_eq!(detect(&jar), (DEFAULT, From::Default));
    }

    #[test]
    fn a_game_that_says_nothing_gets_freej2mes_240x320() {
        let dir = crate::testutil::TestDir::new("j2me-size-none");
        let tile = png(128, 128);
        let jar = write_jar(&dir, "Quiet.jar", &[("META-INF/MANIFEST.MF", b"MIDlet-Name: Quiet\n"), ("t.png", &tile)]);
        assert_eq!(detect(&jar), (DEFAULT, From::Default), "square textures are no screen");
        let broken = dir.join("Broken.jar");
        std::fs::write(&broken, b"not a zip").unwrap();
        assert_eq!(detect(&broken), (DEFAULT, From::Default), "a damaged jar still starts, at the default");
    }

    #[test]
    fn a_size_in_the_file_name_wins_and_a_jad_beside_the_jar_counts() {
        let dir = crate::testutil::TestDir::new("j2me-size-name");
        let manifest = b"Nokia-MIDlet-Original-Display-Size: 176,208\n";
        let jar = write_jar(&dir, "Snake II (128x160).jar", &[("META-INF/MANIFEST.MF", manifest)]);
        assert_eq!(detect(&jar), ((128, 160), From::Name));
        let plain = write_jar(&dir, "Racer.jar", &[("META-INF/MANIFEST.MF", b"MIDlet-Name: Racer\n")]);
        std::fs::write(dir.join("Racer.JAD"), "MIDlet-Name: Racer\nMIDlet-Screen-Size: 240x400\n").unwrap();
        assert_eq!(detect(&plain), ((240, 400), From::Jad));
    }

    #[test]
    fn sizes_are_read_from_names_and_keys_as_phone_collections_write_them() {
        assert_eq!(from_name("Bounce_240x320_Nokia"), Some((240, 320)));
        assert_eq!(from_name("Asphalt 4 [176×220]"), Some((176, 220)));
        assert_eq!(from_name("Game v1.2x3"), None, "a version is no size");
        assert_eq!(from_name("3D Bomberman - riolatino (2007)(v1.2.0)"), None);
        assert_eq!(from_name("Tetris 2008x2009"), None, "years joined by an x are no size");
        assert_eq!(from_manifest("LGE-MIDlet-Width: 240\nLGE-MIDlet-Height: 320\n"), Some((240, 320)));
        assert_eq!(from_manifest("MIDlet-Icon-Width: 32\nMIDlet-Icon-Height: 32\n"), None, "an icon is no screen");
        assert_eq!(from_manifest("Nokia-MIDlet-Target-Display-Size: 352,416\n"), None, "a target only scales the game");
        assert_eq!(from_manifest("Nokia-MIDlet-Original-Display-\n Size: 176,208\n"), None, "keys don't wrap, values do");
        assert_eq!(from_manifest("Nokia-MIDlet-Original-Display-Size: 176,\n 208\n"), Some((176, 208)));
    }

    #[test]
    fn every_size_written_is_one_freej2me_takes() {
        assert_eq!(snap((176, 208)), Some((176, 208)));
        assert_eq!(snap((176, 204)), Some((176, 208)), "a few pixels off: the phone size it is");
        assert_eq!(snap((300, 300)), None);
        assert_eq!(snap((208, 176)), Some((208, 173)), "the nearest of the same shape, landscape stays landscape");
        assert!(SIZES.contains(&DEFAULT));
    }

    #[test]
    fn the_size_goes_into_the_options_file_retroarch_reads_and_nothing_else_changes() {
        let dir = tempfile::tempdir().unwrap();
        let core = dir.path().join(CORE_OPT);
        std::fs::write(&core, "freej2me_backlightcolor = \"Green\"\nfreej2me_resolution = \"240x320\"\nfreej2me_sound = \"on\"\n").unwrap();
        let rom = Path::new("/storage/roms/j2me/Bomber.jar");
        set_resolution(dir.path(), rom, (176, 208)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&core).unwrap(),
            "freej2me_backlightcolor = \"Green\"\nfreej2me_resolution = \"176x208\"\nfreej2me_sound = \"on\"\n"
        );
        // A game with its own options file (made in RetroArch's menu): the size goes there, the core's stays.
        let own = dir.path().join("Diamonds.opt");
        std::fs::write(&own, "freej2me_sound = \"off\"\n").unwrap();
        set_resolution(dir.path(), Path::new("/storage/roms/j2me/Diamonds.jar"), (360, 640)).unwrap();
        assert_eq!(std::fs::read_to_string(&own).unwrap(), "freej2me_sound = \"off\"\nfreej2me_resolution = \"360x640\"\n");
        assert!(std::fs::read_to_string(&core).unwrap().contains("\"176x208\""));
        // No core file yet (the first phone game ever): one line, RetroArch fills in the rest.
        let fresh = tempfile::tempdir().unwrap();
        set_resolution(&fresh.path().join("FreeJ2ME-Plus"), rom, (240, 320)).unwrap();
        assert_eq!(std::fs::read_to_string(fresh.path().join("FreeJ2ME-Plus").join(CORE_OPT)).unwrap(), "freej2me_resolution = \"240x320\"\n");
    }

    /// RG35XX Plus, 14.11.: Alex's first start left game.conf at 240×320, which FreeJ2ME took over the core option.
    #[test]
    fn freej2mes_own_game_conf_from_an_earlier_start_gets_the_size_too() {
        let dir = tempfile::tempdir().unwrap();
        let rom = dir.path().join("3D Bomberman - riolatino (2007)(v1.2.0).jar");
        let manifest = "MIDlet-Name: 3D Bomberman - riolatino\r\nMIDlet-1: 3D Bomberman, i.png, BomberMan3D\r\n";
        let suite = suite_name(manifest).unwrap();
        assert_eq!(suite, "3D Bomberman", "FreeJ2ME's folder is the first MIDlet's name, not MIDlet-Name");
        let conf = game_conf(&rom, &suite).unwrap();
        assert_eq!(conf, dir.path().join("freej2me/config/3D Bomberman/game.conf"));
        set_game_conf(&conf, (176, 208)).unwrap();
        assert!(!conf.exists(), "no game.conf yet: FreeJ2ME takes the core option and writes it itself");
        std::fs::create_dir_all(conf.parent().unwrap()).unwrap();
        std::fs::write(&conf, "fps:0\nphone:Standard\nrotate:0\nscrheight:320\nscrwidth:240\nsound:on\n").unwrap();
        set_game_conf(&conf, (176, 208)).unwrap();
        assert_eq!(std::fs::read_to_string(&conf).unwrap(), "fps:0\nphone:Standard\nrotate:0\nscrheight:208\nscrwidth:176\nsound:on\n");
        assert_eq!(suite_name("MIDlet-1: Snake: II,/icon.png,app.Main\n").as_deref(), Some("Snake II"));
        assert_eq!(suite_name("MIDlet-1: ../x, i.png, A\n"), None, "never a folder outside");
    }

    #[test]
    fn the_picture_fills_the_screen_whole_numbers_where_twice_fits() {
        let plus = (640, 480);
        assert_eq!(placement((176, 208), plus), (144, 32, 352, 416), "Bomberman: x2, centred");
        assert!(integer((176, 208), plus));
        assert_eq!(placement((240, 320), plus), (140, 0, 360, 480), "240x320: as large as fits, not x1");
        assert!(!integer((240, 320), plus));
        assert_eq!(placement((360, 640), plus), (185, 0, 270, 480), "Diamond Rush: scaled down to fit, its shape kept");
        assert_eq!(placement((128, 160), plus), (128, 0, 384, 480), "x3");
        assert_eq!(placement((240, 320), (720, 720)), (120, 40, 480, 640), "a square screen: x2");
        assert_eq!(placement((320, 240), (1280, 720)), (160, 0, 960, 720), "landscape on 16:9: x3");
    }

    #[test]
    fn the_integer_override_is_pocket_uis_own_and_a_players_override_stays() {
        let dir = tempfile::tempdir().unwrap();
        let rom = Path::new("/storage/roms/j2me/Bomber.jar");
        let file = dir.path().join("Bomber.cfg");
        apply_integer(dir.path(), rom, true).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), INTEGER_OVERRIDE);
        apply_integer(dir.path(), rom, false).unwrap();
        assert!(!file.exists());
        std::fs::write(&file, "video_smooth = \"true\"\n").unwrap();
        apply_integer(dir.path(), rom, true).unwrap();
        apply_integer(dir.path(), rom, false).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "video_smooth = \"true\"\n");
    }

    #[test]
    fn the_screen_comes_from_retroarchs_config() {
        assert_eq!(screen("video_fullscreen = \"true\"\nvideo_fullscreen_x = \"640\"\nvideo_fullscreen_y = \"480\"\n"), Some((640, 480)));
        assert_eq!(screen("video_fullscreen_x = \"0\"\nvideo_fullscreen_y = \"0\"\n"), None, "0 = the desktop's: no guess");
    }

    #[test]
    fn a_size_fixed_in_runs_on_wins_over_the_jar() {
        let dir = crate::testutil::TestDir::new("j2me-size-fixed");
        let jar = write_jar(&dir, "Bomber.jar", &[("META-INF/MANIFEST.MF", b"Nokia-MIDlet-Original-Display-Size: 176,208\n")]);
        assert_eq!(chosen("240x320", &jar).0, (240, 320));
        assert_eq!(chosen(AUTO, &jar).0, (176, 208));
        assert_eq!(chosen("", &jar).0, (176, 208));
    }

    #[test]
    fn the_screen_size_row_is_only_on_phone_games_and_offers_sizes_freej2me_takes() {
        let s = crate::emu_choice::RA_SETTINGS.iter().find(|s| s.key == KEY).unwrap();
        assert!(s.applies("j2me") && !s.applies("gba") && s.own);
        assert_eq!(s.unset, AUTO);
        for c in s.choices.iter().filter(|c| c.value != AUTO) {
            assert!(pair(c.value).is_some_and(|p| SIZES.contains(&p)), "{} is a size the core lists", c.value);
        }
    }
}

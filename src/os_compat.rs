//! Whether a new ROCKNIX system still has what pocket-ui and pocket-core need to run, checked in the SYSTEM image of an
//! OS update before it is staged (`update::stage_os`) and by `pocket-ui --check-os-update <tar|SYSTEM|dir>` (the dry
//! run for development, docs/UPDATES.md "OS: what pocket-ui needs from the new system").
//!
//! pocket-ui is built apart from ROCKNIX (zigbuild, glibc 2.38), so an OS update can break it where EmulationStation,
//! built with the image, keeps working (docs ES-REPLACEMENT-GAPS 5.4): a newer glibc is fine, an older one or a
//! library gone is a black screen until the launcher falls back to EmulationStation. What is checked, without running
//! anything from the new system:
//! - the libraries pocket-ui and pocket-core link to (their ELF `DT_NEEDED`) and every symbol version they use from
//!   each (`GLIBC_2.38` and the like, `.gnu.version_r`), against the versions the new library defines
//!   (`.gnu.version_d`);
//! - the libraries they open at run time (`DLOPEN`: Wayland, xkbcommon, EGL, sound);
//! - the programs and ROCKNIX scripts they call (`FILES`: sway, runemu.sh, start_es.sh, wifictl, nmcli, iwd, PipeWire…);
//! - the libretro cores pocket-ui routes systems to (`hostsys::routed_cores`);
//! - ROCKNIX's shell functions pocket-ui relies on (get_setting, set_setting in 001-functions);
//! - whether start_es.sh and es_settings are versions the launcher knows (`KNOWN_UI_START`): the launcher repeats their
//!   steps (deploy/run-pocket-ui.sh, "ROCKNIX's UI start"), so a new step there is for a developer to look at.
//!
//! `Level::Blocks` refuses the update, `Level::Warns` stages it with a note on the Updates page, `Level::Info` is only
//! logged and printed by the dry run. The new system is read through `System`: a squashfs listing plus
//! `unsquashfs -cat` on the device, a directory in tests and for an unpacked tree.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// pocket-ui would not start, or the way back to EmulationStation would be gone: the update is refused.
    Blocks,
    /// Something pocket-ui uses would stop working (Wi-Fi, sound, a core): installed, with a note.
    Warns,
    /// For the developer (dry run, log).
    Info,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub level: Level,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    fn add(&mut self, level: Level, text: impl Into<String>) {
        self.findings.push(Finding { level, text: text.into() });
    }

    fn of(&self, level: Level) -> Vec<&str> {
        self.findings.iter().filter(|f| f.level == level).map(|f| f.text.as_str()).collect()
    }

    pub fn blocking(&self) -> Vec<&str> {
        self.of(Level::Blocks)
    }

    pub fn warnings(&self) -> Vec<&str> {
        self.of(Level::Warns)
    }

    /// The Updates page's message when the update is refused.
    pub fn refusal(&self) -> Option<String> {
        let b = self.blocking();
        let first = b.first()?;
        let more = if b.len() > 1 {
            format!(" (and {} more, see the log)", b.len() - 1)
        } else {
            String::new()
        };
        Some(format!("This system update would break pocket-ui ({first}{more}). It was not installed."))
    }

    /// The note the Updates page adds when the update is staged with warnings.
    pub fn note(&self) -> Option<String> {
        let w = self.warnings();
        let first = w.first()?;
        let more = if w.len() > 1 {
            format!(", and {} more (see the log)", w.len() - 1)
        } else {
            String::new()
        };
        Some(format!("Note: {first}{more}."))
    }

    /// Every finding, one per line, for the log and the dry run.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for level in [Level::Blocks, Level::Warns, Level::Info] {
            let tag = match level {
                Level::Blocks => "BLOCKS",
                Level::Warns => "WARNS ",
                Level::Info => "info  ",
            };
            for t in self.of(level) {
                out.push_str(&format!("{tag} {t}\n"));
            }
        }
        out
    }
}

// ---- what pocket-ui needs ---------------------------------------------------------------------------------------------

/// A file of the new system (paths without the leading `/`); any one of `any_of` will do. `what`: what happens
/// without it.
pub struct FileNeed {
    pub any_of: &'static [&'static str],
    pub what: &'static str,
    pub level: Level,
}

const fn need(any_of: &'static [&'static str], what: &'static str, level: Level) -> FileNeed {
    FileNeed { any_of, what, level }
}

/// The programs, scripts and files pocket-ui, pocket-core and the launcher call or read (`Command::new`, paths in
/// `src/` and `deploy/run-pocket-ui.sh`). Keep it in step when code starts to call something new of ROCKNIX's.
pub const FILES: &[FileNeed] = &[
    need(&["bin/sh"], "the launcher can't run", Level::Blocks),
    need(&["usr/bin/bash"], "ROCKNIX's scripts can't run", Level::Blocks),
    need(&["usr/bin/sway"], "nothing could draw on the screen", Level::Blocks),
    need(&["usr/bin/swaymsg"], "the launcher and pocket-ui can't talk to sway", Level::Blocks),
    need(
        &["usr/bin/systemctl"],
        "the launcher can't stop games or hand over to EmulationStation",
        Level::Blocks,
    ),
    need(&["usr/bin/runemu.sh"], "games pocket-core doesn't play wouldn't start", Level::Blocks),
    need(
        &["usr/bin/start_es.sh"],
        "no way back to EmulationStation when pocket-ui can't start",
        Level::Blocks,
    ),
    need(
        &["usr/bin/emulationstation"],
        "no way back to EmulationStation when pocket-ui can't start",
        Level::Blocks,
    ),
    need(
        &["etc/profile.d/001-functions"],
        "ROCKNIX's get_setting and set_setting are gone",
        Level::Blocks,
    ),
    need(&["usr/config/emulationstation/es_systems.cfg"], "the library would be empty", Level::Blocks),
    need(&["usr/config/system/configs/system.cfg"], "ROCKNIX's default settings are gone", Level::Warns),
    need(&["usr/bin/wifictl"], "Wi-Fi can't be switched on or off", Level::Warns),
    need(&["usr/bin/nmcli"], "Wi-Fi networks can't be listed or joined", Level::Warns),
    need(&["usr/lib/iwd", "usr/libexec/iwd"], "no Wi-Fi", Level::Warns),
    need(&["usr/bin/pipewire"], "no sound", Level::Warns),
    need(&["usr/bin/pactl"], "the volume can't be changed", Level::Warns),
    need(&["usr/bin/pw-metadata"], "games play with more sound latency", Level::Warns),
    need(&["usr/bin/rocknix-bluetooth"], "Bluetooth can't be switched on or off", Level::Warns),
    need(&["usr/bin/bluetoothctl"], "no Bluetooth pairing", Level::Warns),
    need(
        &["usr/bin/steamos-polkit-helpers/steamos-set-timezone"],
        "the time zone can't be changed",
        Level::Warns,
    ),
    need(&["usr/bin/curl"], "no updates, Store or covers", Level::Warns),
    need(&["usr/bin/tar"], "updates and the Store can't unpack", Level::Warns),
    need(&["usr/bin/unsquashfs"], "the next system update can't be checked", Level::Warns),
    need(&["usr/bin/timeout"], "the launcher's helpers have no time limit", Level::Warns),
    need(
        &["usr/sbin/quantum"],
        "the launcher can't put PipeWire's quantum back after a crash",
        Level::Warns,
    ),
    need(&["usr/bin/grim"], "no screenshots", Level::Warns),
    need(&["usr/bin/moonlight"], "no PC streaming", Level::Warns),
];

/// A library opened at run time (dlopen), not linked: it is not in the binary's `DT_NEEDED`. Any one of `any_of`.
pub struct LibNeed {
    pub any_of: &'static [&'static str],
    pub what: &'static str,
    pub level: Level,
}

/// What winit, softbuffer and glutin (`wayland-dlopen`, `egl`), `hw_render` and `audio` open by name (the names in
/// the aarch64 binaries: `grep -aoE 'lib[A-Za-z0-9_+-]+\.so(\.[0-9]+)?' pocket-ui pocket-core`).
pub const DLOPEN: &[LibNeed] = &[
    LibNeed {
        any_of: &["libwayland-client.so.0"],
        what: "pocket-ui can't open its window",
        level: Level::Blocks,
    },
    LibNeed {
        any_of: &["libxkbcommon.so.0"],
        what: "pocket-ui can't open its window",
        level: Level::Blocks,
    },
    LibNeed {
        any_of: &["libEGL.so.1"],
        what: "pocket-core can't draw games",
        level: Level::Blocks,
    },
    LibNeed {
        any_of: &["libwayland-egl.so.1"],
        what: "pocket-core can't draw games",
        level: Level::Blocks,
    },
    LibNeed {
        any_of: &["libasound.so.2", "libpulse-simple.so.0"],
        what: "pocket-core plays without sound",
        level: Level::Warns,
    },
];

/// Where the dynamic linker looks on ROCKNIX (lib is a link to usr/lib there).
const LIB_DIRS: &[&str] = &["usr/lib", "lib", "usr/lib64", "lib64"];

/// start_es.sh and es_settings as the launcher repeats them (deploy/run-pocket-ui.sh, "ROCKNIX's UI start"): path,
/// sha256, where from. Another version is for a developer to compare (scripts/rocknix-diff.sh), then add here.
pub const KNOWN_UI_START: &[(&str, &str, &str)] = &[
    (
        "usr/bin/start_es.sh",
        "463acec36231a2230f311e0281fe80693c0046c531985bd75bd4c8d83b95161e",
        "ROCKNIX 20261001",
    ),
    (
        "usr/bin/start_es.sh",
        "142a7f97832b7af8bda71c144d45a2e58c54ac2b41a184aaa9c67a79b45be9dc",
        "ROCKNIX main 8b70f69",
    ),
    (
        "usr/bin/es_settings",
        "d6498b7f5dc09be4bd42615c48f7b98cdeec02b2fe4e8c25020854847bbfacc6",
        "ROCKNIX 20261001 and main 8b70f69",
    ),
];

/// ROCKNIX functions pocket-ui calls (fn_keys, emu_choice, the launcher) or imitates (system_cfg's `set_setting`).
const FUNCTIONS: &[&str] = &["get_setting", "set_setting"];

/// A library the binaries link to and the symbol versions they use from it.
#[derive(Clone, Debug, PartialEq)]
pub struct Linked {
    pub soname: String,
    pub versions: Vec<String>,
    /// "pocket-ui", "pocket-core" or both.
    pub by: String,
}

/// A libretro core pocket-ui starts systems with on pocket-core.
#[derive(Clone, Debug, PartialEq)]
pub struct Core {
    pub core: String,
    pub systems: Vec<String>,
    /// There is a copy outside the system (the Store's, /storage/cores), which outlives an OS update.
    pub elsewhere: bool,
}

/// Everything checked: the fixed lists above, and what was read from the installed pocket-ui and pocket-core.
pub struct Needs {
    pub files: &'static [FileNeed],
    pub dlopen: &'static [LibNeed],
    pub linked: Vec<Linked>,
    pub cores: Vec<Core>,
    /// Something about pocket-ui itself that couldn't be read (shown as Info).
    pub notes: Vec<String>,
}

impl Needs {
    /// Nothing at all (tests of the other checks).
    pub fn none() -> Needs {
        Needs {
            files: &[],
            dlopen: &[],
            linked: Vec::new(),
            cores: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// The libraries and versions of the given binaries (name, ELF bytes), merged.
    pub fn linked_of(bins: &[(&str, &[u8])]) -> (Vec<Linked>, Vec<String>) {
        let mut linked: Vec<Linked> = Vec::new();
        let mut notes = Vec::new();
        for (name, data) in bins {
            let elf = match parse_elf(data) {
                Ok(e) => e,
                Err(e) => {
                    notes.push(format!("{name}'s libraries could not be read ({e}), they were not checked"));
                    continue;
                }
            };
            for soname in &elf.needed {
                let versions: Vec<String> = elf.needs.iter().filter(|(f, _)| f == soname).map(|(_, v)| v.clone()).collect();
                match linked.iter_mut().find(|l| &l.soname == soname) {
                    Some(l) => {
                        for v in versions {
                            if !l.versions.contains(&v) {
                                l.versions.push(v);
                            }
                        }
                        if !l.by.split(" and ").any(|b| b == *name) {
                            l.by = format!("{} and {name}", l.by);
                        }
                    }
                    None => linked.push(Linked {
                        soname: soname.clone(),
                        versions,
                        by: name.to_string(),
                    }),
                }
            }
        }
        (linked, notes)
    }

    /// What the installed pocket-ui (this program) and pocket-core (`<app_root>/pocket-core`) need.
    pub fn installed(app_root: &Path) -> Needs {
        let exe = std::env::current_exe().ok();
        let core = std::fs::canonicalize(app_root.join("pocket-core")).ok();
        let read = |p: &Option<PathBuf>| p.as_ref().and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
        let (ui, pc) = (read(&exe), read(&core));
        let (linked, notes) = Needs::linked_of(&[("pocket-ui", &ui), ("pocket-core", &pc)]);
        let cores = crate::hostsys::routed_cores()
            .into_iter()
            .map(|(core, systems)| {
                let elsewhere = Path::new(crate::store::CORES_DIR).join(format!("{core}_libretro.so")).exists();
                Core {
                    core: core.to_string(),
                    systems: systems.iter().map(|s| s.to_string()).collect(),
                    elsewhere,
                }
            })
            .collect();
        Needs {
            files: FILES,
            dlopen: DLOPEN,
            linked,
            cores,
            notes,
        }
    }
}

// ---- the new system ---------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    File,
    Dir,
    Link(String),
    Other,
}

/// A system image read without mounting it. Paths are relative to its root, without a leading `/`.
pub trait System {
    /// The entry at `path` itself (a link is not followed).
    fn entry(&self, path: &str) -> Option<Entry>;
    /// The contents of the regular file at `path` (already resolved).
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

/// A squashfs image through `unsquashfs`: one listing (`-lls`) up front, files with `-cat` when asked for.
pub struct Squashfs {
    tool: String,
    image: PathBuf,
    entries: HashMap<String, Entry>,
}

impl Squashfs {
    pub fn open(tool: &str, image: &Path) -> Result<Squashfs, String> {
        let out = Command::new(tool).arg("-lls").arg(image).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err("The new system can't be opened for checking. It was not installed.".into());
        }
        Ok(Squashfs {
            tool: tool.into(),
            image: image.into(),
            entries: parse_listing(&String::from_utf8_lossy(&out.stdout)),
        })
    }
}

impl System for Squashfs {
    fn entry(&self, path: &str) -> Option<Entry> {
        self.entries.get(path).cloned()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let out = Command::new(&self.tool).arg("-cat").arg(&self.image).arg(path).output().ok()?;
        out.status.success().then_some(out.stdout)
    }
}

/// `unsquashfs -lls` output: `<mode> <owner> <size> <date> <time> squashfs-root/<path>[ -> <target>]`.
pub fn parse_listing(text: &str) -> HashMap<String, Entry> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let Some(at) = line.find(" squashfs-root/") else { continue };
        let rest = &line[at + " squashfs-root/".len()..];
        let kind = line.chars().next().unwrap_or(' ');
        let (path, entry) = match kind {
            'l' => match rest.split_once(" -> ") {
                Some((p, t)) => (p, Entry::Link(t.to_string())),
                None => (rest, Entry::Other),
            },
            'd' => (rest, Entry::Dir),
            '-' => (rest, Entry::File),
            _ => (rest, Entry::Other),
        };
        map.insert(path.to_string(), entry);
    }
    map
}

/// An unpacked system in a directory (tests, `--check-os-update <dir>`).
pub struct Dir(pub PathBuf);

impl System for Dir {
    fn entry(&self, path: &str) -> Option<Entry> {
        let p = self.0.join(path);
        let meta = std::fs::symlink_metadata(&p).ok()?;
        Some(if meta.file_type().is_symlink() {
            Entry::Link(std::fs::read_link(&p).ok()?.to_string_lossy().into_owned())
        } else if meta.is_dir() {
            Entry::Dir
        } else if meta.is_file() {
            Entry::File
        } else {
            Entry::Other
        })
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(path)).ok()
    }
}

/// `path` with every link on the way followed, inside the image (an absolute target starts at its root). None when
/// something on the way is missing or the links go round.
pub fn resolve(sys: &dyn System, path: &str) -> Option<String> {
    fn go(sys: &dyn System, path: &str, hops: &mut u32) -> Option<String> {
        let mut cur: Vec<String> = Vec::new();
        let comps: Vec<&str> = path.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
        for (i, comp) in comps.iter().enumerate() {
            if *comp == ".." {
                cur.pop();
                continue;
            }
            let candidate = if cur.is_empty() {
                comp.to_string()
            } else {
                format!("{}/{comp}", cur.join("/"))
            };
            // A folder on the way that a listing leaves out counts as a folder; the last part has to be there.
            let entry = match sys.entry(&candidate) {
                Some(e) => e,
                None if i + 1 < comps.len() => Entry::Dir,
                None => return None,
            };
            match entry {
                Entry::Link(target) => {
                    *hops += 1;
                    if *hops > 40 {
                        return None;
                    }
                    let joined = match target.strip_prefix('/') {
                        Some(abs) => abs.to_string(),
                        None if cur.is_empty() => target,
                        None => format!("{}/{target}", cur.join("/")),
                    };
                    let resolved = go(sys, &joined, hops)?;
                    cur = resolved.split('/').filter(|c| !c.is_empty()).map(str::to_string).collect();
                }
                _ => cur.push(comp.to_string()),
            }
        }
        (!cur.is_empty()).then(|| cur.join("/"))
    }
    go(sys, path, &mut 0)
}

fn find_lib(sys: &dyn System, soname: &str) -> Option<String> {
    LIB_DIRS
        .iter()
        .find_map(|d| resolve(sys, &format!("{d}/{soname}")).filter(|p| sys.entry(p) == Some(Entry::File)))
}

/// `2.38` of `GLIBC_2.38` (versions that are not `GLIBC_<numbers>` give None).
fn glibc_version(v: &str) -> Option<Vec<u32>> {
    v.strip_prefix("GLIBC_")?.split('.').map(|n| n.parse().ok()).collect()
}

/// "needs" after one program, "need" after "pocket-ui and pocket-core".
fn need_verb(by: &str) -> &'static str {
    if by.contains(" and ") {
        "need"
    } else {
        "needs"
    }
}

fn dotted(v: &[u32]) -> String {
    v.iter().map(u32::to_string).collect::<Vec<_>>().join(".")
}

/// Files ROCKNIX's H700 image doesn't have and pocket-ui's H700 profiles handle (docs/DEVICES.md).
const H700_WITHOUT: &[&str] = &["usr/bin/steamos-polkit-helpers/steamos-set-timezone", "usr/sbin/quantum"];

/// Whether the system is ROCKNIX's H700 image (`HW_DEVICE="H700"` in its os-release).
fn is_h700_system(sys: &dyn System) -> bool {
    let text = resolve(sys, "etc/os-release").and_then(|p| sys.read(&p)).map(|b| String::from_utf8_lossy(&b).into_owned());
    text.is_some_and(|t| t.lines().any(|l| l.trim().strip_prefix("HW_DEVICE=").is_some_and(|v| v.trim_matches('"') == "H700")))
}

/// Checks `sys` against `needs`.
pub fn check(sys: &dyn System, needs: &Needs) -> Report {
    let mut r = Report::default();
    for n in &needs.notes {
        r.add(Level::Info, n.clone());
    }
    let h700 = is_h700_system(sys);
    for f in needs.files {
        if !f.any_of.iter().any(|p| resolve(sys, p).is_some_and(|p| sys.entry(&p).is_some())) {
            // ROCKNIX's H700 image never had these two; pocket-ui does without them there (`system_ctl` stores the
            // time zone itself, `audio::restore_quantum` uses pw-metadata): nothing to warn about.
            let level = if h700 && f.any_of.iter().any(|p| H700_WITHOUT.contains(p)) { Level::Info } else { f.level };
            r.add(level, format!("/{} is missing: {}", f.any_of[0], f.what));
        }
    }
    for l in &needs.linked {
        let Some(path) = find_lib(sys, &l.soname) else {
            r.add(
                Level::Blocks,
                format!("{} {} {}, which the new system doesn't have", l.by, need_verb(&l.by), l.soname),
            );
            continue;
        };
        if l.versions.is_empty() {
            continue;
        }
        let defined = match sys.read(&path).as_deref().map(parse_elf) {
            Some(Ok(elf)) => elf.defines,
            _ => {
                r.add(Level::Blocks, format!("{} of the new system can't be read to check it", l.soname));
                continue;
            }
        };
        let missing: Vec<&String> = l.versions.iter().filter(|v| !defined.contains(v)).collect();
        let newest = |vs: &mut dyn Iterator<Item = &String>| vs.filter_map(|v| glibc_version(v)).max();
        if l.soname == "libc.so.6" {
            if let Some(has) = newest(&mut defined.iter()) {
                r.add(Level::Info, format!("glibc {} in the new system", dotted(&has)));
            }
        }
        if missing.is_empty() {
            continue;
        }
        match (newest(&mut missing.iter().copied()), newest(&mut defined.iter())) {
            (Some(want), Some(has)) => r.add(
                Level::Blocks,
                format!(
                    "{} {} glibc {} or later ({}), the new system has glibc {}",
                    l.by,
                    need_verb(&l.by),
                    dotted(&want),
                    l.soname,
                    dotted(&has)
                ),
            ),
            _ => r.add(
                Level::Blocks,
                format!(
                    "{} {} {} from {}, which the new system's doesn't have",
                    l.by,
                    need_verb(&l.by),
                    missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                    l.soname
                ),
            ),
        }
    }
    for d in needs.dlopen {
        if !d.any_of.iter().any(|s| find_lib(sys, s).is_some()) {
            let verb = if d.any_of.len() > 1 { "are" } else { "is" };
            r.add(d.level, format!("{} {verb} missing: {}", d.any_of.join(" and "), d.what));
        }
    }
    for c in &needs.cores {
        let path = format!("usr/lib/libretro/{}_libretro.so", c.core);
        if resolve(sys, &path).is_some_and(|p| sys.entry(&p) == Some(Entry::File)) {
            continue;
        }
        let systems = c.systems.join(", ");
        if c.elsewhere {
            r.add(
                Level::Info,
                format!("no {} core in the new system; the copy in /storage/cores plays {systems}", c.core),
            );
        } else {
            r.add(
                Level::Warns,
                format!("no {} core in the new system, so {systems} games would not start", c.core),
            );
        }
    }
    if needs.files.iter().any(|f| f.any_of.contains(&"etc/profile.d/001-functions")) {
        if let Some(text) = sys.read("etc/profile.d/001-functions").map(|b| String::from_utf8_lossy(&b).into_owned()) {
            for f in FUNCTIONS {
                if !defines_function(&text, f) {
                    r.add(
                        Level::Warns,
                        format!("ROCKNIX's {f} is gone from 001-functions: settings pocket-ui writes for ROCKNIX may not be read"),
                    );
                }
            }
            if crate::system_cfg::uses_flock(&text) {
                r.add(Level::Info, "system.cfg's lock is ROCKNIX's flock (wait_lock), which system_cfg follows");
            }
        }
        for (path, what) in [("usr/bin/start_es.sh", "start_es.sh"), ("usr/bin/es_settings", "es_settings")] {
            let Some(data) = sys.read(path) else { continue };
            let sum = crate::store::sha256_hex(&data);
            match KNOWN_UI_START.iter().find(|(p, s, _)| *p == path && *s == sum) {
                Some((_, _, from)) => r.add(Level::Info, format!("{what} as in {from}")),
                None => r.add(
                    Level::Info,
                    format!("{what} is a version the launcher doesn't know (sha256 {sum}): compare its steps with deploy/run-pocket-ui.sh (scripts/rocknix-diff.sh)"),
                ),
            }
        }
    }
    r
}

/// Whether shell text defines function `name` (`name()`, `function name()`, `function name {`).
pub fn defines_function(text: &str, name: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim_start();
        let (keyword, l) = match l.strip_prefix("function") {
            Some(r) if r.starts_with(char::is_whitespace) => (true, r.trim_start()),
            _ => (false, l),
        };
        let Some(rest) = l.strip_prefix(name) else { return false };
        let rest = rest.trim_start();
        rest.starts_with("()") || (keyword && (rest.starts_with('{') || rest.is_empty()))
    })
}

// ---- ELF ----------------------------------------------------------------------------------------------------------

/// What `check` needs to know of a 64-bit little-endian ELF file (aarch64).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Elf {
    /// `DT_NEEDED`: the libraries it links to.
    pub needed: Vec<String>,
    /// `.gnu.version_r`: (library, version) of every symbol version it uses.
    pub needs: Vec<(String, String)>,
    /// `.gnu.version_d`: the versions it defines (a library), without its own base name.
    pub defines: Vec<String>,
}

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_STRSZ: u64 = 10;
const DT_VERDEF: u64 = 0x6fff_fffc;
const DT_VERDEFNUM: u64 = 0x6fff_fffd;
const DT_VERNEED: u64 = 0x6fff_fffe;
const DT_VERNEEDNUM: u64 = 0x6fff_ffff;
const VER_FLG_BASE: u16 = 1;

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at.checked_add(2)?)?.try_into().ok()?))
}
fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at.checked_add(4)?)?.try_into().ok()?))
}
fn u64_at(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(at..at.checked_add(8)?)?.try_into().ok()?))
}

/// Reads the dynamic section through the program headers (they survive `strip`, section headers need not).
pub fn parse_elf(d: &[u8]) -> Result<Elf, String> {
    if d.get(..4) != Some(b"\x7fELF".as_slice()) {
        return Err("not an ELF file".into());
    }
    if d.get(4) != Some(&2) || d.get(5) != Some(&1) {
        return Err("not a 64-bit little-endian ELF file".into());
    }
    let bad = || "a damaged ELF file".to_string();
    let phoff = u64_at(d, 0x20).ok_or_else(bad)? as usize;
    let phentsize = u16_at(d, 0x36).ok_or_else(bad)? as usize;
    let phnum = u16_at(d, 0x38).ok_or_else(bad)? as usize;
    let mut loads: Vec<(u64, u64, u64)> = Vec::new();
    let mut dynamic: Option<(usize, usize)> = None;
    for i in 0..phnum {
        let ph = phoff + i * phentsize;
        let kind = u32_at(d, ph).ok_or_else(bad)?;
        let (offset, vaddr, filesz) = (
            u64_at(d, ph + 8).ok_or_else(bad)?,
            u64_at(d, ph + 16).ok_or_else(bad)?,
            u64_at(d, ph + 32).ok_or_else(bad)?,
        );
        match kind {
            PT_LOAD => loads.push((vaddr, offset, filesz)),
            PT_DYNAMIC => dynamic = Some((offset as usize, filesz as usize)),
            _ => {}
        }
    }
    // A static program has no dynamic section, and needs nothing.
    let Some((dyn_off, dyn_size)) = dynamic else { return Ok(Elf::default()) };
    let to_offset = |addr: u64| {
        loads
            .iter()
            .find(|(v, _, s)| addr >= *v && addr < v + s)
            .map(|(v, o, _)| (o + (addr - v)) as usize)
    };
    let mut tags: Vec<(u64, u64)> = Vec::new();
    let mut at = dyn_off;
    while at + 16 <= dyn_off + dyn_size {
        let (tag, val) = (u64_at(d, at).ok_or_else(bad)?, u64_at(d, at + 8).ok_or_else(bad)?);
        if tag == DT_NULL {
            break;
        }
        tags.push((tag, val));
        at += 16;
    }
    let tag = |t: u64| tags.iter().find(|(k, _)| *k == t).map(|(_, v)| *v);
    let strtab = tag(DT_STRTAB).and_then(to_offset).ok_or_else(bad)?;
    let strsz = tag(DT_STRSZ).unwrap_or(0) as usize;
    let string = |off: u64| -> Option<String> {
        let start = strtab.checked_add(off as usize)?;
        let end = if strsz > 0 { strtab + strsz } else { d.len() };
        let bytes = d.get(start..end.min(d.len()))?;
        let len = bytes.iter().position(|b| *b == 0)?;
        Some(String::from_utf8_lossy(&bytes[..len]).into_owned())
    };
    let mut elf = Elf::default();
    for (t, v) in &tags {
        if *t == DT_NEEDED {
            elf.needed.push(string(*v).ok_or_else(bad)?);
        }
    }
    if let (Some(mut vn), Some(count)) = (tag(DT_VERNEED).and_then(to_offset), tag(DT_VERNEEDNUM)) {
        for _ in 0..count.min(4096) {
            let (cnt, file, aux, next) = (
                u16_at(d, vn + 2).ok_or_else(bad)?,
                u32_at(d, vn + 4).ok_or_else(bad)?,
                u32_at(d, vn + 8).ok_or_else(bad)?,
                u32_at(d, vn + 12).ok_or_else(bad)?,
            );
            let file = string(file as u64).ok_or_else(bad)?;
            let mut a = vn + aux as usize;
            for _ in 0..cnt {
                let (name, anext) = (u32_at(d, a + 8).ok_or_else(bad)?, u32_at(d, a + 12).ok_or_else(bad)?);
                elf.needs.push((file.clone(), string(name as u64).ok_or_else(bad)?));
                if anext == 0 {
                    break;
                }
                a += anext as usize;
            }
            if next == 0 {
                break;
            }
            vn += next as usize;
        }
    }
    if let (Some(mut vd), Some(count)) = (tag(DT_VERDEF).and_then(to_offset), tag(DT_VERDEFNUM)) {
        for _ in 0..count.min(4096) {
            let (flags, aux, next) = (
                u16_at(d, vd + 2).ok_or_else(bad)?,
                u32_at(d, vd + 12).ok_or_else(bad)?,
                u32_at(d, vd + 16).ok_or_else(bad)?,
            );
            if flags & VER_FLG_BASE == 0 {
                let name = u32_at(d, vd + aux as usize).ok_or_else(bad)?;
                elf.defines.push(string(name as u64).ok_or_else(bad)?);
            }
            if next == 0 {
                break;
            }
            vd += next as usize;
        }
    }
    Ok(elf)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// A minimal ELF64 (little endian) with one PT_LOAD over the whole file and a dynamic section: `needed` libraries,
    /// `needs` (library, version) and `defines` (versions, after the base entry `soname`).
    pub(crate) fn elf(needed: &[&str], needs: &[(&str, &[&str])], soname: &str, defines: &[&str]) -> Vec<u8> {
        let mut strtab = vec![0u8];
        let mut strs: HashMap<String, u32> = HashMap::new();
        let mut s = |t: &str, strtab: &mut Vec<u8>| -> u32 {
            *strs.entry(t.to_string()).or_insert_with(|| {
                let at = strtab.len() as u32;
                strtab.extend_from_slice(t.as_bytes());
                strtab.push(0);
                at
            })
        };
        // verneed
        let mut verneed = Vec::new();
        for (i, (file, versions)) in needs.iter().enumerate() {
            let file_off = s(file, &mut strtab);
            let last = i + 1 == needs.len();
            let size = 16 + 16 * versions.len() as u32;
            verneed.extend_from_slice(&1u16.to_le_bytes());
            verneed.extend_from_slice(&(versions.len() as u16).to_le_bytes());
            verneed.extend_from_slice(&file_off.to_le_bytes());
            verneed.extend_from_slice(&16u32.to_le_bytes());
            verneed.extend_from_slice(&(if last { 0 } else { size }).to_le_bytes());
            for (j, v) in versions.iter().enumerate() {
                let name = s(v, &mut strtab);
                verneed.extend_from_slice(&0u32.to_le_bytes());
                verneed.extend_from_slice(&0u16.to_le_bytes());
                verneed.extend_from_slice(&((j + 2) as u16).to_le_bytes());
                verneed.extend_from_slice(&name.to_le_bytes());
                verneed.extend_from_slice(&(if j + 1 == versions.len() { 0u32 } else { 16 }).to_le_bytes());
            }
        }
        // verdef: the base entry, then one per version (each with one verdaux)
        let mut verdef = Vec::new();
        if !defines.is_empty() {
            let all: Vec<(&str, u16)> = std::iter::once((soname, VER_FLG_BASE)).chain(defines.iter().map(|d| (*d, 0))).collect();
            for (i, (name, flags)) in all.iter().enumerate() {
                let off = s(name, &mut strtab);
                verdef.extend_from_slice(&1u16.to_le_bytes());
                verdef.extend_from_slice(&flags.to_le_bytes());
                verdef.extend_from_slice(&((i + 1) as u16).to_le_bytes());
                verdef.extend_from_slice(&1u16.to_le_bytes());
                verdef.extend_from_slice(&0u32.to_le_bytes());
                verdef.extend_from_slice(&20u32.to_le_bytes());
                verdef.extend_from_slice(&(if i + 1 == all.len() { 0u32 } else { 28 }).to_le_bytes());
                verdef.extend_from_slice(&off.to_le_bytes());
                verdef.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        let needed_offs: Vec<u32> = needed.iter().map(|n| s(n, &mut strtab)).collect();
        // layout: header 64, 2 phdrs 112, dynamic, strtab, verneed, verdef
        let mut dynamic: Vec<(u64, u64)> = needed_offs.iter().map(|o| (DT_NEEDED, *o as u64)).collect();
        let n_dyn = dynamic.len() + 7;
        let dyn_off = 64 + 2 * 56;
        let str_off = dyn_off + n_dyn * 16;
        let vn_off = str_off + strtab.len();
        let vd_off = vn_off + verneed.len();
        dynamic.push((DT_STRTAB, str_off as u64));
        dynamic.push((DT_STRSZ, strtab.len() as u64));
        if !needs.is_empty() {
            dynamic.push((DT_VERNEED, vn_off as u64));
            dynamic.push((DT_VERNEEDNUM, needs.len() as u64));
        }
        if !defines.is_empty() {
            dynamic.push((DT_VERDEF, vd_off as u64));
            dynamic.push((DT_VERDEFNUM, defines.len() as u64 + 1));
        }
        while dynamic.len() < n_dyn {
            dynamic.push((DT_NULL, 0));
        }
        let total = vd_off + verdef.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\0");
        out.extend_from_slice(&3u16.to_le_bytes()); // ET_DYN
        out.extend_from_slice(&183u16.to_le_bytes()); // EM_AARCH64
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes()); // entry
        out.extend_from_slice(&64u64.to_le_bytes()); // phoff
        out.extend_from_slice(&0u64.to_le_bytes()); // shoff
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&64u16.to_le_bytes());
        out.extend_from_slice(&56u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 6]);
        for (kind, off, size) in [(PT_LOAD, 0usize, total), (PT_DYNAMIC, dyn_off, n_dyn * 16)] {
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&4u32.to_le_bytes());
            for v in [off as u64, off as u64, off as u64, size as u64, size as u64, 8u64] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        for (t, v) in dynamic {
            out.extend_from_slice(&t.to_le_bytes());
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&strtab);
        out.extend_from_slice(&verneed);
        out.extend_from_slice(&verdef);
        out
    }

    #[test]
    fn reads_needed_libraries_and_versions() {
        let data = elf(
            &["libc.so.6", "libm.so.6"],
            &[("libc.so.6", &["GLIBC_2.17", "GLIBC_2.38"]), ("libm.so.6", &["GLIBC_2.29"])],
            "",
            &[],
        );
        let e = parse_elf(&data).unwrap();
        assert_eq!(e.needed, ["libc.so.6", "libm.so.6"]);
        assert_eq!(
            e.needs,
            [
                ("libc.so.6".into(), "GLIBC_2.17".into()),
                ("libc.so.6".into(), "GLIBC_2.38".into()),
                ("libm.so.6".into(), "GLIBC_2.29".into())
            ]
        );
        assert!(e.defines.is_empty());
    }

    #[test]
    fn reads_defined_versions_without_the_base_name() {
        let data = elf(&["ld-linux-aarch64.so.1"], &[], "libc.so.6", &["GLIBC_2.17", "GLIBC_2.38", "GLIBC_PRIVATE"]);
        let e = parse_elf(&data).unwrap();
        assert_eq!(e.defines, ["GLIBC_2.17", "GLIBC_2.38", "GLIBC_PRIVATE"]);
    }

    #[test]
    fn refuses_what_is_not_a_64_bit_elf_and_survives_garbage() {
        assert!(parse_elf(b"#!/bin/sh\n").is_err());
        let mut e32 = elf(&["libc.so.6"], &[], "", &[]);
        e32[4] = 1;
        assert!(parse_elf(&e32).is_err());
        // Cut short anywhere: an error, never a panic.
        let full = elf(&["libc.so.6"], &[("libc.so.6", &["GLIBC_2.38"])], "", &[]);
        for len in 0..full.len() {
            let _ = parse_elf(&full[..len]);
        }
    }

    /// A temporary root folder.
    pub(crate) struct Root(pub crate::testutil::TestDir);
    impl Root {
        pub(crate) fn new(name: &str) -> Root {
            let dir = TestDir::new(&format!("oscompat-{name}"));
            Root(dir)
        }
        pub(crate) fn file(&self, path: &str, data: &[u8]) -> &Self {
            let p = self.0.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, data).unwrap();
            self
        }
        pub(crate) fn link(&self, path: &str, target: &str) -> &Self {
            let p = self.0.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(target, p).unwrap();
            self
        }
    }

    #[test]
    fn resolves_links_relative_absolute_and_through_directories() {
        let r = Root::new("resolve");
        r.file("usr/lib/libwayland-client.so.0.26.0", b"x")
            .link("usr/lib/libwayland-client.so.0", "libwayland-client.so.0.26.0")
            .link("lib", "/usr/lib")
            .link("bin", "/usr/bin")
            .file("usr/bin/bash", b"x")
            .link("usr/bin/sh", "bash");
        r.link("usr/lib/loop-a", "loop-b").link("usr/lib/loop-b", "loop-a");
        let sys = Dir(r.0.to_path_buf());
        assert_eq!(
            resolve(&sys, "lib/libwayland-client.so.0").as_deref(),
            Some("usr/lib/libwayland-client.so.0.26.0")
        );
        assert_eq!(resolve(&sys, "bin/sh").as_deref(), Some("usr/bin/bash"));
        assert_eq!(resolve(&sys, "usr/lib/../bin/bash").as_deref(), Some("usr/bin/bash"));
        assert_eq!(resolve(&sys, "usr/lib/nothing"), None);
        assert_eq!(resolve(&sys, "usr/lib/loop-a"), None, "links that go round end");
    }

    #[test]
    fn reads_an_unsquashfs_listing() {
        let text = "drwxr-xr-x root/root 252 2026-10-01 21:02 squashfs-root\n\
                    lrwxrwxrwx root/root 8 2026-10-01 21:02 squashfs-root/bin -> /usr/bin\n\
                    drwxr-xr-x root/root 3 2026-10-01 20:47 squashfs-root/usr/bin\n\
                    -rwxr-xr-x root/root 1160856 2026-10-01 17:50 squashfs-root/usr/bin/bash\n\
                    lrwxrwxrwx root/root 4 2026-10-01 17:50 squashfs-root/usr/bin/sh -> bash\n\
                    crw-r--r-- root/root 1,3 2026-10-01 17:50 squashfs-root/dev/null\n";
        let m = parse_listing(text);
        assert_eq!(m.get("bin"), Some(&Entry::Link("/usr/bin".into())));
        assert_eq!(m.get("usr/bin"), Some(&Entry::Dir));
        assert_eq!(m.get("usr/bin/bash"), Some(&Entry::File));
        assert_eq!(m.get("usr/bin/sh"), Some(&Entry::Link("bash".into())));
        assert_eq!(m.get("dev/null"), Some(&Entry::Other));
        assert_eq!(m.len(), 5, "the root itself is not an entry");
    }

    /// A system with everything in `FILES` and `DLOPEN`, a libc with versions up to `glibc`, and the given cores.
    pub(crate) fn full_system(r: &Root, glibc: &str, cores: &[&str]) {
        for f in FILES {
            if f.any_of[0] == "bin/sh" {
                r.file("usr/bin/sh", b"x");
                r.link("bin", "/usr/bin");
            } else {
                r.file(f.any_of[0], b"x");
            }
        }
        for d in DLOPEN {
            for lib in d.any_of {
                r.file(&format!("usr/lib/{lib}"), b"x");
            }
        }
        r.link("lib", "/usr/lib");
        let minor: u32 = glibc.rsplit('.').next().unwrap().parse().unwrap();
        let versions: Vec<String> = (17..=minor).map(|m| format!("GLIBC_2.{m}")).collect();
        let v: Vec<&str> = versions.iter().map(String::as_str).collect();
        r.file("usr/lib/libc.so.6", &elf(&["ld-linux-aarch64.so.1"], &[], "libc.so.6", &v));
        r.file("usr/lib/ld-linux-aarch64.so.1", &elf(&[], &[], "ld-linux-aarch64.so.1", &["GLIBC_2.17"]));
        r.file(
            "etc/profile.d/001-functions",
            b"function get_setting() {\n:\n}\n\nfunction set_setting() {\n:\n}\n",
        );
        for c in cores {
            r.file(&format!("usr/lib/libretro/{c}_libretro.so"), b"x");
        }
    }

    fn needs(glibc: &str, cores: &[(&str, bool)]) -> Needs {
        let ui = elf(
            &["libc.so.6", "ld-linux-aarch64.so.1"],
            &[("libc.so.6", &["GLIBC_2.17", glibc]), ("ld-linux-aarch64.so.1", &["GLIBC_2.17"])],
            "",
            &[],
        );
        let pc = elf(&["libc.so.6"], &[("libc.so.6", &["GLIBC_2.34"])], "", &[]);
        let (linked, notes) = Needs::linked_of(&[("pocket-ui", &ui), ("pocket-core", &pc)]);
        let cores = cores
            .iter()
            .map(|(c, elsewhere)| Core {
                core: c.to_string(),
                systems: vec![format!("{c}-system")],
                elsewhere: *elsewhere,
            })
            .collect();
        Needs {
            files: FILES,
            dlopen: DLOPEN,
            linked,
            cores,
            notes,
        }
    }

    #[test]
    fn libraries_of_both_programs_are_merged() {
        let n = needs("GLIBC_2.38", &[]);
        let libc = n.linked.iter().find(|l| l.soname == "libc.so.6").unwrap();
        assert_eq!(libc.versions, ["GLIBC_2.17", "GLIBC_2.38", "GLIBC_2.34"]);
        assert_eq!(libc.by, "pocket-ui and pocket-core");
        assert_eq!(n.linked.iter().find(|l| l.soname == "ld-linux-aarch64.so.1").unwrap().by, "pocket-ui");
    }

    #[test]
    fn a_complete_newer_system_passes() {
        let r = Root::new("full");
        full_system(&r, "2.41", &["gambatte"]);
        let rep = check(&Dir(r.0.to_path_buf()), &needs("GLIBC_2.38", &[("gambatte", false)]));
        assert!(rep.blocking().is_empty() && rep.warnings().is_empty(), "{}", rep.text());
        assert!(rep.text().contains("glibc 2.41 in the new system"), "{}", rep.text());
        assert_eq!(rep.refusal(), None);
        assert_eq!(rep.note(), None);
    }

    #[test]
    fn an_older_glibc_is_refused_with_both_versions() {
        let r = Root::new("old-glibc");
        full_system(&r, "2.37", &[]);
        let rep = check(&Dir(r.0.to_path_buf()), &needs("GLIBC_2.38", &[]));
        let refusal = rep.refusal().unwrap();
        assert!(
            refusal.contains("pocket-ui and pocket-core need glibc 2.38 or later (libc.so.6), the new system has glibc 2.37"),
            "{refusal}"
        );
    }

    #[test]
    fn a_missing_library_or_program_is_refused_a_missing_service_only_noted() {
        let r = Root::new("missing");
        full_system(&r, "2.41", &[]);
        std::fs::remove_file(r.0.join("usr/lib/libxkbcommon.so.0")).unwrap();
        std::fs::remove_file(r.0.join("usr/bin/start_es.sh")).unwrap();
        std::fs::remove_file(r.0.join("usr/bin/nmcli")).unwrap();
        std::fs::remove_file(r.0.join("usr/lib/libasound.so.2")).unwrap();
        let rep = check(&Dir(r.0.to_path_buf()), &needs("GLIBC_2.38", &[]));
        let b = rep.blocking();
        assert!(b.contains(&"libxkbcommon.so.0 is missing: pocket-ui can't open its window"), "{}", rep.text());
        assert!(
            b.contains(&"/usr/bin/start_es.sh is missing: no way back to EmulationStation when pocket-ui can't start"),
            "{}",
            rep.text()
        );
        assert_eq!(rep.warnings(), ["/usr/bin/nmcli is missing: Wi-Fi networks can't be listed or joined"]);
        assert!(!rep.text().contains("without sound"), "one of the two sound libraries is enough");
        assert!(rep.refusal().unwrap().contains("(and 1 more, see the log)"));
        std::fs::remove_file(r.0.join("usr/lib/libc.so.6")).unwrap();
        let rep = check(&Dir(r.0.to_path_buf()), &needs("GLIBC_2.38", &[]));
        assert!(
            rep.blocking()
                .contains(&"pocket-ui and pocket-core need libc.so.6, which the new system doesn't have"),
            "{}",
            rep.text()
        );
    }

    #[test]
    fn the_h700_image_may_lack_the_time_zone_helper_and_quantum_but_not_what_pocket_ui_breaks_on() {
        let r = Root::new("h700");
        full_system(&r, "2.41", &[]);
        for gone in ["usr/bin/steamos-polkit-helpers/steamos-set-timezone", "usr/sbin/quantum"] {
            std::fs::remove_file(r.0.join(gone)).unwrap();
        }
        let sd = Dir(r.0.to_path_buf());
        // Another system (the Mini V2's) still warns about both.
        let rep = check(&sd, &needs("GLIBC_2.38", &[]));
        assert_eq!(rep.warnings().len(), 2, "{}", rep.text());
        // The H700 image is told by its os-release.
        r.file("etc/os-release", b"OS_NAME=\"ROCKNIX\"\nHW_DEVICE=\"H700\"\nHW_ARCH=\"aarch64\"\n");
        let rep = check(&sd, &needs("GLIBC_2.38", &[]));
        assert!(rep.warnings().is_empty() && rep.blocking().is_empty(), "{}", rep.text());
        assert!(rep.text().contains("info   /usr/bin/steamos-polkit-helpers/steamos-set-timezone is missing"));
        // Anything pocket-ui would break on is still refused there.
        std::fs::remove_file(r.0.join("usr/bin/sway")).unwrap();
        assert!(check(&sd, &needs("GLIBC_2.38", &[])).blocking().iter().any(|t| t.contains("/usr/bin/sway is missing")));
    }

    #[test]
    fn a_library_that_is_not_elf_cannot_vouch_for_its_versions() {
        let r = Root::new("not-elf");
        full_system(&r, "2.41", &[]);
        std::fs::write(r.0.join("usr/lib/libc.so.6"), b"GLIBC_2.38").unwrap();
        let rep = check(&Dir(r.0.to_path_buf()), &needs("GLIBC_2.38", &[]));
        assert!(rep.blocking().iter().any(|t| t.contains("can't be read")), "{}", rep.text());
    }

    #[test]
    fn a_missing_core_warns_unless_the_store_has_one() {
        let r = Root::new("cores");
        full_system(&r, "2.41", &["mgba"]);
        let rep = check(
            &Dir(r.0.to_path_buf()),
            &needs("GLIBC_2.38", &[("mgba", false), ("gambatte", false), ("snes9x", true)]),
        );
        assert_eq!(rep.warnings(), ["no gambatte core in the new system, so gambatte-system games would not start"]);
        assert!(rep
            .text()
            .contains("info   no snes9x core in the new system; the copy in /storage/cores plays snes9x-system"));
        assert_eq!(
            rep.note().unwrap(),
            "Note: no gambatte core in the new system, so gambatte-system games would not start."
        );
    }

    #[test]
    fn rocknix_functions_and_the_ui_start_scripts_are_looked_at() {
        let r = Root::new("scripts");
        full_system(&r, "2.41", &[]);
        r.file(
            "etc/profile.d/001-functions",
            b"function get_setting() {\n:\n}\nwait_lock() {\n  exec {J_CONF_LOCK_FD}>>\"${J_CONF_LOCK}\"\n  flock -x \"${J_CONF_LOCK_FD}\"\n}\n",
        );
        r.file("usr/bin/start_es.sh", b"#!/bin/bash\nsomething_new\nemulationstation\n");
        let rep = check(&Dir(r.0.to_path_buf()), &needs("GLIBC_2.38", &[]));
        assert!(rep.warnings().iter().any(|t| t.contains("set_setting is gone")), "{}", rep.text());
        assert!(!rep.text().contains("get_setting is gone"));
        assert!(rep.text().contains("lock is ROCKNIX's flock"), "{}", rep.text());
        assert!(rep.text().contains("start_es.sh is a version the launcher doesn't know"), "{}", rep.text());
    }

    #[test]
    fn known_ui_start_scripts_are_recognised() {
        // ROCKNIX 20261001's start_es.sh, byte for byte (projects/ROCKNIX/packages/ui/emulationstation/sources).
        let start_es = "#!/bin/bash\n# SPDX-License-Identifier: GPL-2.0-or-later\n# Copyright (C) 2024 ROCKNIX (https://github.com/ROCKNIX)\n\n### setup is the same\n. $(dirname $0)/es_settings\n\nemulationstation --log-path /var/log --no-splash\n";
        assert_eq!(crate::store::sha256_hex(start_es.as_bytes()), KNOWN_UI_START[0].1);
        let r = Root::new("known");
        full_system(&r, "2.41", &[]);
        r.file("usr/bin/start_es.sh", start_es.as_bytes());
        let rep = check(&Dir(r.0.to_path_buf()), &needs("GLIBC_2.38", &[]));
        assert!(rep.text().contains("start_es.sh as in ROCKNIX 20261001"), "{}", rep.text());
    }

    #[test]
    fn finds_function_definitions_in_all_three_forms() {
        let text = "function get_setting() {\nset_setting () {\nfunction sway_fullscreen {\n  set_kill set x\n# del_setting() in a comment\n";
        assert!(defines_function(text, "get_setting"));
        assert!(defines_function(text, "set_setting"));
        assert!(defines_function(text, "sway_fullscreen"));
        assert!(!defines_function(text, "set_kill"), "a call is no definition");
        assert!(!defines_function(text, "get_settings"));
        assert!(!defines_function(text, "del_setting"));
    }

    /// The real thing, when it has been built and the virtual device's SYSTEM is there (docs/VIRTUAL-DEVICE.md).
    #[test]
    fn the_aarch64_build_reads_as_elf_when_present() {
        let bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/aarch64-unknown-linux-gnu/release/pocket-ui");
        let Ok(data) = std::fs::read(&bin) else { return };
        let e = parse_elf(&data).unwrap();
        assert!(e.needed.iter().any(|n| n == "libc.so.6"), "{e:?}");
        assert!(e.needs.iter().any(|(f, v)| f == "libc.so.6" && v.starts_with("GLIBC_2.")), "{e:?}");
    }

    // ---- scripts/rocknix-contract.txt (the expectations of scripts/rocknix-contract.sh, docs/ROCKNIX-CONTRACT.md) ----

    /// The rows of the contract's data file: (kind, label, code, path, argument).
    pub(crate) fn contract_rows() -> Vec<[String; 5]> {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/rocknix-contract.txt")).unwrap();
        text.lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| {
                let mut f = l.splitn(5, '|').map(str::to_string);
                let row = [(); 5].map(|_| f.next().unwrap_or_default());
                assert!(!row[0].is_empty() && !row[3].is_empty(), "a row without kind or path: {l}");
                row
            })
            .collect()
    }

    /// What the shell script checks has to follow the code: the UI-start versions the launcher knows, the files and
    /// libraries an OS update needs, the cores pocket-core plays with, every system hostsys names, system.cfg's path and
    /// lock, the Runs on page's keys and the function keys. A change on one side without the other is a red test.
    #[test]
    fn the_rocknix_contract_data_matches_the_code() {
        let rows = contract_rows();
        fn of<'a>(rows: &'a [[String; 5]], kind: &'a str) -> impl Iterator<Item = &'a [String; 5]> {
            rows.iter().filter(move |r| r[0] == kind)
        }
        // The sha rows are KNOWN_UI_START.
        for file in ["usr/bin/start_es.sh", "usr/bin/es_settings"] {
            let mut known: Vec<&str> = KNOWN_UI_START.iter().filter(|(p, _, _)| *p == file).map(|(_, sha, _)| *sha).collect();
            let mut data: Vec<&str> = of(&rows, "sha").filter(|r| r[3] == file).flat_map(|r| r[4].split_whitespace()).collect();
            known.sort();
            data.sort();
            assert_eq!(data, known, "sha row of {file} vs os_compat::KNOWN_UI_START");
        }
        // Every path of FILES and DLOPEN is a file or exec row.
        let checked: Vec<&str> = rows.iter().filter(|r| r[0] == "file" || r[0] == "exec").flat_map(|r| r[3].split(';')).collect();
        for need in FILES {
            for path in need.any_of {
                assert!(checked.contains(path), "os_compat::FILES has {path}: add a file row to scripts/rocknix-contract.txt");
            }
        }
        for need in DLOPEN {
            for lib in need.any_of {
                assert!(checked.contains(&format!("usr/lib/{lib}").as_str()), "os_compat::DLOPEN has {lib}: add a file row");
            }
        }
        // The core rows are hostsys::routed_cores.
        let mut cores: Vec<&str> = crate::hostsys::routed_cores().into_iter().map(|(c, _)| c).collect();
        let mut data: Vec<&str> = of(&rows, "core").map(|r| r[4].as_str()).collect();
        cores.sort();
        data.sort();
        assert_eq!(data, cores, "core rows vs hostsys::routed_cores");
        // A sys row for every system name hostsys knows.
        let sys: Vec<&str> = of(&rows, "sys").map(|r| r[3].as_str()).collect();
        let names = crate::hostsys::SYSTEMS.iter().flat_map(|s| s.names.iter()).chain(crate::hostsys::HANDHELD_CORES.iter().map(|(s, _)| s));
        for name in names {
            assert!(sys.contains(name), "hostsys knows {name}: add its sys row (es_systems.cfg's folder and extensions, or -)");
        }
        // system.cfg's lock and folder as system_cfg has them.
        let unescape = |re: &str| re.replace('\\', "");
        let grep = |path: &str, needle: &str| of(&rows, "grep").any(|r| r[3] == path && unescape(&r[4]).contains(needle));
        assert!(grep("etc/profile.d/001-functions", &format!("J_CONF_LOCK=\"{}\"", crate::system_cfg::LOCK)), "lock row vs system_cfg::LOCK");
        let dir = crate::system_cfg::PATH.strip_suffix("/configs/system.cfg").expect("system_cfg::PATH ends in /configs/system.cfg");
        assert!(grep("etc/profile.d/001-functions", &format!("J_DIR=\"{dir}\"")), "J_DIR row vs system_cfg::PATH");
        assert!(of(&rows, "lockstyle").any(|r| r[3] == crate::system_cfg::FUNCTIONS.trim_start_matches('/')), "lockstyle row reads system_cfg::FUNCTIONS");
        // setsettings.sh's keys are the Runs on page's; input_sense's the function keys pocket-ui sets.
        let keys = |path: &str| -> Vec<String> { of(&rows, "keys").filter(|r| r[3] == path).flat_map(|r| r[4].split_whitespace().map(str::to_string)).collect() };
        let set = keys("usr/bin/setsettings.sh");
        for s in crate::emu_choice::RA_SETTINGS {
            assert!(set.iter().any(|k| k == s.key), "Runs on offers {}: add it to setsettings.sh's keys row", s.key);
        }
        let sense = keys("usr/bin/input_sense");
        for (k, _) in crate::fn_keys::KEYS {
            assert!(sense.iter().any(|x| x == k), "fn_keys sets {k}: add it to input_sense's keys row");
        }
    }

    /// docs/ROCKNIX-CONTRACT.md lists every row of the data: a table line per promise, the systems and cores by name.
    #[test]
    fn the_rocknix_contract_doc_lists_every_row() {
        let doc = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/ROCKNIX-CONTRACT.md")).unwrap();
        for r in contract_rows() {
            match r[0].as_str() {
                "src" => {}
                "sys" => assert!(doc.contains(&format!("`{}`", r[3])), "docs/ROCKNIX-CONTRACT.md doesn't name the system {}", r[3]),
                "core" => assert!(doc.contains(&format!("`{}`", r[4])), "docs/ROCKNIX-CONTRACT.md doesn't name the core {}", r[4]),
                _ => assert!(doc.contains(&format!("| {} |", r[1])), "docs/ROCKNIX-CONTRACT.md has no table line for: {}", r[1]),
            }
        }
    }

    /// The shell script itself on a tiny made-up root (about a second): every kind of check, ok and broken.
    #[test]
    fn the_rocknix_contract_script_passes_its_self_test() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test-rocknix-contract.sh");
        let out = std::process::Command::new("sh").arg(&script).output().unwrap();
        assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
}

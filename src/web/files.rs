//! Uploads from the browser, and what the upload page needs to know: where a file may go, under which name, whether
//! it fits, the BIOS files pocket-core looks for (`hostsys`), and the PortMaster ports that wait for the player's files
//! (`port_files`).
//!
//! The rules, all checked here and unit-tested:
//! - Only three kinds of place: a game system's ROM folder (from es_systems.cfg, inside `/storage/roms`, never
//!   `ports`, `bios`, `PortMaster`, `savestates` or `port-files`, never a system that runs scripts), `roms/bios` (and
//!   its `dc/` folder for the arcade sets hostsys names), and a port's drop folder `roms/port-files/<port>/…`, from
//!   which pocket-ui moves the files into the port when it starts (the same path as the file share, docs/DEVELOPING.md).
//!   The share hides `ports/` because the Pocket runs programs from there; so does this.
//! - A ROM must have one of its system's extensions. Names are cut to their last part, control characters and the
//!   characters Windows and the share refuse are replaced, no leading dot, at most 200 bytes. A folder upload's
//!   relative path is cleaned segment by segment; `..`, empty segments and links are refused.
//! - The file is written to a temporary name beside the target (created exclusively, not through a link), synced,
//!   and renamed into place with `RENAME_NOREPLACE`, so an existing file is replaced only when the player confirmed
//!   it (`overwrite`). Anything that goes wrong removes the temporary file.
//! - At most [`MAX_ROM`], [`MAX_BIOS`] or [`MAX_PORT`] bytes, and never more than the card's free space less
//!   [`RESERVE`].

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A game file: PS2 and GameCube images are the largest (a dual-layer DVD is 8.5 GB).
pub const MAX_ROM: u64 = 9 << 30;
/// BIOS files are small; the largest sets (NAOMI, PS2) are a few MB.
pub const MAX_BIOS: u64 = 128 << 20;
/// One file of a port's game data.
pub const MAX_PORT: u64 = 9 << 30;
/// Left free on the card after an upload: saves and states must still fit.
pub const RESERVE: u64 = 256 << 20;
/// Bytes of a name (ext4 allows 255; room for the temporary file's suffix).
const NAME_MAX: usize = 200;
/// Folders deep in a port upload.
const DEPTH_MAX: usize = 12;
/// Folders under `/storage/roms` nothing is ever uploaded into.
pub const NEVER: &[&str] = &["ports", "bios", "portmaster", "savestates", "port-files"];

/// The folders uploads go into; tests point them at a temporary folder.
#[derive(Clone, Debug)]
pub struct Roots {
    pub roms: PathBuf,
    pub bios: PathBuf,
    pub drop: PathBuf,
    pub ports: PathBuf,
    /// Where HOME + A in a game saves its pictures (`screenshot`); read-only for the companion.
    pub screenshots: PathBuf,
}

impl Default for Roots {
    fn default() -> Self {
        Roots {
            roms: PathBuf::from("/storage/roms"),
            bios: PathBuf::from(crate::hostsys::BIOS_DIR),
            drop: PathBuf::from(crate::port_files::DROP_DIR),
            ports: PathBuf::from(crate::ports::PORTS_DIR),
            screenshots: crate::screenshot::dir(),
        }
    }
}

impl Roots {
    /// The same layout under `roms` (tests, the virtual device).
    pub fn under(roms: &Path) -> Roots {
        Roots { roms: roms.to_path_buf(), bios: roms.join("bios"), drop: roms.join("port-files"), ports: roms.join("ports"), screenshots: roms.join("screenshots") }
    }
}

/// Why an upload was refused or failed; each is one status code and a sentence for the page.
#[derive(Debug, PartialEq)]
pub enum Refused {
    /// The name, the place or the file type is not allowed.
    NotAllowed(String),
    /// A file of that name is there and the player did not say to replace it.
    Exists(String),
    TooBig(String),
    NoSpace(String),
    /// The browser sent less than it announced.
    Short,
    Io(String),
}

impl Refused {
    pub fn status(&self) -> u16 {
        match self {
            Refused::NotAllowed(_) => 403,
            Refused::Exists(_) => 409,
            Refused::TooBig(_) => 413,
            Refused::NoSpace(_) => 507,
            Refused::Short => 400,
            Refused::Io(_) => 500,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Refused::NotAllowed(m) | Refused::Exists(m) | Refused::TooBig(m) | Refused::NoSpace(m) | Refused::Io(m) => m.clone(),
            Refused::Short => "The upload was cut off. Nothing was kept.".into(),
        }
    }
}

/// Characters that change how a name looks without being seen: zero-width ones, the marks and overrides of the
/// writing direction ("evil\u{202e}abg.sh" shows as "evilhs.gba"), the byte-order mark, and the Unicode slashes that
/// look like path separators.
fn invisible_or_lookalike(c: char) -> bool {
    matches!(c, '\u{00ad}' | '\u{061c}' | '\u{180e}' | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}' | '\u{fff9}'..='\u{fffb}')
        || matches!(c, '\u{2215}' | '\u{2044}' | '\u{29f8}' | '\u{ff0f}' | '\u{ff3c}')
}

/// A file name as it may be written: the last part of what the browser sent, with control characters, invisible
/// and look-alike characters ([`invisible_or_lookalike`]) and `\ / : * ? " < > |` turned into `_`, no leading dots or
/// spaces, no trailing dots or spaces, at most 200 bytes (cut at a character boundary, the extension kept). None when
/// nothing usable is left.
pub fn sanitize_name(raw: &str) -> Option<String> {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let mut name: String = last.chars().map(|c| if c.is_control() || invisible_or_lookalike(c) || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    name = name.trim_start_matches(['.', ' ']).trim_end_matches(['.', ' ']).to_string();
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    if name.len() > NAME_MAX {
        let (stem, ext) = match name.rfind('.') {
            Some(i) if name.len() - i <= 16 => (name[..i].to_string(), name[i..].to_string()),
            _ => (name.clone(), String::new()),
        };
        let mut cut = NAME_MAX - ext.len();
        while !stem.is_char_boundary(cut) {
            cut -= 1;
        }
        name = format!("{}{ext}", stem[..cut].trim_end());
    }
    Some(name)
}

/// A relative path from a folder upload ("Game/data/pak0.pak"), each segment cleaned like a name. None for `..`, an
/// empty segment in the middle, an absolute path or more than twelve levels.
pub fn sanitize_rel(raw: &str) -> Option<Vec<String>> {
    if raw.starts_with('/') || raw.starts_with('\\') {
        return None;
    }
    let parts: Vec<&str> = raw.split(['/', '\\']).collect();
    if parts.is_empty() || parts.len() > DEPTH_MAX {
        return None;
    }
    let mut out = Vec::new();
    for p in parts {
        if p == ".." || p == "." || p.is_empty() {
            return None;
        }
        out.push(sanitize_name(p)?);
    }
    Some(out)
}

/// The extension of `name` that `exts` lists, longest first ("game.p8.png" is a `p8.png`); None when it has none of
/// them.
pub fn matching_ext<'a>(name: &str, exts: &'a [String]) -> Option<&'a str> {
    let lower = name.to_lowercase();
    exts.iter().filter(|e| !e.is_empty() && lower.ends_with(&format!(".{e}")) && lower.len() > e.len() + 1).max_by_key(|e| e.len()).map(String::as_str)
}

/// Systems that hold programs rather than games (ROCKNIX's ports and tools are scripts the Pocket runs as root).
fn runs_programs(sys: &crate::library::SysInfo) -> bool {
    sys.id == "ports" || sys.extensions.iter().any(|e| matches!(e.as_str(), "sh" | "bash" | "py" | "elf" | "run" | "bin.sh"))
}

/// The folder an upload for `sys` goes into, if uploads may go there at all: inside the ROMs root, not one of the
/// [`NEVER`] folders, not through a link that leads elsewhere.
pub fn system_folder(roots: &Roots, sys: &crate::library::SysInfo) -> Result<PathBuf, Refused> {
    let no = |why: &str| Err(Refused::NotAllowed(format!("{}: {why}", sys.name)));
    if runs_programs(sys) {
        return no("this folder holds programs, not games. Use PortMaster on the Pocket.");
    }
    let dir = sys.path.clone();
    let Ok(rel) = dir.strip_prefix(&roots.roms) else {
        return no("its games are not in the ROMs folder.");
    };
    let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default();
    if rel.as_os_str().is_empty() || NEVER.contains(&first.as_str()) || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return no("uploads can't go there.");
    }
    inside(&roots.roms, &dir, None).map_err(|m| Refused::NotAllowed(format!("{}: {m}", sys.name)))
}

/// Whether uploads for `sys` are offered at all (without making its folder).
pub fn system_folder_ok(roots: &Roots, sys: &crate::library::SysInfo) -> bool {
    if runs_programs(sys) {
        return false;
    }
    let Ok(rel) = sys.path.strip_prefix(&roots.roms) else { return false };
    let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default();
    !rel.as_os_str().is_empty() && !NEVER.contains(&first.as_str()) && rel.components().all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// `dir` exists (or is made, one level below `root`) and is really inside `root` once links are followed; returns
/// it with every link resolved, and that is the path the upload is written through. The first folder below `root`,
/// after resolving, must not be one of the [`NEVER`] folders (a link `roms/gba` → `roms/ports` is refused, review
/// F12), except `allowed` (the BIOS folder for BIOS files, `port-files` for a port's drop folder).
fn inside(root: &Path, dir: &Path, allowed: Option<&str>) -> Result<PathBuf, String> {
    if !dir.exists() {
        if dir.parent() != Some(root) {
            return Err("that folder is missing.".into());
        }
        std::fs::create_dir(dir).map_err(|e| format!("can't make the folder: {e}"))?;
    }
    let real_root = root.canonicalize().map_err(|_| "the ROMs folder is missing.".to_string())?;
    let real = dir.canonicalize().map_err(|_| "that folder is missing.".to_string())?;
    let Ok(rel) = real.strip_prefix(&real_root) else {
        return Err("that folder leads outside the ROMs folder.".into());
    };
    let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase());
    match first {
        None => Err("that folder leads outside the ROMs folder.".into()),
        Some(f) if NEVER.contains(&f.as_str()) && allowed != Some(f.as_str()) => Err("that folder leads where uploads can't go.".into()),
        Some(_) => Ok(real),
    }
}

/// Where a ROM called `raw` for `sys` is written.
pub fn rom_target(roots: &Roots, sys: &crate::library::SysInfo, raw: &str) -> Result<PathBuf, Refused> {
    let name = sanitize_name(raw).ok_or_else(|| Refused::NotAllowed("That file name can't be used.".into()))?;
    let dir = system_folder(roots, sys)?;
    if matching_ext(&name, &sys.extensions).is_none() {
        let shown: Vec<String> = sys.extensions.iter().take(8).map(|e| format!(".{e}")).collect();
        return Err(Refused::NotAllowed(format!("{} takes {} files, not this one.", sys.name, shown.join(", "))));
    }
    Ok(dir.join(name))
}

/// A shell, Python or Perl script by its name.
fn script(name: &str) -> bool {
    let lower = name.to_lowercase();
    [".sh", ".bash", ".py", ".pl"].iter().any(|e| lower.ends_with(e))
}

/// Native code by its name: shared libraries (`.so`, `.so.1`, `.so.1.2`), executables and kernel modules. A port's
/// engine could load these as root; no game data needs them (review F10). The content is checked too ([`ELF`]).
fn native(name: &str) -> bool {
    let lower = name.to_lowercase();
    let so = lower.ends_with(".so") || lower.split(".so.").nth(1).is_some_and(|v| !v.is_empty() && v.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())));
    so || [".elf", ".ko", ".appimage", ".run", ".bin.sh"].iter().any(|e| lower.ends_with(e))
}

/// The first bytes of a Linux program or library.
const ELF: &[u8] = b"\x7fELF";

/// The paths below `roms/bios` that hostsys names inside a folder ("dc/naomi.zip").
fn bios_subpaths() -> Vec<&'static str> {
    crate::hostsys::SYSTEMS.iter().flat_map(|s| s.bios.iter()).flat_map(|b| b.files.iter().copied()).filter(|f| f.contains('/')).collect()
}

/// Where a BIOS file called `raw` is written: `roms/bios/<name>`, or `roms/bios/dc/<name>` for a file hostsys looks
/// for there (`sub` = "dc").
pub fn bios_target(roots: &Roots, raw: &str, sub: Option<&str>) -> Result<PathBuf, Refused> {
    let name = sanitize_name(raw).ok_or_else(|| Refused::NotAllowed("That file name can't be used.".into()))?;
    if script(&name) || native(&name) {
        return Err(Refused::NotAllowed("Scripts and programs don't go into the BIOS folder.".into()));
    }
    // `roms/bios` itself, made when missing, must not lead outside the ROMs folder.
    let bios = inside(&roots.roms, &roots.bios, Some("bios")).map_err(|m| Refused::NotAllowed(format!("The BIOS folder: {m}")))?;
    match sub.filter(|s| !s.is_empty()) {
        None => Ok(bios.join(name)),
        Some(sub) => {
            let wanted = format!("{sub}/{name}");
            // The folder as hostsys spells it, never as the browser sent it.
            let Some(known) = bios_subpaths().into_iter().find(|p| p.eq_ignore_ascii_case(&wanted)) else {
                return Err(Refused::NotAllowed(format!("{wanted} is not a file pocket-core looks for.")));
            };
            let (sub, _) = known.rsplit_once('/').unwrap_or((known, ""));
            let dir = bios.join(sub);
            if std::fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(Refused::NotAllowed("That folder is a link.".into()));
            }
            std::fs::create_dir_all(&dir).map_err(|e| Refused::Io(format!("Can't make the folder: {e}")))?;
            Ok(dir.join(name))
        }
    }
}

/// Where a port's file goes: its drop folder `roms/port-files/<dir>/<rel…>`, for a port that is installed, waits for
/// files and has a drop folder. Folders on the way are made; none of them may be a link.
pub fn port_target(roots: &Roots, dir: &str, rel: &str) -> Result<PathBuf, Refused> {
    let ports = ports_needing_files(roots);
    let Some(port) = ports.iter().find(|p| p.dir == dir) else {
        return Err(Refused::NotAllowed("That port is not installed or doesn't need files.".into()));
    };
    if !port.drop {
        return Err(Refused::NotAllowed(format!("{}: copy its files with the SD card or SSH (its instructions name no folder).", port.title)));
    }
    let parts = sanitize_rel(rel).ok_or_else(|| Refused::NotAllowed("That path can't be used.".into()))?;
    // A port's launcher runs as root: nothing that a shell, Python or the system would run goes into its folder.
    if parts.last().is_some_and(|n| script(n) || native(n)) {
        return Err(Refused::NotAllowed("Scripts and programs don't go into a port's folder.".into()));
    }
    let drop = crate::port_files::make_drop(&roots.drop, dir).ok_or_else(|| Refused::Io("Can't make the drop folder.".into()))?;
    let mut at = inside(&roots.roms, &drop, Some("port-files")).map_err(Refused::NotAllowed)?;
    for seg in &parts[..parts.len() - 1] {
        at = at.join(seg);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.file_type().is_symlink() || !m.is_dir() => return Err(Refused::NotAllowed(format!("{seg} is in the way."))),
            Ok(_) => {}
            Err(_) => std::fs::create_dir(&at).map_err(|e| Refused::Io(format!("Can't make {seg}: {e}")))?,
        }
    }
    Ok(at.join(&parts[parts.len() - 1]))
}

/// Bytes free for us on the file system of `dir`.
pub fn free_space(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs fills the zeroed struct for a valid C string path.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        (libc::statvfs(c.as_ptr(), &mut st) == 0).then(|| st.f_bavail as u64 * st.f_frsize as u64)
    }
}

/// Bytes promised to the uploads under way: each counts against the free space until it ends (review F9).
static RESERVED: std::sync::Mutex<u64> = std::sync::Mutex::new(0);

/// Whether `len` more bytes fit, with `free` bytes free and `reserved` promised to other uploads: [`RESERVE`] stays
/// free. A card whose free space can't be read takes nothing.
pub fn room(free: Option<u64>, reserved: u64, len: u64, max: u64) -> Result<(), Refused> {
    if len > max {
        return Err(Refused::TooBig(format!("That file is larger than {}.", crate::ports::size_label(max))));
    }
    let Some(free) = free else {
        return Err(Refused::NoSpace("The Pocket can't tell how much space is left on the SD card.".into()));
    };
    let left = free.saturating_sub(reserved);
    if len.saturating_add(RESERVE) > left {
        return Err(Refused::NoSpace(format!("Not enough space on the SD card: {} free.", crate::ports::size_label(left.saturating_sub(RESERVE)))));
    }
    Ok(())
}

/// Whether `len` bytes may be written beside `target` now (the upload check).
pub fn fits(target: &Path, len: u64, max: u64) -> Result<(), Refused> {
    let reserved = *RESERVED.lock().unwrap_or_else(|e| e.into_inner());
    room(free_space(target.parent().unwrap_or(Path::new("/"))), reserved, len, max)
}

/// Room for one upload, held until it is dropped.
#[derive(Debug)]
pub struct Reservation(u64);

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut r = RESERVED.lock().unwrap_or_else(|e| e.into_inner());
        *r = r.saturating_sub(self.0);
    }
}

/// Checks and reserves room for `len` bytes beside `target` in one step, so uploads at the same time can't all take
/// the same free space.
pub fn reserve(target: &Path, len: u64, max: u64) -> Result<Reservation, Refused> {
    let mut r = RESERVED.lock().unwrap_or_else(|e| e.into_inner());
    room(free_space(target.parent().unwrap_or(Path::new("/"))), *r, len, max)?;
    *r += len;
    Ok(Reservation(len))
}

/// Bytes reserved by uploads under way.
pub fn reserved() -> u64 {
    *RESERVED.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a finished upload did.
#[derive(Debug, PartialEq)]
pub struct Written {
    pub bytes: u64,
    pub replaced: bool,
}

/// What an upload asks between its chunks.
pub struct Gate<'a> {
    /// A game runs: wait.
    pub wait: &'a dyn Fn() -> bool,
    /// The companion was switched off: stop, keep nothing.
    pub abort: &'a dyn Fn() -> bool,
    /// Refuse a file that starts like a program (port files).
    pub no_programs: bool,
}

fn never() -> bool {
    false
}

impl Gate<'_> {
    /// Never waits, never stops (tests).
    pub const OPEN: Gate<'static> = Gate { wait: &never, abort: &never, no_programs: false };
}

/// The temporary file beside `target`: hidden, with an ending no system lists, so a cut upload is never a game.
fn temp_path(target: &Path, n: u64) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    target.with_file_name(format!(".{name}.{}-{n}.upload", std::process::id()))
}

static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Removes the temporary files an earlier pocket-ui left beside `target` (it ended in the middle of an upload): hidden
/// files ending in `.upload` whose process id is not ours. Only plain files, never through a link.
pub fn sweep_leftovers(target: &Path) {
    let Some(dir) = target.parent() else { return };
    let ours = format!(".{}-", std::process::id());
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') || !name.ends_with(".upload") {
            continue;
        }
        // ".<name>.<pid>-<n>.upload": the pid sits before the last '-'.
        let stem = &name[..name.len() - ".upload".len()];
        let Some(dash) = stem.rfind('-') else { continue };
        let Some(dot) = stem[..dash].rfind('.') else { continue };
        if stem[dot..=dash] == ours || !stem[dot + 1..dash].bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if e.file_type().is_ok_and(|t| t.is_file()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Writes `len` bytes from `r` to `target`: a temporary file, synced, then renamed into place (over an existing file
/// only with `overwrite`). `target`'s folder must be the one the checks resolved (no link in it): it is resolved again
/// just before the rename, and a folder that became a link meanwhile is refused (review F11). `gate` is asked between
/// chunks: while a game runs the upload pauses; switched off, it stops.
pub fn receive(r: &mut impl Read, target: &Path, len: u64, overwrite: bool, gate: &Gate) -> Result<Written, Refused> {
    use std::os::unix::fs::OpenOptionsExt;
    let exists = std::fs::symlink_metadata(target).is_ok();
    if exists && !overwrite {
        return Err(Refused::Exists(format!("{} is there already.", target.file_name().unwrap_or_default().to_string_lossy())));
    }
    if std::fs::symlink_metadata(target).is_ok_and(|m| !m.is_file()) {
        return Err(Refused::NotAllowed("Something that is not a file has that name.".into()));
    }
    sweep_leftovers(target);
    let tmp = temp_path(target, TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&tmp)
        .map_err(|e| Refused::Io(format!("Can't write there: {e}")))?;
    let result = copy(r, &mut f, len, gate).and_then(|n| {
        f.sync_all().map_err(|e| Refused::Io(format!("The SD card refused the write: {e}")))?;
        drop(f);
        unchanged_folder(target)?;
        place(&tmp, target, overwrite)?;
        Ok(Written { bytes: n, replaced: exists })
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    if result.is_ok() {
        sync_dir(target);
    }
    result
}

/// The target's folder, resolved now, is still the path it was written through: no folder on the way became a link.
fn unchanged_folder(target: &Path) -> Result<(), Refused> {
    let dir = target.parent().unwrap_or(Path::new("/"));
    match dir.canonicalize() {
        Ok(real) if real == dir => Ok(()),
        _ => Err(Refused::NotAllowed("A folder along the path changed during the upload. Nothing was kept.".into())),
    }
}

fn copy(r: &mut impl Read, w: &mut std::fs::File, len: u64, gate: &Gate) -> Result<u64, Refused> {
    let off = || Refused::Io("The web companion was switched off. Nothing was kept.".into());
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut head: Vec<u8> = Vec::new();
    // A game that runs for hours: the browser has long given up, so does the upload (after 30 minutes).
    let mut waited = Duration::ZERO;
    while done < len {
        while (gate.wait)() {
            if (gate.abort)() {
                return Err(off());
            }
            if waited >= Duration::from_secs(30 * 60) {
                return Err(Refused::Io("A game was running for too long; the upload stopped. Nothing was kept.".into()));
            }
            std::thread::sleep(Duration::from_millis(500));
            waited += Duration::from_millis(500);
        }
        if (gate.abort)() {
            return Err(off());
        }
        let want = buf.len().min((len - done) as usize);
        let n = match r.read(&mut buf[..want]) {
            Ok(0) => return Err(Refused::Short),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(Refused::Short),
        };
        if gate.no_programs && head.len() < ELF.len() {
            head.extend_from_slice(&buf[..n.min(ELF.len() - head.len())]);
            if head.len() == ELF.len() && head == ELF {
                return Err(Refused::NotAllowed("That file is a program. Programs don't go into a port's folder. Nothing was kept.".into()));
            }
        }
        w.write_all(&buf[..n]).map_err(|e| Refused::Io(format!("The SD card refused the write: {e}")))?;
        done += n as u64;
    }
    if (gate.abort)() {
        return Err(off());
    }
    Ok(done)
}

/// Renames `tmp` to `target`; without `overwrite` it fails rather than replace a file that appeared meanwhile.
fn place(tmp: &Path, target: &Path, overwrite: bool) -> Result<(), Refused> {
    let exists_err = || Refused::Exists(format!("{} is there already.", target.file_name().unwrap_or_default().to_string_lossy()));
    if overwrite {
        return std::fs::rename(tmp, target).map_err(|e| Refused::Io(format!("Can't put the file in place: {e}")));
    }
    match rename_noreplace(tmp, target) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(exists_err()),
        Err(e) => Err(Refused::Io(format!("Can't put the file in place: {e}"))),
    }
}

/// Renames without replacing: `renameat2(RENAME_NOREPLACE)` (ext4, exFAT and FAT take it), else a hard link and an
/// unlink. A file system with neither refuses: a check followed by a plain rename could replace a file that appeared
/// in between (review F11).
#[cfg(target_os = "linux")]
fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let (a, b) = (std::ffi::CString::new(from.as_os_str().as_bytes())?, std::ffi::CString::new(to.as_os_str().as_bytes())?);
    const RENAME_NOREPLACE: libc::c_uint = 1;
    // SAFETY: renameat2 with two valid C strings relative to the working directory.
    let rc = unsafe { libc::syscall(libc::SYS_renameat2, libc::AT_FDCWD, a.as_ptr(), libc::AT_FDCWD, b.as_ptr(), RENAME_NOREPLACE) };
    if rc == 0 {
        return Ok(());
    }
    let e = io::Error::last_os_error();
    match e.raw_os_error() {
        Some(libc::EINVAL) | Some(libc::ENOSYS) => {
            std::fs::hard_link(from, to)?;
            std::fs::remove_file(from)
        }
        _ => Err(e),
    }
}

/// Elsewhere (a desktop, tests on macOS): a hard link fails over an existing name.
#[cfg(not(target_os = "linux"))]
fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::hard_link(from, to)?;
    std::fs::remove_file(from)
}

fn sync_dir(target: &Path) {
    if let Some(dir) = target.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
}

// --- BIOS ------------------------------------------------------------------------------------------------------

/// One BIOS requirement of pocket-core (`hostsys::SYSTEMS`), and whether a file for it is there.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct BiosNeed {
    pub label: String,
    /// Any one of these, relative to `roms/bios`.
    pub files: Vec<String>,
    /// The file found, if any.
    pub found: Option<String>,
}

impl BiosNeed {
    /// "Missing for Famicom Disk System: disksys.rom" (several: "a, b or c").
    pub fn missing_line(&self) -> Option<String> {
        if self.found.is_some() {
            return None;
        }
        let (last, rest) = self.files.split_last()?;
        let names = if rest.is_empty() { last.clone() } else { format!("{} or {last}", rest.join(", ")) };
        Some(format!("Missing for {}: {names}", self.label))
    }
}

/// Every BIOS requirement, once per console, in hostsys's order.
pub fn bios_report(bios_dir: &Path) -> Vec<BiosNeed> {
    let mut out: Vec<BiosNeed> = Vec::new();
    for s in crate::hostsys::SYSTEMS {
        for b in s.bios {
            if out.iter().any(|n| n.label == b.label) {
                continue;
            }
            let found = b.files.iter().find(|f| bios_dir.join(f).is_file()).map(|f| f.to_string());
            out.push(BiosNeed { label: b.label.to_string(), files: b.files.iter().map(|f| f.to_string()).collect(), found });
        }
    }
    out
}

/// The requirement a file of that name (relative to `roms/bios`) answers, if any.
pub fn bios_for(name: &str) -> Option<&'static str> {
    crate::hostsys::SYSTEMS
        .iter()
        .flat_map(|s| s.bios.iter())
        .find(|b| b.files.iter().any(|f| f.eq_ignore_ascii_case(name)))
        .map(|b| b.label)
}

// --- PortMaster ------------------------------------------------------------------------------------------------

/// An installed port that waits for the player's files.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PortNeed {
    pub title: String,
    pub dir: String,
    /// Folders in the port the files belong in, as its instructions name them.
    pub folders: Vec<String>,
    /// Files the instructions name; any one of them is enough.
    pub files: Vec<String>,
    /// The instructions, as PortMaster has them.
    pub instructions: String,
    /// Uploads are taken (the instructions name a folder or the files).
    pub drop: bool,
    /// Entries waiting in the drop folder for the next start of the port.
    pub waiting: usize,
}

pub fn ports_needing_files(roots: &Roots) -> Vec<PortNeed> {
    crate::ports::installed_needing_files(&roots.ports)
        .into_iter()
        .filter(|p| crate::port_files::plain_name(&p.dir))
        .map(|p| {
            let e = crate::port_files::expect(&p.instructions, &p.dir);
            let drop = crate::port_files::drop_target(&roots.ports.join(&p.dir), &e).is_some();
            let waiting = std::fs::read_dir(roots.drop.join(&p.dir)).map(|d| d.count()).unwrap_or(0);
            PortNeed { title: p.title, dir: p.dir, folders: e.folders, files: e.files, instructions: p.instructions, drop, waiting }
        })
        .collect()
}

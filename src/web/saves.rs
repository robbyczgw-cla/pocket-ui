//! A game's saves and save states in the browser (docs/WEB-COMPANION.md, "Saves"): listed with name, time and size,
//! each one downloaded, all saves of the card (or of one game) as one ZIP, and one put back from the browser.
//!
//! Which files are a game's is decided as "Delete from the card" decides it (`game_files::saves_of`, `states_of`): the
//! saves beside the game with its name (on the Brick and dArkOS in the saves' own folder, [`Roots::saves`]), the
//! states named after it in `<states>/<system>/`. A request names a game by its id and a file by its name, which must
//! be one of that game's; a path never comes from the browser.
//!
//! Putting one back: the name of the file sent says only what it is (a save by its type, `.srm`, `.sav` …; a state by
//! its ending, `.pstate1`, `.state`, `.s01` …). Where it goes comes from the game: a save becomes `<game>.<type>` beside
//! the game (the same name as the save that is there, if one is), a state `<game><ending>` in the game's states folder.
//! So a save from another device or another name of the game lands where this Pocket looks. It arrives in a hidden file
//! beside its place; then the save it replaces is copied into a saves backup of its own (`backup_saves::create`,
//! "Before restore", listed in Settings › System › Backup & reset), and only then is it renamed into place. A backup that
//! fails changes nothing. Never while a game runs: the request is answered "playing" (`api::respond`), and a game that
//! starts during the upload ends it, with nothing changed.

use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::{json, Value};

use super::api::Ctx;
use super::files::{self, Refused, Roots};
use super::http::{Request, Response};
use super::zip;
use crate::game_files;

/// A save sent from the browser is at most this large (PS1 memory cards are 128 KB, N64's largest saves 128 KB).
pub const SAVE_MAX: u64 = 64 << 20;
/// A save state sent from the browser is at most this large (a PS2 or Wii state is tens of MB).
pub const STATE_MAX: u64 = 512 << 20;

/// What a file of a game is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Save,
    State,
}

/// One of a game's files.
#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub kind: Kind,
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    /// Unix seconds.
    pub modified: u64,
}

fn seconds(m: &std::fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

fn file_of(kind: Kind, path: PathBuf) -> Option<File> {
    let m = std::fs::symlink_metadata(&path).ok().filter(|m| m.is_file())?;
    let name = path.file_name()?.to_str()?.to_string();
    Some(File { kind, name, bytes: m.len(), modified: seconds(&m), path })
}

/// The folder a game's saves are in: beside it, or the same place below the saves' own folder.
pub fn save_dir(roots: &Roots, game: &Path) -> Option<PathBuf> {
    let dir = game.parent()?;
    match &roots.saves {
        None => Some(dir.to_path_buf()),
        Some(own) => Some(own.join(dir.strip_prefix(&roots.roms).ok()?)),
    }
}

/// The game's saves and states, saves first, each sorted by name. With `pictures`, the states' pictures too (for a
/// ZIP); a list shows a state and says whether it has one.
pub fn files_of(roots: &Roots, system: &str, game: &Path, pictures: bool) -> Vec<File> {
    let system = crate::region_pairs::own_system(system, game);
    let mut out = Vec::new();
    // A game that is a folder (DOS, ScummVM) keeps its saves in its own way.
    if !game.is_dir() {
        if let (Some(dir), Some(file)) = (save_dir(roots, game), game.file_name()) {
            out.extend(game_files::saves_of(&dir.join(file)).into_iter().filter_map(|p| file_of(Kind::Save, p)));
        }
    }
    let states = game_files::states_of(system, game, &roots.states).into_iter();
    out.extend(states.filter(|p| pictures || !is_picture(p)).filter_map(|p| file_of(Kind::State, p)));
    out
}

fn is_picture(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("png"))
}

/// What kind of save a type is, for the page: a game's own save, the cartridge's clock, a memory card.
fn save_kind(name: &str) -> &'static str {
    match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("rtc") => "clock",
        Some("mcr" | "mcd") => "card",
        _ => "save",
    }
}

/// The ending after the game's name (`.pstate2`, `.state.auto`, `.s01`) of one of its states.
fn ending<'a>(name: &'a str, game: &Path) -> &'a str {
    let (stem, file) = (game.file_stem().and_then(|s| s.to_str()).unwrap_or(""), game.file_name().and_then(|s| s.to_str()).unwrap_or(""));
    // The longer base first: "Game.iso.s01" is Dolphin's state of "Game.iso", not one of "Game".
    [file, stem].into_iter().find(|b| !b.is_empty() && game_files::is_state_of(name, b)).map_or(name, |b| &name[b.len()..])
}

/// A state's slot as the pause menu names it: ("slot", n) for Slot n, ("continue", 0) for Continue, ("undo", 0) for
/// Before load.
pub fn slot(ending: &str) -> (&'static str, u32) {
    let num = |s: &str| s.parse::<u32>().ok();
    if let Some(r) = ending.strip_prefix(".pstate").or_else(|| ending.strip_prefix(".state")) {
        if r.is_empty() {
            return ("slot", 1);
        }
        if r.starts_with(".auto") {
            return ("continue", 0);
        }
        if r == ".undo" {
            return ("undo", 0);
        }
        if let Some(n) = num(r) {
            return ("slot", n + 1);
        }
    }
    if let Some(n) = ending.strip_prefix(".s").and_then(num) {
        return ("slot", n.max(1));
    }
    match ending {
        ".pocket-auto" | ".auto.cst" => ("continue", 0),
        ".pocket-undo" | ".undo.cst" => ("undo", 0),
        _ => ("other", 0),
    }
}

/// The game a request names (`id`): its system, file and name.
fn game_of(ctx: &Ctx, id: Option<&str>) -> Result<(String, PathBuf, String), Response> {
    let lib = ctx.library();
    let (s, g) = id.and_then(|id| lib.game(id)).ok_or_else(|| Response::error(404, "That game is not on the Pocket any more."))?;
    let (sys, game) = (&lib.systems[s], &lib.systems[s].games[g]);
    Ok((sys.name.clone(), game.path.clone(), game.name.clone()))
}

/// `GET /api/saves?id=`: the game's saves and states.
pub fn list(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let (system, game, _) = game_of(ctx, req.query("id"))?;
    let files = files_of(&ctx.roots, &system, &game, true);
    let has_picture = |f: &File| files.iter().any(|p| p.name == format!("{}.png", f.name));
    let mut saves = Vec::new();
    let mut states = Vec::new();
    for f in files.iter().filter(|f| !is_picture(&f.path)) {
        match f.kind {
            Kind::Save => saves.push(json!({ "name": f.name, "bytes": f.bytes, "modified": f.modified, "kind": save_kind(&f.name) })),
            Kind::State => {
                let (label, n) = slot(ending(&f.name, &game));
                states.push(json!({ "name": f.name, "bytes": f.bytes, "modified": f.modified, "slot": label, "n": n, "picture": has_picture(f) }));
            }
        }
    }
    // Continue and the slots in order, the newest first within each.
    states.sort_by_key(|v| (slot_order(v["slot"].as_str().unwrap_or("")), v["n"].as_u64().unwrap_or(0), std::cmp::Reverse(v["modified"].as_u64().unwrap_or(0))));
    Ok(Response::json(
        200,
        &json!({
            "saves": saves,
            "states": states,
            "folder": game.is_dir(),
            "bytes": files.iter().map(|f| f.bytes).sum::<u64>(),
            "max": { "save": SAVE_MAX, "state": STATE_MAX },
        }),
    ))
}

fn slot_order(label: &str) -> u8 {
    match label {
        "continue" => 0,
        "undo" => 1,
        "slot" => 2,
        _ => 3,
    }
}

/// A name for the `Content-Disposition` header: the ASCII fallback with anything odd as `_`, and the real name
/// percent-encoded (RFC 6266/5987).
pub fn attachment(name: &str) -> String {
    let plain: String = name.chars().map(|c| if c.is_ascii_graphic() && !matches!(c, '"' | '\\' | '%' | ';') || c == ' ' { c } else { '_' }).collect();
    let encoded: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("attachment; filename=\"{plain}\"; filename*=UTF-8''{encoded}")
}

/// What ends a download: a game started, or the companion was switched off.
fn stopper(ctx: &Ctx) -> impl Fn() -> bool + Send + 'static {
    let (shared, stopped) = (Arc::clone(&ctx.shared), Arc::clone(&ctx.stopped));
    move || shared.playing() || stopped.load(Ordering::Relaxed)
}

/// `GET /api/saves/file?id=&name=`: one of the game's files, as it is.
pub fn download(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let (system, game, _) = game_of(ctx, req.query("id"))?;
    let name = req.query("name").unwrap_or("");
    let file = files_of(&ctx.roots, &system, &game, true).into_iter().find(|f| f.name == name).ok_or_else(|| Response::error(404, "That save is not on the Pocket any more."))?;
    let mut f = std::fs::File::open(&file.path).map_err(|_| Response::error(404, "That save is not on the Pocket any more."))?;
    let stop = stopper(ctx);
    Ok(Response::stream(200, "application/octet-stream", move |w: &mut dyn Write| {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            if stop() {
                return Err(io::Error::other("stopped"));
            }
            match f.read(&mut buf) {
                Ok(0) => return Ok(()),
                Ok(n) => w.write_all(&buf[..n])?,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    })
    .with("Content-Disposition", &attachment(&file.name)))
}

/// Whether a path relative to the card is a state (in `savestates/`) and so not one of "the saves".
fn is_state_rel(rel: &str) -> bool {
    rel.split('/').nth(1) == Some("savestates")
}

/// Every save on the card, as the saves backup takes them (`backup_saves::on_card`), and on the Brick and dArkOS the
/// saves' own folder too: (name in the ZIP, path, size, time).
fn card_files(roots: &Roots, states: bool) -> Vec<(String, PathBuf, u64, u64)> {
    let storage = roots.roms.parent().unwrap_or(Path::new("/"));
    let (items, _) = crate::backup_saves::on_card(storage);
    let mut out: Vec<(String, PathBuf, u64, u64)> =
        items.into_iter().filter(|i| states || !is_state_rel(&i.path)).map(|i| (i.path.clone(), storage.join(&i.path), i.bytes, i.modified)).collect();
    if let Some(own) = &roots.saves {
        let mut found = Vec::new();
        walk_saves(own, own, 4, &mut found, &mut 40_000);
        out.extend(found.into_iter().map(|(rel, p, bytes, modified)| (format!("saves/{rel}"), p, bytes, modified)));
    }
    out
}

/// Saves below `dir` (the Brick's own saves folder), names only and a `stat` per save.
fn walk_saves(base: &Path, dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf, u64, u64)>, budget: &mut usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(t) = e.file_type() else { continue };
        let path = e.path();
        let hidden = e.file_name().to_string_lossy().starts_with('.');
        if t.is_dir() && depth > 1 && !hidden {
            walk_saves(base, &path, depth - 1, out, budget);
        } else if t.is_file() && !hidden && game_files::is_save(&path) {
            if let (Ok(m), Ok(rel)) = (e.metadata(), path.strip_prefix(base)) {
                out.push((rel.to_string_lossy().into_owned(), path.clone(), m.len(), seconds(&m)));
            }
        }
    }
}

/// `GET /api/saves/all`: how many saves and states the card has, and their size.
pub fn summary(ctx: &Ctx) -> Response {
    let all = card_files(&ctx.roots, true);
    let (mut saves, mut states) = ((0usize, 0u64), (0usize, 0u64));
    for (rel, path, bytes, _) in &all {
        let tally = if is_state_rel(rel) { &mut states } else { &mut saves };
        tally.1 += bytes;
        if !is_picture(path) {
            tally.0 += 1;
        }
    }
    let count = |states: bool| all.iter().filter(|(rel, ..)| states || !is_state_rel(rel)).count();
    let backups = crate::backup_saves::list(&backup_root(&ctx.roots)).len();
    Response::json(
        200,
        &json!({
            "saves": { "count": saves.0, "bytes": saves.1, "fits": zip::fits(count(false), saves.1) },
            "states": { "count": states.0, "bytes": states.1, "fits": zip::fits(count(true), saves.1 + states.1) },
            "backups": backups,
        }),
    )
}

/// A file name of today's date: "pocket-saves-2026-11-20.zip".
fn dated(prefix: &str) -> String {
    let (_, date) = zip::dos_time(crate::recents::now_secs());
    format!("{prefix}-{:04}-{:02}-{:02}.zip", (date >> 9) + 1980, (date >> 5) & 15, date & 31)
}

/// `GET /api/saves/zip[?id=][&states=1]`: all saves of the card (with `states=1` the states too), or every save and
/// state of one game, as one ZIP. Paths in it are the card's (`roms/gba/Game.srm`), so it can be copied back as it is.
pub fn zip_of(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let storage = ctx.roots.roms.parent().unwrap_or(Path::new("/")).to_path_buf();
    let rel = |p: &Path| p.strip_prefix(&storage).map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|_| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let (entries, bytes, name): (Vec<zip::Entry>, u64, String) = if req.query("id").is_some() {
        let (system, game, title) = game_of(ctx, req.query("id"))?;
        let files = files_of(&ctx.roots, &system, &game, true);
        if files.is_empty() {
            return Err(Response::error(404, "This game has no saves on the Pocket yet."));
        }
        let bytes = files.iter().map(|f| f.bytes).sum();
        let stem = files::sanitize_name(&title).unwrap_or_else(|| "game".into());
        (files.iter().map(|f| zip::Entry { name: rel(&f.path), path: f.path.clone(), modified: f.modified }).collect(), bytes, format!("{stem} saves.zip"))
    } else {
        let states = req.query("states") == Some("1");
        let all = card_files(&ctx.roots, states);
        let bytes = all.iter().map(|f| f.2).sum();
        let name = dated(if states { "pocket-saves-and-states" } else { "pocket-saves" });
        (all.into_iter().map(|(name, path, _, modified)| zip::Entry { name, path, modified }).collect(), bytes, name)
    };
    if !zip::fits(entries.len(), bytes) {
        return Err(Response::error(413, "That is too much for one ZIP (more than 4 GB). Download the saves without states, or each game's from its page."));
    }
    let stop = stopper(ctx);
    println!("web: saves ZIP, {} files", entries.len());
    Ok(Response::stream(200, "application/zip", move |w: &mut dyn Write| zip::write(w, &entries, &stop)).with("Content-Disposition", &attachment(&name)))
}

// --- putting one back -------------------------------------------------------------------------------------------

/// Where the saves backups go: `backup_saves::ROOT` on the card (`roms/backup/pocket-ui-saves`).
pub fn backup_root(roots: &Roots) -> PathBuf {
    roots.roms.join("backup").join("pocket-ui-saves")
}

/// The base the folder of a target must stay inside: the ROMs folder, the states folder, the saves' own folder.
fn base_of(roots: &Roots, kind: Kind) -> PathBuf {
    match (kind, &roots.saves) {
        (Kind::State, _) => roots.states.clone(),
        (Kind::Save, Some(own)) => own.clone(),
        (Kind::Save, None) => roots.roms.clone(),
    }
}

/// Where a file the browser sends as `raw` goes for the game `game` of `system`, and what it is. Decided from the game
/// and the kind of file alone (see the module's notes); the folder is resolved and must be inside its base, and a
/// state's folder is made when missing.
pub fn target(roots: &Roots, system: &str, game: &Path, raw: &str) -> Result<(Kind, PathBuf), Refused> {
    let system = crate::region_pairs::own_system(system, game);
    let name = files::sanitize_name(raw).ok_or_else(|| Refused::NotAllowed("That file name can't be used.".into()))?;
    let stem = game.file_stem().and_then(|s| s.to_str()).ok_or_else(|| Refused::NotAllowed("This game's name can't hold a save.".into()))?;
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let (kind, dir, file) = if game_files::SAVE_EXTS.contains(&ext.as_str()) {
        if game.is_dir() {
            return Err(Refused::NotAllowed("This game keeps its saves in its own folder.".into()));
        }
        let dir = save_dir(roots, game).ok_or_else(|| Refused::NotAllowed("This game has no place for saves.".into()))?;
        let file = game.file_name().map(|f| dir.join(f)).unwrap_or_default();
        // The save of that type that is there keeps its name ("Game.SRM").
        let there = game_files::saves_of(&file).into_iter().find(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(&ext)));
        let file = there.and_then(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string)).unwrap_or_else(|| format!("{stem}.{ext}"));
        (Kind::Save, dir, file)
    } else {
        // The last split where the rest is a state's ending: "Super Mario Bros. 3.state1" → ".state1".
        let ending = name.char_indices().filter(|(_, c)| *c == '.').map(|(i, _)| i).rev().find(|&i| i > 0 && game_files::is_state_of(&name, &name[..i])).map(|i| &name[i..]);
        let ending = ending.ok_or_else(|| Refused::NotAllowed(format!("{name} is not a save or a save state. Saves end in .srm or .sav, states in .pstate or .state.")))?;
        if ending.to_ascii_lowercase().ends_with(".png") {
            return Err(Refused::NotAllowed("That is a state's picture. The Pocket makes it when you save.".into()));
        }
        let sent_base = &name[..name.len() - ending.len()];
        // Named after the file name with its type (Dolphin, Azahar) when the one sent was, else after the name alone.
        let game_ext = game.extension().and_then(|e| e.to_str()).unwrap_or("");
        let with_type = !game_ext.is_empty() && sent_base.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case(game_ext));
        let base = if with_type { game.file_name().and_then(|f| f.to_str()).unwrap_or(stem) } else { stem };
        let dir = roots.states.join(system);
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(|e| Refused::Io(format!("Can't make the states folder: {e}")))?;
        }
        (Kind::State, dir, format!("{base}{ending}"))
    };
    let base = base_of(roots, kind).canonicalize().map_err(|_| Refused::NotAllowed("The Pocket's saves folder is missing.".into()))?;
    let real = dir.canonicalize().map_err(|_| Refused::NotAllowed("The game's folder is missing.".into()))?;
    if !real.starts_with(&base) {
        return Err(Refused::NotAllowed("That place is outside the saves.".into()));
    }
    Ok((kind, real.join(file)))
}

fn max_of(kind: Kind) -> u64 {
    match kind {
        Kind::Save => SAVE_MAX,
        Kind::State => STATE_MAX,
    }
}

fn refused(e: Refused) -> Response {
    Response::json(e.status(), &json!({ "ok": false, "error": e.message() }))
}

/// What is in a target's place now: its name, size and time.
fn there(target: &Path) -> Option<Value> {
    let m = std::fs::symlink_metadata(target).ok()?;
    Some(json!({ "name": target.file_name().map(|n| n.to_string_lossy().into_owned()), "bytes": m.len(), "modified": seconds(&m), "file": m.is_file() }))
}

/// `POST /api/saves/check?id=&name=&size=`: where a file would go, and what it would replace.
pub fn check(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let (system, game, _) = game_of(ctx, req.query("id"))?;
    let size: u64 = req.query("size").and_then(|s| s.parse().ok()).unwrap_or(0);
    let (kind, t) = target(&ctx.roots, &system, &game, req.query("name").unwrap_or("")).map_err(refused)?;
    files::fits(&t, size, max_of(kind)).map_err(refused)?;
    let kind_name = match kind {
        Kind::Save => "save",
        Kind::State => "state",
    };
    let slot_of = (kind == Kind::State).then(|| t.file_name().and_then(|n| n.to_str()).map(|n| slot(ending(n, &game))).unwrap_or(("other", 0)));
    Ok(Response::json(
        200,
        &json!({
            "ok": true,
            "kind": kind_name,
            "name": t.file_name().map(|n| n.to_string_lossy().into_owned()),
            "slot": slot_of.map(|s| s.0),
            "n": slot_of.map(|s| s.1),
            "replaces": there(&t),
        }),
    ))
}

static STAGED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `POST /api/saves/upload?id=&name=[&replace=1]`: the body is the file. See the module's notes.
pub fn upload(ctx: &Ctx, req: &Request, body: &mut impl BufRead) -> Response {
    let Some(len) = req.content_length else {
        return Response::error(411, "The browser did not say how large the file is.");
    };
    let (system, game, title) = match game_of(ctx, req.query("id")) {
        Ok(g) => g,
        Err(r) => return r,
    };
    let (kind, t) = match target(&ctx.roots, &system, &game, req.query("name").unwrap_or("")) {
        Ok(t) => t,
        Err(e) => return refused(e),
    };
    let replace = req.query("replace") == Some("1");
    match std::fs::symlink_metadata(&t) {
        Ok(m) if !m.is_file() => return refused(Refused::NotAllowed("Something that is not a file has that name.".into())),
        Ok(_) if !replace => return refused(Refused::Exists(format!("{} is there already.", t.file_name().unwrap_or_default().to_string_lossy()))),
        _ => {}
    }
    let _room = match files::reserve(&t, len, max_of(kind)) {
        Ok(r) => r,
        Err(e) => return refused(e),
    };
    // A game that starts meanwhile ends it: its saves are the game's now.
    let stop = stopper(ctx);
    let abort = || stop();
    let gate = files::Gate { wait: &|| false, abort: &abort, no_programs: false };
    let name = t.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let game_started = || stopped_answer(ctx);
    let (tmp_dir, staged) = match staging(ctx, kind, &name) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(e) = files::receive(body, &staged, &tmp_dir, len, false, &gate) {
        let _ = std::fs::remove_file(&staged);
        return if stop() { game_started() } else { refused(e) };
    }
    let done = put_in_place(ctx, &staged, &t, replace, &stop);
    if done.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    put_back_answer(ctx, done, kind, &system, &title, &name, len)
}

/// The answer to a save or state put back, whole or in pieces.
fn put_back_answer(ctx: &Ctx, done: Result<Option<String>, Option<Refused>>, kind: Kind, system: &str, title: &str, name: &str, len: u64) -> Response {
    match done {
        Ok(backup) => {
            println!("web: put back a {} of a {system} game ({len} bytes){}", if kind == Kind::Save { "save" } else { "state" }, if backup.is_some() { ", the old one backed up" } else { "" });
            Response::json(200, &json!({ "ok": true, "name": name, "game": title, "replaced": backup.is_some(), "backup": backup }))
        }
        Err(None) => stopped_answer(ctx),
        Err(Some(e)) => refused(e),
    }
}

/// Why a put-back stopped with nothing changed: a game started, or the companion was switched off.
fn stopped_answer(ctx: &Ctx) -> Response {
    if ctx.stopped.load(Ordering::Relaxed) {
        refused(Refused::Io("The web companion was switched off. Nothing was changed.".into()))
    } else {
        refused(Refused::Io("A game started. Nothing was changed.".into()))
    }
}

/// Where a file put back arrives first: the folder for it and the hidden name in it. In the uploads folder, on the
/// same card as its place, under a name that an earlier pocket-ui's leftovers are swept by
/// (`files::sweep_leftovers`); the Brick's own saves folder may be on another card, so it gets a hidden uploads folder
/// of its own. Never a game's folder.
fn staging(ctx: &Ctx, kind: Kind, name: &str) -> Result<(PathBuf, PathBuf), Response> {
    let tmp_dir = match (kind, &ctx.roots.saves) {
        (Kind::Save, Some(own)) => own.join(files::UPLOADS_DIR),
        _ => ctx.roots.uploads.clone(),
    };
    if std::fs::symlink_metadata(&tmp_dir).is_err() {
        let _ = std::fs::create_dir(&tmp_dir);
    }
    // A folder, not a link to one.
    let tmp_dir = std::fs::symlink_metadata(&tmp_dir).ok().filter(|m| m.is_dir()).and_then(|_| tmp_dir.canonicalize().ok()).ok_or_else(|| refused(Refused::Io("Can't make the uploads folder.".into())))?;
    let staged = tmp_dir.join(format!(".{name}.{}-s{}.upload", std::process::id(), STAGED.fetch_add(1, Ordering::Relaxed)));
    Ok((tmp_dir, staged))
}

// --- putting back in pieces --------------------------------------------------------------------------------------

/// A save or state the browser sends in pieces (`/api/saves/upload/start`, then `/api/upload/part` as for a game
/// file): the game it was checked for and where it goes, fixed at the start. Its pieces go to a temporary file in the
/// uploads folder (`files::Partial`, its target the hidden `staged` name); after the last one it takes its place as a
/// whole upload does ([`put_in_place`]: the one there backed up first). Chrome on Android breaks off a large file sent
/// in one request (25.11.2026, the game uploads); a PS2 state is tens of MB.
pub struct PieceSave {
    pub target: PathBuf,
    staged: PathBuf,
    kind: Kind,
    replace: bool,
    system: String,
    title: String,
    name: String,
    /// `Shared::games_started` at the start: a game that started since, even one already over, ends it.
    games: u64,
}

impl Drop for PieceSave {
    fn drop(&mut self) {
        // Put in place, it is gone already; otherwise nothing half stays.
        let _ = std::fs::remove_file(&self.staged);
    }
}

impl PieceSave {
    /// A game started since the first piece, or the companion was switched off.
    pub fn stopped(&self, ctx: &Ctx) -> bool {
        ctx.shared.playing() || ctx.stopped.load(Ordering::Relaxed) || ctx.shared.games_started.load(Ordering::Relaxed) != self.games
    }
}

/// `POST /api/saves/upload/start?id=&name=&size=[&replace=1]`: every check of [`upload`] (the game, the kind of file
/// and its place, its size, never over a file without `replace`), the room held, the temporary file made. `stale` is
/// called with the place before its room is reserved (the caller drops an upload for it this browser left).
pub fn begin_pieces(ctx: &Ctx, req: &Request, stale: impl FnOnce(&Path)) -> Result<(PieceSave, files::Partial), Response> {
    let size = req.query("size").and_then(|s| s.parse::<u64>().ok()).ok_or_else(|| Response::error(411, "The browser did not say how large the file is."))?;
    let games = ctx.shared.games_started.load(Ordering::Relaxed);
    let (system, game, title) = game_of(ctx, req.query("id"))?;
    let (kind, t) = target(&ctx.roots, &system, &game, req.query("name").unwrap_or("")).map_err(refused)?;
    let replace = req.query("replace") == Some("1");
    match std::fs::symlink_metadata(&t) {
        Ok(m) if !m.is_file() => return Err(refused(Refused::NotAllowed("Something that is not a file has that name.".into()))),
        Ok(_) if !replace => return Err(refused(Refused::Exists(format!("{} is there already.", t.file_name().unwrap_or_default().to_string_lossy())))),
        _ => {}
    }
    stale(&t);
    let room = files::reserve(&t, size, max_of(kind)).map_err(refused)?;
    let name = t.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (tmp_dir, staged) = staging(ctx, kind, &name)?;
    let part = files::Partial::begin(&staged, &tmp_dir, size, false, false, room).map_err(refused)?;
    Ok((PieceSave { target: t, staged, kind, replace, system, title, name, games }, part))
}

/// The last piece arrived: the file takes its place as a whole upload's does, or nothing changes.
pub fn end_pieces(ctx: &Ctx, part: &mut files::Partial, save: &PieceSave) -> Response {
    if save.stopped(ctx) {
        return stopped_answer(ctx);
    }
    if let Err(e) = part.finish(|_| Ok(())) {
        return refused(e);
    }
    let stop = || save.stopped(ctx);
    let done = put_in_place(ctx, &save.staged, &save.target, save.replace, &stop);
    put_back_answer(ctx, done, save.kind, &save.system, &save.title, &save.name, part.total())
}

/// The answer to a piece that ended a put-back early: a game that started says so.
pub fn stopped_piece(ctx: &Ctx, save: &PieceSave, e: Refused) -> Response {
    if save.stopped(ctx) {
        stopped_answer(ctx)
    } else {
        refused(e)
    }
}

/// The last step: the file in `target`'s place now is copied into a saves backup, then `staged` takes its place.
/// `Err(None)`: a game started or the companion stopped. Returns the backup's folder name.
fn put_in_place(ctx: &Ctx, staged: &Path, target: &Path, replace: bool, stop: &dyn Fn() -> bool) -> Result<Option<String>, Option<Refused>> {
    if stop() {
        return Err(None);
    }
    let backup = match std::fs::symlink_metadata(target) {
        Err(_) => None,
        Ok(m) if !m.is_file() => return Err(Some(Refused::NotAllowed("Something that is not a file has that name.".into()))),
        // It appeared during the upload (another Pocket's sync).
        Ok(_) if !replace => return Err(Some(Refused::Exists(format!("{} is there already.", target.file_name().unwrap_or_default().to_string_lossy())))),
        Ok(m) => {
            // Paths in a saves backup are relative to the card's top folder (`/storage`); the Brick's own saves folder
            // may be elsewhere: then relative to `/`.
            let storage = ctx.roots.roms.parent().and_then(|p| p.canonicalize().ok()).filter(|s| target.starts_with(s)).unwrap_or_else(|| PathBuf::from("/"));
            let rel = target.strip_prefix(&storage).map_err(|_| Some(Refused::Io("The save's place can't be named.".into())))?.to_string_lossy().into_owned();
            let item = crate::backup_saves::Item { path: rel.clone(), stored: String::new(), bytes: m.len(), modified: seconds(&m) };
            let made = crate::backup_saves::create(
                &storage,
                &backup_root(&ctx.roots),
                &[item],
                crate::backup::Reason::BeforeRestore,
                crate::recents::now_secs(),
                crate::update::app_version(),
                &crate::backup_saves::Progress::default(),
            )
            .map_err(|e| Some(Refused::Io(format!("The save that is there could not be backed up, so nothing was changed. {e}"))))?;
            if made.manifest.unreadable.contains(&rel) || made.manifest.items.is_empty() {
                let _ = std::fs::remove_dir_all(&made.dir);
                return Err(Some(Refused::Io("The save that is there could not be read, so nothing was changed.".into())));
            }
            Some(made.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        }
    };
    if stop() {
        return Err(None);
    }
    let placed = if backup.is_some() { std::fs::rename(staged, target) } else { crate::fsutil::rename_no_replace(staged, target) };
    placed.map_err(|e| {
        Some(if e.kind() == io::ErrorKind::AlreadyExists {
            Refused::Exists(format!("{} is there already.", target.file_name().unwrap_or_default().to_string_lossy()))
        } else {
            Refused::Io(format!("Can't put the file in place: {e}"))
        })
    })?;
    if let Some(dir) = target.parent().and_then(|d| std::fs::File::open(d).ok()) {
        let _ = dir.sync_all();
    }
    Ok(backup)
}

//! Photo album (`<data>/album/…` JPEGs + `album.tsv`) and screenshots (game screenshots + shelf pictures)
//! for the web companion: list, view, download individually, delete individually (through the App), and
//! stream everything (or one game) as an uncompressed ZIP.
//!
//! Security rules:
//! - Every lookup goes through a library game id (never a path from the request) and matches an exact
//!   entry of `album::list` or `screenshot::find` (`valid_at`).
//! - Symlinks are refused everywhere; magic bytes (`\xFF\xD8\xFF` for JPEG, `\x89PNG` for PNG) and file
//!   size limits are checked before reading.
//! - Deletions go through `Shared::edit` (`Edit::DeleteAlbumPhoto`, `Edit::DeleteScreenshot`) so only
//!   the App writes/removes files, never while a game is running.
//! - No filesystem paths and no secrets ever enter JSON answers or ZIP archives.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde_json::{json, Value};

use crate::album;
use crate::screenshot;
use crate::share_shelf;

use super::api::{self, Ctx};
use super::http::{Request, Response};
use super::zip;
use super::Edit;

/// An album photo is at most 320 px wide JPEG (~15–40 KB); this cap stops a huge bogus file from being read.
const ALBUM_PHOTO_MAX: u64 = 4 << 20;
/// A screenshot PNG is the screen at most (1240 x 1080).
const SHOT_MAX: u64 = 16 << 20;
/// An album.tsv index is a few KB (30 entries).
const TSV_MAX: u64 = 1 << 20;
/// Maximum items per category in `GET /api/album`.
const LIST_MAX: usize = 500;
/// Maximum shelf pictures listed.
const SHELF_MAX: usize = 60;
/// Hard ceiling on entries packed into one ZIP download.
const ZIP_MAX_FILES: usize = 2000;
/// Hard ceiling on total bytes packed into one ZIP download (512 MB; streamed without RAM buffering).
const ZIP_MAX_BYTES: u64 = 512 << 20;

/// One game's album photos for `GET /api/game`, newest first; no paths travel.
pub fn game_album(ctx: &Ctx, system: &str, rom: &Path) -> Vec<Value> {
    let dir = album::dir(&ctx.shared.data_dir, system, rom);
    let mut photos = album::list(&dir);
    photos.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.file.file_name().cmp(&a.file.file_name())));
    photos
        .into_iter()
        .filter_map(|p| {
            let file = p.file.file_name()?.to_str()?.to_string();
            let bytes = std::fs::symlink_metadata(&p.file).ok().filter(|m| m.is_file())?.len();
            Some(json!({
                "file": file,
                "at": p.at,
                "note": p.note_line(),
                "width": p.size.0,
                "height": p.size.1,
                "bytes": bytes,
            }))
        })
        .collect()
}

/// `GET /api/album`: all album photos, game screenshots, and shelf pictures across the library, newest first.
pub fn list(ctx: &Ctx) -> Response {
    let lib = ctx.library();
    let album_root = ctx.shared.data_dir.join("album");
    let has_album_root = album_root.is_dir();

    let mut album_items: Vec<(i64, String, Value)> = Vec::new();
    let mut shot_items: Vec<(String, u32, Value)> = Vec::new();

    for sys in &lib.systems {
        let sys_stripe = api::stripe(&sys.name);
        let has_sys_shots = ctx.roots.screenshots.join(screenshot::clean(&sys.name)).is_dir();
        let has_sys_album = has_album_root && album_root.join(clean_album_part(&sys.name)).is_dir();
        if !has_sys_shots && !has_sys_album {
            continue;
        }
        for gm in &sys.games {
            let gid = api::game_id(&gm.path);
            if has_sys_album {
                let dir = album::dir(&ctx.shared.data_dir, &sys.name, &gm.path);
                for p in album::list(&dir) {
                    let Some(file) = p.file.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
                    let Ok(meta) = std::fs::symlink_metadata(&p.file) else { continue };
                    if !meta.is_file() {
                        continue;
                    }
                    album_items.push((
                        p.at,
                        file.clone(),
                        json!({
                            "id": gid,
                            "game": gm.name,
                            "system": sys.name,
                            "systemName": sys.fullname,
                            "stripe": sys_stripe,
                            "file": file,
                            "at": p.at,
                            "note": p.note_line(),
                            "width": p.size.0,
                            "height": p.size.1,
                            "bytes": meta.len(),
                        }),
                    ));
                }
            }
            if has_sys_shots {
                let stem = gm.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                for e in screenshot::list(&ctx.roots.screenshots, &sys.name, stem) {
                    let Some(path) = screenshot::find(&ctx.roots.screenshots, &sys.name, stem, &e.at) else { continue };
                    let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
                    if !meta.is_file() {
                        continue;
                    }
                    let (stamp, seq) = shot_sort_key(&e.at);
                    shot_items.push((
                        stamp,
                        seq,
                        json!({
                            "id": gid,
                            "game": gm.name,
                            "system": sys.name,
                            "systemName": sys.fullname,
                            "stripe": sys_stripe,
                            "at": e.at,
                            "bytes": meta.len(),
                        }),
                    ));
                }
            }
        }
    }

    album_items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    shot_items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let shelf_raw = screenshot::list(&ctx.roots.screenshots, share_shelf::FOLDER, share_shelf::NAME);
    let shelf_total = shelf_raw.len();
    let shelf: Vec<Value> = shelf_raw
        .into_iter()
        .take(SHELF_MAX)
        .filter_map(|e| {
            let path = screenshot::find(&ctx.roots.screenshots, share_shelf::FOLDER, share_shelf::NAME, &e.at)?;
            let meta = std::fs::symlink_metadata(&path).ok().filter(|m| m.is_file())?;
            Some(json!({ "shelf": true, "at": e.at, "bytes": meta.len() }))
        })
        .collect();

    let album_total = album_items.len();
    let shots_total = shot_items.len();
    let album_json: Vec<Value> = album_items.into_iter().take(LIST_MAX).map(|(_, _, v)| v).collect();
    let shots_json: Vec<Value> = shot_items.into_iter().take(LIST_MAX).map(|(_, _, v)| v).collect();

    Response::json(
        200,
        &json!({
            "albumOn": album::on(&ctx.shared.data_dir),
            "albumTotal": album_total,
            "shotsTotal": shots_total,
            "shelfTotal": shelf_total,
            "album": album_json,
            "shots": shots_json,
            "shelf": shelf,
        }),
    )
}

/// `GET /api/album/photo?id=<game_id>&file=<file>[&download=1]`: serves one verified album JPEG.
pub fn photo(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let none = || Response::error(404, "No photo.");
    let file = req.query("file").filter(|f| valid_album_file(f)).ok_or_else(none)?;
    let lib = ctx.library();
    let (s, g) = req.query("id").and_then(|id| lib.game(id)).ok_or_else(none)?;
    let sys = &lib.systems[s];
    let gm = &sys.games[g];
    let dir = album::dir(&ctx.shared.data_dir, &sys.name, &gm.path);
    let entry = album::list(&dir)
        .into_iter()
        .find(|p| p.file.file_name().and_then(|n| n.to_str()) == Some(file))
        .ok_or_else(none)?;

    // Refuse symlinks and oversized files before reading a single byte.
    let small = std::fs::symlink_metadata(&entry.file).is_ok_and(|m| m.is_file() && m.len() <= ALBUM_PHOTO_MAX);
    let mut bytes = Vec::new();
    if !small || std::fs::File::open(&entry.file).and_then(|f| f.take(ALBUM_PHOTO_MAX).read_to_end(&mut bytes)).is_err() || !is_jpeg(&bytes) {
        return Err(none());
    }
    let mut resp = Response::new(200, "image/jpeg", bytes).with("Cache-Control", "private, max-age=86400");
    if req.query("download") == Some("1") {
        let dl = format!("{} - {file}", screenshot::clean(&gm.name));
        resp = resp.with("Content-Disposition", &attachment_header(&dl));
    }
    Ok(resp)
}

/// `POST /api/album/delete` `{"id": "...", "file": "..."}`: deletes one album photo through the App.
pub fn delete_photo(ctx: &Ctx, req: &Request, body: &mut impl Read) -> Result<Response, Response> {
    let v = api::json_body(req, body)?;
    let id = v.get("id").and_then(Value::as_str).unwrap_or("");
    let file = v.get("file").and_then(Value::as_str).filter(|f| valid_album_file(f)).ok_or_else(|| Response::error(400, "Which photo?"))?;
    let lib = ctx.library();
    let (s, g) = lib.game(id).ok_or_else(|| Response::error(404, "That game is not on the Pocket any more."))?;
    let sys = &lib.systems[s];
    let gm = &sys.games[g];
    let dir = album::dir(&ctx.shared.data_dir, &sys.name, &gm.path);
    let exists = album::list(&dir)
        .into_iter()
        .any(|p| p.file.file_name().and_then(|n| n.to_str()) == Some(file) && std::fs::symlink_metadata(&p.file).is_ok_and(|m| m.is_file()));
    if !exists {
        return Err(Response::error(404, "That photo is already gone."));
    }
    match ctx.shared.edit(Edit::DeleteAlbumPhoto { system: sys.name.clone(), rom: gm.path.clone(), file: file.to_string() }) {
        Ok(()) => Ok(Response::json(200, &json!({ "ok": true }))),
        Err(msg) => Err(Response::error(409, &msg)),
    }
}

/// `POST /api/shot/delete` `{"id": "...", "at": "..."}` or `{"shelf": true, "at": "..."}`: deletes one screenshot through the App.
pub fn delete_shot(ctx: &Ctx, req: &Request, body: &mut impl Read) -> Result<Response, Response> {
    let v = api::json_body(req, body)?;
    let at = v.get("at").and_then(Value::as_str).filter(|a| screenshot::valid_at(a)).ok_or_else(|| Response::error(400, "Which screenshot?"))?;
    let (system, game) = if v.get("shelf").and_then(Value::as_bool) == Some(true) {
        (share_shelf::FOLDER.to_string(), share_shelf::NAME.to_string())
    } else {
        let id = v.get("id").and_then(Value::as_str).unwrap_or("");
        let lib = ctx.library();
        let (s, g) = lib.game(id).ok_or_else(|| Response::error(404, "That game is not on the Pocket any more."))?;
        let gm = &lib.systems[s].games[g];
        let stem = gm.path.file_stem().and_then(|st| st.to_str()).unwrap_or_default().to_string();
        (lib.systems[s].name.clone(), stem)
    };
    if screenshot::find(&ctx.roots.screenshots, &system, &game, at).is_none() {
        return Err(Response::error(404, "That screenshot is already gone."));
    }
    match ctx.shared.edit(Edit::DeleteScreenshot { system, game, at: at.to_string() }) {
        Ok(()) => Ok(Response::json(200, &json!({ "ok": true }))),
        Err(msg) => Err(Response::error(409, &msg)),
    }
}

/// `GET /api/media/zip[?id=<game_id>&scope=all|album|shots]`: streams a ZIP of album photos and/or screenshots.
pub fn zip(ctx: &Ctx, req: &Request) -> Result<Response, Response> {
    let scope = match req.query("scope") {
        None | Some("") | Some("all") => Scope::All,
        Some("album") => Scope::Album,
        Some("shots") => Scope::Shots,
        Some(_) => return Err(Response::error(400, "Unknown download scope.")),
    };
    let lib = ctx.library();
    let only_game = match req.query("id").filter(|id| !id.is_empty()) {
        Some(id) => Some(lib.game(id).ok_or_else(|| Response::error(404, "That game is not on the Pocket any more."))?),
        None => None,
    };

    let mut entries: Vec<zip::Entry> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut total_bytes: u64 = 0;

    let mut push_file = |raw_name: String, path: PathBuf, max_file: u64, magic: &[u8]| -> Result<bool, Response> {
        let Ok(meta) = std::fs::symlink_metadata(&path) else { return Ok(false) };
        if !meta.is_file() || meta.len() > max_file {
            return Ok(false);
        }
        if !magic.is_empty() && !starts_with_magic(&path, magic) {
            return Ok(false);
        }
        if entries.len() >= ZIP_MAX_FILES || total_bytes.saturating_add(meta.len()) > ZIP_MAX_BYTES {
            return Err(Response::error(413, "Too many pictures for one ZIP. Download one game at a time instead."));
        }
        total_bytes += meta.len();
        let name = unique_entry_name(raw_name, &mut seen_names);
        entries.push(zip::Entry { name, path, modified: file_mtime(&meta) });
        Ok(true)
    };

    let systems_iter: Vec<(usize, Option<usize>)> = match only_game {
        Some((s, g)) => vec![(s, Some(g))],
        None => (0..lib.systems.len()).map(|s| (s, None)).collect(),
    };

    for (s, single_g) in systems_iter {
        let sys = &lib.systems[s];
        let sys_folder = screenshot::clean(&sys.name);
        let games_range: Vec<usize> = match single_g {
            Some(g) => vec![g],
            None => (0..sys.games.len()).collect(),
        };
        for g in games_range {
            let gm = &sys.games[g];
            let game_folder = screenshot::clean(&gm.name);
            let stem = gm.path.file_stem().and_then(|st| st.to_str()).unwrap_or_default();

            if matches!(scope, Scope::All | Scope::Album) {
                let dir = album::dir(&ctx.shared.data_dir, &sys.name, &gm.path);
                let photos = album::list(&dir);
                if !photos.is_empty() {
                    let mut added_any = false;
                    for p in &photos {
                        let Some(fname) = p.file.file_name().and_then(|n| n.to_str()) else { continue };
                        if !valid_album_file(fname) {
                            continue;
                        }
                        added_any |= push_file(format!("album/{sys_folder}/{game_folder}/{fname}"), p.file.clone(), ALBUM_PHOTO_MAX, b"\xFF\xD8\xFF")?;
                    }
                    let tsv = dir.join("album.tsv");
                    if added_any && std::fs::symlink_metadata(&tsv).is_ok_and(|m| m.is_file() && m.len() <= TSV_MAX) {
                        let _ = push_file(format!("album/{sys_folder}/{game_folder}/album.tsv"), tsv, TSV_MAX, b"")?;
                    }
                }
            }

            if matches!(scope, Scope::All | Scope::Shots) {
                for e in screenshot::list(&ctx.roots.screenshots, &sys.name, stem) {
                    let Some(path) = screenshot::find(&ctx.roots.screenshots, &sys.name, stem, &e.at) else { continue };
                    let fname = format!("{} {}.png", game_folder, e.at);
                    let _ = push_file(format!("screenshots/{sys_folder}/{fname}"), path, SHOT_MAX, b"\x89PNG")?;
                }
            }
        }
    }

    if only_game.is_none() && matches!(scope, Scope::All | Scope::Shots) {
        for e in screenshot::list(&ctx.roots.screenshots, share_shelf::FOLDER, share_shelf::NAME) {
            let Some(path) = screenshot::find(&ctx.roots.screenshots, share_shelf::FOLDER, share_shelf::NAME, &e.at) else { continue };
            let fname = format!("{} {}.png", share_shelf::NAME, e.at);
            let _ = push_file(format!("shelf/{fname}"), path, SHOT_MAX, b"\x89PNG")?;
        }
    }

    if entries.is_empty() {
        return Err(Response::error(404, "There are no photos or screenshots to download yet."));
    }

    let dl_name = match only_game {
        Some((s, g)) => format!("{}-photos.zip", screenshot::clean(&lib.systems[s].games[g].name)),
        None => match scope {
            Scope::All => "pocket-ui-photos.zip".into(),
            Scope::Album => "pocket-ui-album.zip".into(),
            Scope::Shots => "pocket-ui-screenshots.zip".into(),
        },
    };
    let shared = ctx.shared.clone();
    let stopped = ctx.stopped.clone();
    Ok(Response::stream(200, "application/zip", move |out| {
        zip::write(out, &entries, &|| shared.playing() || stopped.load(Ordering::Relaxed))
    })
    .with("Content-Disposition", &attachment_header(&dl_name)))
}

#[derive(Clone, Copy)]
enum Scope {
    All,
    Album,
    Shots,
}

/// Executes an album photo deletion on the App thread (shared by `app_web.rs` and `examples/web_dev.rs`).
/// Returns the deleted file path on success so the App can drop it from an open card cache.
pub fn apply_delete_photo(data_dir: &Path, system: &str, rom: &Path, file: &str) -> Result<(PathBuf, PathBuf), String> {
    if !valid_album_file(file) {
        return Err("Invalid photo name.".into());
    }
    let dir = album::dir(data_dir, system, rom);
    let photo = album::list(&dir)
        .into_iter()
        .find(|p| p.file.file_name().and_then(|n| n.to_str()) == Some(file))
        .ok_or_else(|| "That photo is already gone.".to_string())?;
    if !std::fs::symlink_metadata(&photo.file).is_ok_and(|m| m.is_file()) {
        return Err("That photo cannot be deleted.".into());
    }
    album::delete(&dir, &photo.file).map_err(|e| format!("Couldn't delete the photo: {e}"))?;
    Ok((dir, photo.file))
}

/// Executes a screenshot deletion on the App thread (shared by `app_web.rs` and `examples/web_dev.rs`).
pub fn apply_delete_shot(shots_root: &Path, system: &str, game: &str, at: &str) -> Result<(), String> {
    let path = screenshot::find(shots_root, system, game, at).ok_or_else(|| "That screenshot is already gone.".to_string())?;
    // `screenshot::find` already verifies `valid_at(at)` and `symlink_metadata().is_file()`.
    std::fs::remove_file(&path).map_err(|e| format!("Couldn't delete the screenshot: {e}"))
}

/// Strict ASCII-safe `Content-Disposition: attachment; filename="..."` value (no quotes, control chars, or paths).
pub fn attachment_header(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("download");
    let mut clean: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' })
        .collect();
    while clean.starts_with(['.', ' ']) {
        clean.remove(0);
    }
    while clean.ends_with(' ') {
        clean.pop();
    }
    if clean.len() > 140 {
        // Keep the extension if there is one.
        let ext = clean.rsplit_once('.').filter(|(_, e)| !e.is_empty() && e.len() <= 8).map(|(_, e)| format!(".{e}")).unwrap_or_default();
        clean.truncate(140usize.saturating_sub(ext.len()));
        clean.push_str(&ext);
    }
    if clean.is_empty() {
        clean = "download".into();
    }
    format!("attachment; filename=\"{clean}\"")
}

/// An album photo filename as `album::add` writes it: plain name ending in `.jpg`, no separators or leading dot.
pub fn valid_album_file(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0', '\t', '\n', '\r'])
        && name.ends_with(".jpg")
}

fn clean_album_part(s: &str) -> String {
    let c: String = s.chars().map(|c| if matches!(c, '/' | '\\' | '\0' | '\t' | '\n') { '_' } else { c }).collect();
    if c.is_empty() || c == "." || c == ".." {
        "_".to_string()
    } else {
        c
    }
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\xFF\xD8\xFF")
}

fn starts_with_magic(path: &Path, magic: &[u8]) -> bool {
    let mut head = vec![0u8; magic.len()];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && head == magic
}

fn shot_sort_key(at: &str) -> (String, u32) {
    let stamp = at.get(..19).unwrap_or(at).to_string();
    let seq = at.get(19..).unwrap_or("").trim_matches([' ', '(', ')']).parse::<u32>().unwrap_or(1);
    (stamp, seq)
}

fn file_mtime(meta: &std::fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

fn unique_entry_name(name: String, seen: &mut HashSet<String>) -> String {
    if seen.insert(name.clone()) {
        return name;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !e.contains('/') => (s, format!(".{e}")),
        _ => (name.as_str(), String::new()),
    };
    for n in 2..=9999 {
        let candidate = format!("{stem} ({n}){ext}");
        if seen.insert(candidate.clone()) {
            return candidate;
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_header_strips_quotes_paths_and_control_chars() {
        assert_eq!(attachment_header("Golden Sun - 123.jpg"), "attachment; filename=\"Golden Sun - 123.jpg\"");
        assert_eq!(attachment_header("../../etc/passwd\"\r\nX-Evil: 1"), "attachment; filename=\"passwd___X-Evil_ 1\"");
        assert_eq!(attachment_header("..."), "attachment; filename=\"download\"");
        assert_eq!(attachment_header("Pokémon.png"), "attachment; filename=\"Pok_mon.png\"");
    }

    #[test]
    fn valid_album_file_accepts_only_plain_jpegs() {
        assert!(valid_album_file("1761234567.jpg"));
        assert!(valid_album_file("1761234567-2.jpg"));
        for bad in ["", ".hidden.jpg", "../1.jpg", "a/b.jpg", "a\\b.jpg", "album.tsv", "1.png", "1.jpg\0"] {
            assert!(!valid_album_file(bad), "{bad:?}");
        }
    }
}

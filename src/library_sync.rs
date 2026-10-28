//! Settings → Connections → Sync → **Library data**: the player's curation (favourites, play status, notes, ratings,
//! stickers, hidden games, smart shelves and play history) the same on every Pocket they own, through one more
//! Syncthing folder. The layout and the rules are in docs/SYNC-CURATION.md; in short:
//!
//! * **One folder per device.** `<data>/library-sync/<device>/` holds `library.json` and `history.json`, written only
//!   by that device (`<device>` is the start of its Syncthing device ID, [`device_folder`]). Syncthing replaces whole
//!   files, so two devices never write the same file and nothing one of them wrote is lost to a whole-file copy.
//! * **Every value carries its time.** A device's `library.json` is its whole knowledge: per game (keyed by system
//!   folder and file name, [`game_key`], never by the absolute path) and per field the value, when it was set and by
//!   whom ([`Reg`]). A removal is a value too (`null`, a tombstone), so "unstarred" beats an older "starred".
//! * **Merging on read.** A pass ([`pass`]) takes this device's files as they are now, stamps what changed since its
//!   own `library.json` with the time the file was written, then takes, field by field, the newest value of all
//!   devices (last writer wins; the device folder breaks a tie). What differs from this device's files comes back as
//!   [`Change`]s, which the App applies on the UI thread like any other edit ([`apply`]); only after that is the own
//!   `library.json` written ([`store`]), so a pass that never got applied is simply done again.
//! * **Play history is a union.** `history.json` holds every session and status change the device knows (its own play
//!   log's and the other devices'), de-duplicated by game and start. The other devices' sessions are shown with the
//!   own log (`play_log::set_extra`), never written into this device's play log.
//! * **Nothing foreign can break the local data.** A file that is missing, cut off, damaged or from a newer version is
//!   skipped (and said in the log); fields this version doesn't know are carried along untouched.
//!
//! Every function here reads or writes files except the pure merge helpers: run them on a thread.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::curator::prefs::{Prefs as CuratorPrefs, SmartShelf};
use crate::favorites::Favorites;
use crate::library_meta::{GameMeta, LibraryMeta, Status, Sticker, RATING_MAX};
use crate::play_log::{self, Log};

/// The Syncthing folder below the data directory (Settings → Sync's "Pocket library data").
pub const FOLDER: &str = "library-sync";
/// A device's curation, field by field with times.
pub const STATE_FILE: &str = "library.json";
/// A device's play history: every session it knows.
pub const HISTORY_FILE: &str = "history.json";
/// The version written now.
pub const VERSION: u32 = 1;
/// A file from another device larger than this is not read (it is no file of ours).
pub const MAX_FILE: u64 = 64 << 20;

// ---- keys -------------------------------------------------------------------------------------------------------

/// The key of a game on every device: its system folder and file name, `gba/Pokemon Emerald (USA).gba`. Below
/// `/storage/roms` the system is the first folder (a game in a subfolder keeps its system); elsewhere the folder the
/// file is in. None for a path without a file name.
pub fn game_key(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let system = match path.strip_prefix(crate::sync::ROMS_DIR) {
        Ok(rel) => {
            let mut parts = rel.components();
            let first = parts.next()?.as_os_str().to_str()?;
            // A file right in /storage/roms has no system folder.
            parts.next()?;
            first
        }
        Err(_) => path.parent()?.file_name()?.to_str()?,
    };
    Some(format!("{system}/{name}"))
}

/// The folder of a device below [`FOLDER`]: the first two groups of its Syncthing device ID (`P7T4NX2-24CSKCT`), unique
/// enough among one person's devices and readable in a file manager. None for an ID that isn't one.
pub fn device_folder(device_id: &str) -> Option<String> {
    let groups: Vec<&str> = device_id.split('-').take(2).collect();
    let ok = groups.len() == 2 && groups.iter().all(|g| g.len() == 7 && g.bytes().all(|b| b.is_ascii_alphanumeric()));
    ok.then(|| groups.join("-"))
}

// ---- the files ----------------------------------------------------------------------------------------------------

/// A value with when it was set (Unix milliseconds) and by which device. `null` is "removed".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reg {
    #[serde(default)]
    pub v: Value,
    pub t: u64,
    pub d: String,
}

impl Reg {
    /// Whether `self` wins over `other`: newer, then the larger device folder, then the larger value (two devices
    /// never write the same field at the same millisecond with the same folder, but the order must be total).
    fn beats(&self, other: &Reg) -> bool {
        (self.t, &self.d, self.v.to_string()) > (other.t, &other.d, other.v.to_string())
    }
}

/// What a device knew about a game, a sticker or a shelf, unstamped: for showing and for finding the game. The
/// game's path is the one on the device that wrote it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Info {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub system: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
}

impl Info {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.system.is_empty() && self.path.is_empty()
    }

    /// Fills what `self` lacks from `other`.
    fn fill(&mut self, other: &Info) {
        for (mine, theirs) in [(&mut self.name, &other.name), (&mut self.system, &other.system), (&mut self.path, &other.path)] {
            if mine.is_empty() {
                mine.clone_from(theirs);
            }
        }
    }
}

/// A game, sticker or shelf: its info and its fields.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rec {
    #[serde(default, skip_serializing_if = "Info::is_empty")]
    pub info: Info,
    #[serde(default)]
    pub f: BTreeMap<String, Reg>,
}

/// A device's `library.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    /// The device folder that wrote it.
    #[serde(default)]
    pub device: String,
    /// Games by [`game_key`]: `favorite`, `status`, `note`, `rating`, `hidden`, `sticker:<name>`.
    #[serde(default)]
    pub games: BTreeMap<String, Rec>,
    /// The player's stickers by name in lower case: `name`, `pinned`.
    #[serde(default)]
    pub stickers: BTreeMap<String, Rec>,
    /// Smart shelves by name in lower case: `lens`, `pinned`, `order`.
    #[serde(default)]
    pub shelves: BTreeMap<String, Rec>,
}

/// One session in `history.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionRec {
    /// The game's [`game_key`].
    pub g: String,
    /// Unix seconds of the start.
    pub s: u64,
    #[serde(default)]
    pub tz: i32,
    #[serde(default)]
    pub secs: u64,
    /// Its end was recorded.
    #[serde(default)]
    pub e: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub em: String,
    /// The stretches between sleeps: (unix start, seconds).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub p: Vec<(u64, u64)>,
}

/// One status change in `history.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatusRec {
    pub g: String,
    pub at: u64,
    #[serde(default)]
    pub tz: i32,
    pub st: Status,
}

/// A device's `history.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    pub version: u32,
    #[serde(default)]
    pub device: String,
    /// The games of the sessions: name and system (for a game not on this device) and the writer's path.
    #[serde(default)]
    pub games: BTreeMap<String, Info>,
    #[serde(default)]
    pub sessions: Vec<SessionRec>,
    #[serde(default)]
    pub statuses: Vec<StatusRec>,
}

// ---- this device's data as fields ---------------------------------------------------------------------------------

/// Fields that are set (not default); a field that isn't here is `null`.
pub type Fields = BTreeMap<String, Value>;

/// Where a record lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    Game,
    Sticker,
    Shelf,
}

/// A game as this device has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub info: Info,
    pub fields: Fields,
}

/// This device's curation, as fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Local {
    pub games: BTreeMap<String, Entry>,
    pub stickers: BTreeMap<String, Fields>,
    pub shelves: BTreeMap<String, Fields>,
    /// The shelves' names as the player wrote them, by key.
    pub shelf_names: BTreeMap<String, String>,
    /// Keys two of this device's games share (the same file name in two folders of one system): left out of sync.
    pub ambiguous: BTreeSet<String>,
}

const GAME_FIELDS: [&str; 5] = ["favorite", "status", "note", "rating", "hidden"];
const STICKER_PREFIX: &str = "sticker:";

/// Whether this version knows a field: the others (a newer pocket-ui's) are carried along, never changed here.
fn known(space: Space, field: &str) -> bool {
    match space {
        Space::Game => GAME_FIELDS.contains(&field) || field.strip_prefix(STICKER_PREFIX).is_some_and(|s| !s.is_empty()),
        Space::Sticker => matches!(field, "name" | "pinned"),
        Space::Shelf => matches!(field, "lens" | "pinned" | "order"),
    }
}

/// A sticker's or shelf's key: its name in lower case (names are unique regardless of case).
pub fn name_key(name: &str) -> String {
    name.to_lowercase()
}

fn status_value(s: Status) -> Value {
    serde_json::to_value(s).unwrap_or(Value::Null)
}

/// The fields of a game: what the card and the lists show of the player's choices.
pub fn game_fields(g: &GameMeta, favorite: bool, stickers: &[Sticker]) -> Fields {
    let mut f = Fields::new();
    if favorite {
        f.insert("favorite".into(), Value::Bool(true));
    }
    if g.status_set || g.status != Status::NotStarted {
        f.insert("status".into(), status_value(g.status));
    }
    if !g.note.is_empty() {
        f.insert("note".into(), Value::String(g.note.clone()));
    }
    if g.rating > 0 {
        f.insert("rating".into(), Value::from(g.rating));
    }
    if g.hidden {
        f.insert("hidden".into(), Value::Bool(true));
    }
    for id in &g.stickers {
        if let Some(s) = stickers.iter().find(|s| s.id == *id) {
            f.insert(format!("{STICKER_PREFIX}{}", name_key(&s.name)), Value::Bool(true));
        }
    }
    f
}

fn sticker_fields(s: &Sticker) -> Fields {
    let mut f = Fields::new();
    f.insert("name".into(), Value::String(s.name.clone()));
    if s.pinned {
        f.insert("pinned".into(), Value::Bool(true));
    }
    f
}

fn shelf_fields(s: &SmartShelf) -> Fields {
    let mut f = Fields::new();
    f.insert("lens".into(), serde_json::to_value(&s.lens).unwrap_or(Value::Null));
    f.insert("pinned".into(), Value::Bool(s.pinned));
    if !s.order.is_name() {
        f.insert("order".into(), serde_json::to_value(s.order).unwrap_or(Value::Null));
    }
    f
}

/// A file name without its extension, for a game's name when nothing better is known.
fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Local {
    /// This device's curation from the App's own types (read from the files on a thread, or the App's own copies).
    pub fn of(meta: &LibraryMeta, favs: &Favorites, prefs: &CuratorPrefs) -> Local {
        let stickers = meta.stickers();
        let mut local = Local::default();
        let mut paths: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
        let games = meta.games();
        for p in games.keys().map(PathBuf::from).chain(favs.list.iter().map(|f| f.path.clone())) {
            if let Some(k) = game_key(&p) {
                paths.entry(k).or_default().insert(p);
            }
        }
        for (key, set) in paths {
            if set.len() > 1 {
                local.ambiguous.insert(key);
                continue;
            }
            let Some(path) = set.into_iter().next() else { continue };
            let g = games.get(path.to_string_lossy().as_ref()).cloned().unwrap_or_default();
            let fav = favs.list.iter().find(|f| f.path == path);
            let fields = game_fields(&g, fav.is_some(), stickers);
            if fields.is_empty() {
                continue;
            }
            let system = key.split('/').next().unwrap_or_default().to_string();
            let info = Info {
                name: fav.map_or_else(|| stem(&path), |f| f.name.clone()),
                system: fav.map_or(system, |f| f.system.clone()),
                path: path.to_string_lossy().into_owned(),
            };
            local.games.insert(key, Entry { path, info, fields });
        }
        for s in stickers {
            local.stickers.insert(name_key(&s.name), sticker_fields(s));
        }
        for s in &prefs.shelves {
            local.shelves.insert(name_key(&s.name), shelf_fields(s));
            local.shelf_names.insert(name_key(&s.name), s.name.clone());
        }
        local
    }

    fn space(&self, space: Space) -> BTreeMap<&str, &Fields> {
        match space {
            Space::Game => self.games.iter().map(|(k, e)| (k.as_str(), &e.fields)).collect(),
            Space::Sticker => self.stickers.iter().map(|(k, f)| (k.as_str(), f)).collect(),
            Space::Shelf => self.shelves.iter().map(|(k, f)| (k.as_str(), f)).collect(),
        }
    }
}

/// A value as this device would hold it (None: this version can't hold it, and it is left alone): a rating above five
/// is five, a status or lens of a newer version is not taken, a shelf's lens drops what this version doesn't know.
fn normalize(space: Space, field: &str, v: &Value) -> Option<Value> {
    if v.is_null() {
        return Some(Value::Null);
    }
    match (space, field) {
        (Space::Game, "favorite" | "hidden") | (Space::Sticker, "pinned") => v.as_bool().map(|b| if b { Value::Bool(true) } else { Value::Null }),
        (Space::Game, f) if f.starts_with(STICKER_PREFIX) => v.as_bool().map(|b| if b { Value::Bool(true) } else { Value::Null }),
        (Space::Game, "status") => serde_json::from_value::<Status>(v.clone()).ok().map(status_value),
        (Space::Game, "note") => v.as_str().map(|s| {
            if s.is_empty() {
                Value::Null
            } else {
                Value::String(s.chars().take(crate::library_meta::NOTE_MAX).collect())
            }
        }),
        (Space::Game, "rating") => v.as_u64().map(|r| if r == 0 { Value::Null } else { Value::from(r.min(RATING_MAX as u64)) }),
        (Space::Sticker, "name") => v.as_str().filter(|s| !s.trim().is_empty()).map(|s| Value::String(s.to_string())),
        (Space::Shelf, "lens") => serde_json::from_value::<crate::curator::lens::Lens>(v.clone())
            .ok()
            .and_then(|l| serde_json::to_value(l).ok()),
        (Space::Shelf, "pinned") => v.as_bool().map(Value::Bool),
        (Space::Shelf, "order") => serde_json::from_value::<crate::curator::sort::Order>(v.clone()).ok().map(|o| {
            if o.is_name() {
                Value::Null
            } else {
                serde_json::to_value(o).unwrap_or(Value::Null)
            }
        }),
        _ => None,
    }
}

// ---- merging ------------------------------------------------------------------------------------------------------

fn space_of(state: &State, space: Space) -> &BTreeMap<String, Rec> {
    match space {
        Space::Game => &state.games,
        Space::Sticker => &state.stickers,
        Space::Shelf => &state.shelves,
    }
}

fn space_mut(state: &mut State, space: Space) -> &mut BTreeMap<String, Rec> {
    match space {
        Space::Game => &mut state.games,
        Space::Sticker => &mut state.stickers,
        Space::Shelf => &mut state.shelves,
    }
}

const SPACES: [Space; 3] = [Space::Game, Space::Sticker, Space::Shelf];

/// When this device's files were last written (Unix milliseconds): the time a change found in them is given.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Times {
    /// `library.json`: status, note, rating, hidden, stickers.
    pub library: u64,
    /// `favorites.json`.
    pub favorites: u64,
    /// `curator.json`: the smart shelves.
    pub curator: u64,
}

impl Times {
    /// The modification times of the three files in `data_dir` (0 for a missing one), none later than `now`.
    pub fn of(data_dir: &Path, now: u64) -> Times {
        let t = |name: &str| -> u64 {
            std::fs::metadata(data_dir.join(name))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64)
                .min(now)
        };
        Times {
            library: t("library.json"),
            favorites: t("favorites.json"),
            curator: t(crate::curator::prefs::FILE),
        }
    }

    fn of_field(&self, space: Space, field: &str) -> u64 {
        match (space, field) {
            (Space::Game, "favorite") => self.favorites,
            (Space::Shelf, _) => self.curator,
            _ => self.library,
        }
    }
}

/// `own` with what changed on this device since it was written: every field whose value in `local` differs from the
/// one in `own` gets that value, stamped with the time its file was written and with `me`. A game whose every field
/// was cleared gets tombstones. Ambiguous games and fields this version doesn't know or can't hold are left as they are.
pub fn stamp(local: &Local, own: &State, times: &Times, me: &str) -> State {
    let mut out = own.clone();
    out.version = VERSION;
    out.device = me.to_string();
    for space in SPACES {
        let local_space = local.space(space);
        let keys: BTreeSet<String> = local_space.keys().map(|k| k.to_string()).chain(space_of(own, space).keys().cloned()).collect();
        for key in keys {
            if space == Space::Game && local.ambiguous.contains(&key) {
                continue;
            }
            let empty = Fields::new();
            let lf = local_space.get(key.as_str()).copied().unwrap_or(&empty);
            let rec = space_mut(&mut out, space).entry(key.clone()).or_default();
            let fields: BTreeSet<String> = lf.keys().chain(rec.f.keys()).filter(|f| known(space, f)).cloned().collect();
            for field in fields {
                let lv = lf.get(&field).cloned().unwrap_or(Value::Null);
                let held = match rec.f.get(&field) {
                    // A value this version can't hold (a newer status) was never applied: it is no change of ours.
                    Some(r) => match normalize(space, &field, &r.v) {
                        Some(v) => v,
                        None => continue,
                    },
                    None => Value::Null,
                };
                if lv != held {
                    let t = times.of_field(space, &field);
                    rec.f.insert(field, Reg { v: lv, t, d: me.to_string() });
                }
            }
            // This device's own info, else what was known.
            match (space, local.games.get(&key)) {
                (Space::Game, Some(e)) => {
                    let mut info = e.info.clone();
                    info.fill(&rec.info);
                    rec.info = info;
                }
                (Space::Sticker, _) => {
                    if let Some(name) = lf.get("name").and_then(Value::as_str) {
                        rec.info.name = name.to_string();
                    }
                }
                (Space::Shelf, _) => {
                    if let Some(name) = local.shelf_names.get(&key) {
                        rec.info.name.clone_from(name);
                    }
                }
                _ => {}
            }
            if rec.f.is_empty() && rec.info.is_empty() {
                space_mut(&mut out, space).remove(&key);
            }
        }
    }
    out
}

/// Takes into `into`, field by field, every value of `other` that is newer.
pub fn merge(into: &mut State, other: &State) {
    for space in SPACES {
        let mine = space_mut(into, space);
        for (key, rec) in space_of(other, space) {
            let e = mine.entry(key.clone()).or_default();
            e.info.fill(&rec.info);
            for (field, reg) in &rec.f {
                match e.f.get(field) {
                    Some(cur) if !reg.beats(cur) => {}
                    _ => {
                        e.f.insert(field.clone(), reg.clone());
                    }
                }
            }
        }
    }
}

/// A value of this device to change because another device set a newer one.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub space: Space,
    pub key: String,
    pub field: String,
    /// The value on this device when the pass read it (`null`: not set). Applied only while it still is.
    pub from: Value,
    pub to: Value,
    /// The game's file on this device (games only).
    pub path: PathBuf,
    pub info: Info,
    /// For a game's sticker: the sticker's name, to make it when this device has no sticker of that name.
    pub sticker: String,
}

/// Finds a game's file on this device by its key: the path this device already uses for it, else a game of the
/// library with that key (`find`, from the game list), else `/storage/roms/<key>` when that file exists, else the path
/// on the device that wrote it (the same on every Pocket unless the folders differ).
fn resolve(key: &str, info: &Info, local: &Local, find: &dyn Fn(&str) -> Option<PathBuf>) -> PathBuf {
    if let Some(e) = local.games.get(key) {
        return e.path.clone();
    }
    if let Some(p) = find(key) {
        return p;
    }
    let guess = Path::new(crate::sync::ROMS_DIR).join(key);
    if guess.is_file() || info.path.is_empty() {
        return guess;
    }
    PathBuf::from(&info.path)
}

/// What differs between this device and the merged state: the changes to apply here.
pub fn changes(local: &Local, merged: &State, find: &dyn Fn(&str) -> Option<PathBuf>) -> Vec<Change> {
    let mut out = Vec::new();
    for space in SPACES {
        let local_space = local.space(space);
        for (key, rec) in space_of(merged, space) {
            if space == Space::Game && local.ambiguous.contains(key) {
                continue;
            }
            let empty = Fields::new();
            let lf = local_space.get(key.as_str()).copied().unwrap_or(&empty);
            for (field, reg) in rec.f.iter().filter(|(f, _)| known(space, f)) {
                let Some(to) = normalize(space, field, &reg.v) else { continue };
                let from = lf.get(field).cloned().unwrap_or(Value::Null);
                if from != to {
                    let path = if space == Space::Game {
                        resolve(key, &rec.info, local, find)
                    } else {
                        PathBuf::new()
                    };
                    let sticker = field
                        .strip_prefix(STICKER_PREFIX)
                        .map(|k| {
                            merged
                                .stickers
                                .get(k)
                                .map_or_else(|| k.to_string(), |r| if r.info.name.is_empty() { k.to_string() } else { r.info.name.clone() })
                        })
                        .unwrap_or_default();
                    out.push(Change {
                        space,
                        key: key.clone(),
                        field: field.clone(),
                        from,
                        to,
                        path,
                        info: rec.info.clone(),
                        sticker,
                    });
                }
            }
        }
    }
    out
}

// ---- applying -----------------------------------------------------------------------------------------------------

/// What [`apply`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Applied {
    pub applied: usize,
    /// Changed on this device meanwhile: left as the player set it (the next pass sends it on).
    pub skipped: usize,
    pub library: bool,
    pub favorites: bool,
    pub shelves: bool,
    /// A game was hidden or shown: the lists are read again.
    pub hidden: bool,
}

/// The value a change's field has on this device now.
fn current(c: &Change, meta: &LibraryMeta, favs: &Favorites, prefs: &CuratorPrefs) -> Value {
    let f = match c.space {
        Space::Game => game_fields(&meta.get(&c.path), favs.contains(&c.path), meta.stickers()),
        Space::Sticker => meta
            .stickers()
            .iter()
            .find(|s| name_key(&s.name) == c.key)
            .map(sticker_fields)
            .unwrap_or_default(),
        Space::Shelf => prefs.shelves.iter().find(|s| name_key(&s.name) == c.key).map(shelf_fields).unwrap_or_default(),
    };
    f.get(&c.field).cloned().unwrap_or(Value::Null)
}

/// A sticker by its key, made (with `name`, else the key) when there is none; its id.
fn sticker_id(meta: &mut LibraryMeta, key: &str, name: &str) -> u32 {
    if let Some(s) = meta.stickers().iter().find(|s| name_key(&s.name) == key) {
        return s.id;
    }
    let id = meta.stickers().iter().map(|s| s.id + 1).max().unwrap_or(1);
    let name = if name.trim().is_empty() { key } else { name };
    let name: String = name.chars().take(crate::library_meta::STICKER_MAX).collect();
    meta.stickers_mut().push(Sticker { id, name, pinned: false });
    id
}

/// Applies the changes of a pass to the App's copies, skipping any whose value changed on this device since the pass
/// read it (all are checked before the first is applied, so a shelf made by one change takes its pin from the next).
/// Saves `library.json` and `favorites.json` when they changed (the caller saves `curator.json` when `shelves` says
/// so: the App does that with its own refresh). Sticker definitions go first, then shelves and games, then the stickers
/// removed (only when no game holds one any more).
pub fn apply(changes: &[Change], meta: &mut LibraryMeta, favs: &mut Favorites, prefs: &mut CuratorPrefs) -> (Applied, io::Result<()>) {
    let mut done = Applied::default();
    let order = |c: &Change| match (c.space, c.to.is_null()) {
        (Space::Sticker, false) => 0,
        (Space::Shelf, _) => 1,
        (Space::Game, _) => 2,
        (Space::Sticker, true) => 3,
    };
    let mut valid: Vec<&Change> = changes.iter().filter(|c| current(c, meta, favs, prefs) == c.from).collect();
    done.skipped = changes.len() - valid.len();
    valid.sort_by_key(|c| order(c));
    for c in valid {
        let ok = match c.space {
            Space::Game => apply_game(c, meta, favs, &mut done),
            Space::Sticker => apply_sticker(c, meta, &mut done),
            Space::Shelf => apply_shelf(c, prefs, &mut done),
        };
        if ok {
            done.applied += 1;
        }
    }
    let mut result = Ok(());
    if done.library {
        result = meta.save_now();
    }
    if done.favorites {
        if let Err(e) = favs.save_now() {
            result = result.and(Err(e));
        }
    }
    (done, result)
}

fn apply_game(c: &Change, meta: &mut LibraryMeta, favs: &mut Favorites, done: &mut Applied) -> bool {
    let to = &c.to;
    if let Some(sticker) = c.field.strip_prefix(STICKER_PREFIX) {
        let on = to.as_bool() == Some(true);
        let id = match meta.stickers().iter().find(|s| name_key(&s.name) == sticker) {
            Some(s) => s.id,
            None if on => sticker_id(meta, sticker, &c.sticker),
            None => return true,
        };
        let g = meta.game_mut(&c.path);
        g.stickers.retain(|s| *s != id);
        if on {
            g.stickers.push(id);
        }
        done.library = true;
        return true;
    }
    match c.field.as_str() {
        "favorite" => {
            let system = if c.info.system.is_empty() {
                c.key.split('/').next().unwrap_or_default().to_string()
            } else {
                c.info.system.clone()
            };
            let name = if c.info.name.is_empty() { stem(&c.path) } else { c.info.name.clone() };
            done.favorites |= favs.set(&system, &name, &c.path, to.as_bool() == Some(true));
        }
        "status" => {
            let g = meta.game_mut(&c.path);
            match serde_json::from_value::<Status>(to.clone()) {
                Ok(s) => {
                    g.status = s;
                    g.status_set = true;
                }
                Err(_) => {
                    g.status = Status::NotStarted;
                    g.status_set = false;
                }
            }
            done.library = true;
        }
        "note" => {
            meta.game_mut(&c.path).note = to.as_str().unwrap_or_default().to_string();
            done.library = true;
        }
        "rating" => {
            meta.game_mut(&c.path).rating = to.as_u64().unwrap_or(0).min(RATING_MAX as u64) as u8;
            done.library = true;
        }
        "hidden" => {
            meta.game_mut(&c.path).hidden = to.as_bool() == Some(true);
            done.library = true;
            done.hidden = true;
        }
        _ => return false,
    }
    true
}

fn apply_sticker(c: &Change, meta: &mut LibraryMeta, done: &mut Applied) -> bool {
    match (c.field.as_str(), c.to.as_str(), c.to.as_bool()) {
        ("name", Some(name), _) => {
            let id = sticker_id(meta, &c.key, name);
            if let Some(s) = meta.stickers_mut().iter_mut().find(|s| s.id == id) {
                s.name = name.chars().take(crate::library_meta::STICKER_MAX).collect();
            }
        }
        ("name", None, _) => {
            // Removed on another device: gone here too, unless a game still holds it (put on later elsewhere).
            let Some(id) = meta.stickers().iter().find(|s| name_key(&s.name) == c.key).map(|s| s.id) else {
                return true;
            };
            if meta.games().values().any(|g| g.stickers.contains(&id)) {
                return false;
            }
            meta.stickers_mut().retain(|s| s.id != id);
            for g in meta.games_mut() {
                g.stickers.retain(|s| *s != id);
            }
        }
        ("pinned", _, on) => {
            let exists = meta.stickers().iter().any(|s| name_key(&s.name) == c.key);
            if !exists && on != Some(true) {
                return true;
            }
            let name = c.info.name.clone();
            let id = sticker_id(meta, &c.key, &name);
            if let Some(s) = meta.stickers_mut().iter_mut().find(|s| s.id == id) {
                s.pinned = on == Some(true);
            }
        }
        _ => return false,
    }
    done.library = true;
    true
}

fn apply_shelf(c: &Change, prefs: &mut CuratorPrefs, done: &mut Applied) -> bool {
    let at = prefs.shelves.iter().position(|s| name_key(&s.name) == c.key);
    match (c.field.as_str(), at) {
        ("lens", _) if c.to.is_null() => {
            if let Some(i) = at {
                prefs.shelves.remove(i);
            }
        }
        ("lens", Some(i)) => match serde_json::from_value(c.to.clone()) {
            Ok(lens) => prefs.shelves[i].lens = lens,
            Err(_) => return false,
        },
        ("lens", None) => match serde_json::from_value(c.to.clone()) {
            Ok(lens) => {
                let name = if c.info.name.is_empty() { c.key.clone() } else { c.info.name.clone() };
                let name: String = name.chars().take(crate::curator::prefs::SHELF_NAME_MAX).collect();
                prefs.shelves.push(SmartShelf {
                    name,
                    lens,
                    pinned: true,
                    order: Default::default(),
                });
            }
            Err(_) => return false,
        },
        ("pinned", Some(i)) => prefs.shelves[i].pinned = c.to.as_bool() != Some(false),
        ("order", Some(i)) => prefs.shelves[i].order = serde_json::from_value(c.to.clone()).unwrap_or_default(),
        // A shelf removed here: its pin and order went with it.
        ("pinned" | "order", None) => return true,
        _ => return false,
    }
    done.shelves = true;
    true
}

// ---- play history -------------------------------------------------------------------------------------------------

/// This device's play log as history records.
pub fn export(log: &Log) -> History {
    let mut h = History {
        version: VERSION,
        ..History::default()
    };
    let keys: Vec<Option<String>> = log.games.iter().map(|g| game_key(Path::new(&g.path))).collect();
    for (i, g) in log.games.iter().enumerate() {
        if let Some(k) = &keys[i] {
            h.games.entry(k.clone()).or_insert_with(|| Info {
                name: g.name.clone(),
                system: g.system.clone(),
                path: g.path.clone(),
            });
        }
    }
    for s in &log.sessions {
        let Some(Some(g)) = keys.get(s.game) else { continue };
        h.sessions.push(SessionRec {
            g: g.clone(),
            s: s.start,
            tz: s.tz,
            secs: s.secs,
            e: s.ended,
            em: s.emulator.clone(),
            p: s.parts.iter().map(|p| (p.at, p.secs)).collect(),
        });
    }
    for c in &log.statuses {
        let Some(Some(g)) = keys.get(c.game) else { continue };
        h.statuses.push(StatusRec {
            g: g.clone(),
            at: c.at,
            tz: c.tz,
            st: c.status,
        });
    }
    h
}

/// Takes `other`'s sessions and status changes into `into`, each once: a session is its game and start (of two
/// copies the one with its end, then the longer, is kept), a status change its game, time and status.
pub fn union(into: &mut History, other: &History) {
    for (k, info) in &other.games {
        into.games.entry(k.clone()).or_default().fill(info);
    }
    let mut at: HashMap<(String, u64), usize> = into.sessions.iter().enumerate().map(|(i, s)| ((s.g.clone(), s.s), i)).collect();
    for s in &other.sessions {
        match at.get(&(s.g.clone(), s.s)) {
            Some(&i) => {
                let cur = &into.sessions[i];
                if (s.e, s.secs) > (cur.e, cur.secs) {
                    into.sessions[i] = s.clone();
                }
            }
            None => {
                at.insert((s.g.clone(), s.s), into.sessions.len());
                into.sessions.push(s.clone());
            }
        }
    }
    let seen: BTreeSet<(String, u64, String)> = into.statuses.iter().map(|c| (c.g.clone(), c.at, format!("{:?}", c.st))).collect();
    let mut seen = seen;
    for c in &other.statuses {
        if seen.insert((c.g.clone(), c.at, format!("{:?}", c.st))) {
            into.statuses.push(c.clone());
        }
    }
    into.sessions.sort_by(|a, b| (a.s, &a.g).cmp(&(b.s, &b.g)));
    into.statuses.sort_by(|a, b| (a.at, &a.g).cmp(&(b.at, &b.g)));
}

/// The sessions of `merged` that this device's own log doesn't have, as `play_log::Extra`, each game at its path here.
pub fn extra(merged: &History, own: &Log, local: &Local, find: &dyn Fn(&str) -> Option<PathBuf>) -> play_log::Extra {
    let mine: BTreeSet<(String, u64)> = own
        .sessions
        .iter()
        .filter_map(|s| own.games.get(s.game).and_then(|g| game_key(Path::new(&g.path))).map(|k| (k, s.start)))
        .collect();
    let mine_status: BTreeSet<(String, u64)> = own
        .statuses
        .iter()
        .filter_map(|c| own.games.get(c.game).and_then(|g| game_key(Path::new(&g.path))).map(|k| (k, c.at)))
        .collect();
    let mut out = play_log::Extra::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut game = |key: &str, out: &mut play_log::Extra| -> usize {
        if let Some(&i) = index.get(key) {
            return i;
        }
        let info = merged.games.get(key).cloned().unwrap_or_default();
        let path = resolve(key, &info, local, find);
        let name = if info.name.is_empty() { stem(&path) } else { info.name.clone() };
        let system = if info.system.is_empty() {
            key.split('/').next().unwrap_or_default().to_string()
        } else {
            info.system.clone()
        };
        out.games.push(play_log::Game {
            path: path.to_string_lossy().into_owned(),
            name,
            system,
        });
        index.insert(key.to_string(), out.games.len() - 1);
        out.games.len() - 1
    };
    for s in merged.sessions.iter().filter(|s| !mine.contains(&(s.g.clone(), s.s))) {
        let g = game(&s.g, &mut out);
        out.sessions.push(play_log::Session {
            game: g,
            start: s.s,
            tz: s.tz,
            secs: s.secs,
            ended: s.e,
            emulator: s.em.clone(),
            parts: s.p.iter().map(|&(at, secs)| play_log::Part { at, secs }).collect(),
            id: 0,
        });
    }
    for c in merged.statuses.iter().filter(|c| !mine_status.contains(&(c.g.clone(), c.at))) {
        let g = game(&c.g, &mut out);
        out.statuses.push(play_log::StatusChange {
            game: g,
            at: c.at,
            tz: c.tz,
            status: c.st,
        });
    }
    out
}

// ---- a pass -------------------------------------------------------------------------------------------------------

/// What a pass needs.
pub struct Setup<'a> {
    pub data_dir: &'a Path,
    /// The Syncthing folder (`<data>/library-sync`).
    pub root: &'a Path,
    /// This device's folder in it ([`device_folder`]).
    pub me: &'a str,
    /// Unix milliseconds now.
    pub now: u64,
    /// This device's play log; None with Play history off (then no history is read or written).
    pub log: Option<Arc<Log>>,
    /// A game of the library by its key (from the game list), for a game this device has no notes on yet.
    pub find: &'a dyn Fn(&str) -> Option<PathBuf>,
}

/// What a pass found.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Set by the caller: which pass this is, to write its files only once its changes are applied.
    pub number: u64,
    /// The merged state: this device's `library.json` once the changes are applied.
    pub state: State,
    /// The merged history (None with Play history off).
    pub history: Option<History>,
    /// What to change on this device.
    pub changes: Vec<Change>,
    /// The other devices' sessions to show with the own log.
    pub extra: Option<play_log::Extra>,
    /// The other devices' folders that were read.
    pub devices: Vec<String>,
    /// Files that could not be read (for the log; never shown as an error).
    pub problems: Vec<String>,
    /// `state` differs from the file on the card.
    pub state_changed: bool,
    pub history_changed: bool,
}

/// Reads a JSON file of another device: None (and why, in `problems`) when it is missing, too big or not ours.
fn read_foreign<T: serde::de::DeserializeOwned>(path: &Path, problems: &mut Vec<String>) -> Option<T> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            problems.push(format!("{}: {e}", path.display()));
            return None;
        }
    };
    if !meta.is_file() || meta.len() > MAX_FILE {
        problems.push(format!("{}: not a file of ours ({} bytes)", path.display(), meta.len()));
        return None;
    }
    match std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    {
        Ok(v) => Some(v),
        Err(e) => {
            problems.push(format!("{}: {e}", path.display()));
            None
        }
    }
}

/// The other devices' folders in `root`, sorted (hidden ones such as Syncthing's `.stfolder` left out).
pub fn devices(root: &Path, me: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && n != me)
        .collect();
    out.sort();
    out
}

/// One pass: this device's files as they are, the own folder's state, every other device's; the merged result and what
/// to change here. Reads only; [`store`] writes the own folder once the changes are applied.
pub fn pass(s: &Setup) -> Outcome {
    let mut out = Outcome::default();
    let meta = LibraryMeta::load(s.data_dir);
    let favs = Favorites::load(s.data_dir);
    let prefs = CuratorPrefs::load(s.data_dir);
    let times = Times::of(s.data_dir, s.now);
    let local = Local::of(&meta, &favs, &prefs);
    let own_dir = s.root.join(s.me);
    let own: State = crate::fsutil::read_json_or_backup(&own_dir.join(STATE_FILE)).unwrap_or_default();
    let mut merged = stamp(&local, &own, &times, s.me);
    let own_history: Option<History> = s
        .log
        .as_ref()
        .map(|_| crate::fsutil::read_json_or_backup(&own_dir.join(HISTORY_FILE)).unwrap_or_default());
    let mut history = s.log.as_ref().map(|log| {
        let mut h = export(log);
        if let Some(before) = &own_history {
            union(&mut h, before);
        }
        h
    });
    for d in devices(s.root, s.me) {
        let dir = s.root.join(&d);
        let mut read_any = false;
        if let Some(state) = read_foreign::<State>(&dir.join(STATE_FILE), &mut out.problems) {
            merge(&mut merged, &state);
            read_any = true;
        }
        if let Some(h) = history.as_mut() {
            if let Some(theirs) = read_foreign::<History>(&dir.join(HISTORY_FILE), &mut out.problems) {
                union(h, &theirs);
                read_any = true;
            }
        }
        if read_any {
            out.devices.push(d);
        }
    }
    out.changes = changes(&local, &merged, s.find);
    out.state_changed = merged != own;
    if let (Some(h), Some(log)) = (history.as_mut(), s.log.as_ref()) {
        h.version = VERSION;
        h.device = s.me.to_string();
        out.history_changed = own_history.as_ref() != Some(h);
        out.extra = Some(extra(h, log, &local, s.find));
    }
    out.state = merged;
    out.history = history;
    out
}

/// Writes the own folder: `library.json` and `history.json` when they changed (Syncthing then sends them on).
pub fn store(root: &Path, me: &str, out: &Outcome) -> io::Result<()> {
    let dir = root.join(me);
    if out.state_changed {
        crate::fsutil::write_json(&dir.join(STATE_FILE), &out.state)?;
    }
    if let (true, Some(h)) = (out.history_changed, out.history.as_ref()) {
        crate::fsutil::write_json(&dir.join(HISTORY_FILE), h)?;
    }
    Ok(())
}

/// A map of the library's games by key for [`Setup::find`]; a key two games share is left out.
pub fn index<'a>(paths: impl IntoIterator<Item = &'a Path>) -> HashMap<String, Option<PathBuf>> {
    let mut out: HashMap<String, Option<PathBuf>> = HashMap::new();
    for p in paths {
        if let Some(k) = game_key(p) {
            out.entry(k).and_modify(|e| *e = None).or_insert_with(|| Some(p.to_path_buf()));
        }
    }
    out
}

/// Whether passes run: Syncthing is switched on here and Library data is shared. Off, nothing in the folder is read or
/// written and the other devices' sessions are not shown.
pub fn wanted(prefs: &crate::sync::Prefs) -> bool {
    prefs.enabled && prefs.library
}

/// The Syncthing folder of a data directory.
pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join(FOLDER)
}

/// Unix milliseconds now.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests;

//! Pocket-to-Pocket sync, the core: which save goes where, decided from both devices' indexes (docs/SYNC-OWN.md).
//! No network and no UI here (those come with the transport and the Sync page); this part is pure apart from
//! reading the roms folder and the device's own record, so every case is a unit test (`pocket_sync/tests.rs`, with
//! a random run of four simulated Pockets).
//!
//! **Any number of Pockets, no server.** Every save carries a version vector ([`Vv`]): per device, how many changes
//! of that device the version contains. A device that sees its own save changed adds one to its own count
//! ([`bookkeep`]); a save that arrives brings its vector ([`received`]). Between any two Pockets ([`decide`]):
//! - same bytes: nothing to do (the vectors are merged);
//! - one vector contains the other: that version has all the other's play and more, it goes over. An old version can
//!   never come back: its vector is contained in the newer one's on every device the newer one reaches;
//! - neither contains the other: the two were played independently. Only then is it a conflict.
//!
//! **One question per save at a time.** The device that finds a conflict puts the other version beside the save as a
//! `.sync-conflict-` copy ([`conflict_name`]: Syncthing's form, so `sync_conflicts` and the "Two versions" page show
//! it) and records the question ([`Rec::asking`]); the other version gets a mark ([`Rec::marks`]: the question's whole
//! union), and marks travel with their version and spread to every Pocket with the same bytes. A Pocket meeting a
//! version that is in an open question waits; one meeting the asking Pocket adds its version to that question
//! (`Action::Offer`). The choice gets exactly the union of every version shown and replaces them everywhere; two
//! choices that agree settle by themselves, a question answered elsewhere closes by itself ([`answered`]). Only two
//! Pockets that each asked before hearing of the other's question can ask about the same saves twice (counted in the
//! random test, rare); answered both ways, one answer stands everywhere ([`decide`]). No clock is used for any
//! decision; times are only shown.
//!
//! **Matching.** A save belongs to the game file beside it (same name, another ending). Saves match across devices by
//! the CRC32 of that game file when both have it ([`Entry::crc`]), else by system folder and file name, so a game
//! called "Pokemon Emerald (USA)" here and "Emerald" there shares its save, and two different games with one name
//! never mix. A save new here lands under this device's name for the game ([`target`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// The battery saves beside the games: the same endings as the Syncthing saves folder (`sync::save_extensions`, a
/// test keeps them equal). Memory cards (`mcr`, `mcd`) go whole, as there.
pub const SAVE_EXTS: [&str; 9] = ["srm", "rtc", "sav", "eep", "fla", "sra", "mcr", "mcd", "dsv"];
/// Files beside a save that are never its game.
const NOT_GAMES: [&str; 13] = ["png", "jpg", "txt", "xml", "cfg", "json", "pstate", "state", "z", "md", "nfo", "dat", "auto"];
/// Folders of the roms folder that hold no games' saves of ours: states, our own backups, BIOS files, Azahar's data.
const SKIP_DIRS: [&str; 5] = ["savestates", "sync-backups", "backup", "bios", "azahar"];
/// Game files larger than this get no CRC (disc images): they match by name.
pub const CRC_MAX: u64 = 64 << 20;
/// A save larger than this is not a battery save; it is left out.
pub const MAX_SAVE: u64 = 16 << 20;
/// At most this many directory entries are looked at per roms folder, so an odd folder never makes a scan long.
const BUDGET: usize = 60_000;
/// The record's format; a newer one is not half understood.
pub const FORMAT: u64 = 1;
/// At most this many open questions are remembered per version (more only come from a damaged or hostile index).
pub const MAX_MARKS: usize = 16;

// ---------------------------------------------------------------------------------------------------------------
// Version vectors.

/// Device id → how many of its changes a version contains.
pub type Vv = BTreeMap<String, u64>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Equal,
    /// The first contains the second.
    Newer,
    /// The second contains the first.
    Older,
    /// Played independently.
    Concurrent,
}

pub fn order(a: &Vv, b: &Vv) -> Order {
    match (le(a, b), le(b, a)) {
        (true, true) => Order::Equal,
        (false, true) => Order::Newer,
        (true, false) => Order::Older,
        (false, false) => Order::Concurrent,
    }
}

/// `a ≤ b`: every change in `a` is also in `b`.
pub fn le(a: &Vv, b: &Vv) -> bool {
    a.iter().all(|(d, n)| b.get(d).copied().unwrap_or(0) >= *n)
}

pub fn merge(a: &Vv, b: &Vv) -> Vv {
    let mut m = a.clone();
    for (d, n) in b {
        let e = m.entry(d.clone()).or_insert(0);
        *e = (*e).max(*n);
    }
    m
}

/// A device id: 1 to 32 ASCII letters and digits (it is part of file names and of the vector's text form).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// `ID:3,ID:1` and back; None for anything else (a vector from another device is data).
pub fn vv_str(v: &Vv) -> String {
    v.iter().map(|(d, n)| format!("{d}:{n}")).collect::<Vec<_>>().join(",")
}

pub fn vv_parse(s: &str) -> Option<Vv> {
    if s.is_empty() {
        return Some(Vv::new());
    }
    let v: Option<Vv> = s
        .split(',')
        .map(|p| {
            let (d, n) = p.split_once(':')?;
            valid_id(d).then_some(())?;
            Some((d.to_string(), n.parse().ok()?))
        })
        .collect();
    v.filter(|v| v.len() <= 256)
}

// ---------------------------------------------------------------------------------------------------------------
// Saves.

/// One save on one device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// `gba/Pokemon Emerald (USA).srm`, below the device's roms folder (see [`safe_rel`]).
    pub rel: String,
    /// The system folder, `gba`.
    pub system: String,
    /// The file name without the ending.
    pub stem: String,
    /// `srm`, `rtc`, ... (lower case).
    pub ext: String,
    /// CRC32 of the game file beside it, when there is one (not for disc images, [`CRC_MAX`]).
    pub crc: Option<u32>,
    pub size: u64,
    /// Unix seconds, for showing only.
    pub mtime: u64,
    /// SHA-256 of the bytes, hex.
    pub sha: String,
    pub vv: Vv,
    /// A question about this save is open on its device: the union of the other versions it shows ([`Rec::asking`]).
    pub asking: Option<Vv>,
    /// Questions open on other Pockets that this version is one of ([`Rec::marks`]).
    pub marks: Vec<Vv>,
    /// A `.sync-conflict-` copy is beside it (only known on its own device; not sent).
    pub open_copy: bool,
}

impl Entry {
    // The system folder in any letters: Android's ROMs folders are often `GBA` where ROCKNIX has `gba` (found by the
    // Android sync test, 19.11.2026: the two Emerald saves never paired and the Plus got a second one under `GBA/`).
    fn name_key(&self) -> String {
        format!("name:{}/{}.{}", self.system.to_lowercase(), self.stem.to_lowercase(), self.ext)
    }

    fn crc_key(&self) -> Option<String> {
        self.crc.map(|c| format!("crc:{}/{c:08x}.{}", self.system.to_lowercase(), self.ext))
    }

    /// The form one device sends the other (the index).
    pub fn to_json(&self) -> Value {
        json!({"rel": self.rel, "crc": self.crc, "size": self.size, "mtime": self.mtime, "sha": self.sha,
               "vv": vv_str(&self.vv), "asking": self.asking.as_ref().map(vv_str),
               "marks": self.marks.iter().map(vv_str).collect::<Vec<_>>()})
    }

    /// An entry of another device's index. None for anything that is not a plain save path, a hash or a vector.
    pub fn from_json(v: &Value) -> Option<Entry> {
        let rel = v["rel"].as_str()?.to_string();
        let (system, stem, ext) = split_rel(&rel)?;
        let sha = v["sha"].as_str()?;
        if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let asking = match &v["asking"] {
            Value::Null => None,
            a => Some(vv_parse(a.as_str()?)?),
        };
        let marks = match &v["marks"] {
            Value::Null => Vec::new(),
            m => m.as_array()?.iter().map(|q| vv_parse(q.as_str()?)).collect::<Option<Vec<_>>>()?,
        };
        if marks.len() > MAX_MARKS {
            return None;
        }
        Some(Entry {
            system,
            stem,
            ext,
            rel,
            crc: v["crc"].as_u64().and_then(|c| u32::try_from(c).ok()),
            size: v["size"].as_u64()?,
            mtime: v["mtime"].as_u64().unwrap_or(0),
            sha: sha.to_ascii_lowercase(),
            vv: vv_parse(v["vv"].as_str()?)?,
            asking,
            marks,
            open_copy: false,
        })
    }
}

/// A save's path below the roms folder, from this device or another: `system/name.ext` or `system/sub/name.ext`,
/// plain parts, no hidden ones, a save ending. Anything else is refused: a path from the network is data.
pub fn safe_rel(rel: &str) -> Option<PathBuf> {
    split_rel(rel).map(|_| PathBuf::from(rel))
}

/// (system, stem, lower-case ending) of a [`safe_rel`] path.
fn split_rel(rel: &str) -> Option<(String, String, String)> {
    if rel.contains('\\') || rel.contains('\0') {
        return None;
    }
    let p = Path::new(rel);
    let parts: Vec<_> = p.components().collect();
    let plain = parts.iter().all(|c| matches!(c, Component::Normal(s) if !s.to_string_lossy().starts_with('.')));
    if !plain || !(2..=3).contains(&parts.len()) {
        return None;
    }
    let system = parts[0].as_os_str().to_str()?.to_string();
    let file = p.file_name()?.to_str()?;
    let (stem, ext) = file.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    (!stem.is_empty() && SAVE_EXTS.contains(&ext.as_str()) && !file.contains(".sync-conflict-"))
        .then(|| (system, stem.to_string(), ext))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// The plan.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Same bytes. `merged`: the vectors differ; both devices keep their union.
    Same { merged: Option<Vv> },
    /// This version contains the peer's, or the peer has none: send it. `expect`: the peer's hash now (the peer
    /// takes it only over that version).
    Push { expect: Option<String> },
    /// The peer's version contains ours, or we have none: fetch it.
    Pull,
    /// Played independently, and neither version is in a question yet (or the question is here): put the peer's
    /// version beside ours, overwrite nothing, ask here (a question already open here grows by it).
    Conflict,
    /// Played independently and the peer is already asking about this save: our version joins its question
    /// (sent as one more copy beside the peer's save, [`offered`]), so there is still one question.
    Offer,
    /// One of the two versions is in a question already: here (`here`) or on another Pocket. One question per save
    /// at a time; its choice replaces every version it showed.
    Waiting { here: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub local: Option<Entry>,
    pub remote: Option<Entry>,
    pub action: Action,
}

impl Step {
    /// Where the peer's version goes here (Pull) or beside what (Conflict): our file for the game, else
    /// `target` (our game file's name, found by CRC), else the peer's own path.
    pub fn local_rel(&self, target: Option<&str>) -> Option<String> {
        self.local.as_ref().map(|l| l.rel.clone()).or_else(|| target.map(str::to_string)).or_else(|| self.remote.as_ref().map(|r| r.rel.clone()))
    }
}

/// Pairs this device's saves with the peer's, each save once, never two whose game files have known, different CRCs:
/// 1. the same path (`gba/RPG/Game.srm` with `gba/RPG/Game.srm`);
/// 2. the game's CRC, where it is one save on each side (the same game under another name or folder);
/// 3. system and file name without the folder, case-insensitive, where that is one save on each side.
///
/// What stays ambiguous stays unpaired (Sol review 19.11.2026: `gba/A/Game.srm` was paired with `gba/B/Game.srm`
/// although `gba/B/Game.srm` was here too); an unpaired save is only ever sent to or fetched into a free place
/// (`expect` "none", [`collides`]), so nothing is overwritten for it.
pub fn pair(local: &[Entry], remote: &[Entry]) -> Vec<(Option<Entry>, Option<Entry>)> {
    let clash = |l: &Entry, r: &Entry| l.crc.is_some() && r.crc.is_some() && l.crc != r.crc;
    let mut used_l = vec![false; local.len()];
    let mut used_r = vec![false; remote.len()];
    let mut out = Vec::new();
    // 1. The same path.
    let by_rel: BTreeMap<&str, usize> = remote.iter().enumerate().map(|(i, r)| (r.rel.as_str(), i)).collect();
    for (i, l) in local.iter().enumerate() {
        if let Some(&j) = by_rel.get(l.rel.as_str()) {
            if !clash(l, &remote[j]) {
                used_l[i] = true;
                used_r[j] = true;
                out.push((Some(l.clone()), Some(remote[j].clone())));
            }
        }
    }
    // 2. and 3.: a key that is one save on each side among those still unpaired.
    for key in [Entry::crc_key as fn(&Entry) -> Option<String>, |e: &Entry| Some(e.name_key())] {
        let mut here: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut there: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, l) in local.iter().enumerate().filter(|(i, _)| !used_l[*i]) {
            if let Some(k) = key(l) {
                here.entry(k).or_default().push(i);
            }
        }
        for (j, r) in remote.iter().enumerate().filter(|(j, _)| !used_r[*j]) {
            if let Some(k) = key(r) {
                there.entry(k).or_default().push(j);
            }
        }
        for (k, ls) in &here {
            let Some(rs) = there.get(k) else { continue };
            if let ([i], [j]) = (ls.as_slice(), rs.as_slice()) {
                if !clash(&local[*i], &remote[*j]) {
                    used_l[*i] = true;
                    used_r[*j] = true;
                    out.push((Some(local[*i].clone()), Some(remote[*j].clone())));
                }
            }
        }
    }
    out.extend(local.iter().enumerate().filter(|(i, _)| !used_l[*i]).map(|(_, l)| (Some(l.clone()), None)));
    out.extend(remote.iter().enumerate().filter(|(j, _)| !used_r[*j]).map(|(_, r)| (None, Some(r.clone()))));
    out
}

/// `v` is one of the versions a question shows, or older: contained in its union `q` and not that union itself
/// (the union is what the choice gets: the answer, not a version asked about).
fn inside(v: &Vv, q: &Vv) -> bool {
    le(v, q) && v != q
}

/// The action for a matched pair.
pub fn decide(l: &Entry, r: &Entry) -> Action {
    if l.sha == r.sha {
        return Action::Same { merged: (l.vv != r.vv).then(|| merge(&l.vv, &r.vv)) };
    }
    match order(&l.vv, &r.vv) {
        Order::Newer => return Action::Push { expect: Some(r.sha.clone()) },
        Order::Older => return Action::Pull,
        // Equal vectors with other bytes: the same question answered both ways on two Pockets (only possible when
        // both asked before hearing of each other). The same play is in both; one answer stands, the same one on
        // every Pocket (the larger hash), without asking a third time. The other save goes to the backup folder like
        // every save that is replaced.
        Order::Equal if l.sha > r.sha => return Action::Push { expect: Some(r.sha.clone()) },
        Order::Equal => return Action::Pull,
        Order::Concurrent => {}
    }
    // The peer's version is shown here already, or ours there.
    if l.asking.as_ref().is_some_and(|a| le(&r.vv, a)) {
        return Action::Waiting { here: true };
    }
    if r.asking.as_ref().is_some_and(|a| le(&l.vv, a)) {
        return Action::Waiting { here: false };
    }
    // Both are in one question open on another Pocket: its choice replaces both.
    let same_question = |a: &Entry, b: &Entry| a.marks.iter().any(|q| le(&b.vv, q));
    if same_question(l, r) || same_question(r, l) {
        return Action::Waiting { here: false };
    }
    match (l.asking.is_some(), r.asking.is_some()) {
        // Asking here: the question grows by the peer's version.
        (true, false) => Action::Conflict,
        // Both asking (two questions that began apart): the first choice joins the other question.
        (true, true) => Action::Waiting { here: true },
        // Asking there: ours joins that question.
        (false, true) => Action::Offer,
        // A version in a question elsewhere meeting one that is not in it is new play that question doesn't
        // show: asked here rather than waited for. A question whose Pocket lost its record would otherwise wait
        // for ever (the random run with lost records, 19.11.2026); asking a second time instead stays rare.
        (false, false) => Action::Conflict,
    }
}

/// Every step of one sync with one peer, in a stable order (by path).
pub fn plan(local: &[Entry], remote: &[Entry]) -> Vec<Step> {
    let mut steps: Vec<Step> = pair(local, remote)
        .into_iter()
        .map(|(l, r)| {
            let action = match (&l, &r) {
                (Some(l), Some(r)) => decide(l, r),
                (Some(_), None) => Action::Push { expect: None },
                _ => Action::Pull,
            };
            Step { local: l, remote: r, action }
        })
        .collect();
    steps.sort_by(|a, b| {
        let k = |s: &Step| s.local.as_ref().or(s.remote.as_ref()).map(|e| e.rel.clone()).unwrap_or_default();
        k(a).cmp(&k(b))
    });
    steps
}

// ---------------------------------------------------------------------------------------------------------------
// What a device records per save.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rec {
    pub sha: String,
    pub vv: Vv,
    /// A question is open here (the other versions' copies are beside the save): the union of the versions it shows
    /// besides ours. The choice gets this merged with ours.
    pub asking: Option<Vv>,
    /// Questions open on other Pockets that this version is one of, each the union of every version it shows. A mark
    /// lasts while this version is inside it ([`tidy`]): playing on, or the answer arriving, ends it.
    pub marks: Vec<Vv>,
    /// `.sync-conflict-` copies are beside the save that no recorded question shows (the record was lost while a
    /// question was open): a choice over them counts as a change of this device ([`chose`]).
    pub orphan: bool,
}

/// Marks this version is no longer inside go; no mark twice; at most [`MAX_MARKS`].
fn tidy(mut r: Rec) -> Rec {
    let vv = r.vv.clone();
    r.marks.retain(|q| inside(&vv, q));
    r.marks.sort();
    r.marks.dedup();
    if r.marks.len() > MAX_MARKS {
        r.marks.drain(..r.marks.len() - MAX_MARKS);
    }
    r
}

/// This device's record for its save, now (`prev`: the record from last time, `open_copy`: a `.sync-conflict-` copy
/// is beside it).
/// - Unchanged: as it was.
/// - Changed (a game saved): one more change of ours; it leaves every question elsewhere (new play). A question
///   asked here stays open (its choice will contain this play).
/// - A question asked here whose copies are gone without a choice through [`chose`] (removed by hand): closed as
///   if chosen, and changed bytes count as play (an extra question later is better than lost play).
///
/// pocket-ui looks at the saves after every game and before it applies a choice, so play is counted before a
/// choice can hide it.
pub fn bookkeep(me: &str, prev: Option<&Rec>, sha: &str, open_copy: bool) -> Rec {
    let mut r = prev.cloned().unwrap_or_default();
    let changed = r.sha != sha;
    // Copies beside a save this record knows nothing of: a question a lost record no longer shows.
    if prev.is_none() && open_copy {
        r.orphan = true;
    }
    if !open_copy {
        r.orphan = false;
    }
    if r.asking.is_some() && !open_copy {
        let a = r.asking.take().unwrap_or_default();
        r.vv = merge(&r.vv, &a);
    }
    if changed || r.vv.is_empty() {
        *r.vv.entry(me.to_string()).or_insert(0) += 1;
    }
    r.sha = sha.to_string();
    tidy(r)
}

/// The player chose in the question asked here and `kept` (a hash) is the save now: the record gets exactly the
/// union of every version shown, nothing more (keeping the other version changes the bytes but is no play). So two
/// choices on two Pockets that agree settle without a new question, and the kept save replaces every version shown
/// wherever it goes. Call it when the choice is applied (after [`bookkeep`] counted any play before it).
///
/// With no question recorded here (the record was lost while it was open, 19.11.2026), the versions shown are
/// unknown: the choice counts as a change of this device (`writer`), whichever save was kept, so it is never taken
/// for the version it replaced (undone by a tie), and never quietly lost to another Pocket's later choice.
pub fn chose(rec: &mut Rec, kept: &str, writer: &str) {
    let unknown = rec.orphan || rec.asking.is_none();
    if let Some(a) = rec.asking.take() {
        rec.vv = merge(&rec.vv, &a);
    }
    // Some of the versions shown are known to no record here: the choice is news (see `Rec::orphan`).
    if unknown {
        *rec.vv.entry(writer.to_string()).or_insert(0) += 1;
    }
    rec.orphan = false;
    rec.sha = kept.to_string();
    *rec = tidy(std::mem::take(rec));
}

/// The record of a save that arrived from a peer (Pull here, or a Push received) over `prev`: its vector and its
/// marks. A question asked here stays open (the arrival contains our version, so the question is now between it and
/// the versions beside it), unless the arrival contains every version it shows: then it was answered on another
/// Pocket ([`answered`] tells the caller to move the copies to the backup folder).
pub fn received(prev: Option<&Rec>, e: &Entry) -> Rec {
    let asking = prev.and_then(|p| p.asking.clone()).filter(|a| !le(a, &e.vv));
    let orphan = prev.is_some_and(|p| p.orphan);
    tidy(Rec { sha: e.sha.clone(), vv: e.vv.clone(), asking, marks: e.marks.clone(), orphan })
}

/// Both Pockets have the same bytes (`Action::Same`): ours takes the union of the vectors and the peer's questions
/// about these bytes (open there, or marks), so a third Pocket meeting us waits as well. A question open here whose
/// versions the union contains was answered elsewhere: it closes (see [`answered`]).
pub fn same(rec: &mut Rec, peer: &Entry) {
    rec.vv = merge(&rec.vv, &peer.vv);
    if rec.asking.as_ref().is_some_and(|a| le(a, &rec.vv)) {
        rec.asking = None;
    }
    rec.marks.extend(peer.marks.iter().cloned());
    if let Some(a) = &peer.asking {
        rec.marks.push(merge(&peer.vv, a));
    }
    *rec = tidy(std::mem::take(rec));
}

/// A question that was open (`before`) is closed now without a choice here: the copies beside the save go to the
/// backup folder, as after a choice.
pub fn answered(before: Option<&Rec>, now: &Rec) -> bool {
    before.is_some_and(|b| b.asking.is_some()) && now.asking.is_none()
}

/// A conflict found here between our `l` and the peer's `r` (`Action::Conflict`): our record asks (a question
/// already open here grows by `r`), and the mark for the peer's version: the whole question.
pub fn conflict_found(prev: Option<&Rec>, l: &Entry, r: &Entry) -> (Rec, Vv) {
    let mut rec = prev.cloned().unwrap_or_else(|| Rec { sha: l.sha.clone(), vv: l.vv.clone(), ..Rec::default() });
    let asking = rec.asking.take().map_or_else(|| r.vv.clone(), |a| merge(&a, &r.vv));
    let whole = merge(&rec.vv, &asking);
    rec.asking = Some(asking);
    (tidy(rec), whole)
}

/// The peer `l` offered its version to the question open here (`Action::Offer`, `rec` is ours): the question grows
/// by it. Returns the mark for the peer's version (the whole question).
pub fn offered(rec: &mut Rec, l: &Entry) -> Vv {
    let asking = rec.asking.take().map_or_else(|| l.vv.clone(), |a| merge(&a, &l.vv));
    let whole = merge(&rec.vv, &asking);
    rec.asking = Some(asking);
    whole
}

/// The peer put our version (`sha`) into a question open there (`q`: its whole union). Taken only while our save is
/// still that version (a game may have written it since) and no question is open here. True when marked.
pub fn mark(rec: &mut Rec, sha: &str, q: &Vv) -> bool {
    if rec.sha != sha || rec.asking.is_some() {
        return false;
    }
    rec.marks.push(q.clone());
    *rec = tidy(std::mem::take(rec));
    true
}

/// `Game.srm` + time + peer → `Game.sync-conflict-20261117-231502-ABCDEF7.srm`, the copy of the peer's version
/// beside ours (Syncthing's form, read by `sync_conflicts::original_name`). `unix`: the version's time, UTC.
pub fn conflict_name(rel: &str, unix: u64, peer: &str) -> String {
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let (stem, ext) = file.rsplit_once('.').map_or((file, String::new()), |(s, e)| (s, format!(".{e}")));
    let (y, mo, d) = crate::civil_date::civil((unix / 86_400) as i64);
    let t = unix % 86_400;
    let id: String = peer.chars().filter(char::is_ascii_alphanumeric).take(7).collect::<String>().to_uppercase();
    let name = format!("{stem}.sync-conflict-{y:04}{mo:02}{d:02}-{:02}{:02}{:02}-{id}{ext}", t / 3600, t % 3600 / 60, t % 60);
    if dir.is_empty() {
        name
    } else {
        format!("{dir}/{name}")
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The record on the card: `<data>/pocket-sync/versions.json`.

/// Why the record is not used: sync waits (the saves are left alone) rather than guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    /// Written by a newer pocket-ui: left for it, not half read.
    Newer,
    /// The card did not give it (a read error other than "not there"): this time no sync, the next look tries again.
    Unreadable,
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RecordError::Newer => "the sync record was written by a newer pocket-ui",
            RecordError::Unreadable => "the sync record could not be read from the card",
        })
    }
}

/// How the record came to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Read as it was written.
    Read,
    /// Damaged or missing; its copy (`versions.json.bak`, written with it) was whole.
    Restored,
    /// No record and no whole copy: a new history (see [`Versions::open`]).
    Started,
}

/// This device's records, by save path, and the id its changes are counted under (`.1`; empty: the device's id).
///
/// A history that is lost must not look like the old one (Sol review 19.11.2026 #3): with the count starting at 1
/// again under the same id, a peer's older save with a higher count of this device looked newer and would have
/// replaced the newer save here. A new history therefore counts under a new id ([`Versions::open`]): its saves are
/// independent of every version the peers know, so different bytes become a question, never a silent winner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Versions(pub BTreeMap<String, Rec>, pub String);

impl Versions {
    /// The record at `path`: as written, else its whole copy, else nothing (a first start, or lost). A read error other
    /// than "not there" and a newer format are errors, so nothing is decided on a guess.
    pub fn load(path: &Path) -> Result<(Versions, Origin), RecordError> {
        match Self::read(path)? {
            Some(v) => Ok((v, Origin::Read)),
            None => match Self::read(&crate::fsutil::backup_path(path)) {
                Ok(Some(v)) => Ok((v, Origin::Restored)),
                Err(RecordError::Newer) => Err(RecordError::Newer),
                _ => Ok((Versions::default(), Origin::Started)),
            },
        }
    }

    /// [`Versions::load`], and a new history counted under `me` + `salt` when there was none (a first start costs
    /// nothing: no peer knows that id yet).
    pub fn open(path: &Path, me: &str, salt: &str) -> Result<Versions, RecordError> {
        let (mut v, origin) = Self::load(path)?;
        if origin == Origin::Started {
            v.1 = new_writer(me, salt);
        }
        Ok(v)
    }

    /// The file's content; `Ok(None)` when it is not there or not whole (damaged, cut off).
    fn read(path: &Path) -> Result<Option<Versions>, RecordError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RecordError::Unreadable),
        };
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else { return Ok(None) };
        if !v.is_object() || !v["saves"].is_object() {
            return Ok(None);
        }
        if v["format"].as_u64().unwrap_or(0) > FORMAT {
            return Err(RecordError::Newer);
        }
        let mut m = BTreeMap::new();
        for (rel, r) in v["saves"].as_object().into_iter().flatten() {
            let rec = (|| {
                Some(tidy(Rec {
                    sha: r["sha"].as_str()?.to_string(),
                    vv: vv_parse(r["vv"].as_str()?)?,
                    asking: r["asking"].as_str().and_then(vv_parse),
                    marks: r["marks"].as_array().map_or_else(Vec::new, |m| m.iter().filter_map(|q| vv_parse(q.as_str()?)).collect()),
                    orphan: r["orphan"].as_bool().unwrap_or(false),
                }))
            })();
            if let (Some(rec), Some(_)) = (rec, safe_rel(rel)) {
                m.insert(rel.clone(), rec);
            }
        }
        let writer = v["writer"].as_str().filter(|w| valid_id(w)).unwrap_or("").to_string();
        Ok(Some(Versions(m, writer)))
    }

    /// Written whole (`fsutil::write_atomic`), and its copy too, so a record damaged later comes back from the copy.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let saves: serde_json::Map<String, Value> = self
            .0
            .iter()
            .map(|(k, r)| {
                let marks: Vec<String> = r.marks.iter().map(vv_str).collect();
                (k.clone(), json!({"sha": r.sha, "vv": vv_str(&r.vv), "asking": r.asking.as_ref().map(vv_str), "marks": marks, "orphan": r.orphan}))
            })
            .collect();
        let bytes = json!({"format": FORMAT, "writer": self.1, "saves": saves}).to_string();
        crate::fsutil::write_atomic(path, bytes.as_bytes())?;
        crate::fsutil::write_atomic(&crate::fsutil::backup_path(path), bytes.as_bytes())
    }

    /// The id this record counts this device's changes under.
    pub fn writer<'a>(&'a self, me: &'a str) -> &'a str {
        if self.1.is_empty() { me } else { &self.1 }
    }
}

/// A new history's id: the device's id and a few random letters, at most 32 letters and digits.
pub fn new_writer(me: &str, salt: &str) -> String {
    let w: String = me.chars().chain(salt.chars()).filter(char::is_ascii_alphanumeric).take(32).collect();
    if valid_id(&w) { w } else { "P".to_string() }
}

// ---------------------------------------------------------------------------------------------------------------
// The saves on this device (ROCKNIX: beside the games in /storage/roms).

/// Where the games and the saves are.
#[derive(Clone, Debug)]
pub struct Layout {
    /// The games: `/storage/roms` on ROCKNIX; on Android the first ROMs folder (internal storage).
    pub roms: PathBuf,
    pub saves: SavesAt,
}

/// Where a game's saves are. The path in the index is always `<system>/<sub>/<stem>.<ext>`, as if beside the game.
#[derive(Clone, Debug)]
pub enum SavesAt {
    /// Beside the game (ROCKNIX).
    BesideGames,
    /// In a folder of their own with the games' layout: `<root>/<system>/<sub>/<stem>.<ext>`, the game for the CRC in
    /// `<roms>/<system>/<sub>/<stem>.*` (the Brick's and dArkOS's cores).
    Root(PathBuf),
    /// Android's player: by the game's key (see [`Keyed`]).
    Android(Keyed),
}

/// A game's key in Android's player (`android_handheld::game_key`): game file and system id (the system folder's name in
/// small letters) → the 16 letters of `saves/<key>/`. None: no key for that file.
pub type KeyFn = std::sync::Arc<dyn Fn(&Path, &str) -> Option<String> + Send + Sync>;

/// Android's handheld view (docs/SYNC-OWN.md "Android"): the games are in `<root>/<system>/` of each ROMs folder (internal
/// storage, then the cards), the player keeps a game's saves in `<saves>/<key>/<stem>.<ext>`. A save's path in the index
/// stays the ROCKNIX one, `<system>/<sub>/<stem>.<ext>` (as if beside its game), so matching and the record are the same.
#[derive(Clone)]
pub struct Keyed {
    /// Every ROMs folder, `roms` first.
    pub roots: Vec<PathBuf>,
    /// The player's `saves`.
    pub saves: PathBuf,
    /// Where replaced saves and answered copies go (the app's own folder: nothing on the card is touched).
    pub backups: PathBuf,
    pub key: KeyFn,
}

impl std::fmt::Debug for Keyed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keyed").field("roots", &self.roots).field("saves", &self.saves).field("backups", &self.backups).finish()
    }
}

impl Layout {
    pub fn rocknix() -> Layout {
        Layout::beside(PathBuf::from("/storage/roms"))
    }

    /// The saves beside the games in `roms` (ROCKNIX, tests).
    pub fn beside(roms: PathBuf) -> Layout {
        Layout { roms, saves: SavesAt::BesideGames }
    }

    /// Saves in a folder of their own with the games' layout ([`SavesAt::Root`]).
    pub fn saves_root(roms: PathBuf, saves: PathBuf) -> Layout {
        Layout { roms, saves: SavesAt::Root(saves) }
    }

    /// Android's handheld view: games in `roots`, saves in `saves/<key>/` (see [`Keyed`]).
    pub fn android(roots: Vec<PathBuf>, saves: PathBuf, backups: PathBuf, key: KeyFn) -> Layout {
        let roms = roots.first().cloned().unwrap_or_default();
        Layout { roms, saves: SavesAt::Android(Keyed { roots, saves, backups, key }) }
    }

    /// Android's player's folders, None elsewhere.
    pub fn keyed(&self) -> Option<&Keyed> {
        match &self.saves {
            SavesAt::Android(k) => Some(k),
            _ => None,
        }
    }

    /// The folder the saves' system folders are in: the games' folder, or the saves' own.
    fn save_root(&self) -> &Path {
        match &self.saves {
            SavesAt::Root(r) => r,
            _ => &self.roms,
        }
    }

    /// The folder with the backups: `<roms>/sync-backups` (the "Two versions" page's), `<saves>/sync-backups` for saves in
    /// a folder of their own, Android's own folder there.
    pub fn backups(&self) -> PathBuf {
        match &self.saves {
            SavesAt::Android(k) => k.backups.clone(),
            SavesAt::Root(r) => r.join("sync-backups"),
            SavesAt::BesideGames => self.roms.join("sync-backups"),
        }
    }

    /// The folder a save's file is in (its copies beside it): `<roms>/<system>/<sub>`, on Android `<saves>/<key>` of the
    /// game with the save's name in that folder of a ROMs folder. None: not a save path, or (Android) no such game here.
    pub fn dir_of(&self, rel: &str) -> Option<PathBuf> {
        let (system, stem, _) = split_rel(rel)?;
        let parent = Path::new(rel).parent()?;
        let Some(k) = self.keyed() else { return Some(self.save_root().join(parent)) };
        let game = k.roots.iter().find_map(|root| game_named(&root.join(parent), &stem))?;
        let key = (k.key)(&game, &system.to_ascii_lowercase())?;
        (key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit())).then(|| k.saves.join(key))
    }

    /// The file of a save (see [`dir_of`](Layout::dir_of)).
    pub fn file(&self, rel: &str) -> Option<PathBuf> {
        Some(self.dir_of(rel)?.join(Path::new(rel).file_name()?))
    }

    /// The path in the index of a save's file here: the inverse of [`file`](Layout::file) (the "Two versions" page has
    /// the file, the record wants the path).
    pub fn rel_of(&self, file: &Path) -> Option<String> {
        let Some(k) = self.keyed() else {
            let rel = file.strip_prefix(self.save_root()).ok()?.to_str()?.to_string();
            return safe_rel(&rel).map(|_| rel);
        };
        let dir = file.parent()?;
        let name = file.file_name()?.to_str()?;
        let (stem, _) = name.rsplit_once('.')?;
        keyed_games(k).into_iter().find_map(|(prefix, game, system)| {
            let rel = format!("{prefix}{name}");
            (game_stem(&game) == Some(stem) && split_rel(&rel).is_some() && (k.key)(&game, &system).is_some_and(|key| dir == k.saves.join(key)))
                .then_some(rel)
        })
    }
}

fn game_stem(p: &Path) -> Option<&str> {
    p.file_name()?.to_str()?.rsplit_once('.').map(|(s, _)| s)
}

/// The game file named `stem` (any ending a game has) in `dir`; the first by name when there are two.
fn game_named(dir: &Path, stem: &str) -> Option<PathBuf> {
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| is_game(n) && n.rsplit_once('.').is_some_and(|(s, _)| s == stem))
        .map(|n| dir.join(n))
        .collect();
    hits.sort();
    hits.into_iter().next()
}

/// Every game of Android's ROMs folders: (`<system>/` or `<system>/<sub>/`, the file, the system id), a system folder and
/// one subfolder deep like [`scan`]; a path two ROMs folders both have is the first's.
fn keyed_games(k: &Keyed) -> Vec<(String, PathBuf, String)> {
    let mut out: Vec<(String, PathBuf, String)> = Vec::new();
    let mut budget = BUDGET;
    for root in &k.roots {
        let Ok(systems) = std::fs::read_dir(root) else { continue };
        let mut dirs: Vec<(PathBuf, String, String)> = systems
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_str()?.to_string();
                (e.file_type().ok()?.is_dir() && !n.starts_with('.') && !SKIP_DIRS.contains(&n.to_ascii_lowercase().as_str()))
                    .then(|| (e.path(), n.to_ascii_lowercase(), format!("{n}/")))
            })
            .collect();
        dirs.sort();
        while let Some((dir, system, prefix)) = dirs.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let mut found = Vec::new();
            for e in rd.flatten() {
                if budget == 0 {
                    break;
                }
                budget -= 1;
                let Some(n) = e.file_name().to_str().map(str::to_string) else { continue };
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    if prefix.matches('/').count() == 1 && !n.starts_with('.') && !SKIP_DIRS.contains(&n.to_ascii_lowercase().as_str()) {
                        dirs.push((e.path(), system.clone(), format!("{prefix}{n}/")));
                    }
                } else if ft.is_file() && is_game(&n) {
                    found.push(n);
                }
            }
            found.sort();
            for n in found {
                let stem = n.rsplit_once('.').map_or(n.as_str(), |(s, _)| s).to_string();
                // Two game files with one name in a folder share one save name: the first keeps it (as `dir_of`).
                if out.iter().any(|(p, g, _)| *p == prefix && game_stem(g) == Some(&stem)) {
                    continue;
                }
                out.push((prefix.clone(), dir.join(&n), system.clone()));
            }
        }
    }
    out
}

/// [`scan`] for Android: the saves in `<saves>/<key>/` of every game of the ROMs folders, by the path they would have beside
/// their game.
fn scan_keyed(k: &Keyed, crcs: &mut CrcCache) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for (prefix, game, system) in keyed_games(k) {
        let Some(stem) = game_stem(&game).map(str::to_string) else { continue };
        let Some(key) = (k.key)(&game, &system) else { continue };
        let dir = k.saves.join(&key);
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut names: Vec<String> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_file())).filter_map(|e| e.file_name().to_str().map(str::to_string)).collect();
        names.sort();
        for n in &names {
            let rel = format!("{prefix}{n}");
            let Some((sys, s, ext)) = split_rel(&rel) else { continue };
            if s != stem || out.iter().any(|e| e.rel == rel) {
                continue;
            }
            let path = dir.join(n);
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if meta.len() > MAX_SAVE {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let copy_start = format!("{stem}.sync-conflict-");
            let copy_end = format!(".{}", n.rsplit_once('.').map_or("", |(_, e)| e));
            out.push(Entry {
                rel,
                system: sys,
                stem: s,
                ext,
                crc: crcs.crc(&game),
                size: meta.len(),
                mtime: meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()),
                sha: sha256_hex(&bytes),
                vv: Vv::new(),
                asking: None,
                marks: Vec::new(),
                open_copy: names.iter().any(|c| c.starts_with(&copy_start) && c.ends_with(&copy_end)),
            });
        }
    }
    crcs.prune();
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

/// CRC32 of game files by path, size and time, so a scan reads a cartridge once (a 32 MB one took 0.16 s on the
/// RG35XX Plus, 17.11.2026).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CrcCache(pub BTreeMap<String, u32>);

impl CrcCache {
    pub fn load(path: &Path) -> CrcCache {
        let m = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<BTreeMap<String, u32>>(&b).ok())
            .unwrap_or_default();
        CrcCache(m)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::fsutil::write_atomic(path, serde_json::to_string(&self.0).unwrap_or_default().as_bytes())
    }

    /// The game file's CRC32; None for one larger than [`CRC_MAX`] or unreadable.
    pub fn crc(&mut self, p: &Path) -> Option<u32> {
        let m = std::fs::metadata(p).ok()?;
        if !m.is_file() || m.len() > CRC_MAX {
            return None;
        }
        let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let key = format!("{}|{}|{t}", p.display(), m.len());
        if let Some(c) = self.0.get(&key) {
            return Some(*c);
        }
        let c = crc32fast::hash(&std::fs::read(p).ok()?);
        self.0.insert(key, c);
        Some(c)
    }

    /// Forget game files that are gone or changed (keeps the cache the size of the library).
    pub fn prune(&mut self) {
        self.0.retain(|k, _| {
            let mut it = k.rsplitn(3, '|');
            let (Some(t), Some(len), Some(path)) = (it.next(), it.next(), it.next()) else { return false };
            std::fs::metadata(path).is_ok_and(|m| {
                let mt = m.modified().ok().and_then(|x| x.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                m.len().to_string() == len && mt.to_string() == t
            })
        });
    }
}

fn is_game(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(s, e)| {
        let e = e.to_ascii_lowercase();
        !s.is_empty() && !SAVE_EXTS.contains(&e.as_str()) && !NOT_GAMES.contains(&e.as_str())
    })
}

/// Every save of the roms folder (a system folder and one level of subfolders), with the CRC of its game file and
/// whether a `.sync-conflict-` copy is beside it. Vectors are empty: [`index`] adds them.
pub fn scan(layout: &Layout, crcs: &mut CrcCache) -> Vec<Entry> {
    if let Some(k) = layout.keyed() {
        return scan_keyed(k, crcs);
    }
    let mut out = Vec::new();
    let mut budget = BUDGET;
    let Ok(systems) = std::fs::read_dir(layout.save_root()) else { return out };
    let mut dirs: Vec<(PathBuf, String, String)> = systems
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_str()?.to_string();
            (e.file_type().ok()?.is_dir() && !n.starts_with('.') && !SKIP_DIRS.contains(&n.as_str()))
                .then(|| (e.path(), n.clone(), format!("{n}/")))
        })
        .collect();
    dirs.sort();
    while let Some((dir, system, prefix)) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut names = Vec::new();
        for e in rd.flatten() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let Some(n) = e.file_name().to_str().map(str::to_string) else { continue };
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                // One level of subfolders below a system (`psx/Final Fantasy VII/`), never more.
                if prefix.matches('/').count() == 1 && !n.starts_with('.') && !SKIP_DIRS.contains(&n.as_str()) {
                    dirs.push((e.path(), system.clone(), format!("{prefix}{n}/")));
                }
            } else if ft.is_file() {
                names.push(n);
            }
        }
        names.sort();
        for n in &names {
            let rel = format!("{prefix}{n}");
            let Some((_, stem, ext)) = split_rel(&rel) else { continue };
            let path = dir.join(n);
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if meta.len() > MAX_SAVE {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let crc = match &layout.saves {
                // The game in the games' folder at the same place.
                SavesAt::Root(_) => Path::new(&rel).parent().and_then(|p| game_named(&layout.roms.join(p), &stem)).and_then(|g| crcs.crc(&g)),
                _ => names.iter().find(|g| g.rsplit_once('.').is_some_and(|(s, _)| s == stem) && is_game(g)).and_then(|g| crcs.crc(&dir.join(g))),
            };
            let copy_start = format!("{stem}.sync-conflict-");
            let copy_end = format!(".{}", n.rsplit_once('.').map_or("", |(_, e)| e));
            let open_copy = names.iter().any(|c| c.starts_with(&copy_start) && c.ends_with(&copy_end));
            out.push(Entry {
                rel,
                system: system.clone(),
                stem,
                ext,
                crc,
                size: meta.len(),
                mtime: meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()),
                sha: sha256_hex(&bytes),
                vv: Vv::new(),
                asking: None,
                marks: Vec::new(),
                open_copy,
            });
        }
    }
    crcs.prune();
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

/// This device's index: [`scan`] with each save's record brought up to date ([`bookkeep`]) in `versions`. Records of
/// saves that are gone are kept (a save moved away and back keeps its history).
pub fn index(me: &str, layout: &Layout, versions: &mut Versions, crcs: &mut CrcCache) -> Vec<Entry> {
    let mut out = scan(layout, crcs);
    let writer = versions.writer(me).to_string();
    for e in &mut out {
        let r = bookkeep(&writer, versions.0.get(&e.rel), &e.sha, e.open_copy);
        e.vv = r.vv.clone();
        e.asking = r.asking.clone();
        e.marks = r.marks.clone();
        versions.0.insert(e.rel.clone(), r);
    }
    out
}

/// Where a peer's save of game `crc` goes here when we have no save for it yet: beside our game file with that CRC,
/// under our name for it (`gba/Pokemon Emerald (USA).srm` for the peer's `gba/Emerald.srm`), in the system folder or
/// one subfolder of it (the depth [`scan`] looks at). None: no such game here, or more than one (which, nobody can
/// say).
pub fn target(layout: &Layout, crcs: &mut CrcCache, system: &str, ext: &str, crc: u32) -> Option<String> {
    split_rel(&format!("{system}/x.{ext}"))?;
    // This device's folder for the system, in its own letters (`gba` here for a peer's `GBA`).
    let found = std::fs::read_dir(layout.roms.as_path()).ok().and_then(|rd| {
        let mut names: Vec<String> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).filter_map(|e| e.file_name().to_str().map(str::to_string)).collect();
        names.sort();
        names.iter().find(|n| *n == system).or_else(|| names.iter().find(|n| n.eq_ignore_ascii_case(system))).cloned()
    });
    let system = found.as_deref().unwrap_or(system);
    if let Some(k) = layout.keyed() {
        // Android: the games of that system in every ROMs folder (the folder in any letters: `GBA` here, `gba` there).
        let system = system.to_ascii_lowercase();
        let hits: Vec<String> = keyed_games(k)
            .into_iter()
            .filter(|(_, g, s)| *s == system && crcs.crc(g) == Some(crc))
            .filter_map(|(prefix, g, _)| Some(format!("{prefix}{}.{ext}", game_stem(&g)?)))
            .collect();
        return match hits.as_slice() {
            [one] if split_rel(one).is_some() => Some(one.clone()),
            _ => None,
        };
    }
    let root = layout.roms.join(system);
    let mut games: Vec<String> = Vec::new();
    let mut dirs = vec![(root.clone(), String::new())];
    while let Some((dir, sub)) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let Some(n) = e.file_name().to_str().map(str::to_string) else { continue };
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() && sub.is_empty() && !n.starts_with('.') && !SKIP_DIRS.contains(&n.as_str()) {
                dirs.push((e.path(), format!("{n}/")));
            } else if t.is_file() && is_game(&n) {
                games.push(format!("{sub}{n}"));
            }
        }
    }
    games.sort();
    let hits: Vec<String> = games
        .into_iter()
        .filter(|g| crcs.crc(&root.join(g)) == Some(crc))
        .filter_map(|g| g.rsplit_once('.').map(|(stem, _)| format!("{system}/{stem}.{ext}")))
        .collect();
    match hits.as_slice() {
        [one] if split_rel(one).is_some() => Some(one.clone()),
        _ => None,
    }
}

/// Another file is at `rel` here in other letters (`Game.SRM` for `Game.srm`): on the cards' FAT and exFAT they are one
/// file, so a save new here must not go there.
pub fn collides(layout: &Layout, rel: &str) -> bool {
    let Some(p) = layout.file(rel) else { return false };
    let (Some(dir), Some(name)) = (p.parent(), p.file_name().and_then(|n| n.to_str())) else { return false };
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
        e.file_name().to_str().is_some_and(|n| n != name && n.eq_ignore_ascii_case(name))
    })
}

/// The connection: pairing, the service, one sync with a paired Pocket (merge 2).
pub mod log;
pub mod net;

#[cfg(test)]
mod tests;

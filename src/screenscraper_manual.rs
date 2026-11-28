//! A game's real manual from ScreenScraper (`manuel`, docs/SCREENSCRAPER.md "Manuals"): asked for when the player
//! opens it (Details → Manual), one request (`mediaManuelJeu.php`), then kept on the card as its pages' pictures.
//!
//! * **Known without a request.** Whether a game has a manual is in the index already (`jeuInfos` lists `manuel` with
//!   the other media, [`Index`]): [`available`] reads it; the Details page shows the row only then.
//! * **Pages, not a PDF.** The PDF goes through `manual_pdf` once, on the loading thread: every page's JPEG is written
//!   out unchanged (`<data>/manuals/<system>/<rom stem>/001.jpg …`), then `pages.txt` (one line per page: `jpg W H`
//!   or `other`), which marks the manual as complete. The PDF itself is not kept.
//! * **The player's language first.** A German player gets the German manual where there is one, then the regions of
//!   the game file ([`region_order`]).
use super::*;

/// The manuals' folder under the data folder.
pub const FOLDER: &str = "manuals";
/// The largest manual taken (the study's largest was 10 MB; 32 to 72 pages).
pub const MAX_MANUAL_BYTES: u64 = 64 << 20;
const PAGES_FILE: &str = "pages.txt";
/// The longest `pages.txt` read (a line per page, `manual_pdf::MAX_PAGES` of them).
const MAX_PAGES_FILE: u64 = 64 << 10;

/// A game's own folder: `<data>/manuals/<system>/<rom stem>/`, a folder per region of the manual in it.
pub fn game_dir(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
    data_dir.join(FOLDER).join(system).join(stem)
}

/// A region as a folder name: two or three lowercase letters as ScreenScraper writes them ("us", "de", "wor"), anything
/// else "xx". The region comes from the network: it never becomes a path of its own ("../..", "/", a separator,
/// nothing, other letters; Sol 26.11.2026).
pub fn region_id(region: &str) -> &str {
    if (2..=3).contains(&region.len()) && region.bytes().all(|b| b.is_ascii_lowercase()) { region } else { "xx" }
}

/// Where the game's manual of `region` is kept, whether or not it is there yet: the cache belongs to the manual picked
/// (a player who switches to German gets the German one, Sol 26.11.2026).
pub fn dir_of(data_dir: &Path, system: &str, rom: &Path, region: &str) -> PathBuf {
    game_dir(data_dir, system, rom).join(region_id(region))
}

/// Removes the folder `path` only when it lies inside the manuals' folder of `data_dir` (both made canonical first);
/// anything else is refused, never removed.
fn remove_inside(data_dir: &Path, path: &Path) -> std::io::Result<()> {
    let root = std::fs::canonicalize(data_dir.join(FOLDER))?;
    let at = std::fs::canonicalize(path)?;
    if at == root || !at.starts_with(&root) {
        return Err(std::io::Error::other("not inside the manuals' folder"));
    }
    std::fs::remove_dir_all(at)
}

/// The data folder a manual's folder `dir` (`<data>/manuals/<system>/<stem>/<region>`) belongs to.
fn data_of(dir: &Path) -> Option<&Path> {
    dir.ancestors().nth(4)
}

/// Where a manual of `region` that can't be read is marked: that one is not fetched again, the game's other regions
/// still are ([`available`]); with every region marked the row is left out ([`listed`]).
pub fn unreadable_mark(data_dir: &Path, system: &str, rom: &Path, region: &str) -> PathBuf {
    game_dir(data_dir, system, rom).join(format!("{}.unreadable", region_id(region)))
}

/// One page of a kept manual.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kept {
    /// The page's picture and its size.
    Jpeg(PathBuf, u32, u32),
    /// A page pocket-ui can't show (not one picture).
    Other,
}

/// The pages of the game's manual of `region` on the card; None while it is not (complete) there. Reads a small file:
/// on the loading thread.
pub fn kept(data_dir: &Path, system: &str, rom: &Path, region: &str) -> Option<Vec<Kept>> {
    let dir = dir_of(data_dir, system, rom, region);
    let list = dir.join(PAGES_FILE);
    if crate::background_io::metadata(&list).ok()?.len() > MAX_PAGES_FILE {
        return None;
    }
    let text = crate::background_io::read_to_string(&list).ok()?;
    let pages: Vec<Kept> = text
        .lines()
        .take(crate::manual_pdf::MAX_PAGES)
        .enumerate()
        .map(|(i, l)| {
            let f: Vec<&str> = l.split_whitespace().collect();
            match f[..] {
                ["jpg", w, h] => match (w.parse(), h.parse()) {
                    (Ok(w), Ok(h)) => Kept::Jpeg(dir.join(page_name(i)), w, h),
                    _ => Kept::Other,
                },
                _ => Kept::Other,
            }
        })
        .collect();
    (!pages.is_empty()).then_some(pages)
}

fn page_name(i: usize) -> String {
    format!("{:03}.jpg", i + 1)
}

/// ScreenScraper's region names for the player's language: its own manual first (`sp` is Spanish, `br` Brazil's).
pub fn language_regions(lang: &str) -> &'static [&'static str] {
    match lang {
        "de" => &["de"],
        "fr" => &["fr"],
        "es" => &["sp"],
        "it" => &["it"],
        "nl" => &["nl"],
        "pt" => &["pt", "br"],
        "ja" => &["jp"],
        "ko" => &["kr"],
        "zh" => &["cn", "tw"],
        _ => &[],
    }
}

/// The manual to fetch: the player's language, then the file's regions and ScreenScraper's usual order, else any;
/// never one `skip` says no to (marked unreadable).
pub fn pick<'m>(medias: &'m [Media], lang: &str, rom_said: &[String], stem: &str, skip: &dyn Fn(&str) -> bool) -> Option<&'m Media> {
    let manuals: Vec<&Media> = medias.iter().filter(|m| m.kind == "manuel" && !skip(&m.region)).collect();
    let mut order: Vec<String> = language_regions(lang).iter().map(|r| r.to_string()).collect();
    order.extend(region_order(rom_said, stem));
    order.iter().find_map(|r| manuals.iter().find(|m| &m.region == r).copied()).or_else(|| manuals.first().copied())
}

/// What the index says of the game's manual: (ScreenScraper's system for it, the game, the manual to fetch). None:
/// ScreenScraper knows no manual for it (or none that can be read), or the game was not looked up yet. Reads the
/// system's index file: on the loading thread.
pub fn available(data_dir: &Path, system: &str, rom: &Path, lang: &str) -> Option<(u32, u64, Media)> {
    let stem = rom.file_stem().and_then(|s| s.to_str())?;
    let index = Index::load(data_dir, system);
    let (_, _, Entry::Found { game, system: game_system, medias, rom_regions, .. }) = index.files.get(stem)? else { return None };
    let marked = |r: &str| crate::background_io::exists(unreadable_mark(data_dir, system, rom, r));
    let media = pick(medias, lang, rom_regions, stem, &marked)?.clone();
    let sys = if *game_system != 0 { *game_system } else { system_id(system)? };
    Some((sys, *game, media))
}

/// The games of `system` ScreenScraper lists a manual for that is not marked unreadable, by ROM stem (the index,
/// `<data>/screenscraper/<system>.tsv`): for the Details row, read on a thread of its own.
pub fn listed(data_dir: &Path, system: &str) -> std::collections::HashSet<String> {
    Index::load(data_dir, system)
        .files
        .into_iter()
        .filter(|(stem, (_, _, e))| {
            // The stem as it is (a file name's stem may have dots of its own: "Game.v1").
            let game = data_dir.join(FOLDER).join(system).join(stem);
            matches!(e, Entry::Found { medias, .. } if medias.iter().any(|m| m.kind == "manuel" && !crate::background_io::exists(game.join(format!("{}.unreadable", region_id(&m.region))))))
        })
        .map(|(stem, _)| stem)
        .collect()
}

/// How fetching a manual ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// On the card now: its page count.
    Saved(usize),
    /// ScreenScraper has none after all (`NOMEDIA`, 404).
    Nothing,
    /// A PDF pocket-ui can't read (no page is one picture, encrypted, broken, over a limit of `manual_pdf`): that
    /// region is marked.
    Unreadable,
    /// No answer this time (offline, a broken transfer, a full card): tried again when opened again.
    NoAnswer,
    /// Stopped (B on the page, a game started).
    Stopped,
}

/// A name in the game's folder no other run and no region uses (`.part-…`, `.pdf-…`): two downloads of the same game,
/// or stems that differ only after a dot, never share one (Sol 26.11.2026).
fn staging(game_dir: &Path, what: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    game_dir.join(format!(".{what}-{}-{n}", std::process::id()))
}

/// Writes the pages of the PDF `bytes` into `dir` (by way of a staging folder beside it, so a half-written manual is
/// never read; it goes on any failure). None: not a manual pocket-ui can read (no page, or no page that is a picture,
/// or pages that together are more than the file: a PDF that uses one picture on many pages would write it again for
/// each, Sol 26.11.2026). The game's manuals of other regions go once this one is there.
pub fn save_pages(bytes: &[u8], dir: &Path) -> std::io::Result<Option<usize>> {
    let Some(pages) = crate::manual_pdf::pages(bytes) else { return Ok(None) };
    if !pages.iter().any(|p| matches!(p, crate::manual_pdf::Page::Jpeg { .. })) {
        return Ok(None);
    }
    let total: usize = pages.iter().map(|p| if let crate::manual_pdf::Page::Jpeg { len, .. } = p { *len } else { 0 }).sum();
    if total > bytes.len() {
        return Ok(None);
    }
    let (Some(game), Some(data)) = (dir.parent(), data_of(dir)) else { return Ok(None) };
    std::fs::create_dir_all(game)?;
    let part = staging(game, "part");
    let written = write_part(data, bytes, &pages, &part, dir);
    if written.is_err() {
        // A full card or a refused write: nothing half-written stays.
        let _ = remove_inside(data, &part);
    }
    if written.as_ref().is_ok_and(|n| n.is_some()) {
        // One manual per game on the card: the one of the other language goes; only region folders, only inside.
        for e in std::fs::read_dir(game).into_iter().flatten().flatten() {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if p != dir && region_id(&name) == name && p.is_dir() {
                let _ = remove_inside(data, &p);
            }
        }
    }
    written
}

fn write_part(data: &Path, bytes: &[u8], pages: &[crate::manual_pdf::Page], part: &Path, dir: &Path) -> std::io::Result<Option<usize>> {
    std::fs::create_dir_all(part)?;
    let mut list = String::new();
    for (i, p) in pages.iter().enumerate() {
        match *p {
            crate::manual_pdf::Page::Jpeg { start, len, width, height } => {
                std::fs::write(part.join(page_name(i)), &bytes[start..start + len])?;
                list.push_str(&format!("jpg {width} {height}\n"));
            }
            crate::manual_pdf::Page::Other => list.push_str("other\n"),
        }
    }
    std::fs::write(part.join(PAGES_FILE), list)?;
    if dir.exists() {
        remove_inside(data, dir)?;
    }
    std::fs::rename(part, dir)?;
    Ok(Some(pages.len()))
}

impl<'a> Client<'a> {
    /// Downloads the game's manual `media` (`mediaManuelJeu.php`) and keeps its pages in `dir`. Paced and counted like
    /// every request; a 429 is waited out as for a picture.
    /// `progress`: the bytes that came so far, as they come.
    pub fn manual(&mut self, system_id: u32, game: u64, media: &Media, dir: &Path, stop: &(dyn Fn() -> bool + Sync), progress: &(dyn Fn(u64) + Sync)) -> Result<Fetched, Refusal> {
        let params = [("systemeid", system_id.to_string()), ("jeuid", game.to_string()), ("media", format!("{}({})", media.kind, media.region))];
        let url = self.url("mediaManuelJeu.php", &params);
        let Some(game_dir) = dir.parent() else { return Ok(Fetched::NoAnswer) };
        let _ = std::fs::create_dir_all(game_dir);
        let pdf = staging(game_dir, "pdf");
        let mut busy = 0;
        loop {
            if !self.turn(stop)? {
                return Ok(Fetched::Stopped);
            }
            let _ = std::fs::remove_file(&pdf);
            let stopping = AtomicBool::new(stop());
            let fetched = {
                let _lane = LANE.lock().unwrap_or_else(|e| e.into_inner());
                // Booked in the day's ledger before it goes, like a picture; while the day's limit is not known yet,
                // as one of the few requests that may go without it.
                let kind = if self.limit_known() { Booking::Media } else { Booking::Ask };
                match self.reserve(kind) {
                    Ok(()) => {}
                    Err(NotBooked::Refused(r)) => return Err(r),
                    Err(NotBooked::GameHeld) => return Ok(Fetched::NoAnswer),
                }
                if stop() {
                    return Ok(Fetched::Stopped);
                }
                let fetched = self.net.fetch(&url, "", &pdf, MAX_MANUAL_BYTES, &stopping, &|n| {
                    progress(n);
                    if stop() {
                        stopping.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                });
                // A miss or the day's end (430, 431) in the day's file before the next request may be booked, as for a
                // picture (a manual's answer has no `ssuser`; a 403 is told apart below).
                let status = match fetched {
                    crate::romm::Fetched::Status(s) => s,
                    crate::romm::Fetched::Done => 200,
                    _ => 0,
                };
                let refusal = (status != 403).then(|| refusal_for(status, b"", self.login.is_some())).flatten();
                self.answered(if status == 404 { 404 } else { 0 }, refusal, b"");
                fetched
            };
            use crate::romm::Fetched as F;
            let got = match fetched {
                F::Done if stop() => Fetched::Stopped,
                F::Done => match std::fs::read(&pdf) {
                    Ok(bytes) if bytes.starts_with(b"%PDF") => match save_pages(&bytes, dir) {
                        Ok(Some(n)) => Fetched::Saved(n),
                        Ok(None) => {
                            let _ = std::fs::write(game_dir.join(format!("{}.unreadable", region_id(&media.region))), b"");
                            Fetched::Unreadable
                        }
                        Err(_) => Fetched::NoAnswer,
                    },
                    Ok(bytes) if bytes.trim_ascii().starts_with(b"NOMEDIA") => Fetched::Nothing,
                    _ => Fetched::NoAnswer,
                },
                F::Status(429) if busy < BUSY_RETRIES => {
                    busy += 1;
                    if !self.clock.wait(BUSY_WAIT, stop) {
                        let _ = std::fs::remove_file(&pdf);
                        return Ok(Fetched::Stopped);
                    }
                    continue;
                }
                F::Status(429) => {
                    let _ = std::fs::remove_file(&pdf);
                    return Err(Refusal::Busy);
                }
                F::Status(403) => {
                    let _ = std::fs::remove_file(&pdf);
                    return Err(Refusal::DevRefused);
                }
                F::Status(status) => {
                    let _ = std::fs::remove_file(&pdf);
                    // The day's end (430, 431) is in the day's ledger already (`answered` above, Sol 26.11.2026):
                    // opening the manual again sends nothing more today.
                    if let Some(r) = refusal_for(status, b"", self.login.is_some()) {
                        return Err(r);
                    }
                    if status == 404 { Fetched::Nothing } else { Fetched::NoAnswer }
                }
                F::Stopped => Fetched::Stopped,
                _ => Fetched::NoAnswer,
            };
            let _ = std::fs::remove_file(&pdf);
            return Ok(got);
        }
    }
}

/// One manual opened at a time, from its download to its pages on the card: a B and A again on the same game waits for
/// the first run to end instead of writing beside it (Sol 26.11.2026).
static PUBLISH: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What opening a game's manual came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opened {
    /// Its pages, on the card (now).
    Pages(Vec<Kept>),
    /// Not on the card and no network to fetch it.
    Offline,
    /// It ended otherwise ([`Fetched`]), or ScreenScraper refused.
    Ended(Result<Fetched, Refusal>),
}

/// Opens the game's manual on the caller's thread (the App's loading thread): the one for the player's language from
/// the card, else fetched now (one request: the developer id, the player's account if any, the day's ledger).
/// `online`: whether a download can be tried.
pub fn open(net: &dyn Net, data_dir: &Path, system: &str, rom: &Path, lang: &str, online: bool, stop: &(dyn Fn() -> bool + Sync), progress: &(dyn Fn(u64) + Sync)) -> Opened {
    let _one = PUBLISH.lock().unwrap_or_else(|e| e.into_inner());
    let Some((sys, game, media)) = available(data_dir, system, rom, lang) else { return Opened::Ended(Ok(Fetched::Nothing)) };
    if let Some(pages) = kept(data_dir, system, rom, &media.region) {
        return Opened::Pages(pages);
    }
    if !online {
        return Opened::Offline;
    }
    let Some(dev) = dev() else { return Opened::Ended(Ok(Fetched::NoAnswer)) };
    let clock = RealClock::new();
    let Some(mut client) = Client::new(net, &clock, API, dev, login(data_dir)) else { return Opened::Ended(Ok(Fetched::NoAnswer)) };
    client.keep_day(data_dir);
    match client.manual(sys, game, &media, &dir_of(data_dir, system, rom, &media.region), stop, progress) {
        Ok(Fetched::Saved(_)) => match kept(data_dir, system, rom, &media.region) {
            Some(pages) => Opened::Pages(pages),
            None => Opened::Ended(Ok(Fetched::NoAnswer)),
        },
        other => Opened::Ended(other),
    }
}

#[cfg(test)]
#[path = "screenscraper_manual_tests.rs"]
mod tests;

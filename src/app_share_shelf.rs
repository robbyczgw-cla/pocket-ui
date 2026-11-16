//! Part of the pocket-ui binary: "Share my shelf" (`pocket_ui::share_shelf`). Gathers the player's games as cases
//! (which game, its box art's file, its stickers: a few lookups, nothing read) and hands them to a thread of its own that
//! reads the box art, paints the picture with the library module, encodes it and writes it where the screenshots go
//! (`<screenshots>/shelf/pocket-ui shelf <time>.png`), so the gallery lists it, the file share carries it and the web
//! companion offers it. The UI thread only gathers: it never waits for a cover. A shelf with no game says so instead
//! of making an empty picture.
//!
//! The way in is the first row of Settings → Games → Screenshots (`app_gallery`): a row on a page that is already there,
//! reached with Up from the first screenshot, A makes the picture.

use std::path::PathBuf;
use std::time::Instant;

use pocket_ui::media::{Medium, Prop};
use pocket_ui::room_deco::Earned;
use pocket_ui::share_shelf::{self, Candidate, Entry, Error, Room};

use super::App;

impl App {
    /// Every game the shelf shows: `(system, game)` indexes, not the hidden systems', not hidden games, not what the
    /// device brings itself (PortMaster's own launcher, Steam).
    fn share_games(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for s in pocket_ui::shelf::visible_systems(self.systems.iter().map(|s| s.name.as_str()), &self.hidden) {
            let sys = &self.systems[s];
            for (g, game) in sys.games.iter().enumerate() {
                if !pocket_ui::library::supplied_by_device(&sys.name, &game.path) && !self.library.is_hidden(&game.path) {
                    out.push((s, g));
                }
            }
        }
        out
    }

    /// The cases to put on the picture, in the order they stand: the games `pick` chose. `covers`: false leaves the box
    /// art out (the printed title cards: what a shelf whose games have no pictures looks like).
    pub(crate) fn share_shelf_entries(&mut self, max: usize, covers: bool) -> Vec<Entry> {
        let games = self.share_games();
        let mut candidates: Vec<Candidate> = Vec::with_capacity(games.len());
        for &(s, g) in &games {
            let (sys, path, title) = (self.systems[s].name.clone(), self.systems[s].games[g].path.clone(), self.systems[s].games[g].name.clone());
            candidates.push(Candidate {
                favourite: self.favorites.contains(&path),
                stickers: !self.case_stickers_of(&path).is_empty(),
                cover: covers && self.cover_file(&sys, &path).is_some(),
                recent: self.recents.list.iter().position(|r| r.path == path),
                system: sys,
                title,
            });
        }
        let chosen = share_shelf::pick(&candidates, max);
        let mut out = Vec::with_capacity(chosen.len());
        for i in chosen {
            let (s, g) = games[i];
            let (name, title, rom) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
            out.push(Entry {
                medium: Medium::of(&name),
                stripe: super::stripe(&name),
                title,
                system_name: super::system_short(&self.systems, &name),
                console: pocket_ui::system_art::get(&name),
                cover_file: if covers { self.cover_file(&name, &rom) } else { None },
                cover: None,
                stickers: self.case_stickers_of(&rom),
                seed: super::app_case_stickers::seed(&rom),
                system: name,
            });
        }
        out
    }

    /// The pieces of the room that stand after the cases: the trophy and the framed "100 %" the player earned, the plant
    /// at its stage (the shelves' own Decorations setting decides what counts as earned).
    fn share_shelf_deco(&mut self) -> (Vec<Prop>, Earned) {
        let earned = self.room_earned();
        let mut deco = pocket_ui::room_props::deco_for(&earned, None, true);
        deco.push(Prop::Plant);
        (deco, (*earned).clone())
    }

    /// A on the page's first row: the picture is made and written on a thread; `Event::Shared` says how it went.
    pub(crate) fn share_shelf_make(&mut self) {
        let entries = self.share_shelf_entries(share_shelf::MAX_CASES, true);
        let Some(page) = self.gallery_page_mut() else { return };
        if entries.is_empty() {
            page.failed(Error::Empty.words());
            return;
        }
        page.making(true);
        let (deco, earned) = self.share_shelf_deco();
        let path = target(&pocket_ui::screenshot::Stamp::now());
        let (proxy, sync, blend) = (self.proxy.clone(), self.pictures.sync, pocket_ui::palette::pal().t);
        let job = move || write_picture(entries, &deco, &earned, blend, &path);
        if sync {
            let result = job();
            self.gallery_event(super::app_gallery::Event::Shared(result));
            return;
        }
        let spawned = std::thread::Builder::new().name("share-shelf".into()).spawn(move || {
            let _ = proxy.send_event(super::UserEvent::Gallery(super::app_gallery::Event::Shared(job())));
        });
        if spawned.is_err() {
            self.gallery_event(super::app_gallery::Event::Shared(Err(Error::Failed)));
        }
    }

    /// `--snapshot share-shelf[:<n>][:plain]`: the picture of the shelf (the first `n` games `pick` takes; `plain`: no
    /// box art, as a shelf whose games have none) as a PNG at `out`, 1440 x 1440.
    pub(crate) fn share_shelf_snapshot(&mut self, state: &str, out: &str) {
        let mut max = share_shelf::MAX_CASES;
        let mut covers = true;
        for part in state.split(':').skip(1) {
            match part {
                "plain" => covers = false,
                n => max = n.parse().unwrap_or(max),
            }
        }
        let entries = self.share_shelf_entries(max, covers);
        let (deco, earned) = self.share_shelf_deco();
        match picture_png(entries, &deco, &earned, pocket_ui::palette::pal().t) {
            Ok(png) => std::fs::write(out, png).expect("write the picture"),
            Err(why) => eprintln!("share-shelf: {}", why.words()),
        }
    }
}

/// The picture as a PNG, the work of the thread: the box art is read, the cases painted with this thread's own fonts and
/// the page's palette (`blend`: 0 dark, 255 light, the UI thread's at the press).
fn picture_png(mut entries: Vec<Entry>, deco: &[Prop], earned: &Earned, blend: u8) -> Result<Vec<u8>, Error> {
    let t0 = Instant::now();
    pocket_ui::palette::set_blend(blend);
    let mut text = pocket_ui::text::Text::new();
    share_shelf::read_covers(&mut entries);
    let read = t0.elapsed();
    let shot = share_shelf::compose(&mut text, &entries, &Room { deco, earned })?;
    let composed = t0.elapsed();
    let png = shot.png().ok_or(Error::Failed)?;
    println!("share-shelf: {} cases, covers read in {read:?}, painted in {composed:?}, encoded in {:?}, {} bytes", entries.len(), t0.elapsed(), png.len());
    Ok(png)
}

/// [`picture_png`], written to `path` (its folder made) the way screenshots are: atomically. What went wrong goes to the
/// log; the page says only that it could not be saved (a card that is full or read-only).
fn write_picture(entries: Vec<Entry>, deco: &[Prop], earned: &Earned, blend: u8, path: &std::path::Path) -> Result<(), Error> {
    let png = picture_png(entries, deco, earned, blend)?;
    let wrote = match path.parent() {
        Some(dir) => std::fs::create_dir_all(dir).and_then(|()| pocket_ui::fsutil::write_atomic(path, &png)),
        None => pocket_ui::fsutil::write_atomic(path, &png),
    };
    wrote.map_err(|e| {
        eprintln!("share-shelf: can't write {}: {e}", path.display());
        Error::Failed
    })
}

/// Where the picture goes: the screenshots folder's `shelf/` (the gallery and the web companion list it from there).
pub(crate) fn target(stamp: &pocket_ui::screenshot::Stamp) -> PathBuf {
    pocket_ui::screenshot::target(&pocket_ui::screenshot::dir(), share_shelf::FOLDER, share_shelf::NAME, stamp)
}

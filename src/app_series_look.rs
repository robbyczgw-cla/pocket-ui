//! A series' cover and its spine's logo on the Shelf look by series (`pocket_ui::series_look`, docs/SERIES-SHELF.md
//! "Its cover and its spine"). A child of `app_cabinet`: it uses the cabinet's reader thread and caches.
//!
//! - **The cover** (`series_front`): the series' games in rank order (`App::series_ranked`: key games first), the
//!   first four with box art on the card (the library's own pictures, nothing is downloaded for it), each read as a
//!   game's front is (`cover_label`, `case_look`, on the reader thread) and made into its case; stacked once all are
//!   read, and kept per series and size until new covers come (`CabinetUi::covers_changed`). Until then the plain front.
//! - **The logo** (`series_logo`): ScreenScraper's `wheel` of the first of the series' first [`LOGO_LOOK`] ranked games
//!   that has one on the card, shown while Real spines is On; read with its alpha and stood up on the reader thread.

use std::rc::Rc;
use std::time::Instant;

use pocket_ui::cabinet_art::Pic;
use pocket_ui::media::{self, Medium};
use pocket_ui::screenscraper::Face;
use pocket_ui::series_look::{collage, COLLAGE_MAX, LOGO_LOOK};

use std::path::PathBuf;

use super::{Info, Kind, Slot};
use crate::{App, Item};

/// Series' fronts kept at once (`keep_series_front`).
const FRONTS_KEEP: usize = 24;

/// What a series' front shows now.
pub(super) enum SeriesFront {
    /// No game of it has box art: the plain front with its name, as before.
    Plain,
    /// Its games' covers are being read: the plain front until they are in.
    Reading,
    /// Its games' cases stacked.
    Cases(Rc<Pic>),
}

impl App {
    /// Series `i`'s front at `k` times the shelf size of `m` (module docs).
    pub(super) fn series_front(&mut self, i: usize, m: Medium, k: f32) -> SeriesFront {
        let Some(shelf) = self.series.shelves.get(i).filter(|s| !s.alone) else { return SeriesFront::Plain };
        let (fw, fh) = media::front_size(m, k);
        let key = (shelf.name.clone(), fh);
        if let Some(done) = self.cabinet.series_fronts.get(&key) {
            return done.clone().map_or(SeriesFront::Plain, SeriesFront::Cases);
        }
        // Being stacked: nothing to do until it is in (no case made again in each frame meanwhile; Sol's recheck).
        if self.cabinet.series_composing.contains(&key) {
            return SeriesFront::Reading;
        }
        let ranked = self.series_ranked(i);
        let names: Vec<String> = ranked.iter().map(|&it| self.case_info(it).title).collect();
        let picks = pocket_ui::series_look::cover_games(
            &(0..ranked.len()).collect::<Vec<_>>(),
            |k| pocket_ui::series_look::title_key(&names[k]),
            |k| {
                let info = self.case_info(ranked[k]);
                // A cover whose cut can't be read is passed over (the next with box art takes its place).
                self.cover_file(&info.system, &info.rom).is_some_and(|f| !self.cabinet.labels.iter().any(|((p, _, _), s)| *p == f && matches!(s, Slot::Missing)))
            },
        );
        let picked: Vec<(Item, Info, PathBuf)> = picks
            .into_iter()
            .filter_map(|k| {
                let info = self.case_info(ranked[k]);
                let file = self.cover_file(&info.system, &info.rom)?;
                Some((ranked[k], info, file))
            })
            .collect();
        // As large as the stack lets each case be (`series_look::collage`): made at that size, so it is not scaled twice.
        let (slot_w, slot_h) = if picked.len() <= 1 { (fw as f32, fh as f32) } else { (fw as f32 * 0.56, fh as f32 * 0.80) };
        let mut parts = Vec::new();
        let mut reading = false;
        for (it, info, file) in &picked {
            let mg = self.medium(*it);
            let (gw, gh) = media::front_size(mg, 1.0);
            let kg = (slot_w / gw.max(1) as f32).min(slot_h / gh.max(1) as f32);
            let lh = (mg.size().h as f32 * kg).round() as i32;
            let held = (file.clone(), mg, lh);
            let label = match self.cabinet.series_labels.get(&held) {
                Some(l) => Some(l.clone()),
                None => {
                    let l = self.cover_label(file, mg, lh);
                    if let Some(l) = &l {
                        self.cabinet.series_labels.insert(held, l.clone());
                    }
                    l
                }
            };
            let look = self.case_look(info, mg);
            let colours_in = matches!(self.cabinet.colors.get(file), Some(Slot::Ready(_) | Slot::Missing));
            match label {
                Some(label) if colours_in => parts.push((mg, kg, look, label, info.system.clone(), info.title.clone())),
                // A cover that can't be read is left out; one still being read is waited for.
                None if matches!(self.cabinet.labels.get(&(file.clone(), mg, lh)), Some(Slot::Missing)) => {}
                _ => reading = true,
            }
        }
        if reading {
            return SeriesFront::Reading;
        }
        // All in: the held cuts are not needed any more.
        for (_, _, file) in &picked {
            self.cabinet.series_labels.retain(|(f, _, _), _| f != file);
        }
        if parts.is_empty() {
            self.keep_series_front(key, None);
            return SeriesFront::Plain;
        }
        let t0 = Instant::now();
        let fronts: Vec<Pic> = parts
            .iter()
            .map(|(mg, kg, look, label, system, title)| {
                let sprite = pocket_ui::system_art::get(system);
                let name = crate::system_short(&self.systems, system);
                media::front(&mut self.text, *mg, *kg, *look, &media::printed_title(title, false), Some(label), sprite, &name)
            })
            .collect();
        self.cabinet.made("series cover's cases", t0);
        // Stacked (each case turned, with its shadow: several milliseconds on a Brick) on a thread of its own, once;
        // a snapshot at once.
        if self.cabinet.inline {
            let refs: Vec<&Pic> = fronts.iter().collect();
            let pic = Rc::new(collage(&refs, fw, fh));
            self.keep_series_front(key, Some(pic.clone()));
            return SeriesFront::Cases(pic);
        }
        self.cabinet.series_composing.insert(key.clone());
        self.cabinet.pending += 1;
        let done = self.cabinet.series_composed.clone();
        let generation = self.cabinet.series_gen;
        let failed = key.clone();
        let spawned = std::thread::Builder::new().name("series-cover".into()).spawn(move || {
            // Whatever happens, it reports back: the shelf stops waiting for it (`pending`).
            let pic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let refs: Vec<&Pic> = fronts.iter().collect();
                collage(&refs, fw, fh)
            }))
            .ok();
            if let Ok(mut done) = done.lock() {
                done.push((key, generation, pic));
            }
        });
        if spawned.is_err() {
            // No thread: the plain front, for good (until the series' covers change).
            self.cabinet.pending = self.cabinet.pending.saturating_sub(1);
            self.cabinet.series_composing.remove(&failed);
            self.keep_series_front(failed, None);
            return SeriesFront::Plain;
        }
        SeriesFront::Reading
    }

    /// Series covers the thread finished: kept, and the shelf shows them with its next frame (`cabinet_poll`).
    pub(super) fn series_fronts_poll(&mut self) {
        let done: Vec<_> = match self.cabinet.series_composed.lock() {
            Ok(mut d) => std::mem::take(&mut *d),
            Err(_) => return,
        };
        for (key, generation, pic) in done {
            if self.cabinet.series_composing.remove(&key) {
                self.cabinet.pending = self.cabinet.pending.saturating_sub(1);
            }
            // Made from covers that changed since: made again when next shown. One that failed: the plain front.
            if generation == self.cabinet.series_gen {
                self.keep_series_front(key, pic.map(Rc::new));
            }
        }
    }

    /// Keeps a series' front; past [`FRONTS_KEEP`] the oldest go (made again when shown: a few series' worth of memory).
    fn keep_series_front(&mut self, key: (String, i32), front: Option<Rc<Pic>>) {
        if self.cabinet.series_fronts.len() >= FRONTS_KEEP {
            self.cabinet.series_fronts.clear();
        }
        self.cabinet.series_fronts.insert(key, front);
    }

    /// Series `i`'s logo, stood up, once it is read; None while Real spines is Off, for a series without one on the card
    /// and until it is read (module docs).
    pub(super) fn series_logo(&mut self, i: usize) -> Option<Rc<Pic>> {
        if !pocket_ui::series_look::SERIES_LOGOS || !self.screenscraper_shows(Face::BoxSide) {
            return None;
        }
        let name = self.series.shelves.get(i).filter(|s| !s.alone)?.name.clone();
        let file = match self.cabinet.logo_files.get(&name) {
            Some(f) => f.clone(),
            None => {
                let data_dir = self.data_dir.clone();
                let any = *self.cabinet.logo_dir.get_or_insert_with(|| pocket_ui::background_io::exists(data_dir.join(Face::Wheel.folder())));
                let f = if any {
                    self.series_logo_order(i).into_iter().take(LOGO_LOOK).find_map(|it| {
                        let info = self.case_info(it);
                        // One that can't be read (broken, all see-through) leaves the next its place.
                        pocket_ui::screenscraper::existing(&data_dir, Face::Wheel, &info.system, &info.rom).filter(|f| !matches!(self.cabinet.logos.get(f), Some(Slot::Missing)))
                    })
                } else {
                    None
                };
                self.cabinet.logo_files.insert(name.clone(), f.clone());
                f
            }
        }?;
        if !self.cabinet.logos.contains_key(&file) {
            self.want(&file, Kind::Logo);
        }
        match self.cabinet.logos.get(&file) {
            Some(Slot::Ready(logo)) => Some(logo.clone()),
            // Not readable: the next game's logo is looked for.
            Some(Slot::Missing) => {
                self.cabinet.logo_files.remove(&name);
                None
            }
            _ => None,
        }
    }

    /// For tests/series_shelf.rs, after a snapshot by series: what each series' cover and spine show, a line each:
    /// `series-look <name>|cases=<n>|plain|logo` (`cases=0`: the plain front; `name`: the spine's printed name).
    pub(crate) fn series_look_report(&mut self) {
        for i in 0..self.series.shelves.len() {
            if self.series.shelves[i].alone {
                continue;
            }
            let cases = match self.series_front(i, Medium::Keep, 1.0) {
                SeriesFront::Cases(_) => {
                    let ranked = self.series_ranked(i);
                    ranked.into_iter().filter(|&it| { let info = self.case_info(it); self.cover_file(&info.system, &info.rom).is_some() }).count().min(COLLAGE_MAX)
                }
                SeriesFront::Plain => 0,
                SeriesFront::Reading => usize::MAX,
            };
            let spine = if self.series_logo(i).is_some() { "logo" } else { "name" };
            println!("series-look {}|cases={cases}|{spine}", self.series.shelves[i].name);
        }
    }

    /// Whether `item` is a series whose front is its stack of cases (made already): it casts no case's shadow of its own.
    pub(super) fn series_stacked(&self, item: Item, fh: i32) -> bool {
        let Item::Series(i) = item else { return false };
        let Some(shelf) = self.series.shelves.get(i) else { return false };
        matches!(self.cabinet.series_fronts.get(&(shelf.name.clone(), fh)), Some(Some(_)))
    }

    /// `series_logo` of an entry that is a series.
    pub(super) fn series_logo_of(&mut self, item: Item) -> Option<Rc<Pic>> {
        match item {
            Item::Series(i) => self.series_logo(i),
            _ => None,
        }
    }
}

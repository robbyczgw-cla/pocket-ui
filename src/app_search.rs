//! Part of the pocket-ui binary: finding games fast. The game search (`pocket_ui::search_page`, the Search row at the
//! top of Home) and the letter jump of the lists (`pocket_ui::letters`, L2 and R2), with what they need from the App:
//! the index made on a thread after the first frame, covers read on a thread of their own, painting and touch.
//!
//! Nothing here runs on a keypress beyond the filter (`search::Index::search`, well under a millisecond for 5,000
//! games on a desktop) and painting what is on screen.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::image::{self, Image};
use pocket_ui::keyboard::{Key, Special};
use pocket_ui::letters::{self, Edge, Flash, Triggers};
use pocket_ui::pad::{Button, PadEvent};
use pocket_ui::palette::*;
use pocket_ui::search::{Hit, Index, Recent};
use pocket_ui::search_page::{Action, Focus, SearchPage};
use pocket_ui::settings::Theme;
use pocket_ui::text::Face;
use pocket_ui::touch::{Area, Gesture};
use winit::event_loop::ActiveEventLoop;

use super::{stripe, system_short, window_start, App, Item, Screen, UserEvent, CANCEL, CONFIRM};

/// The query field under the heading.
const FIELD_Y: i32 = 100;
const FIELD_H: i32 = 84;
/// Under "No game matches …": a game that isn't on the SD card is the likelier reason before a typo, which the search
/// mostly forgives (the first-day run, 2026-10-26, searched "mario kart" on a card without it and was told to spell).
const NO_MATCH: &str = "It may not be on the SD card. Or check the spelling, or type less of the name.";
/// Where the list starts, and where the keyboard starts while it is shown: measured from the screen's bottom edge,
/// so the keyboard stays above the hints on a shorter screen (612 on 1080 lines, 492 on the Mini V1's 960).
const LIST_TOP: i32 = 208;
const KB_FROM_BOTTOM: i32 = 1080 - 612;

fn kb_top(h: i32) -> i32 {
    h - KB_FROM_BOTTOM
}
const KEY_H: i32 = 66;
const KEY_GAP: i32 = 8;
/// Rows of results while typing (four fit over the keyboard) and in the results (the keyboard stepped aside).
const TYPING_ROW: (i32, i32) = (88, 72);
const RESULTS_ROW: (i32, i32) = (100, 84);
const ROW_GAP: i32 = 8;
/// Rows of the recent searches: 60 px around Title 37, taller with Settings → Text size (`type_scale`).
fn recent_row() -> i32 {
    pocket_ui::type_scale::row(60, 37)
}
/// The letter of a jump: a square this big in the middle of the screen.
const FLASH: i32 = 260;
/// Covers kept while the search is open; more and the oldest go (they are read again when needed).
const THUMBS_MAX: usize = 240;

enum Slot {
    Pending,
    Ready(Rc<Image>),
    Missing,
}

/// Reads covers on its own thread, newest request first; shrunk to the size asked for.
#[derive(Clone)]
struct Reader {
    queue: Arc<(Mutex<VecDeque<Job>>, Condvar)>,
    done: Arc<Mutex<Vec<Read>>>,
}

/// A cover by ROM and size; a job (the cover's possible files); what was read.
type ThumbKey = (PathBuf, i32);
type Job = (ThumbKey, Vec<PathBuf>);
type Read = (ThumbKey, Option<Image>);

impl Reader {
    fn start() -> Option<Reader> {
        let r = Reader { queue: Default::default(), done: Default::default() };
        let worker = r.clone();
        std::thread::Builder::new()
            .name("search-covers".into())
            .spawn(move || loop {
                let (key, candidates) = {
                    let (lock, cv) = &*worker.queue;
                    let mut q = lock.lock().unwrap();
                    loop {
                        if let Some(job) = q.pop_front() {
                            break job;
                        }
                        q = cv.wait(q).unwrap();
                    }
                };
                let img = read_thumb(&candidates, key.1);
                worker.done.lock().unwrap().push((key, img));
            })
            .ok()?;
        Some(r)
    }

    fn ask(&self, job: Job) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap().push_front(job);
        cv.notify_one();
    }

    fn forget(&self) {
        self.queue.0.lock().unwrap().clear();
    }

    fn take(&self) -> Vec<Read> {
        std::mem::take(&mut *self.done.lock().unwrap())
    }
}

/// The first of `candidates` that is there, fitted into `size` x `size`.
fn read_thumb(candidates: &[PathBuf], size: i32) -> Option<Image> {
    let path = candidates.iter().find(|p| p.exists())?;
    let img = image::load_image(path)?;
    let (w, h) = img.fit(size, size);
    Some(img.resized(w, h))
}

/// The search, the letter jump, and what they keep between frames.
#[derive(Default)]
pub(crate) struct FindUi {
    /// The open search.
    pub page: Option<SearchPage>,
    index: Option<Arc<Index>>,
    building: Option<mpsc::Receiver<Index>>,
    /// The index has been asked for (after the first frame, or when the library changed).
    asked: bool,
    /// Read on the first opening; tiny.
    recent: Option<Recent>,
    triggers: Triggers,
    flash: Option<Flash>,
    reader: Option<Reader>,
    thumbs: HashMap<ThumbKey, Slot>,
    /// Where the last frame put the keys and the rows, for taps.
    keys: Vec<(Area, usize, usize)>,
    rows: Vec<(Area, usize)>,
    /// Snapshots read covers at once rather than on the thread.
    inline: bool,
}

impl FindUi {
    pub fn is_open(&self) -> bool {
        self.page.is_some()
    }
}

impl App {
    // ---- the index ------------------------------------------------------------------------------------------

    /// Called as the loop turns: once the first frame is out, the index is made on a thread; what it and the cover
    /// reader bring is taken in.
    pub(crate) fn find_tick(&mut self) {
        if self.drew_first && !self.find.asked {
            self.find_build_index();
        }
        let mut changed = false;
        if let Some(rx) = &self.find.building {
            match rx.try_recv() {
                Ok(index) => {
                    let index = Arc::new(index);
                    self.find.index = Some(index.clone());
                    self.find.building = None;
                    if let Some(page) = self.find.page.as_mut() {
                        page.set_index(index);
                        changed = true;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.find.building = None,
            }
        }
        if let Some(reader) = self.find.reader.as_ref().filter(|_| self.find.page.is_some()) {
            for (key, img) in reader.take() {
                self.find.thumbs.insert(key, img.map_or(Slot::Missing, |i| Slot::Ready(Rc::new(i))));
                changed = true;
            }
        }
        if changed && self.find.is_open() {
            self.request_redraw();
        }
    }

    /// Makes the index of every game on a thread; the search uses it once it is there.
    fn find_build_index(&mut self) {
        self.find.asked = true;
        let names: Vec<(usize, usize, String)> =
            self.systems.iter().enumerate().flat_map(|(s, sys)| sys.games.iter().enumerate().map(move |(g, game)| (s, g, game.name.clone()))).collect();
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("search-index".into()).spawn(move || {
            let started = Instant::now();
            let index = Index::build(names.iter().map(|(s, g, n)| (*s, *g, n.as_str())));
            println!("search: index of {} games made in {:?}", index.len(), started.elapsed());
            let _ = tx.send(index);
        });
        match spawned {
            Ok(_) => self.find.building = Some(rx),
            Err(e) => eprintln!("search: no thread for the index: {e}"),
        }
    }

    /// The library was read again: the index is made anew (an open search fills in when it is there).
    pub(crate) fn find_library_changed(&mut self) {
        self.find.index = None;
        self.find.building = None;
        self.find_build_index();
        // New names may bring new Japanese letters (`app_library`).
        self.library_prewarm_names();
    }

    /// Something moves: the letter of a jump, or an open search waits for its index or covers.
    pub(crate) fn find_animating(&self) -> bool {
        let now = Instant::now();
        self.find.flash.is_some_and(|f| f.needs_frames(now))
            || (self.find.is_open() && (self.find.building.is_some() || self.find.thumbs.values().any(|s| matches!(s, Slot::Pending))))
    }

    /// A fingerprint of the search and the jump, for the UI sounds and the haptic tick.
    pub(crate) fn find_mark(&self) -> Option<(Focus, usize, usize, usize, usize, usize)> {
        self.find.page.as_ref().map(SearchPage::mark)
    }

    // ---- the letter jump ------------------------------------------------------------------------------------

    /// L2/R2 arrive as travel on the Mini V2: on the shelf and its pages they become presses (and a pull reported
    /// both ways counts once). None: the event is used up.
    pub(crate) fn trigger_event(&mut self, ev: UserEvent) -> Option<UserEvent> {
        let UserEvent::Pad(pe) = &ev else { return Some(ev) };
        // In a game they are the game's (and pocket-core's shortcuts).
        if self.running.is_some() {
            self.find.triggers.reset();
            return Some(ev);
        }
        let source = match pe {
            PadEvent::Axis { source, .. } | PadEvent::Button { source, .. } => *source,
            _ => return Some(ev),
        };
        match self.find.triggers.feed(pe) {
            Edge::Other => Some(ev),
            Edge::Drop => None,
            Edge::Button(button, pressed) => Some(UserEvent::Pad(PadEvent::Button { source, button, pressed })),
        }
    }

    /// The group a row sorts under for the jump: its first letter, `letters::TOP` for Home's rows above the systems.
    fn letter_group(&self, item: Item) -> char {
        match item {
            Item::Game(s, g) => letters::initial(&self.systems[s].games[g].name),
            Item::System(s) => letters::initial(&self.systems[s].fullname),
            _ => letters::TOP,
        }
    }

    /// L2 (`forward` false) or R2: to the previous or next first letter, with the letter shown large.
    pub(crate) fn letter_jump(&mut self, forward: bool) {
        if self.theme == Theme::Cabinet {
            // The cabinet's L1/R1 already jump by letter along a shelf (`cabinet::step`).
            let before = self.items.get(self.sel).copied();
            self.cabinet_step(if forward { Button::R1 } else { Button::L1 });
            let now = self.items.get(self.sel).copied();
            if now != before {
                self.find.flash = now.and_then(|i| Flash::new(self.letter_group(i), Instant::now()));
            }
            return;
        }
        // Home's Search row is not part of the alphabet.
        let first = Item::first_row(&self.items);
        let groups: Vec<char> = self.items[first..].iter().map(|i| self.letter_group(*i)).collect();
        if let Some(to) = letters::jump(&groups, self.sel.saturating_sub(first), forward) {
            self.select(first + to);
            self.find.flash = Flash::new(groups[to], Instant::now());
        }
    }

    /// The letter of the last jump, large in the middle, fading out.
    pub(crate) fn paint_letter_flash(&mut self, c: &mut Canvas) {
        let Some(flash) = self.find.flash else { return };
        let Some(alpha) = flash.alpha(Instant::now()) else {
            self.find.flash = None;
            return;
        };
        let (x, y) = ((c.w - FLASH) / 2, (c.h - FLASH) / 2 - 20);
        // The inverse of the page, like the selected row of the Minimal look: it reads over spines and labels alike.
        // Painted whole and then mixed over the page as one layer, so box and letter fade together into what is
        // behind them (mixing the letter's colour towards the box's left a bright letter on a faded box).
        let (back, ink) = (pal().text, pal().void);
        let letter = flash.letter.to_string();
        let w = self.text.width(Face::Title, 200, &letter);
        let text = &mut self.text;
        pocket_ui::motion::layer(c, x, y, FLASH, FLASH, alpha, |c| {
            blend_round_rect(c, x, y, FLASH, FLASH, 44, back, 240);
            text.draw(c, Face::Title, 200, x + (FLASH - w) / 2, y + FLASH / 2 + 70, ink, &letter);
        });
    }

    // ---- the search -----------------------------------------------------------------------------------------

    /// A on the Search row (or Up on the cabinet's top shelf): the search, with the keyboard and the recent searches.
    pub(crate) fn open_search(&mut self) {
        // A direction still held on the shelf would go on moving it underneath.
        self.held = None;
        if !self.find.asked {
            self.find_build_index();
        }
        self.find_tick();
        if self.find.reader.is_none() && !self.find.inline {
            self.find.reader = Reader::start();
        }
        let recent = self.find.recent.get_or_insert_with(|| Recent::load(&self.data_dir)).clone();
        let shown = self.systems.iter().map(|s| !self.hidden.contains(&s.name)).collect();
        self.find.page = Some(SearchPage::new(recent, self.find.index.clone(), shown));
    }

    pub(crate) fn close_search(&mut self) {
        self.find.page = None;
        self.find.thumbs.clear();
        self.find.keys.clear();
        self.find.rows.clear();
        if let Some(reader) = &self.find.reader {
            reader.forget();
        }
    }

    /// A button while the search is open.
    pub(crate) fn search_input(&mut self, b: Button) {
        let Some(page) = self.find.page.as_mut() else { return };
        let before = page.kb.text.clone();
        let action = page.press(b, CONFIRM, CANCEL);
        if page.kb.text != before {
            println!("search: {:?} → {} games in {:.2} ms", page.kb.text, page.hits.len(), page.took.as_secs_f64() * 1000.0);
        }
        self.search_action(action);
    }

    fn search_action(&mut self, action: Action) {
        // A search to remember: kept for the next opening and saved (a few hundred bytes).
        if let Some(page) = self.find.page.as_mut().filter(|p| p.recent_changed) {
            page.recent_changed = false;
            page.recent.save(&self.data_dir);
            self.find.recent = Some(page.recent.clone());
        }
        match action {
            Action::Nothing => {}
            Action::Refuse => pocket_ui::ui_sound::refuse(),
            Action::Close => self.close_search(),
            Action::Play(hit) => {
                let Some((system, name, path)) = self.hit_game(hit) else { return };
                self.launch(system, name, path);
            }
            Action::Star(hit) => {
                let Some((system, name, path)) = self.hit_game(hit) else { return };
                let starred = self.favorites.toggle(&system, &name, &path);
                self.toast = Some((if starred { "Added to Favourites" } else { "Removed from Favourites" }.to_string(), Instant::now()));
                // Home's Favorites row comes or goes with it.
                self.rebuild();
            }
            Action::Details(hit) => {
                let Some((system, name, path)) = self.hit_game(hit) else { return };
                // The card opens over the search; B on it comes back here.
                self.show_card(Item::Game(hit.system, hit.game), system, name, path);
            }
        }
    }

    /// (system id, name, ROM) of a result.
    fn hit_game(&self, hit: Hit) -> Option<(String, String, PathBuf)> {
        let sys = self.systems.get(hit.system)?;
        let game = sys.games.get(hit.game)?;
        Some((sys.name.clone(), game.name.clone(), game.path.clone()))
    }

    /// A game starts while the search is open (A on a result, or Play on its card): the search closes, and the shelf
    /// under the launch fade is the game's system with the game selected, which is where B leads after the game.
    pub(crate) fn search_launching(&mut self, path: &Path) {
        if !self.find.is_open() {
            return;
        }
        self.close_search();
        let found = self.systems.iter().enumerate().find_map(|(s, sys)| sys.games.iter().position(|g| g.path == path).map(|g| (s, g)));
        let Some((s, g)) = found else { return };
        self.home_item = Some(Item::System(s));
        self.screen = Screen::Games(s);
        self.rebuild();
        if let Some(at) = self.items.iter().position(|i| *i == Item::Game(s, g)) {
            self.sel = at;
            self.prev_sel = at;
            // Fully pulled out, as a selection at rest.
            self.moved_at = Instant::now() - Duration::from_secs(1);
        }
    }

    /// The cover of a game for a result row, read on the thread; None while it is coming or when there is none.
    fn search_thumb(&mut self, system: &str, rom: &Path, size: i32) -> Option<Rc<Image>> {
        let key = (rom.to_path_buf(), size);
        if let Some(slot) = self.find.thumbs.get(&key) {
            return match slot {
                Slot::Ready(img) => Some(img.clone()),
                _ => None,
            };
        }
        if self.find.thumbs.len() >= THUMBS_MAX {
            self.find.thumbs.retain(|_, s| matches!(s, Slot::Pending));
        }
        let mut candidates = vec![pocket_ui::covers::cover_path(&self.data_dir, system, rom)];
        if system == "ports" {
            let launcher = rom.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            candidates.extend(pocket_ui::ports::launcher_images(&launcher));
        }
        // After pocket-ui's own: the picture in the player's EmulationStation gamelist (`app_gamelist`).
        candidates.extend(self.gamelist_cover(system, rom));
        match (&self.find.reader, self.find.inline) {
            (Some(reader), false) => {
                reader.ask((key.clone(), candidates));
                self.find.thumbs.insert(key, Slot::Pending);
                None
            }
            _ => {
                let img = read_thumb(&candidates, size).map(Rc::new);
                self.find.thumbs.insert(key, img.clone().map_or(Slot::Missing, Slot::Ready));
                img
            }
        }
    }

    pub(crate) fn paint_search(&mut self, c: &mut Canvas) {
        let Some(page) = self.find.page.take() else { return };
        self.paint_search_page(c, &page);
        self.find.page = Some(page);
    }

    fn paint_search_page(&mut self, c: &mut Canvas, page: &SearchPage) {
        c.clear(pal().void);
        self.find.keys.clear();
        self.find.rows.clear();
        let right = c.w - MARGIN;
        let width = c.w - 2 * MARGIN;
        self.paint_top_line(c, Some("Search"), pal().dust);
        let typing = page.focus == Focus::Keys;

        // The query: paper while it is being typed, a spine while the results have the buttons.
        let (field, ink, dim) = if typing { (pal().label, pal().ink, pal().label_dust) } else { (pal().spine, pal().chalk, pal().dust) };
        c.rect(MARGIN, FIELD_Y, width, FIELD_H, field);
        let count = if page.waiting() {
            "Getting the list ready…".to_string()
        } else if page.showing_recent() || page.hits.is_empty() {
            // Nothing typed, or nothing found (the list says so in words).
            String::new()
        } else {
            // Only the closest matches (a typo in the first letter was forgiven): said quietly, where the count is.
            match (page.hits.len(), page.closest) {
                (1, false) => "1 game".to_string(),
                (n, false) => format!("{n} games"),
                (1, true) => "1 close match".to_string(),
                (n, true) => format!("{n} close matches"),
            }
        };
        let count_w = if count.is_empty() { 0 } else { self.text.width(Face::Body, 28, &count) + 28 };
        if !count.is_empty() {
            self.text.draw_right(c, Face::Body, 28, right - 28, FIELD_Y + 54, dim, &count);
        }
        let room = width - 56 - count_w;
        let text_x = MARGIN + 28;
        let w = if page.kb.text.is_empty() {
            self.text.draw(c, Face::Title, 50, text_x, FIELD_Y + 62, dim, "Any game, by its name");
            0
        } else {
            // A long query shows its end, where the typing is.
            let shown = tail_fit(&mut self.text, &page.kb.text, room);
            self.text.draw(c, Face::Title, 50, text_x, FIELD_Y + 62, ink, &shown)
        };
        if typing {
            c.rect(text_x + w + 4, FIELD_Y + 16, 4, 52, ink);
        }

        let bottom = if typing { kb_top(c.h) - 16 } else { c.h - 110 };
        if page.showing_recent() {
            self.paint_recent(c, page, bottom);
        } else if page.hits.is_empty() && !page.waiting() {
            // A magnifying glass by an empty box (`ui_art`), right of the words: the keyboard may take the room below.
            let k = pocket_ui::ui_art::empty_scale(c.h);
            let (aw, ah) = pocket_ui::ui_art::size(pocket_ui::ui_art::Art::Search, k);
            let width = if LIST_TOP + 16 + ah <= bottom && aw * 3 < width {
                pocket_ui::ui_art::paint(c, pocket_ui::ui_art::Art::Search, c.w - MARGIN - aw, LIST_TOP + 16, k);
                width - aw - 40
            } else {
                width
            };
            let q = self.text.fit(Face::Title, 37, &format!("No game matches “{}”", page.kb.text.trim()), width);
            self.text.draw(c, Face::Title, 37, MARGIN, LIST_TOP + 60, pal().chalk, &q);
            let mut y = LIST_TOP + 106 + pocket_ui::type_scale::grow(28);
            for line in self.text.wrap_at_most(Face::Body, 28, NO_MATCH, width, 2) {
                self.text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, &line);
                y += pocket_ui::type_scale::line(28, 10);
            }
        } else {
            self.paint_hits(c, page, bottom);
        }
        if typing {
            self.paint_search_keys(c, page);
        }

        let hints: Vec<(&str, &str)> = match (page.focus, page.selected_hit()) {
            (Focus::Keys, _) => vec![("A", "Type"), ("B", if page.kb.text.is_empty() { "Close" } else { "Delete" }), ("START", "Results")],
            (Focus::Results, Some(hit)) => {
                let starred = self.hit_game(hit).is_some_and(|(_, _, path)| self.favorites.contains(&path));
                vec![("A", "Play"), ("Y", if starred { "Unstar" } else { "Star" }), ("X", "Details"), ("B", "Keyboard")]
            }
            (Focus::Results, None) => vec![("A", "Search"), ("B", "Keyboard")],
        };
        self.paint_hint_row(c, &hints);
        self.paint_toast(c);
    }

    /// Nothing typed yet: the recent searches, or a word on what can be typed.
    fn paint_recent(&mut self, c: &mut Canvas, page: &SearchPage, bottom: i32) {
        let width = c.w - 2 * MARGIN;
        if page.recent.list.is_empty() {
            self.text.draw(c, Face::Title, 37, MARGIN, LIST_TOP + 60, pal().chalk, "Type part of a game's name.");
            let games: usize = self.systems.iter().filter(|s| !self.hidden.contains(&s.name)).map(|s| s.games.len()).sum();
            let line = format!("Every game on the shelf is searched, {games} in all. Small typos are fine.");
            let line = self.text.fit(Face::Body, 28, &line, width);
            self.text.draw(c, Face::Body, 28, MARGIN, LIST_TOP + 106 + pocket_ui::type_scale::grow(28), pal().dust, &line);
            return;
        }
        self.text.draw(c, Face::Body, 28, MARGIN, LIST_TOP + 30, pal().dust, "Recent searches");
        let top = LIST_TOP + 52 + pocket_ui::type_scale::grow(28);
        let pitch = recent_row() + ROW_GAP;
        let selected = (page.focus == Focus::Results).then_some(page.sel);
        for (i, q) in page.recent.list.iter().enumerate() {
            let y = top + i as i32 * pitch;
            if y + recent_row() > bottom {
                break;
            }
            let on = selected == Some(i);
            c.rect(MARGIN, y, width, recent_row(), if on { pal().label } else { pal().spine });
            let q = self.text.fit(Face::Title, 37, q, width - 56);
            self.text.draw(c, Face::Title, 37, MARGIN + 28, y + recent_row() / 2 + pocket_ui::type_scale::base(13, 37), if on { pal().ink } else { pal().chalk }, &q);
            self.find.rows.push((Area::new(MARGIN, y, width, recent_row()), i));
        }
        self.hits.list(page.sel, page.recent.list.len(), 0.0, pitch as f32);
    }

    /// The games found, a window of them around the selection.
    fn paint_hits(&mut self, c: &mut Canvas, page: &SearchPage, bottom: i32) {
        let typing = page.focus == Focus::Keys;
        let (row_h, thumb) = if typing { TYPING_ROW } else { RESULTS_ROW };
        let pitch = row_h + ROW_GAP;
        let visible = ((bottom - LIST_TOP + ROW_GAP) / pitch).max(1) as usize;
        let first = window_start(page.sel, visible, page.hits.len());
        // The results scroll smoothly under their edges (`app_motion`); new results start at rest.
        let key = (page.hits.len() as u64) << 1 | typing as u64;
        let first_f = self.motion.scroll_at(5, key, first, Instant::now());
        let w = c.w - 2 * MARGIN;
        let mut band = c.rows(LIST_TOP, LIST_TOP + visible as i32 * pitch - ROW_GAP);
        for (i, off) in super::app_motion::rows_in_view(first_f, visible, page.hits.len()) {
            let y = (off * pitch as f32).round() as i32;
            let selected = !typing && i == page.sel;
            self.paint_hit(&mut band, page.hits[i], y, row_h, thumb, selected);
            self.find.rows.push((Area::new(MARGIN, LIST_TOP + y, w, row_h), i));
            self.motion.row(MARGIN, LIST_TOP + y, w, pitch);
        }
        self.hits.list(page.sel, page.hits.len(), 0.0, pitch as f32);
        // More below or above: a thin bar on the right, as on the game card.
        if page.hits.len() > visible {
            let track = (visible as i32 * pitch - ROW_GAP).max(1);
            let h = (track * visible as i32 / page.hits.len() as i32).max(24);
            let y = LIST_TOP + (track - h) * first as i32 / (page.hits.len() - visible).max(1) as i32;
            c.round_rect(c.w - MARGIN + 18, y, 4, h, 2, pal().chip);
        }
    }

    /// One result: the system's stripe, the cover, the name (with its star), the system on a badge at the right.
    fn paint_hit(&mut self, c: &mut Canvas, hit: Hit, y: i32, h: i32, thumb: i32, selected: bool) {
        let Some((system, name, path)) = self.hit_game(hit) else { return };
        let (x, w) = (MARGIN, c.w - 2 * MARGIN);
        let colour = stripe(&system);
        let (bg, fg, sub) = if selected { (pal().label, pal().ink, pal().label_dust) } else { (pal().spine, pal().chalk, pal().dust) };
        c.rect(x, y, w, h, bg);
        if selected {
            pocket_ui::game_card::lift_shadow(c, x, y + h, w, pal().t);
        }
        c.rect(x, y, STRIPE, h, colour);
        // The cover, or a tile in the system's colour with the first letter while there is none (or it is coming).
        let (tx, ty) = (x + STRIPE + 14, y + (h - thumb) / 2);
        match self.search_thumb(&system, &path, thumb) {
            Some(img) => {
                let (iw, ih) = (img.w as i32, img.h as i32);
                c.blit_scaled(&img, tx + (thumb - iw) / 2, ty + (thumb - ih) / 2, iw, ih);
            }
            None => {
                c.rect(tx, ty, thumb, thumb, mix(colour, bg, 150));
                let letter = letters::initial(&name).to_string();
                let lw = self.text.width(Face::Title, 37, &letter);
                self.text.draw(c, Face::Title, 37, tx + (thumb - lw) / 2, ty + thumb / 2 + pocket_ui::type_scale::base(13, 37), sub, &letter);
            }
        }
        // The system badge: a dot of its colour and its short name.
        let short = system_short(&self.systems, &system);
        let short = self.text.fit(Face::Body, 21, &short, pocket_ui::type_scale::wide(220, 21));
        let sw = self.text.width(Face::Body, 21, &short);
        let (bw, bh) = (sw + 52, pocket_ui::type_scale::row(36, 21));
        let bx = x + w - 20 - bw;
        let by = y + (h - bh) / 2;
        c.round_rect(bx, by, bw, bh, bh / 2, if selected { mix(pal().label, pal().ink, 24) } else { pal().keycap });
        c.disc(bx + 20, by + bh / 2, 6, colour);
        self.text.draw(c, Face::Body, 21, bx + 34, by + bh / 2 + pocket_ui::type_scale::base(8, 21), if selected { pal().ink } else { pal().chalk }, &short);
        let title_x = tx + thumb + 22;
        let fav = self.favorites.contains(&path);
        self.draw_title(c, Face::Title, 37, title_x, y + h / 2 + pocket_ui::type_scale::base(13, 37), bx - 20 - title_x, fg, &name, selected, fav);
    }

    /// The keyboard under the results while typing.
    fn paint_search_keys(&mut self, c: &mut Canvas, page: &SearchPage) {
        let width = c.w - 2 * MARGIN;
        let key_w = (width - KEY_GAP * 9) / 10;
        for (r, row) in page.kb.rows().iter().enumerate() {
            let y = kb_top(c.h) + r as i32 * (KEY_H + KEY_GAP);
            let widths: Vec<i32> = row
                .iter()
                .map(|k| match k {
                    Key::Special(Special::Space) => 4 * key_w + 3 * KEY_GAP,
                    Key::Special(_) => 2 * key_w + KEY_GAP,
                    Key::Char(_) => key_w,
                })
                .collect();
            let total: i32 = widths.iter().sum::<i32>() + KEY_GAP * (widths.len() as i32 - 1);
            let mut x = (c.w - total) / 2;
            for (col, key) in row.iter().enumerate() {
                let on = r == page.kb.row && col == page.kb.col;
                c.rect(x, y, widths[col], KEY_H, if on { pal().label } else { pal().spine });
                let (label, face, px) = match key {
                    Key::Char(ch) => (ch.to_string(), Face::Title, 37),
                    Key::Special(Special::Space) => ("Space".to_string(), Face::Body, 24),
                    Key::Special(Special::Delete) => ("Delete".to_string(), Face::Body, 24),
                    Key::Special(_) => ("Results".to_string(), Face::Body, 24),
                };
                let lw = self.text.width(face, px, &label);
                self.text.draw(c, face, px, x + (widths[col] - lw) / 2, y + KEY_H / 2 + pocket_ui::type_scale::base(12, px), if on { pal().ink } else { pal().chalk }, &label);
                self.find.keys.push((Area::new(x, y, widths[col], KEY_H), r, col));
                x += widths[col] + KEY_GAP;
            }
        }
    }

    /// A toast over the search ("Added to Favourites"), as on the shelf.
    fn paint_toast(&mut self, c: &mut Canvas) {
        if let Some((msg, at)) = self.toast.clone() {
            if at.elapsed() < super::TOAST {
                let w = self.text.width(Face::Body, 28, &msg) + 48;
                c.round_rect(c.w / 2 - w / 2, 36, w, 52, 26, KEYCAP);
                self.text.draw(c, Face::Body, 28, c.w / 2 - w / 2 + 24, 72, CHALK, &msg);
            }
        }
    }

    /// Touch on the search: a key types, a row is selected and then started, a swipe moves the list. False when the
    /// touch is for the rest of the screen (the hints at the bottom).
    pub(crate) fn search_touch(&mut self, el: &ActiveEventLoop, gesture: Option<Gesture>) -> bool {
        let Some(page) = self.find.page.as_mut() else { return false };
        match gesture {
            Some(Gesture::Tap { x, y }) => {
                if let Some(&(_, r, col)) = self.find.keys.iter().find(|(a, _, _)| a.contains(x, y)) {
                    // As if the cursor were on it and A pressed (with its sound and tick).
                    page.focus = Focus::Keys;
                    page.kb.row = r;
                    page.kb.col = col;
                    self.tap_button(el, CONFIRM);
                    return true;
                }
                if let Some(&(_, i)) = self.find.rows.iter().find(|(a, _)| a.contains(x, y)) {
                    if page.focus == Focus::Results && page.sel == i {
                        self.tap_button(el, CONFIRM);
                    } else {
                        let action = page.tap_row(i, CONFIRM, CANCEL);
                        self.search_action(action);
                        self.request_redraw();
                    }
                    return true;
                }
                false
            }
            // The list follows the finger: a swipe up shows what is further down.
            Some(Gesture::Scroll { steps, vertical: true }) => {
                page.scroll(-steps);
                self.request_redraw();
                true
            }
            _ => false,
        }
    }

    /// `--snapshot search:<home|continue>[:<look>]`, `search:<fresh|empty|typing|results|none>[:<text>[:<downs>]]` and
    /// `--snapshot letters:<system>:<R2 presses>[:<look>]`: the search screen, Home's Search row, the letter of a jump.
    pub(crate) fn find_snapshot(&mut self, state: &str) -> bool {
        if let Some(rest) = state.strip_prefix("letters:") {
            let mut parts = rest.split(':');
            let id = parts.next().unwrap_or_default();
            let presses: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1);
            self.theme = snapshot_theme(parts.next());
            let Some(s) = self.systems.iter().position(|sys| sys.name == id) else { return false };
            self.screen = Screen::Games(s);
            self.rebuild();
            self.sel = 0;
            for _ in 0..presses {
                self.letter_jump(true);
            }
            self.prev_sel = self.sel;
            self.moved_at = Instant::now() - Duration::from_secs(2);
            return true;
        }
        let Some(rest) = state.strip_prefix("search:") else { return false };
        let mut parts = rest.split(':');
        let mode = parts.next().unwrap_or_default();
        self.find.inline = true;
        let index = Index::build(self.systems.iter().enumerate().flat_map(|(s, sys)| sys.games.iter().enumerate().map(move |(g, game)| (s, g, game.name.as_str()))));
        self.find.index = Some(Arc::new(index));
        self.find.asked = true;
        // "home": Home with the Search row selected; "continue": Home as it opens, on Continue under it.
        if mode == "home" || mode == "continue" {
            self.theme = snapshot_theme(parts.next());
            self.screen = Screen::Home;
            self.rebuild();
            // The Search row is under Curator's when that is on top (`shelf::Item::with_curator`).
            self.sel = if mode == "home" { self.items.iter().position(|i| *i == Item::Search).unwrap_or(0) } else { Item::first_row(&self.items) };
            self.prev_sel = self.sel;
            self.moved_at = Instant::now() - Duration::from_secs(2);
            return true;
        }
        let text = parts.next().unwrap_or_default().to_string();
        let downs: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        // The recent searches the snapshot shows (not the ones saved in POCKETUI_DATA).
        self.find.recent = Some(if mode == "fresh" {
            Recent::default()
        } else {
            Recent { list: ["zelda", "mario kart", "pokemon crystal", "castlevania", "metroid"].iter().map(|s| s.to_string()).collect() }
        });
        self.open_search();
        let Some(page) = self.find.page.as_mut() else { return false };
        for ch in text.chars() {
            page.kb.text.push(ch);
        }
        // One keystroke filters what was typed.
        page.kb.text.push(' ');
        page.press(CANCEL, CONFIRM, CANCEL);
        // The cursor on the last letter typed, as after typing it.
        if let Some(last) = text.chars().last() {
            let rows = page.kb.rows();
            if let Some((r, col)) = rows.iter().enumerate().find_map(|(r, row)| row.iter().position(|k| *k == Key::Char(last)).map(|col| (r, col))) {
                page.kb.row = r;
                page.kb.col = col;
            }
        }
        if mode == "results" {
            page.press(Button::Start, CONFIRM, CANCEL);
            for _ in 0..downs {
                page.press(Button::Down, CONFIRM, CANCEL);
            }
        }
        true
    }

    /// Hint for the cabinet's top shelf, where Up opens the search.
    pub(crate) fn search_hint(&self) -> Option<(&'static str, &'static str)> {
        (self.theme == Theme::Cabinet && self.cabinet_cursor().is_some_and(|(s, _)| s == 0) && !self.systems.is_empty()).then_some(("UP", "Search"))
    }
}

fn snapshot_theme(name: Option<&str>) -> Theme {
    match name.unwrap_or("shelf") {
        "minimal" => Theme::Minimal,
        "split" => Theme::Split,
        "cards" => Theme::Cards,
        "cabinet" => Theme::Cabinet,
        _ => Theme::Shelf,
    }
}

/// The end of `s` that fits in `max` pixels, with an ellipsis in front when something was left out.
fn tail_fit(text: &mut pocket_ui::text::Text, s: &str, max: i32) -> String {
    if text.width(Face::Title, 50, s) <= max {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    for start in 1..chars.len() {
        let tail: String = std::iter::once('…').chain(chars[start..].iter().copied()).collect();
        if text.width(Face::Title, 50, &tail) <= max {
            return tail;
        }
    }
    String::new()
}

/// A rounded rectangle mixed over what is there with `alpha` (0–255).
#[allow(clippy::too_many_arguments)]
fn blend_round_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32, alpha: u8) {
    let r = r.min(w / 2).min(h / 2);
    for dy in 0..h {
        // How far this row is inset at the corners.
        let from_edge = if dy < r { r - dy } else if dy >= h - r { dy - (h - r - 1) } else { 0 };
        let inset = if from_edge > 0 { r - ((r * r - from_edge * from_edge) as f32).sqrt() as i32 } else { 0 };
        for dx in inset..w - inset {
            c.blend(x + dx, y + dy, color, alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_match_says_the_game_may_not_be_on_the_card_in_two_lines_at_most() {
        assert!(NO_MATCH.contains("not be on the SD card"));
        let de = pocket_ui::i18n::german().translate(NO_MATCH).expect("a German line");
        let mut text = pocket_ui::text::Text::new();
        text.set_translate(false);
        for s in [NO_MATCH, de.as_ref()] {
            // As painted: wrapped to the page's width (1240 less the margins), two lines at most, nothing left over.
            let lines = text.wrap(Face::Body, 28, s, 1240 - 2 * MARGIN);
            assert!(lines.len() <= 2, "{s:?} takes {} lines", lines.len());
        }
    }
}

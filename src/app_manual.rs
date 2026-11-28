//! Part of the pocket-ui binary: the game's real manual (`pocket_ui::manual_view`, `screenscraper::manual`;
//! docs/SCREENSCRAPER.md "Manuals").
//!
//! * **The row.** Details shows "Manual" only for a game ScreenScraper lists one for (its index, read on a thread of
//!   its own the first time a card of that system opens: [`ManualsUi::listed`]), only while ScreenScraper and Real
//!   manuals are on. Without one the page is exactly as before.
//! * **Opening it.** A manual on the card opens at once; one that is not, is fetched now (one request) on a thread,
//!   the page saying so; offline it says that it needs Wi-Fi. B leaves at any time and stops the download.
//! * **The pages** are decoded on threads (`manual_view::decode`), this page first; the UI thread only paints.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use pocket_ui::game_card_model::{CuratorRow, RowKind};
use pocket_ui::gfx::Canvas;
use pocket_ui::image::Image;
use pocket_ui::manual_view::{Action, ManualView};
use pocket_ui::pad::Button;
use pocket_ui::screenscraper::manual::{self as manual, Fetched};

use super::{App, UserEvent, CANCEL, CONFIRM};

#[derive(Debug)]
pub(crate) enum ManualEvent {
    /// The games of `system` ScreenScraper lists a manual for (by ROM stem).
    Listed { system: String, stems: HashSet<String> },
    /// Opening the manual ended: its pages, or why not (`manual::open`).
    Opened { generation: u64, result: manual::Opened },
    /// The bytes of the open manual's download so far.
    Progress { generation: u64, bytes: u64 },
    /// A page decoded (None: the file did not read).
    Page { generation: u64, index: usize, img: Option<Arc<Image>> },
}

struct Open {
    view: ManualView,
    system: String,
    rom: PathBuf,
    cancel: Arc<AtomicBool>,
    decoding: HashSet<usize>,
    generation: u64,
}

#[derive(Default)]
pub(crate) struct ManualsUi {
    /// By system: the ROM stems with a manual listed; a system whose index is being read is in `asked`.
    listed: HashMap<String, Arc<HashSet<String>>>,
    asked: HashSet<String>,
    open: Option<Open>,
    generation: u64,
}

/// Pages decoding right now, in the whole program: at most [`pocket_ui::manual_view::KEEP`], also when B and A open
/// the manual again while the last ones still decode (Sol 26.11.2026). Each thread gives its slot back when it ends.
static DECODING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The player's language as ScreenScraper's manuals are named (`screenscraper::manual::language_regions`).
fn lang() -> String {
    format!("{:?}", pocket_ui::i18n::current()).to_lowercase()
}

impl App {
    /// Real manuals may be shown: ScreenScraper on, its row Real manuals on, in a build that has it, pictures not
    /// switched off.
    fn game_manuals_on(&self) -> bool {
        self.screenscraper.row.on && self.screenscraper.row.show.manuals && pocket_ui::screenscraper::shown() && !self.art_off
    }

    /// Reads which games of `system` have a manual, once, on a thread; the open card gets its row when it comes.
    pub(crate) fn game_manual_listing(&mut self, system: &str) {
        if !self.game_manuals_on() || self.manuals.listed.contains_key(system) || !self.manuals.asked.insert(system.to_string()) {
            return;
        }
        // Snapshots are settled pictures: read at once.
        if self.pictures.sync {
            let stems = manual::listed(&self.data_dir, system);
            self.game_manual_event(ManualEvent::Listed { system: system.to_string(), stems });
            return;
        }
        let (proxy, data_dir, system) = (self.proxy.clone(), self.data_dir.clone(), system.to_string());
        std::thread::spawn(move || {
            let stems = manual::listed(&data_dir, &system);
            let _ = proxy.send_event(UserEvent::Manual(ManualEvent::Listed { system, stems }));
        });
    }

    /// A ScreenScraper run ended: the index may list manuals it did not, read it again when a card of a system opens.
    pub(crate) fn game_manuals_index_changed(&mut self) {
        self.manuals.listed.clear();
    }

    /// The card's "Manual" row: only for a game ScreenScraper lists a manual for.
    pub(crate) fn game_card_manual_row(&self, system: &str, path: &Path) -> Option<CuratorRow> {
        if !self.game_manuals_on() {
            return None;
        }
        let stem = path.file_stem()?.to_str()?;
        self.manuals.listed.get(system)?.contains(stem).then(|| CuratorRow { kind: RowKind::Manual, title: "Game manual".into(), line: "The original, page by page".into() })
    }

    pub(crate) fn game_manual_open(&self) -> bool {
        self.manuals.open.is_some() && self.running.is_none()
    }

    /// A on the card's "Manual" row.
    pub(crate) fn game_open_manual_for_card(&mut self) {
        let Some(open) = self.card.as_ref() else { return };
        let (system, rom, title) = (open.system.clone(), open.path.clone(), open.name.clone());
        self.manuals.generation += 1;
        let generation = self.manuals.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut view = ManualView::loading(&title);
        // Everything happens on the loading thread (`manual::open`): the index, the card's list of pages, the download;
        // the page says it is getting the manual until then.
        if self.pictures.sync {
            // Snapshots send nothing: the page as it waits (`POCKETUI_SNAPSHOT_MANUAL=loading`), or as it opens offline.
            if !std::env::var("POCKETUI_SNAPSHOT_MANUAL").is_ok_and(|v| v == "loading") {
                let net = pocket_ui::romm_net::system();
                let opened = manual::open(net.as_ref(), &self.data_dir, &system, &rom, &lang(), false, &|| false, &|_| {});
                Self::game_manual_opened(&mut view, opened);
            }
        } else {
            let (proxy, data_dir, system, rom, cancel) = (self.proxy.clone(), self.data_dir.clone(), system.clone(), rom.clone(), cancel.clone());
            let lang = lang();
            std::thread::spawn(move || {
                pocket_ui::web::lower_priority();
                let net = pocket_ui::romm_net::system();
                let stop = || cancel.load(Ordering::Relaxed) || pocket_ui::background_io::cancelled();
                // The bytes so far, at most about seven times a second (each one repaints the page).
                let last = std::sync::Mutex::new(None::<std::time::Instant>);
                let progress_proxy = proxy.clone();
                let progress = move |bytes: u64| {
                    let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
                    if last.is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(150)) {
                        *last = Some(std::time::Instant::now());
                        let _ = progress_proxy.send_event(UserEvent::Manual(ManualEvent::Progress { generation, bytes }));
                    }
                };
                let online = pocket_ui::wifi::ip().is_some();
                let result = manual::open(net.as_ref(), &data_dir, &system, &rom, &lang, online, &stop, &progress);
                let _ = proxy.send_event(UserEvent::Manual(ManualEvent::Opened { generation, result }));
            });
        }
        println!("manual: {system} {} opened", rom.display());
        self.manuals.open = Some(Open { view, system, rom, cancel, decoding: HashSet::new(), generation });
        self.game_manual_decode();
        self.request_redraw();
    }

    /// What opening came to, on the page.
    fn game_manual_opened(view: &mut ManualView, result: manual::Opened) {
        match result {
            manual::Opened::Pages(pages) => view.set_pages(pages),
            manual::Opened::Offline => view.failed("Needs Wi-Fi: the manual comes from ScreenScraper"),
            manual::Opened::Ended(Ok(Fetched::Nothing | Fetched::Saved(_))) => view.failed("ScreenScraper has no manual for this game after all"),
            manual::Opened::Ended(Ok(Fetched::Unreadable)) => view.failed("This manual can't be read here"),
            manual::Opened::Ended(Ok(Fetched::NoAnswer)) => view.failed("No answer from ScreenScraper. Try again later"),
            manual::Opened::Ended(Ok(Fetched::Stopped)) => {}
            manual::Opened::Ended(Err(why)) => view.failed(why.message()),
        }
    }

    fn game_manual_close(&mut self) {
        if let Some(open) = self.manuals.open.take() {
            open.cancel.store(true, Ordering::Relaxed);
        }
        self.request_redraw();
    }

    /// The pages the view wants and no thread decodes yet: one thread each (at most three at a time).
    fn game_manual_decode(&mut self) {
        let Some(open) = self.manuals.open.as_mut() else { return };
        for (index, path) in open.view.wanted() {
            // At most three pictures decode at a time in the whole program, however fast R1 (or B and A) is pressed;
            // the rest when one is done.
            if open.decoding.len() >= pocket_ui::manual_view::KEEP || DECODING.load(Ordering::Acquire) >= pocket_ui::manual_view::KEEP {
                break;
            }
            if !open.decoding.insert(index) {
                continue;
            }
            if self.pictures.sync {
                let img = pocket_ui::manual_view::decode(&path).map(Arc::new);
                open.decoding.remove(&index);
                match img {
                    Some(img) => open.view.decoded(index, img),
                    None => open.view.broken_page(index),
                }
                continue;
            }
            let (proxy, generation) = (self.proxy.clone(), open.generation);
            DECODING.fetch_add(1, Ordering::AcqRel);
            std::thread::spawn(move || {
                pocket_ui::web::lower_priority();
                let img = pocket_ui::manual_view::decode(&path).map(Arc::new);
                DECODING.fetch_sub(1, Ordering::AcqRel);
                let _ = proxy.send_event(UserEvent::Manual(ManualEvent::Page { generation, index, img }));
            });
        }
    }

    pub(crate) fn game_manual_input(&mut self, b: Button) {
        let Some(open) = self.manuals.open.as_mut() else { return };
        match open.view.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => self.game_manual_close(),
            Action::Moved => {
                self.game_manual_decode();
                self.request_redraw();
            }
        }
    }

    /// A finger on the manual (`app_touch`): a swipe or a tap on a half of the page turns it, as L1/R1 do.
    pub(crate) fn game_manual_touch(&mut self, gesture: pocket_ui::touch::Gesture, w: i32) {
        let Some(open) = self.manuals.open.as_mut() else { return };
        if open.view.touch(gesture, w) == Action::Moved {
            self.game_manual_decode();
            self.request_redraw();
        }
    }

    /// A finger came down on the manual: its swipe may turn one page.
    pub(crate) fn game_manual_finger_down(&mut self) {
        if let Some(open) = self.manuals.open.as_mut() {
            open.view.finger_down();
        }
    }

    pub(crate) fn game_manual_event(&mut self, e: ManualEvent) {
        match e {
            ManualEvent::Listed { system, stems } => {
                println!("manual: {system}: {} games with a manual", stems.len());
                self.manuals.asked.remove(&system);
                let had = self.manuals.listed.insert(system.clone(), Arc::new(stems)).is_some();
                if !had && self.card.as_ref().is_some_and(|o| o.system == system) {
                    self.curator_card_refresh();
                    self.request_redraw();
                }
            }
            ManualEvent::Opened { generation, result } => {
                let Some(open) = self.manuals.open.as_mut().filter(|o| o.generation == generation) else { return };
                // A region found unreadable: the row may go (all regions marked) when the card opens next.
                if matches!(result, manual::Opened::Ended(Ok(Fetched::Unreadable))) {
                    let system = open.system.clone();
                    self.manuals.listed.remove(&system);
                }
                let Some(open) = self.manuals.open.as_mut() else { return };
                Self::game_manual_opened(&mut open.view, result);
                self.game_manual_decode();
                self.request_redraw();
            }
            ManualEvent::Progress { generation, bytes } => {
                if let Some(open) = self.manuals.open.as_mut().filter(|o| o.generation == generation) {
                    open.view.loaded = bytes;
                    self.request_redraw();
                }
            }
            ManualEvent::Page { generation, index, img } => {
                // A page of a manual closed since: its slot is free again, the open one may go on.
                let Some(open) = self.manuals.open.as_mut().filter(|o| o.generation == generation) else {
                    self.game_manual_decode();
                    return;
                };
                open.decoding.remove(&index);
                // A page that did not decode is not asked for again (it says it can't be shown).
                match img {
                    Some(img) => open.view.decoded(index, img),
                    None => open.view.broken_page(index),
                }
                self.game_manual_decode();
                self.request_redraw();
            }
        }
    }

    /// `--snapshot manual:<system>:<game>[:<keys>]`: the game's card, its manual opened on it, then `keys` (`r`/`l`
    /// R1/L1, `a` A, `u` `d` `<` `>` the D-pad). True when the state was this one.
    pub(crate) fn game_manual_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("manual:") else { return false };
        let mut parts = rest.splitn(3, ':');
        let (Some(system), Some(name)) = (parts.next(), parts.next()) else { return false };
        let keys = parts.next().unwrap_or("");
        self.curator_snapshot(&format!("card:{system}:{name}"));
        self.game_open_manual_for_card();
        for k in keys.chars() {
            let b = match k {
                'r' => Button::R1,
                'l' => Button::L1,
                'a' => CONFIRM,
                'u' => Button::Up,
                'd' => Button::Down,
                '<' => Button::Left,
                '>' => Button::Right,
                _ => continue,
            };
            self.game_manual_input(b);
        }
        true
    }

    /// Where the page stands, for the repaint check.
    pub(crate) fn game_manual_mark(&self) -> Option<(usize, u32, usize, bool, u64)> {
        self.manuals.open.as_ref().map(|o| (o.view.page(), o.view.level(), o.decoding.len(), o.view.wait.is_some(), o.view.loaded))
    }

    pub(crate) fn game_paint_manual(&mut self, c: &mut Canvas) {
        let Some(open) = self.manuals.open.as_ref() else { return };
        open.view.paint(c, &mut self.text, &mut self.hits);
    }
}

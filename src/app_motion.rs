//! Part of the pocket-ui binary: motion on the App's side (`pocket_ui::motion`, docs/DESIGN.md, Motion).
//! Which page is on screen (a push slides in from the right, a pop from the left, the game card rises out of
//! its label), the glides of the lists, the hint bar's crossfade, and the frame log. Kept out of main.rs so
//! the paint functions there only carry small hooks.
//!
//! Everything here is a picture of a state that has already changed: the input handlers never wait for it, and
//! nothing here runs unless a frame is drawn anyway.

use std::time::{Duration, Instant};

use pocket_ui::gfx::Canvas;
use pocket_ui::motion::{self, Enter, FrameStats, Glide, Kind, Transition};

use super::{App, Screen, SettingsPage};

/// The hint bar crossfades this fast when its hints change.
const HINTS: Duration = Duration::from_millis(120);

/// What is on screen, to tell a push from a pop. Matches with a fallback, so screens and Settings pages that
/// other parts add later count as pages of their own without a change here. Pages opened over others (the card,
/// the console page, Curator's pages, the search) are one level deeper than what they opened over.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    Home,
    Shelf(usize),
    Favorites,
    /// Another screen of the shelf's (a hash of it) and its depth: Curator's list, its smart shelves, stickers
    /// and the Lens's games.
    Screen(u64, u8),
    /// The first-start setup (depth 0) or a Settings page made of it (Look, Wi-Fi, ...; depth 3).
    Setup(u8),
    /// Settings' categories.
    Settings,
    /// A category's list (which category).
    SettingsList(u8),
    /// A page of Settings (Battery, Updates, ...), opened from a category's list.
    Sub(std::mem::Discriminant<SettingsPage>),
    Manual,
    Card(u8),
    Console(u8),
    /// A page of Curator's (which one, depth): the Lens, Echoes, Versions, naming a shelf.
    Curator(u8, u8),
    Search(u8),
    PortFiles,
    Switcher,
}

impl Page {
    fn depth(self) -> u8 {
        match self {
            Page::Home => 0,
            Page::Shelf(_) | Page::Favorites | Page::Settings | Page::Switcher => 1,
            Page::Screen(_, d) | Page::Setup(d) | Page::Card(d) | Page::Console(d) | Page::Curator(_, d) | Page::Search(d) => d,
            Page::SettingsList(_) | Page::PortFiles => 2,
            Page::Sub(_) | Page::Manual => 3,
        }
    }
}

/// Which of the scrolling lists a glide belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum List {
    /// The shelf in the Shelf, Minimal and Split looks (one is shown at a time).
    Home,
    /// The Settings list.
    Settings,
}

/// A list's glides: the first row shown and the highlighted row (both in rows), the highlight's width.
#[derive(Clone, Copy)]
struct ListGlides {
    scroll: Glide,
    hilite: Glide,
    width: Glide,
}

impl ListGlides {
    fn new() -> Self {
        ListGlides { scroll: Glide::rows(), hilite: Glide::rows(), width: Glide::px() }
    }
}

pub(crate) struct MotionUi {
    /// The page of the last frame, and the transition into the one now shown.
    page: Option<Page>,
    transition: Option<Transition>,
    home: ListGlides,
    settings: ListGlides,
    /// Lists of other pages: (which, what it shows, glides).
    pages: Vec<(u8, u64, ListGlides)>,
    /// The Cards look's carousel, in cards.
    cards: Glide,
    /// The hint bar: what it shows, what it showed before and since when it changes.
    hints: Vec<(String, String)>,
    old_hints: Vec<(String, String)>,
    hints_at: Option<Instant>,
    /// Where the selected label was in the last frame of the Shelf look (the game card rises from it).
    pub(crate) sel_y: Option<i32>,
    /// The rows of the list painted in this frame, from the top (x, y, w, h): they come in one after another when
    /// the list has just been opened.
    rows: Vec<(i32, i32, i32, i32)>,
    /// `POCKETUI_FRAME_LOG`: paint times of the frames of the animation in progress, and the first press not
    /// on screen yet (for the press-to-frame latency).
    /// Per kind of frame: a page coming in, a glide or crossfade of this module, anything else that animates.
    stats: Vec<(&'static str, FrameStats)>,
    pub(crate) input_at: Option<Instant>,
}

impl Default for MotionUi {
    fn default() -> Self {
        MotionUi {
            page: None,
            transition: None,
            home: ListGlides::new(),
            settings: ListGlides::new(),
            pages: Vec::new(),
            cards: Glide::rows(),
            hints: Vec::new(),
            old_hints: Vec::new(),
            hints_at: None,
            sel_y: None,
            rows: Vec::new(),
            stats: Vec::new(),
            input_at: None,
        }
    }
}

impl MotionUi {
    /// A new page shows up at rest: its lists don't glide in from where the last page's were.
    fn rest(&mut self) {
        self.home = ListGlides::new();
        self.settings = ListGlides::new();
        self.pages.clear();
        self.cards = Glide::rows();
        self.hints.clear();
        self.hints_at = None;
    }

    fn list(&mut self, which: List) -> &mut ListGlides {
        match which {
            List::Home => &mut self.home,
            List::Settings => &mut self.settings,
        }
    }

    /// The glides of list `n` of another page (Curator's Lens, Echoes and Versions, the search's results); `key` says what it shows, and a list that shows something else starts at
    /// rest (the Lens's values of another facet don't glide in from where the last facet's were).
    fn page_list(&mut self, n: u8, key: u64) -> &mut ListGlides {
        let i = match self.pages.iter().position(|(m, _, _)| *m == n) {
            Some(i) => i,
            None => {
                self.pages.push((n, key, ListGlides::new()));
                self.pages.len() - 1
            }
        };
        let entry = &mut self.pages[i];
        if entry.1 != key {
            *entry = (n, key, ListGlides::new());
        }
        &mut entry.2
    }

    /// The first row shown of page list `n` in this frame (a fraction of a row), scrolling smoothly to `first`;
    /// `key`: what the list shows.
    pub(crate) fn scroll_at(&mut self, n: u8, key: u64, first: usize, now: Instant) -> f32 {
        let l = self.page_list(n, key);
        l.scroll.set(first as f32, now);
        l.scroll.value(now)
    }

    /// Where list `which` is in this frame: (first row shown, highlighted row), both as fractions of a row, gliding
    /// to `first` and `sel`.
    pub(crate) fn list_at(&mut self, which: List, first: usize, sel: usize, now: Instant) -> (f32, f32) {
        let l = self.list(which);
        l.scroll.set(first as f32, now);
        l.hilite.set(sel as f32, now);
        (l.scroll.value(now), l.hilite.value(now))
    }

    /// The highlight's width in this frame, gliding to `w`.
    pub(crate) fn width_at(&mut self, which: List, w: i32, now: Instant) -> i32 {
        let l = self.list(which);
        l.width.set(w as f32, now);
        l.width.value(now).round() as i32
    }

    /// A row of the list being painted, for the stagger of a page that has just opened. Only while one runs.
    pub(crate) fn row(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if self.transition.is_some() && h > 0 {
            self.rows.push((x, y, w, h));
        }
    }

    /// The carousel's position in this frame, gliding to `sel`.
    pub(crate) fn cards_at(&mut self, sel: usize, now: Instant) -> f32 {
        self.cards.set(sel as f32, now);
        self.cards.value(now)
    }

    /// The hint bar's crossfade: the hints shown before and how far the new ones are in (0-1), while it lasts.
    pub(crate) fn hints_fade(&mut self, hints: &[(&str, &str)], now: Instant) -> Option<(Vec<(String, String)>, f32)> {
        let same = self.hints.len() == hints.len() && self.hints.iter().zip(hints).all(|(a, b)| a.0 == b.0 && a.1 == b.1);
        if !same {
            let new: Vec<(String, String)> = hints.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect();
            let old = std::mem::replace(&mut self.hints, new);
            // The first hints of a page come with the page.
            if !old.is_empty() && motion::allowed(Kind::Fade) {
                self.old_hints = old;
                self.hints_at = Some(now);
                motion::keep_alive(motion::end_of(now, HINTS));
            }
        }
        let at = self.hints_at?;
        let p = motion::progress(Kind::Fade, at, now, HINTS);
        if p >= 1.0 {
            self.hints_at = None;
            return None;
        }
        Some((self.old_hints.clone(), p))
    }
}

/// How much row `i` is under a highlight at row `at` (1 = right under it), for its text colour while the highlight
/// glides past. The turn is kept to the middle of the way: a text half way between its two colours would match
/// neither background.
pub(crate) fn under(i: usize, at: f32) -> f32 {
    let cover = (1.0 - (i as f32 - at).abs()).clamp(0.0, 1.0);
    ((cover - 0.3) / 0.4).clamp(0.0, 1.0)
}

/// The rows `first..` of a list scrolled to `first_f` (a fraction of a row) that are at least partly in view of
/// `visible` rows: (index, offset in rows from the top of the list's band).
pub(crate) fn rows_in_view(first_f: f32, visible: usize, len: usize) -> impl Iterator<Item = (usize, f32)> {
    let from = first_f.floor().max(0.0) as usize;
    let to = ((first_f + visible as f32).ceil().max(0.0) as usize).min(len);
    (from..to).map(move |i| (i, i as f32 - first_f))
}

impl App {
    /// The page on screen; None while a game has the screen (and the pause menu over it, which has its own).
    pub(crate) fn page(&self) -> Option<Page> {
        if self.sleep.overlay().is_some() || self.menu.is_some() {
            return None;
        }
        if self.running.is_some() {
            return self.switcher.as_ref().map(|_| Page::Switcher);
        }
        if self.settings_page == Some(SettingsPage::Manual) {
            return Some(Page::Manual);
        }
        if self.setup.is_some() {
            return Some(Page::Setup(if self.onboarded { 3 } else { 0 }));
        }
        if let Some(p) = self.settings_page {
            return Some(match p {
                SettingsPage::Groups(_) => Page::Settings,
                SettingsPage::List(row) => Page::SettingsList(super::app_settings_groups::group_of(row) as u8),
                SettingsPage::Manual => Page::Manual,
                other => Page::Sub(std::mem::discriminant(&other)),
            });
        }
        if self.port_files.is_some() {
            return Some(Page::PortFiles);
        }
        // In the order `paint` looks at them: Curator's pages, the card, the console page, the search.
        let shelf = self.screen_page();
        let over = shelf.depth() + 1;
        let card = if self.card.is_some() { Some(Page::Card(over + self.find.is_open() as u8)) } else { None };
        if let Some(page) = self.curator.page.as_ref() {
            use super::app_curator::Page as C;
            return Some(match page {
                // Sheets over the card and over the Curator list: the page under them stays.
                C::Stickers { .. } => card.unwrap_or(shelf),
                C::Options { .. } => shelf,
                // The order sheet over the Lens (the Lens stays) or over the Curator list.
                C::Order { back: Some(_), .. } => Page::Curator(1, over),
                C::Order { .. } => shelf,
                C::Lens(_) => Page::Curator(1, over),
                C::Echoes(_) => Page::Curator(2, card.map_or(over, |c| c.depth() + 1)),
                C::Versions(_) => Page::Curator(3, card.map_or(over, |c| c.depth() + 1)),
                C::Name(_) => Page::Curator(4, over + 1),
                // History over the Curator list; its review one deeper; back from a card they come in from the left.
                C::History(_) => Page::Curator(5, over),
                C::Review(_) => Page::Curator(6, over + 1),
            });
        }
        if let Some(card) = card {
            return Some(card);
        }
        if self.console.is_some() {
            return Some(Page::Console(over));
        }
        if self.find.is_open() {
            return Some(Page::Search(over));
        }
        if self.switcher.is_some() {
            return Some(Page::Switcher);
        }
        Some(shelf)
    }

    /// The shelf's own screen as a page (Home, a system's games, Curator's lists).
    fn screen_page(&self) -> Page {
        // The Cabinet look's shelves are one page with its own scrolling (`app_cabinet`).
        if self.theme == super::Theme::Cabinet {
            return Page::Home;
        }
        #[allow(unreachable_patterns)]
        match self.screen {
            Screen::Home => Page::Home,
            Screen::Games(s) => Page::Shelf(s),
            Screen::Favorites => Page::Favorites,
            other => {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                other.hash(&mut h);
                // Curator's list is a level under Home; its shelves, stickers and the Lens's games under it.
                Page::Screen(h.finish(), if other == Screen::Curator { 1 } else { 2 })
            }
        }
    }

    /// Before a frame is painted: a new page starts its transition and shows its lists at rest.
    pub(crate) fn motion_frame_begin(&mut self, now: Instant) {
        self.motion.rows.clear();
        let page = self.page();
        let last = self.motion.page;
        if page == last {
            return;
        }
        self.motion.page = page;
        self.marquee.restart();
        self.motion.rest();
        self.motion.transition = None;
        let (Some(from), Some(to)) = (last, page) else { return };
        let enter = match to {
            Page::Switcher => Enter::Fade,
            Page::Card(_) if from.depth() < to.depth() => match self.motion.sel_y {
                Some(from_y) => Enter::Rise { from_y },
                None => Enter::Push,
            },
            _ if to.depth() > from.depth() => Enter::Push,
            _ if to.depth() < from.depth() => Enter::Pop,
            _ => Enter::Fade,
        };
        self.motion.transition = Transition::new(enter, now, 112);
    }

    /// After the page is painted (before the quick settings and the OSD, which stay where they are): the
    /// transition into it, over the finished picture.
    pub(crate) fn motion_after_paint(&mut self, c: &mut Canvas) {
        // The rows were registered by this paint (a crossfade between dark and light paints twice).
        let rows = std::mem::take(&mut self.motion.rows);
        let Some(t) = self.motion.transition else { return };
        // The page colour, from the right edge half way down (the Split look and the Cards wash have their own).
        let bg = c.buf[(c.h / 2 * c.w + c.w - 2).max(0) as usize];
        if !t.apply(c, Instant::now(), bg, &rows) {
            self.motion.transition = None;
        }
    }

    /// Frames are needed for an animation of this module (only while our screens are on view).
    pub(crate) fn motion_animating(&self, now: Instant) -> bool {
        self.page().is_some() && motion::busy(now)
    }

    /// A button press: the frame log measures from here to the frame that shows it.
    pub(crate) fn motion_input(&mut self) {
        if motion::frame_log() && self.motion.input_at.is_none() {
            self.motion.input_at = Some(Instant::now());
        }
    }

    /// After a frame went to the screen (`POCKETUI_FRAME_LOG=1`): the press-to-frame latency, and the paint time
    /// of each frame an animation takes, summed up when it ends.
    pub(crate) fn motion_frame_end(&mut self, painted: Duration) {
        if !motion::frame_log() {
            return;
        }
        let animating = self.animating();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        if let Some(at) = self.motion.input_at.take() {
            println!("motion: press to frame {:.2} ms (paint {:.2} ms, {:?})", ms(at.elapsed()), ms(painted), self.motion.page);
        }
        if animating {
            let kind = if self.motion.transition.is_some() {
                "page transition"
            } else if motion::busy(Instant::now()) || self.pull_moving(Instant::now()) {
                "glide"
            } else {
                "other (marquee, toast, OSD, ...)"
            };
            match self.motion.stats.iter_mut().find(|(k, _)| *k == kind) {
                Some((_, s)) => s.add(painted),
                None => {
                    let mut s = FrameStats::default();
                    s.add(painted);
                    self.motion.stats.push((kind, s));
                }
            }
        } else if !self.motion.stats.is_empty() {
            for (kind, s) in std::mem::take(&mut self.motion.stats) {
                println!("motion: {} frames ({kind}, {:?}), paint average {:.2} ms, worst {:.2} ms", s.frames, self.motion.page, ms(s.average()), ms(s.worst));
            }
        }
    }

    /// The pull-out of the Shelf look's selected spine, 0-1 (ease-out, `PULL` long; at once in Reduced and Off).
    pub(crate) fn pull_progress(&self, now: Instant) -> f32 {
        motion::progress(Kind::Move, self.moved_at, now, super::PULL)
    }

    /// The pull-out is still moving.
    pub(crate) fn pull_moving(&self, now: Instant) -> bool {
        motion::allowed(Kind::Move) && now < motion::end_of(self.moved_at, super::PULL) + motion::SETTLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_partly_in_view_while_a_list_scrolls() {
        let rows: Vec<(usize, f32)> = rows_in_view(2.5, 3, 10).collect();
        assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), vec![2, 3, 4, 5]);
        assert_eq!(rows[0].1, -0.5, "half under the top edge");
        let rest: Vec<usize> = rows_in_view(0.0, 3, 2).map(|r| r.0).collect();
        assert_eq!(rest, vec![0, 1], "a short list");
    }

    #[test]
    fn a_rows_text_turns_as_the_highlight_passes() {
        assert_eq!(under(3, 3.0), 1.0);
        assert!((under(4, 3.5) - 0.5).abs() < 1e-5, "half way: half turned");
        assert_eq!(under(4, 3.2), 0.0, "only a fifth under it: still its own colour");
        assert_eq!(under(4, 3.8), 1.0);
        assert_eq!(under(5, 3.0), 0.0);
    }

    #[test]
    fn deeper_pages_push_and_shallower_ones_pop() {
        assert!(Page::Shelf(0).depth() > Page::Home.depth());
        assert!(Page::Card(2).depth() > Page::Shelf(0).depth());
        // Settings: the categories, a category's list, its pages (and the setup steps it opens, such as Look).
        assert!(Page::SettingsList(0).depth() > Page::Settings.depth());
        assert!(Page::Sub(std::mem::discriminant(&SettingsPage::About)).depth() > Page::SettingsList(4).depth());
        assert!(Page::Manual.depth() > Page::SettingsList(4).depth());
        assert!(Page::Setup(3).depth() > Page::SettingsList(0).depth(), "Settings › Look & feel › Look");
        assert_eq!(Page::Setup(0).depth(), 0, "the first start");
    }

    #[test]
    fn the_hint_bar_crossfades_only_when_its_hints_change() {
        motion::set_level(motion::Motion::Full);
        let mut m = MotionUi::default();
        let t0 = Instant::now();
        assert!(m.hints_fade(&[("A", "Play")], t0).is_none(), "the first hints come with the page");
        assert!(m.hints_fade(&[("A", "Play")], t0).is_none());
        let (old, p) = m.hints_fade(&[("A", "Open")], t0).unwrap();
        assert_eq!(old, vec![("A".to_string(), "Play".to_string())]);
        assert!(p > 0.0 && p < 1.0);
        assert!(m.hints_fade(&[("A", "Open")], t0 + HINTS).is_none(), "over");
        motion::set_level(motion::Motion::Off);
        assert!(m.hints_fade(&[("B", "Back")], t0 + HINTS).is_none(), "Off: at once");
        motion::set_level(motion::Motion::Full);
    }
}

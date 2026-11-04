//! Part of the pocket-ui binary: the Easy look, "Big" in the code (docs/BIG-SHELF.md; the order and the jumps are
//! `pocket_ui::big_shelf`). Kids Mode's shelf for a grown-up with 10 to 30 games: its big case and its big pause buttons
//! (`app_kids`), the normal game start and Quit, a short details card and the normal pause menu one press away, and
//! none of its closed doors.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::big_shelf::{self, Group};
use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::kids::Colours;
use pocket_ui::pad::Button;
use pocket_ui::palette::pal;
use pocket_ui::settings::Theme;
use pocket_ui::shelf::Item;
use pocket_ui::text::Face;

use super::{App, MainItem, Screen, CANCEL, CONFIRM, REPEAT_DELAY};

/// The lines under a game's name (`big_info`).
#[derive(Default)]
pub(crate) struct Info {
    top: Vec<String>,
    about: Option<String>,
    played: Option<String>,
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

#[derive(Default)]
pub(crate) struct Big {
    /// The lines under the selected game's name, made when the selection or the list changes.
    info: Option<(Item, pocket_ui::i18n::Lang, Rc<Info>)>,
    groups: Vec<Group>,
    /// How many places in front are the favourites, and how many games on the systems behind them.
    front: usize,
    total: usize,
    /// Every game with a usable moment (`frozen::moments`), and which read that was.
    moments: Vec<PathBuf>,
    moments_read: u64,
    /// The list on Home is this look's.
    built: bool,
    /// A game didn't start: what to say and its file, until it is read (`big_paint_failure`).
    failure: Option<(big_shelf::Failure, String)>,
    /// A game with no saved moment: Quit has asked once.
    asked: bool,
    /// On the pause screen, the More row is selected.
    more_selected: bool,
    /// The line about an open case ("Open means your moment is waiting") was told once, ever (`Settings::open_case_told`).
    pub(crate) open_case_told: bool,
    /// That line is on the screen now.
    open_hint_up: bool,
    /// More was pressed: this pause shows the normal menu.
    more_open: bool,
}

/// Where the case row starts and how much of the screen it may take: smaller than Kids Mode's (0.095, (0.64, 0.61)).
const SHELF_TOP: f32 = 0.10;
const SHELF_ROOM: (f32, f32) = (0.55, 0.46);
/// The pause screen's three rows, in a 1080-high screen's pixels: where the first starts, how tall each is, the gap.
pub(crate) const PAUSE_TOP: f32 = 232.0;
pub(crate) const PAUSE_ROW: f32 = 150.0;
pub(crate) const PAUSE_GAP: f32 = 26.0;
/// The name's type and its distance from the case, against Kids Mode's.
const NAME_SCALE: f32 = 0.84;

/// The player's own colours, in the shape Kids Mode's screens take them.
pub(crate) fn colours() -> Colours {
    let p = pal();
    Colours { top: p.void, bottom: mix(p.void, p.spine, 180), shape: p.spine, spark: p.dust, ink: p.chalk, accent: p.label, on_accent: p.ink }
}

pub(crate) fn paint_background(c: &mut Canvas) {
    let k = colours();
    for y in 0..c.h {
        c.rect(0, y, c.w, 1, mix(k.top, k.bottom, (y * 255 / c.h.max(1)) as u32));
    }
}

/// The colour a page's transition fades from (`motion_after_paint`): the background's middle, never what stands on it.
pub(crate) fn page_colour() -> u32 {
    let k = colours();
    mix(k.top, k.bottom, 128)
}

/// A tap beside the hints: as on Kids Mode's shelf, left and right of the case move, the case plays.
pub(crate) fn tap_button(x: f32, w: u32) -> Button {
    let w = w as f32;
    if x < w / 4.0 {
        Button::Left
    } else if x > w - w / 4.0 {
        Button::Right
    } else {
        CONFIRM
    }
}

impl Big {
    pub(crate) fn new(open_case_told: bool) -> Big {
        Big { open_case_told, ..Default::default() }
    }
}

impl App {
    /// The look is Big. (A child's shelf sets the Shelf look for itself: `kids_enter`.)
    pub(crate) fn big_on(&self) -> bool {
        self.theme == Theme::Big && !self.kids_live()
    }

    /// The big case is on Home. An empty card keeps the normal way (`paint_home`: "No games yet", the starter pack).
    pub(crate) fn big_live(&self) -> bool {
        self.big_on() && self.screen == Screen::Home && !self.items.is_empty()
    }

    /// The pause menu is Continue, Stop and More; More and its other pages (a card that refuses saves) are the normal ones.
    pub(crate) fn big_pause(&self) -> bool {
        self.big_on() && !self.big.more_open && self.running.is_some() && self.menu.as_ref().is_some_and(|m| matches!(m.page, super::Page::Main))
    }

    pub(crate) fn big_menu_opened(&mut self) {
        self.kids.stop_selected = false;
        self.big.more_selected = false;
        self.big.more_open = false;
        self.big.asked = false;
    }

    pub(crate) fn big_more_selected(&self) -> bool {
        self.big.more_selected && self.big_pause()
    }

    /// Whether this pause's Quit keeps the moment, by the route and Continue where I quit (`quit_leaves_out_resume`).
    pub(crate) fn big_keeps_moment(&self) -> bool {
        self.menu.as_ref().is_some_and(|m| !m.without_saving && !matches!(m.saves, pocket_ui::pause_saves::Saves::InGame(_)))
    }

    pub(crate) fn big_quit_word(&self) -> Option<&'static str> {
        self.big_on().then(|| if self.big_keeps_moment() { "Save & quit" } else { "Quit" })
    }

    /// The line at the bottom of the pause screen. No emulator's name: only what happens.
    pub(crate) fn big_pause_line(&self) -> Option<&'static str> {
        if !self.big_on() {
            return None;
        }
        let in_game = self.menu.as_ref().is_some_and(|m| matches!(m.saves, pocket_ui::pause_saves::Saves::InGame(_)));
        Some(if self.big.asked {
            "Have you saved in the game? A quits."
        } else if self.big_keeps_moment() {
            "Next time, A brings you back to this moment."
        } else if in_game {
            "This game keeps only what you save in it."
        } else {
            "Doesn't save where you are. In-game saves are kept."
        })
    }

    /// Up and Down go through the three; A on More opens the normal menu; START the quick settings; B and HOME go on.
    pub(crate) fn big_pause_press(&mut self, b: Button) {
        if self.menu.as_ref().is_some_and(|m| m.quitting) {
            return;
        }
        let at = if self.big.more_selected { 2 } else { self.kids.stop_selected as usize };
        match b {
            Button::Up | Button::Down => {
                let to = if b == Button::Up { (at + 2) % 3 } else { (at + 1) % 3 };
                self.big.more_selected = to == 2;
                self.kids.stop_selected = to == 1;
                self.big.asked = false;
            }
            Button::Start if !self.pad.is_pressed(Button::Select) => self.open_quick_settings_over_game(),
            Button::Mode => self.resume_game(None),
            b if b == CANCEL => self.resume_game(None),
            b if b == CONFIRM => match at {
                0 => self.resume_game(None),
                1 => self.big_quit(),
                _ => self.big.more_open = true,
            },
            _ => {}
        }
    }

    /// The pause menu's own Quit. A game that saves only in the game asks once; Continue where I quit off is the player's own choice.
    fn big_quit(&mut self) {
        let saves = self.menu.as_ref().map(|m| m.saves);
        if saves.is_some_and(asks_before_quit) && !std::mem::replace(&mut self.big.asked, true) {
            return;
        }
        let Some(menu) = self.menu.as_mut() else { return };
        menu.sel = menu.items().iter().position(|i| *i == MainItem::Quit).unwrap_or(menu.sel);
        self.menu_input(CONFIRM);
    }

    /// B on the normal menu's first page: back to Continue, Stop and More, where it came from. True: it is taken.
    pub(crate) fn big_back_from_more(&mut self, b: Button) -> bool {
        let first_page = self.menu.as_ref().is_some_and(|m| !m.quitting && matches!(m.page, super::Page::Main));
        if b == CANCEL && first_page && self.big_on() && self.big.more_open {
            self.big.more_open = false;
            return true;
        }
        false
    }

    /// A touch on the pause screen, on its release: each of the three where it stands.
    pub(crate) fn big_pause_touch(&mut self, ev: pocket_ui::touch::TouchEvent) {
        if ev.phase != pocket_ui::touch::Phase::Up || self.menu.as_ref().is_some_and(|m| m.quitting) {
            return;
        }
        let h = super::snapshot_dims().1 as f32;
        let y = ev.y;
        // The row the finger is nearest to (`kids_paint_pause`'s layout).
        let row = (((y / h * 1080.0 - PAUSE_TOP + PAUSE_GAP / 2.0) / (PAUSE_ROW + PAUSE_GAP)).floor() as i32).clamp(0, 2);
        self.big.more_selected = row == 2;
        self.kids.stop_selected = row == 1;
        self.big_pause_press(CONFIRM);
    }

    /// All the games of the systems on the shelf, in their stable order (`big_shelf::arrange`).
    pub(crate) fn big_items(&mut self) -> Vec<Item> {
        let shown: Vec<usize> = (0..self.systems.len()).filter(|s| !self.hidden.contains(&self.systems[*s].name)).collect();
        let favourites: Vec<(usize, usize)> = self
            .favorite_items()
            .into_iter()
            .filter_map(|i| match i {
                Item::Game(s, g) if shown.contains(&s) => Some((s, g)),
                _ => None,
            })
            .collect();
        let counts: Vec<(usize, usize)> = shown.iter().map(|s| (*s, self.systems[*s].games.len())).collect();
        let order = big_shelf::arrange(&counts, &favourites);
        (self.big.groups, self.big.front, self.big.total) = (order.groups, order.front, order.total);
        order.games.into_iter().map(|(s, g)| Item::Game(s, g)).collect()
    }

    /// Before the list is made again: the game selected, and whether it is on the favourites in front.
    pub(crate) fn big_before_rebuild(&self) -> Option<(Item, bool)> {
        self.big_live().then(|| self.items.get(self.sel).map(|i| (*i, self.sel < self.big.front))).flatten()
    }

    /// A new list put the selection on a game's first place by its path (`KeepSelection`): back in the part of the list it was in.
    /// `in_front`: before the new list.
    pub(crate) fn big_keep_part(&mut self, in_front: bool) {
        if !self.big_live() || in_front || self.sel >= self.big.front {
            return;
        }
        let item = self.items[self.sel];
        if let Some(at) = self.items.iter().skip(self.big.front).position(|i| *i == item) {
            self.sel = self.big.front + at;
            self.prev_sel = self.sel;
        }
    }

    pub(crate) fn big_in_front(&self) -> bool {
        self.sel < self.big.front
    }

    /// After: the same game stays selected, in the same part of the list when it is still there. Coming to this look it
    /// opens on the game played last; leaving it, the other look's first row.
    pub(crate) fn big_after_rebuild(&mut self, was: Option<(Item, bool)>) {
        // The games, their plays or their descriptions may have changed.
        self.big.info = None;
        let here = self.big_on() && self.screen == Screen::Home;
        if here && !self.big.built {
            self.sel = self.big_last_played().unwrap_or(0);
            self.big_moments_refresh();
        } else if let Some((item, in_front)) = was.filter(|_| here) {
            let from = if in_front { 0 } else { self.big.front };
            let at = self.items.iter().skip(from).position(|i| *i == item).map(|at| at + from).or_else(|| self.items.iter().position(|i| *i == item));
            if let Some(at) = at {
                self.sel = at;
            }
        } else if !here && self.big.built && self.screen == Screen::Home {
            self.sel = Item::first_row(&self.items);
        }
        self.big.built = here;
    }

    /// At the start, after the first list: on the game played last.
    pub(crate) fn big_open_on_last(&mut self) {
        if self.big_live() {
            self.sel = self.big_last_played().unwrap_or(0);
            self.prev_sel = self.sel;
        }
    }

    fn big_last_played(&self) -> Option<usize> {
        let last = self.recents.list.iter().find(|r| self.recent_visible(r))?;
        self.items.iter().position(|i| matches!(i, Item::Game(s, g) if self.systems.get(*s).and_then(|s| s.games.get(*g)).is_some_and(|g| g.path == last.path)))
    }

    /// A game didn't start: the notice stays as a page of its own until it is read.
    pub(crate) fn big_note_failure(&mut self, path: &Path, system: &str, name: &str) {
        if !self.big_on() {
            return;
        }
        if let Some(notice) = self.notice.take() {
            println!("easy: {name} didn't start: {}", notice.text());
            let file = path.file_name().map_or_else(PathBuf::new, PathBuf::from).to_string_lossy().into_owned();
            self.big.failure = Some((big_shelf::failure(name, system, notice.text()), file));
        }
    }

    /// Reads which games have a usable moment, on a thread. Not on a device whose host owns the states (`frozen::available`).
    pub(crate) fn big_moments_refresh(&mut self) {
        if !self.big_on() || !pocket_ui::frozen::available() {
            return;
        }
        let games = self.big_game_list();
        self.big.moments_read += 1;
        self.moment_reads.big_started();
        let (proxy, read) = (self.proxy.clone(), self.big.moments_read);
        std::thread::spawn(move || {
            let _ = proxy.send_event(super::UserEvent::BigMoments(read, pocket_ui::frozen::moments_timed(&games)));
        });
    }

    fn big_game_list(&self) -> Vec<pocket_ui::frozen::Game> {
        let host = Path::new(super::host_bin()).exists();
        let mut games = Vec::new();
        for sys in self.systems.iter().filter(|s| !self.hidden.contains(&s.name)) {
            for g in &sys.games {
                let (emulator, core) = self.route(sys, Some(&g.path), host);
                // The moment of another game of this file name: a start won't continue it (`frozen_at_launch`), so no Resume.
                if self.shared_moment_not_own(&sys.name, &g.path, &emulator) {
                    continue;
                }
                let mark = self.recents.list.iter().find(|r| r.path == g.path).and_then(|r| r.frozen.clone());
                games.push(pocket_ui::frozen::Game { system: sys.name.clone(), path: g.path.clone(), emulator, core, mark });
            }
        }
        games
    }

    pub(crate) fn big_moments_got(&mut self, read: u64, list: Vec<pocket_ui::frozen::Moment>) {
        if read == self.big.moments_read {
            self.big.moments = list.iter().map(|m| m.0.clone()).collect();
            self.moment_reads.big_got(list);
            self.request_redraw();
        }
    }

    pub(crate) fn big_has_moment(&self, item: Item) -> bool {
        let Item::Game(s, g) = item else { return false };
        self.systems.get(s).and_then(|s| s.games.get(g)).is_some_and(|g| self.big.moments.contains(&g.path))
    }

    /// A button on the shelf. SELECT does nothing, START opens Settings as everywhere.
    pub(crate) fn big_press(&mut self, b: Button) {
        // The failure's page: B goes back, A does the one thing that helps, and plays nothing.
        if let Some((failure, _)) = self.big.failure.as_ref() {
            let fix = failure.fix;
            if b == CANCEL {
                self.big.failure = None;
            } else if b == CONFIRM {
                self.big.failure = None;
                match fix {
                    big_shelf::Fix::GameCheck => self.settings_page = self.open_game_check(),
                    big_shelf::Fix::AddGames => self.setup = Some(self.new_setup().only(pocket_ui::setup::Only::Games)),
                }
            }
            return;
        }
        if matches!(b, Button::Left | Button::Right | Button::Up | Button::Down) || b == CONFIRM {
            self.big_open_told();
        }
        match b {
            Button::Left | Button::Right | Button::Up | Button::Down => {
                self.big_step(b, false);
                self.held = Some((b, Instant::now() + REPEAT_DELAY));
            }
            b if b == CONFIRM => self.activate(),
            // The normal game card, as in the other looks (`show_card`); B comes back here.
            Button::North => self.open_details(),
            Button::West => self.toggle_favorite(),
            // B does nothing here, as B on any other Home: Quick settings are HOME's (`quick::Sys`).
            _ => {}
        }
    }

    /// `held`: the key repeat, which doesn't go round at the ends.
    pub(crate) fn big_step(&mut self, b: Button, held: bool) {
        let (len, at) = (self.items.len(), self.sel);
        match b {
            Button::Left | Button::Right => {
                if let Some(to) = big_shelf::step(len, at, b == Button::Right, held) {
                    self.select(to);
                }
            }
            Button::Up | Button::Down if self.big_jumps() => {
                if let Some(to) = big_shelf::group_jump(&self.big.groups, at, b == Button::Down, held) {
                    self.select(to);
                }
            }
            _ => {}
        }
    }

    /// Up and Down go between systems when there is more than one.
    fn big_jumps(&self) -> bool {
        self.big.groups.iter().filter(|g| g.system.is_some()).count() > 1
    }

    /// The top line: where you are, and the game's system. The favourites in front have no place; one game has none to say.
    pub(crate) fn big_title(&self) -> Option<String> {
        if self.big.failure.is_some() {
            return None;
        }
        let Item::Game(s, _) = *self.items.get(self.sel)? else { return None };
        let system = self.systems.get(s)?.fullname.clone();
        if self.sel < self.big.front {
            return Some(format!("{}  ·  {system}", pocket_ui::i18n::tr("Favourites")));
        }
        if self.big.total < 2 {
            return Some(system);
        }
        let number = self.sel - self.big.front + 1;
        Some(format!("{}  ·  {system}", pocket_ui::i18n::tr("{n} of {m}").replace("{n}", &number.to_string()).replace("{m}", &self.big.total.to_string())))
    }

    /// What stands under the name: "system · year · players", a line or two of what the game is, and how long and how
    /// lately it was played. Each line is left out when nothing is known for it. Made once per game (`Big::info`).
    fn big_info(&mut self, item: Item) -> Rc<Info> {
        let lang = pocket_ui::i18n::current();
        if let Some((_, _, info)) = self.big.info.as_ref().filter(|(at, l, _)| *at == item && *l == lang) {
            return info.clone();
        }
        let info = Rc::new(self.big_info_of(item));
        self.big.info = Some((item, lang, info.clone()));
        info
    }

    fn big_info_of(&mut self, item: Item) -> Info {
        let Item::Game(s, g) = item else { return Info::default() };
        let (system, path) = (self.systems[s].name.clone(), self.systems[s].games[g].path.clone());
        let data = self.game_info(&system, &path);
        let entry = self.gamelist.index.get(&system, &path).cloned();
        // Year and players: pocket-ui's own data first, then the player's EmulationStation gamelist.
        let year = data.as_ref().and_then(|i| i.year).or_else(|| entry.as_ref().map(|e| e.year).filter(|y| *y != 0));
        let players = data
            .as_ref()
            .and_then(pocket_ui::gamedb::players_text)
            .or_else(|| entry.as_ref().filter(|e| !e.players.is_empty()).map(|e| pocket_ui::gamelist::players_text(&e.players)))
        ;
        // English parts: the painter translates each as it is drawn (`big_paint`).
        let top = [Some(super::system_label(&self.systems, &system)), year.map(|y| y.to_string()), players].into_iter().flatten().collect::<Vec<_>>();
        // What the game is: the player's own description, else the sentence made of the facts (English only; in
        // another language the genre alone: as the tables say it, else Curator's word for its kind, which every
        // language has; "Action game" stood in English on the German Easy look).
        let about = entry.as_ref().map(|e| e.desc.trim().to_string()).filter(|d| !d.is_empty()).or_else(|| {
            let data = data.as_ref()?;
            let lang = pocket_ui::i18n::current();
            if lang == pocket_ui::i18n::Lang::En {
                pocket_ui::game_card::sentence(data)
            } else {
                let raw = data.genres.first()?;
                let genre = capitalised(raw.strip_suffix(" video game").unwrap_or(raw));
                if pocket_ui::i18n::tr_in(lang, &genre) != genre {
                    return Some(genre);
                }
                let vocab = pocket_ui::curatordb::vocabulary();
                let kind = vocab.genre_tags(raw).first().map(|&t| capitalised(&pocket_ui::i18n::tr_in(lang, &vocab.tag(t).label)));
                Some(kind.unwrap_or(genre))
            }
        });
        let played = self.recents.list.iter().find(|r| r.path == path).map(|r| {
            let time = (r.play_secs >= 60).then(|| pocket_ui::recents::duration(r.play_secs));
            let last = pocket_ui::recents::ago(r.last_played);
            [time.map(|t| format!("{} {t}", pocket_ui::i18n::tr("Played"))), Some(last)].into_iter().flatten().collect::<Vec<_>>().join("  ·  ")
        });
        Info { top, about, played }
    }


    pub(crate) fn big_animating(&self) -> bool {
        self.big_live() && self.running.is_none() && (self.cabinet.pending > 0 || self.moved_at.elapsed() < Duration::from_millis(320))
    }

    /// On a case with a saved moment: the word on the case itself, as big as the name's neighbours, across its lower edge.
    fn big_paint_resume_mark(&mut self, c: &mut Canvas, colours: Colours) {
        let Some((front, fx, fy)) = self.cabinet.shelf_lit.clone() else { return };
        let u = c.h as f32 / 1080.0;
        let px = (50.0 * u).round() as u32;
        let word = "Continue";
        let w = self.text.width(Face::Title, px, word);
        let (pad, h) = ((30.0 * u) as i32, (78.0 * u) as i32);
        let (x, y) = (fx + (front.w - w) / 2 - pad, fy + front.h - h * 3 / 4);
        c.round_rect(x, y, w + 2 * pad, h, h / 2, colours.accent);
        self.text.draw(c, Face::Title, px, x + pad, y + h / 2 + px as i32 * 34 / 100, colours.on_accent, word);
    }

    /// A game that didn't start, until it is read: what happened, the file under it, and Back.
    fn big_paint_failure(&mut self, c: &mut Canvas, colours: Colours, failure: &big_shelf::Failure, file: &str) {
        let u = c.h as f32 / 1080.0;
        super::app_rescue::paint_plaster(c, colours, 70.0);
        let (big, small) = ((76.0 * u).round() as u32, (44.0 * u).round() as u32);
        let width = c.w - (120.0 * u) as i32;
        let mut y = c.h * 44 / 100;
        for l in self.text.wrap_at_most(Face::Title, big, "This game didn't start", width, 2) {
            let w = self.text.width(Face::Title, big, &l);
            self.text.draw(c, Face::Title, big, (c.w - w) / 2, y, colours.ink, &l);
            y += (big as f32 * 1.15) as i32;
        }
        y += (10.0 * u) as i32;
        for l in self.text.wrap_at_most(Face::Body, small, &failure.says, width, 4) {
            let w = self.text.width(Face::Body, small, &l);
            self.text.draw(c, Face::Body, small, (c.w - w) / 2, y, colours.ink, &l);
            y += (small as f32 * 1.3) as i32;
        }
        y += (14.0 * u) as i32;
        // What was tried, small.
        let tiny = (34.0 * u).round() as u32;
        let file = self.text.fit(Face::Body, tiny, file, width);
        let w = self.text.width(Face::Body, tiny, &file);
        self.text.draw(c, Face::Body, tiny, (c.w - w) / 2, y, pal().dust, &file);
        y += (tiny as f32 * 1.3) as i32;
        for l in failure.detail.as_ref().map_or_else(Vec::new, |d| self.text.wrap_at_most(Face::Body, tiny, d, width, 2)) {
            let w = self.text.width(Face::Body, tiny, &l);
            self.text.draw(c, Face::Body, tiny, (c.w - w) / 2, y, pal().dust, &l);
            y += (tiny as f32 * 1.3) as i32;
        }
        let action = match failure.fix {
            big_shelf::Fix::GameCheck => "Game check",
            big_shelf::Fix::AddGames => "Add games",
        };
        self.paint_hint_row(c, &[("A", action), ("B", "Back")]);
    }

    /// The shelf: the case and the name as a child has them, the picture larger. The rest of the screen is `paint_home`'s.
    pub(crate) fn big_paint(&mut self, c: &mut Canvas) {
        let colours = colours();
        if let Some((failure, file)) = self.big.failure.clone() {
            self.cabinet.shelf_lit = None;
            return self.big_paint_failure(c, colours, &failure, &file);
        }
        self.cabinet.shelf_lit = None;
        let item = self.items[self.sel.min(self.items.len() - 1)];
        // Calmer than a child's shelf: the case, the name and the lines a little smaller, with more air between them.
        let below = self.kids_case_row(c, colours, SHELF_TOP, SHELF_ROOM);
        if self.launch_at.is_some() {
            return;
        }
        let (end, baseline) = self.kids_shelf_name_at(c, item, below, colours, NAME_SCALE);
        let u = c.h as f32 / 1080.0;
        if self.is_favorite(item) {
            App::paint_name_star(c, end, baseline, (50.0 * u) as u32);
        }
        // Under the name, calm and short: system · year · players, a line or two of what the game is, time and last
        // played. A line without data is left out, with no gap.
        let info = self.big_info(item);
        let wide = c.w - (80.0 * u) as i32;
        let mut line = baseline;
        if !info.top.is_empty() {
            let px = (30.0 * u).round() as u32;
            let sep = "  ·  ";
            // Each part is painted (and so translated) on its own; the last parts go first where there is no room.
            let mut parts: Vec<&str> = info.top.iter().map(String::as_str).collect();
            let sepw = self.text.width(Face::Body, px, sep);
            let total = |text: &mut pocket_ui::text::Text, parts: &[&str]| parts.iter().map(|p| text.width(Face::Body, px, p)).sum::<i32>() + sepw * (parts.len() as i32 - 1).max(0);
            while parts.len() > 1 && total(&mut self.text, &parts) > wide {
                parts.pop();
            }
            let mut x = (c.w - total(&mut self.text, &parts)) / 2;
            line += (50.0 * u) as i32;
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    x += self.text.draw(c, Face::Body, px, x, line, pal().dust, sep);
                }
                x += self.text.draw(c, Face::Body, px, x, line, pal().dust, part);
            }
        }
        if let Some(about) = info.about.as_deref() {
            let px = (28.0 * u).round() as u32;
            let room = ((c.w as f32 * 0.72) as i32).min(wide);
            for l in self.big_about_lines(about, px, room) {
                let w = self.text.width(Face::Body, px, &l);
                line += (40.0 * u) as i32;
                self.text.draw(c, Face::Body, px, (c.w - w) / 2, line, pal().chalk, &l);
            }
        }
        if let Some(played) = info.played.as_deref() {
            let px = (28.0 * u).round() as u32;
            let played = self.text.fit(Face::Body, px, played, wide);
            let w = self.text.width(Face::Body, px, &played);
            line += (44.0 * u) as i32;
            self.text.draw(c, Face::Body, px, (c.w - w) / 2, line, pal().dust, &played);
        }
        let moment = self.big_has_moment(item);
        if moment {
            self.big_paint_resume_mark(c, colours);
        }
        // The first open case ever on screen says what open means, once (`big_open_told`).
        self.big.open_hint_up = moment && !self.big.open_case_told;
        if self.big.open_hint_up {
            let px = (30.0 * u).round() as u32;
            let said = self.text.fit(Face::Body, px, "Open means your moment is waiting", c.w - (80.0 * u) as i32);
            let w = self.text.width(Face::Body, px, &said);
            self.text.draw(c, Face::Body, px, (c.w - w) / 2, line + (56.0 * u) as i32, pal().chalk, &said);
        }
        let word = if moment { "Continue" } else { "Play" };
        // Easy shows three hints: what plays, the quick settings, Settings. Details, Favourite and the jump between
        // systems are in the tour and the manual (`kit::top_hints`).
        let mut hints = vec![("A", word), ("HOME", "Quick settings"), ("X", "Details"), ("Y", "Favourite"), ("START", "Settings")];
        if self.big_jumps() {
            hints.push(("UP/DOWN", "Systems"));
        }
        let hints = pocket_ui::kit::top_hints(&hints, pocket_ui::kit::EASY_HINTS);
        self.paint_hint_row(c, &hints);
    }

    /// A description in at most two lines, cut after a whole sentence where one fits; a longer first sentence ends in "…".
    fn big_about_lines(&mut self, about: &str, px: u32, room: i32) -> Vec<String> {
        let mut kept = String::new();
        for sentence in about.split_inclusive(['.', '!', '?']) {
            let tried = format!("{kept}{sentence}");
            if self.text.wrap(Face::Body, px, tried.trim(), room).len() > 2 {
                break;
            }
            kept = tried;
        }
        let kept = kept.trim();
        if kept.is_empty() {
            self.text.wrap_at_most(Face::Body, px, about.trim(), room, 2)
        } else {
            self.text.wrap_prose(Face::Body, px, kept, room)
        }
    }

    /// The line about an open case has been on screen and the player went on (a step, or A): it never comes again.
    pub(crate) fn big_open_told(&mut self) {
        if std::mem::take(&mut self.big.open_hint_up) {
            self.big.open_case_told = true;
            self.save_settings();
        }
    }

    // ---- snapshots ----------------------------------------------------------------------------------------------

    /// "big-shelf[:<game>]", "big-twin", "big-back", "big-thread", "big-jump", "big-failed", "big-frozen", "big-details", "big-insert[:<ms>]", "big-pause[:stop|more|more-open|saving|forgets|retroarch|port|port-asked]": the Big
    /// shelf whatever the settings say; the covers are read before the picture. Says how much is on it on stdout.
    pub(crate) fn big_snapshot(&mut self, state: &str, background: Option<pocket_ui::image::Image>, w: u32, h: u32) -> bool {
        let Some(rest) = state.strip_prefix("big-") else { return false };
        let mut parts = rest.split(':');
        let (what, arg) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
        self.theme = Theme::Big;
        self.screen = Screen::Home;
        self.cabinet.inline = true;
        let t0 = Instant::now();
        self.rebuild();
        println!("big-shelf games={} groups={} rebuild_us={}", self.items.len(), self.big.groups.len(), t0.elapsed().as_micros());
        // Without a number: where the look opens.
        if let Ok(at) = arg.parse::<usize>() {
            self.sel = at.min(self.items.len().saturating_sub(1));
        }
        self.prev_sel = self.sel;
        self.moved_at = Instant::now() - Duration::from_secs(5);
        match what {
            "shelf" => {}
            // Two games of one file name: what the Resume list leaves out, the note as a RetroArch start shows it, and the
            // size of the pill pocket-core shows it in on this panel.
            "twin" => {
                println!("big-twin candidates={}", self.big_game_list().len());
                self.notice = Some(super::Notice::new(pocket_ui::frozen::SHARED));
                let _fixed = pocket_ui::type_scale::fixed();
                let (pw, ph, _) = pocket_ui::overlay::paint_pill_on(&mut self.text, pocket_ui::frozen::SHARED, super::snapshot_panel_dims().0);
                println!("big-twin pill={pw}x{ph}");
            }
            // The page's transition just begun, as B back from the card: says the colour it fades from.
            "back" => println!("big-back bg={:06x} neighbour-side={:06x}", page_colour(), {
                let mut buf = vec![0u32; (w * h) as usize];
                let mut c = Canvas::new(&mut buf, w, h);
                self.big_paint(&mut c);
                buf[(h / 2 * w + w - 2) as usize] & 0xffffff
            }),
            "details" => self.open_details(),
            // The covers come from the thread, as on a device: paints as the paint loop does until they stop coming.
            "thread" => {
                self.cabinet.inline = false;
                let mut buf = vec![0u32; (w * h) as usize];
                let mut c = Canvas::new(&mut buf, w, h);
                let mut frames = 0;
                loop {
                    self.big_paint(&mut c);
                    frames += 1;
                    if self.cabinet.pending == 0 || frames > 300 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                let ready = self.items.get(self.sel).copied().is_some_and(|item| self.kids_cover_ready(item));
                println!("big-cover ready={ready} frames={frames}");
            }
            "failed" => {
                let name = self.case_title(self.items.get(self.sel).copied().unwrap_or(Item::Search));
                let (system, notice) = match parts.next() {
                    Some("bios") => ("psx", pocket_ui::fallback::not_started_notice_on("psx", &name, Some("PlayStation needs a BIOS file in roms/bios: scph5501.bin"))),
                    Some("core") => ("gba", pocket_ui::fallback::core_missing_notice(&name)),
                    Some("port") => ("ports", pocket_ui::fallback::port_not_started_notice(&name)),
                    _ => ("psx", pocket_ui::fallback::not_started_notice_on("psx", &name, None)),
                };
                self.notice = Some(super::Notice::new(notice));
                self.big_note_failure(Path::new("/roms/Some Game (USA).chd"), system, &name);
            }
            // A game with a moment to go back to (the list is read on a thread on a device).
            "frozen" => {
                let path = self.items.get(self.sel).and_then(|i| self.item_rom(*i)).map(|(_, p)| p);
                self.big.moments = path.into_iter().collect();
            }
            "jump" => {
                self.big_step(Button::Down, false);
                self.moved_at = Instant::now() - Duration::from_secs(5);
            }
            "insert" => {
                let mut buf = vec![0u32; (w * h) as usize];
                let mut c = Canvas::new(&mut buf, w, h);
                self.big_paint(&mut c);
                self.launch_animation_snapshot(Duration::from_millis(arg.parse().unwrap_or(900)));
            }
            "pause" => {
                let Some(Item::Game(s, g)) = self.items.first().copied() else { return false };
                let (system, name, path) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
                let emulator = match arg {
                    "retroarch" => "retroarch",
                    "port" | "port-asked" => "ports",
                    _ => "pocket-core",
                };
                self.running = Some(super::Running { system, name, path, emulator: emulator.into(), core: String::new(), started: Instant::now(), pgid: None, options: Default::default() });
                self.open_menu();
                if let Some(m) = self.menu.as_mut() {
                    m.frame = background;
                    m.quitting = arg == "saving";
                    m.without_saving = m.without_saving || arg == "forgets";
                }
                self.big.asked = arg == "port-asked";
                self.kids.stop_selected = arg == "stop" || arg == "port-asked";
                self.big.more_selected = arg == "more";
                self.big.more_open = arg == "more-open";
            }
            _ => return false,
        }
        true
    }
}

/// Only a game that saves nothing but in the game asks before it quits.
fn asks_before_quit(saves: pocket_ui::pause_saves::Saves) -> bool {
    matches!(saves, pocket_ui::pause_saves::Saves::InGame(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocket_ui::pause_saves::Saves;

    #[test]
    fn the_pause_screens_three_rows_end_above_the_line_and_the_hint() {
        // In a 1080-high screen: the line (two lines of type) starts about 135 + 2 * 55 above the bottom.
        let last_row_ends = PAUSE_TOP + 3.0 * PAUSE_ROW + 2.0 * PAUSE_GAP;
        assert!(last_row_ends < 1080.0 - 135.0 - 2.0 * 55.0 - 20.0, "{last_row_ends}");
        assert!(PAUSE_ROW > 120.0 && PAUSE_GAP >= 20.0, "rows a thumb can hit, with air between");
        // Smaller than a child's two buttons (210 high, 40 apart), as the shelf's case is smaller than the child's.
        assert!(PAUSE_ROW < 210.0 && SHELF_ROOM.0 < 0.64 && SHELF_ROOM.1 < 0.61 && NAME_SCALE < 1.0);
    }

    #[test]
    fn only_a_game_that_saves_in_the_game_asks_before_quitting() {
        assert!(asks_before_quit(Saves::InGame("This emulator keeps its own saves.")));
        assert!(!asks_before_quit(Saves::Slots), "Continue where I quit off: no extra press");
        assert!(!asks_before_quit(Saves::One));
    }
}

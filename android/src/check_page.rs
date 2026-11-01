//! Settings → Game check in the phone layout: the state and what a press does are the shared `game_check_page`, the check
//! itself `document_check`, run on a worker by the host (`Command::Check`). Words and big buttons here; every state has
//! something to read and a way on, also before a folder is chosen, with no games and when Android took a folder back.
use super::phone::{MenuRow, Room};
use super::*;
use pocket_ui::document_check::{Outcome, Unreadable};
use pocket_ui::game_check::Group;
use pocket_ui::game_check_page::{Action as PageAction, Event, GameCheckPage, Level};
use pocket_ui::ui_art::Art;

#[derive(Default)]
pub struct CheckUi {
    page: Option<GameCheckPage>,
    unreadable: Vec<Unreadable>,
    no_folder: bool,
    /// The Settings row's value: what the last finished check found, this session.
    last: Option<String>,
}

impl Ui {
    /// The Settings row: the page, looking at once.
    pub(super) fn open_check(&mut self) -> Command {
        self.check.page = Some(GameCheckPage::running());
        self.check.unreadable.clear();
        self.check.no_folder = false;
        self.navigate(Screen::GameCheck);
        Command::Check
    }
    /// The page looks again (the folders changed under it).
    pub fn begin_check(&mut self) {
        self.check.page = Some(GameCheckPage::running());
        self.check.unreadable.clear();
        self.check.no_folder = false;
        self.menu = 0;
        self.scroll = 0.0;
    }
    /// A folder was chosen and `added` new games are on the shelf: the page offers to look at them.
    pub fn offer_check(&mut self, added: usize) {
        self.check.page = Some(GameCheckPage::offer(added));
        self.navigate(Screen::GameCheck);
    }
    /// The page is waiting for a check it started: its dots move, and the host stops the worker when it is left.
    pub fn check_running(&self) -> bool {
        self.screen == Screen::GameCheck && self.check.page.as_ref().is_some_and(|p| p.level == Level::Running)
    }
    pub fn check_open(&self) -> bool {
        self.screen == Screen::GameCheck
    }
    pub fn check_progress(&mut self, done: usize, total: usize, system: String) {
        if let Some(page) = &mut self.check.page {
            page.event(Event::Progress { done, total, system });
        }
    }
    pub fn check_failed(&mut self) {
        if let Some(page) = &mut self.check.page {
            page.event(Event::Failed);
        }
    }
    pub fn check_done(&mut self, outcome: Outcome) {
        self.check.last = (!outcome.no_folder).then(|| outcome.report.summary());
        self.check.no_folder = outcome.no_folder;
        self.check.unreadable = outcome.unreadable;
        if let Some(page) = &mut self.check.page {
            page.event(Event::Done(outcome.report));
        }
        self.menu = 0;
        self.scroll = 0.0;
        self.follow = true;
    }
    /// The Settings row's value.
    pub(super) fn check_value(&self) -> String {
        self.check.last.clone().unwrap_or_default()
    }
    /// Nothing but a folder that can't be read: the whole page is about it.
    fn only_unreadable(&self) -> Option<&Unreadable> {
        let report = self.check.page.as_ref()?.report.as_ref()?;
        (self.check.unreadable.len() == 1 && report.total == 0 && report.findings.iter().all(|f| f.group == Group::Access)).then(|| &self.check.unreadable[0])
    }
    /// Nothing to list: no folder yet, or no game in the folders.
    fn nothing_to_list(&self) -> bool {
        self.check.no_folder || self.check.page.as_ref().and_then(|p| p.report.as_ref()).is_some_and(|r| r.total == 0 && r.findings.is_empty())
    }
    /// The folder the open page is about: a detail's, or the whole page's.
    fn folder_on_page(&self) -> Option<&Unreadable> {
        let page = self.check.page.as_ref()?;
        match page.level {
            Level::Detail(Group::Access) => {
                let name = &page.finding()?.name;
                self.check.unreadable.iter().find(|u| u.name == *name)
            }
            Level::Groups => self.only_unreadable(),
            _ => None,
        }
    }
    pub(super) fn check_press(&mut self, b: Button) -> Command {
        if matches!(b, Button::Up | Button::Down) {
            self.move_by(if b == Button::Up { -1 } else { 1 });
            return Command::None;
        }
        // A: choose the folder again; Y: forget it.
        if let Some(uri) = self.folder_on_page().map(|u| u.uri.clone()) {
            match b {
                Button::East => return Command::Folder,
                Button::West => return Command::Forget(uri),
                _ => {}
            }
        }
        if b == Button::East && self.check.page.as_ref().is_some_and(|p| p.level == Level::Groups) && self.nothing_to_list() {
            return Command::Folder;
        }
        let menu = self.menu;
        let Some(page) = self.check.page.as_mut() else {
            self.navigate(Screen::Settings);
            return Command::None;
        };
        match page.level {
            Level::Groups => page.group_sel = menu,
            Level::Items(g) => {
                // The "and N more" row opens nothing.
                if b == Button::East && menu >= page.items(g).len() {
                    return Command::None;
                }
                page.item_sel = menu;
            }
            _ => {}
        }
        let before = page.level;
        let action = page.press(b, Button::East, Button::South);
        let after = page.level;
        let at = match after {
            Level::Items(_) => page.item_sel,
            Level::Groups => page.group_sel,
            _ => 0,
        };
        match action {
            PageAction::Close => {
                self.check.page = None;
                self.navigate(Screen::Settings);
                Command::None
            }
            PageAction::Start => {
                self.check.unreadable.clear();
                self.check.no_folder = false;
                self.menu = 0;
                self.scroll = 0.0;
                Command::Check
            }
            _ => {
                if before != after {
                    self.menu = at;
                    self.scroll = 0.0;
                    self.follow = true;
                }
                Command::None
            }
        }
    }
    /// The rows of the list on screen.
    pub(super) fn check_rows(&self) -> Vec<MenuRow> {
        let Some(page) = &self.check.page else { return Vec::new() };
        let row = |label: String, value: String| MenuRow { label, value, check: None };
        match page.level {
            Level::Groups if !self.nothing_to_list() && self.only_unreadable().is_none() => page.groups().iter().map(|(g, n)| row(tr(g.name()), n.to_string())).collect(),
            Level::Items(g) => {
                let mut rows: Vec<_> = page.items(g).iter().map(|f| row(f.name.clone(), f.system.clone())).collect();
                if let Some((_, more)) = page.report.as_ref().and_then(|r| r.more.iter().find(|(m, _)| *m == g)) {
                    rows.push(row(tr(&format!("And {more} more. Fix these first, then check again.")), String::new()));
                }
                rows
            }
            _ => Vec::new(),
        }
    }
    /// The page's title.
    pub(super) fn check_title(&self) -> String {
        let Some(page) = &self.check.page else { return tr("Game check") };
        match page.level {
            Level::Items(g) => tr(g.name()),
            Level::Detail(_) => page.finding().map_or_else(|| tr("Game check"), |f| f.name.clone()),
            _ => tr("Game check"),
        }
    }
    /// A row of big buttons at the bottom of the page: returns the room above it.
    fn check_buttons(&mut self, c: &mut Canvas, text: &mut Text, room: Room, buttons: &[(&str, Action, bool)]) -> Room {
        let h = self.d(48.0);
        let gap = self.d(8.0);
        let y = room.bottom - h - gap;
        let n = buttons.len() as i32;
        let w = (c.w - 2 * room.margin - gap * (n - 1)) / n;
        for (i, (label, action, primary)) in buttons.iter().enumerate() {
            self.pill(c, text, Area::new(room.margin + i as i32 * (w + gap), y, w, h), &tr(label), *primary, action.clone());
        }
        Room { bottom: y - gap, ..room }
    }
    pub(super) fn paint_check(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let back = ("Back", Action::Press(Button::South), false);
        let again = ("Check again", Action::Press(Button::North), true);
        let choose = ("Choose your ROM folder", Action::Folder, true);
        let Some(level) = self.check.page.as_ref().map(|p| p.level) else { return };
        match level {
            Level::Offer(n) => {
                let room = self.check_buttons(c, text, room, &[("Not now", Action::Press(Button::South), false), ("Check", Action::Press(Button::East), true)]);
                let head = if n == 1 { tr("1 new game") } else { tr(&format!("{n} new games")) };
                self.empty_page(c, text, room, Art::Search, &head, &tr("Want to see if they will start? This takes a moment and changes nothing."), None);
            }
            Level::Running => {
                let room = self.check_buttons(c, text, room, &[("Stop", Action::Press(Button::South), false)]);
                let (done, total, system) = self.check.page.as_ref().map(|p| p.progress.clone()).unwrap_or_default();
                let ticks = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() / 400 % 3) as usize;
                let head = tr(&format!("Looking at your games{}", ".".repeat(1 + ticks)));
                let line = if total == 0 {
                    tr("Reading your folders. Nothing is changed.")
                } else if system.is_empty() {
                    tr(&format!("{done} of {total} games"))
                } else {
                    tr(&format!("{done} of {total} games · {system}"))
                };
                let bar_h = self.d(8.0);
                let bar_y = room.bottom - bar_h - self.d(12.0);
                self.empty_page(c, text, Room { bottom: bar_y - self.d(8.0), ..room }, Art::Search, &head, &line, None);
                if total > 0 {
                    let w = c.w - 2 * room.margin;
                    let frac = (done as f32 / total as f32).clamp(0.0, 1.0);
                    c.round_rect(room.margin, bar_y, w, bar_h, bar_h / 2, pal().keycap);
                    c.round_rect(room.margin, bar_y, ((w as f32 * frac) as i32).max(bar_h), bar_h, bar_h / 2, pal().label);
                }
            }
            Level::Failed => {
                let room = self.check_buttons(c, text, room, &[back, again]);
                self.empty_page(c, text, room, Art::Search, &tr("Can't read your folders"), &tr("Your games are safe. Try again in a moment."), None);
            }
            Level::Groups => self.paint_check_groups(c, text, room, back, again, choose),
            Level::Items(_) => {
                let room = self.check_buttons(c, text, room, &[back]);
                let rows = self.check_rows();
                self.paint_menu_phone(c, text, room, rows);
            }
            Level::Detail(_) => {
                let forget = ("Forget this folder", Action::Press(Button::West), false);
                let room = if self.folder_on_page().is_some() { self.check_buttons(c, text, room, &[forget, ("Choose it again", Action::Folder, true)]) } else { self.check_buttons(c, text, room, &[back]) };
                self.paint_check_detail(c, text, room);
            }
        }
    }
    fn paint_check_groups(&mut self, c: &mut Canvas, text: &mut Text, room: Room, back: (&str, Action, bool), again: (&str, Action, bool), choose: (&str, Action, bool)) {
        let Some(report) = self.check.page.as_ref().and_then(|p| p.report.clone()) else {
            if self.check.no_folder {
                let room = self.check_buttons(c, text, room, &[back, choose]);
                self.empty_page(c, text, room, Art::NoGames, &tr("No folder yet"), &tr("Choose the folder with your games, and Game check looks them over."), None);
            }
            return;
        };
        if self.check.no_folder {
            let room = self.check_buttons(c, text, room, &[back, choose]);
            self.empty_page(c, text, room, Art::NoGames, &tr("No folder yet"), &tr("Choose the folder with your games, and Game check looks them over."), None);
            return;
        }
        if self.only_unreadable().is_some() {
            let forget = ("Forget this folder", Action::Press(Button::West), false);
            let room = self.check_buttons(c, text, room, &[forget, ("Choose it again", Action::Folder, true)]);
            let line = report.findings.first().map_or(String::new(), |f| format!("{} {}", tr(&f.what), tr(&f.todo)));
            self.empty_page(c, text, room, Art::Search, &tr("Can't read your folder"), &line, None);
            return;
        }
        if self.nothing_to_list() {
            let room = self.check_buttons(c, text, room, &[back, choose]);
            self.empty_page(c, text, room, Art::NoGames, &tr("No games found"), &tr("Nothing in your folders is a game yet. Game folders are named after their system, like gba or psx."), None);
            return;
        }
        let mut room = self.check_buttons(c, text, room, &[back, again]);
        let rows = self.check_rows();
        if report.all_good() {
            // The moment: big words, nothing else. A tip is one quiet row under it.
            if !rows.is_empty() {
                let pitch = self.d(60.0);
                let below = Room { top: room.bottom - pitch, ..room };
                room.bottom = below.top;
                self.paint_menu_phone(c, text, below, rows);
            }
            let ready = if report.total == 1 { tr("1 game ready") } else { tr(&format!("{} games ready", report.total)) };
            self.empty_page(c, text, room, Art::Logo, &tr("All good"), &ready, None);
            return;
        }
        let size = self.sp(15.0);
        let line = tr(&format!("{} of {} games ready", report.ready, report.total));
        let base = room.top + size as i32;
        text.draw(c, Face::Body, size, room.margin, base, pal().dust, &line);
        self.target(Area::new(room.margin, room.top, c.w - 2 * room.margin, size as i32 + self.d(4.0)), &line, Action::Label);
        self.paint_menu_phone(c, text, Room { top: base + self.d(8.0), ..room }, rows);
    }
    /// One finding: what is wrong, what to do, where it is; it scrolls when the words don't fit.
    fn paint_check_detail(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let Some(f) = self.check.page.as_ref().and_then(|p| p.finding()).cloned() else { return };
        let w = c.w - 2 * room.margin;
        let (size, small) = (self.sp(17.0), self.sp(14.0));
        let (step, small_step) = (size as i32 + self.d(8.0), small as i32 + self.d(6.0));
        let (what, todo) = (tr(&f.what), tr(&f.todo));
        let mut lines: Vec<(String, u32, i32, u32)> = Vec::new();
        for l in text.wrap(Face::Body, size, &what, w) {
            lines.push((l, size, step, pal().chalk));
        }
        if !todo.is_empty() {
            lines.push((String::new(), size, self.d(12.0), 0));
            for l in text.wrap(Face::Body, size, &todo, w) {
                lines.push((l, size, step, pal().text));
            }
        }
        if let Some(place) = &f.place {
            lines.push((String::new(), size, self.d(14.0), 0));
            for l in text.wrap(Face::Body, small, place, w) {
                lines.push((l, small, small_step, pal().dust));
            }
        }
        let height: i32 = lines.iter().map(|l| l.2).sum();
        let view = room.bottom - room.top;
        self.scroll_max = (height + self.d(8.0) - view).max(0) as f32;
        self.scroll = self.scroll.clamp(0.0, self.scroll_max);
        let mut y = self.d(4.0) - self.scroll.round() as i32;
        {
            let mut clip = c.rows(room.top, room.bottom);
            for (line, s, h, colour) in &lines {
                y += h;
                if !line.is_empty() && y > -*h && y - h < clip.h {
                    text.draw(&mut clip, Face::Body, *s, room.margin, y - self.d(4.0), *colour, line);
                }
            }
        }
        // What TalkBack (and the emulator's checks) read.
        self.target(Area::new(room.margin, room.top, w, (view / 2).max(1)), &what, Action::Label);
        if !todo.is_empty() {
            self.target(Area::new(room.margin, room.top + view / 2, w, (view / 2).max(1)), &todo, Action::Label);
        }
    }
}

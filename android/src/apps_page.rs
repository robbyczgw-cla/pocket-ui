//! The drawer under the shelf (`Screen::Apps`): the phone's other apps as quiet cards with small, muted icons, and
//! the ones the player calls games as the Android shelf beside the consoles. Android's own "game" category is only a
//! hint on the card; the player decides (Y, or a held finger). See docs/ANDROID-MVP.md "Android apps".
use super::phone::{type_mid, ClipY, Room};
use super::*;
use pocket_ui::type_scale;

/// The system the Android shelf's games stand under (`media::is_digital`: a case without a cartridge).
pub const SYSTEM_ID: &str = "android";
pub const SYSTEM: &str = "Android";
/// A game key of the Android shelf: never a document identity (those are `authority:documentId`).
const KEY_PREFIX: &str = "android-app:";
/// The Android shelf's stripe: a muted green, quieter than Android's own.
const STRIPE: u32 = 0x5f8f6a;

/// An app Android lists with a start icon.
#[derive(Clone, Debug, PartialEq)]
pub struct AndroidApp {
    pub package: String,
    pub label: String,
    /// Android files it under games (`ApplicationInfo.CATEGORY_GAME`): a suggestion on the card, nothing more.
    pub game: bool,
    pub icon: Option<AppIcon>,
}

/// An app's icon, already muted (`mute`): 0xAARRGGBB.
#[derive(Clone, Debug, PartialEq)]
pub struct AppIcon {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

#[derive(Default)]
pub struct AppsUi {
    /// What Android listed, by name; None until its first answer.
    pub list: Option<Vec<AndroidApp>>,
    pub selected: usize,
    /// Cards a row of the last paint held.
    cols: usize,
}

/// The apps of `AppDrawer.list`'s answer, by name as a home screen orders them. `icon` reads an icon file.
pub fn parse(json: &str, icon: impl Fn(&str) -> Option<AppIcon>) -> Vec<AndroidApp> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let mut apps: Vec<AndroidApp> = items
        .iter()
        .filter_map(|v| {
            let package = v["package"].as_str().filter(|p| !p.is_empty())?.to_string();
            let label = v["label"].as_str().map(str::trim).filter(|l| !l.is_empty()).unwrap_or(&package).to_string();
            let path = v["icon"].as_str().unwrap_or_default();
            Some(AndroidApp { package, label, game: v["game"].as_bool().unwrap_or(false), icon: (!path.is_empty()).then(|| icon(path)).flatten() })
        })
        .collect();
    apps.sort_by_cached_key(|a| (a.label.to_lowercase(), a.package.clone()));
    apps.dedup_by(|a, b| a.package == b.package);
    apps
}

/// An icon file of `AppDrawer.writeIcon` (two big-endian ints, width and height, then ARGB pixels), muted. A file
/// that says something else is no icon.
pub fn read_icon(path: &str) -> Option<AppIcon> {
    let bytes = std::fs::read(path).ok()?;
    let int = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let (w, h) = (int(0)?, int(4)?);
    if w == 0 || h == 0 || w > 512 || h > 512 || bytes.len() != 8 + 4 * (w * h) as usize {
        return None;
    }
    let px = (0..(w * h) as usize).map(|i| int(8 + 4 * i).unwrap_or(0)).collect();
    Some(AppIcon { w, h, px: mute(px) })
}

/// Bright app colours, quietened for the shelf: four fifths grey (by brightness), a little less opaque.
pub fn mute(px: Vec<u32>) -> Vec<u32> {
    px.into_iter()
        .map(|p| {
            let a = p >> 24;
            let (r, g, b) = ((p >> 16) & 0xff, (p >> 8) & 0xff, p & 0xff);
            let y = (r * 299 + g * 587 + b * 114) / 1000;
            let grey = (y << 16) | (y << 8) | y;
            (a * 85 / 100) << 24 | pocket_ui::gfx::mix(p & 0xff_ffff, grey, 200)
        })
        .collect()
}

/// The Android shelf's game for an app.
pub fn game_of(package: &str, label: &str) -> Game {
    Game {
        name: label.to_string(),
        key: format!("{KEY_PREFIX}{package}"),
        uri: format!("{KEY_PREFIX}{package}"),
        relative: label.to_string(),
        system: SYSTEM,
        system_id: SYSTEM_ID,
        profile: "",
        color: STRIPE,
        missing: Vec::new(),
        size: -1,
        document_size: -1,
        covers: Vec::new(),
        pictures: Vec::new(),
        cheats: String::new(),
        parts: Vec::new(),
    }
}

/// The app a game of the Android shelf starts; None for every other game.
pub fn package_of(game: &Game) -> Option<&str> {
    game.key.strip_prefix(KEY_PREFIX).filter(|_| game.system_id == SYSTEM_ID)
}

/// Puts `games` (the Android shelf's) at the end of the library in place of the ones there; false when nothing changed.
/// Curator's index and the connections know only the chosen folders' games; search finds both.
fn put_android_games(lib: &mut Library, games: Vec<Game>) -> bool {
    let docs = lib.games.iter().position(|g| g.system_id == SYSTEM_ID).unwrap_or(lib.games.len());
    if lib.games[docs..].iter().map(|g| (&g.key, &g.name)).eq(games.iter().map(|g| (&g.key, &g.name))) {
        return false;
    }
    lib.games.truncate(docs);
    lib.cards.truncate(docs);
    for game in games {
        lib.cards.push(CardData { title: game.name.clone(), system: SYSTEM.into(), stripe: game.color, ..Default::default() });
        lib.games.push(game);
    }
    lib.search = pocket_ui::search::Index::build(lib.games.iter().enumerate().map(|(i, g)| (0, i, g.name.as_str())));
    true
}

impl Ui {
    /// The games of the Android shelf: the marked apps Android still lists (before its first answer, as they were
    /// marked), by name.
    fn android_shelf(&self) -> Vec<Game> {
        let mut marked: Vec<(&str, &str)> = match &self.apps.list {
            Some(list) => list.iter().filter(|a| self.prefs.android_games.contains_key(&a.package)).map(|a| (a.package.as_str(), a.label.as_str())).collect(),
            None => self.prefs.android_games.iter().map(|(p, l)| (p.as_str(), l.as_str())).collect(),
        };
        marked.sort_by_cached_key(|(p, l)| (l.to_lowercase(), p.to_string()));
        marked.into_iter().map(|(p, l)| game_of(p, l)).collect()
    }
    /// A new library from the chosen folders takes the Android shelf along (`install`).
    pub(super) fn with_android_shelf(&self, lib: &mut Library) {
        put_android_games(lib, self.android_shelf());
    }
    /// The Android shelf after a mark or Android's list changed: the selection stays on its game, here and on the page
    /// under this one.
    pub fn sync_android_shelf(&mut self) {
        let games = self.android_shelf();
        if self.library.is_none() {
            // Only Android games, no folder chosen yet: they stand on the Games page all the same.
            if games.is_empty() || !self.prefs.onboarding_done {
                return;
            }
            self.library = Some(library(Vec::new()));
        }
        let key = self.game().map(|g| g.key.clone());
        let row = self.shelf_row().map(|r| r.key);
        if !put_android_games(self.library.as_mut().unwrap(), games) {
            return;
        }
        self.cards.clear();
        // The list under this page: the one Back comes to (a card's, the drawer's: the Games page).
        let base = if Self::list_screen(self.screen) {
            self.screen
        } else if matches!(self.screen, Screen::Box | Screen::Stickers) && Self::list_screen(self.prior) {
            self.prior
        } else {
            Screen::Library
        };
        let screen = std::mem::replace(&mut self.screen, base);
        self.filter();
        let at = key.and_then(|k| self.library.as_ref()?.games.iter().position(|g| g.key == k));
        if let Some(game) = at {
            let place = self.rows.as_ref().map_or_else(|| self.visible.iter().position(|g| *g == game), |s| s.place_of(game, row));
            self.selected = place.unwrap_or(0);
        }
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        if base != screen {
            self.positions.insert(base, self.selected);
        }
        self.screen = screen;
    }
    /// Android's list of apps came (at the start and whenever the launcher comes back): names of marked games follow
    /// their app's, the shelf loses apps that went.
    pub fn apps_got(&mut self, list: Vec<AndroidApp>) {
        for app in &list {
            if let Some(label) = self.prefs.android_games.get_mut(&app.package) {
                if *label != app.label {
                    label.clone_from(&app.label);
                }
            }
        }
        // The same app keeps the selection when the list around it changed.
        let at = self.apps.list.as_ref().and_then(|l| l.get(self.apps.selected)).map(|a| a.package.clone());
        self.apps.selected = at.and_then(|p| list.iter().position(|a| a.package == p)).unwrap_or(self.apps.selected).min(list.len().saturating_sub(1));
        self.apps.list = Some(list);
        self.sync_android_shelf();
    }
    pub(super) fn open_apps(&mut self) -> Command {
        self.navigate(Screen::Apps);
        Command::None
    }
    pub(super) fn selected_app(&self) -> Option<&AndroidApp> {
        self.apps.list.as_ref()?.get(self.apps.selected)
    }
    /// The drawer's keys; None for the ones every page shares (Start, Select, L1/R1).
    pub(super) fn apps_press(&mut self, b: Button) -> Option<Command> {
        let n = self.apps.list.as_ref().map_or(0, Vec::len);
        let cols = self.apps.cols.max(1) as i32;
        let step = |s: &mut Self, d: i32| {
            let to = s.apps.selected as i32 + d;
            if n > 0 && (0..n as i32).contains(&to) {
                s.apps.selected = to as usize;
            }
            s.follow = true;
        };
        Some(match b {
            Button::South => {
                self.navigate(Screen::Library);
                Command::None
            }
            Button::Left | Button::Right => {
                step(self, if b == Button::Left { -1 } else { 1 });
                Command::None
            }
            Button::Up | Button::Down => {
                step(self, if b == Button::Up { -cols } else { cols });
                Command::None
            }
            Button::East => self.selected_app().map_or(Command::None, |a| Command::OpenApp(a.package.clone())),
            Button::West => self.toggle_app_game(),
            Button::Start | Button::Select | Button::L1 | Button::R1 => return None,
            _ => Command::None,
        })
    }
    /// Y on an app, or a finger held on it: onto the Android shelf, or off it.
    pub(super) fn toggle_app_game(&mut self) -> Command {
        let Some(app) = self.selected_app().cloned() else { return Command::None };
        if self.prefs.android_games.remove(&app.package).is_none() {
            self.prefs.android_games.insert(app.package, app.label);
        }
        self.sync_android_shelf();
        Command::Persist
    }
    /// A tap on a card opens the app; a held finger marks it (`long_press`).
    pub(super) fn app_tap(&mut self, i: usize, hold: bool) -> Command {
        self.apps.selected = i;
        if hold {
            self.toggle_app_game()
        } else {
            self.apps_press(Button::East).unwrap_or(Command::None)
        }
    }
    /// What the card says under the app's name: on the shelf, or that Android files it under games.
    fn app_note(&self, app: &AndroidApp) -> &'static str {
        if self.prefs.android_games.contains_key(&app.package) {
            "On the shelf"
        } else if app.game {
            "Looks like a game"
        } else {
            ""
        }
    }
    /// The line above the cards: how to open and how to mark, and that nothing is marked yet.
    fn apps_line(&self) -> &'static str {
        match (self.prefs.android_games.is_empty(), self.controller_hints()) {
            (true, false) => "No Android games on the shelf yet. Hold a game to put it there. Tap an app to open it.",
            (true, true) => "No Android games on the shelf yet. Press Y on a game to put it there.",
            (false, false) => "Tap an app to open it. Hold a game to put it on the Android shelf or take it off.",
            (false, true) => "Y puts a game on the Android shelf or takes it off.",
        }
    }
    pub(super) fn apps_note(&self) -> String {
        match self.apps.list.as_ref().map(Vec::len) {
            Some(1) => tr("1 app"),
            Some(n) if n > 0 => tr(&format!("{n} apps")),
            _ => String::new(),
        }
    }
    /// The drawer: a line on how it works, then the apps as cards in rows, scrolling. Reading and an empty list say so.
    pub(super) fn paint_apps(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let Some(count) = self.apps.list.as_ref().map(Vec::len) else {
            self.empty_page(c, text, room, pocket_ui::ui_art::Art::NoGames, "Reading your apps...", "", None);
            return;
        };
        if count == 0 {
            let line = tr("Android lists no other apps with a start icon. Apps you install show up here.");
            self.empty_page(c, text, room, pocket_ui::ui_art::Art::NoGames, "No apps found", &line, None);
            return;
        }
        let margin = room.margin;
        let size = self.sp(14.0);
        let line_h = type_scale::px(size) as i32 + self.d(4.0);
        let line = tr(self.apps_line());
        let lines = text.wrap_at_most(Face::Body, size, &line, c.w - 2 * margin, 5);
        if lines.last().is_some_and(|l| l.ends_with('…')) {
            self.cut.push(line.clone());
        }
        let mut y = room.top;
        for l in &lines {
            y += line_h;
            text.draw(c, Face::Body, size, margin, y - self.d(4.0), pal().dust, l);
        }
        self.target(Area::new(margin, room.top, c.w - 2 * margin, y - room.top), &line, Action::Label);
        let top = y + self.d(10.0);
        let gap = self.d(10.0);
        let cols = (((c.w - 2 * margin + gap) / (self.d(220.0) + gap)).max(1) as usize).min(count);
        self.apps.cols = cols;
        let name_size = self.sp(16.0);
        let note_size = self.sp(13.0);
        let card_h = self.d(68.0).max(type_scale::px(name_size) as i32 + type_scale::px(note_size) as i32 + self.d(26.0));
        let card_w = (c.w - 2 * margin - (cols as i32 - 1) * gap) / cols as i32;
        let pitch = card_h + gap;
        let view = room.bottom - top;
        let rows = count.div_ceil(cols) as i32;
        self.scroll_max = (rows * pitch - gap - view).max(0) as f32;
        self.step = (0.0, 0.0);
        self.apps.selected = self.apps.selected.min(count - 1);
        if self.follow {
            self.reveal((self.apps.selected / cols) as i32 * pitch, card_h, view);
        }
        self.scroll = self.scroll.clamp(0.0, self.scroll_max);
        let offset = self.scroll.round() as i32;
        let hints = self.controller_hints();
        let icon_s = self.d(30.0);
        let mut targets = Vec::new();
        {
            let mut clip = c.rows(top, room.bottom);
            let apps = self.apps.list.as_ref().unwrap();
            for (i, app) in apps.iter().enumerate() {
                let (col, row) = ((i % cols) as i32, (i / cols) as i32);
                let (x, cy) = (margin + col * (card_w + gap), row * pitch - offset);
                if cy + card_h < 0 || cy > clip.h {
                    continue;
                }
                let selected = hints && i == self.apps.selected;
                kit::shelf_row(&mut clip, x, cy, card_w, card_h, selected);
                let (fg, quiet) = if selected { (pal().ink, pal().ink) } else { (pal().chalk, pal().dust) };
                let ix = x + self.d(12.0);
                if let Some(icon) = &app.icon {
                    paint_icon(&mut clip, icon, ix, cy + (card_h - icon_s) / 2, icon_s);
                }
                let tx = ix + icon_s + self.d(12.0);
                let tw = x + card_w - self.d(10.0) - tx;
                let note = self.app_note(app);
                let name_base = if note.is_empty() { cy + card_h / 2 + type_mid(name_size) } else { cy + card_h / 2 - self.d(2.0) };
                let name = text.fit_name(Face::Body, name_size, &app.label, tw);
                text.draw_name(&mut clip, Face::Body, name_size, tx, name_base, fg, &name);
                let mut spoken = app.label.clone();
                if !note.is_empty() {
                    let said = tr(note);
                    let shown = text.fit(Face::Body, note_size, note, tw);
                    if shown != said {
                        self.cut.push(said.clone());
                    }
                    text.draw_name(&mut clip, Face::Body, note_size, tx, cy + card_h / 2 + self.d(18.0), quiet, &shown);
                    spoken = format!("{spoken} · {said}");
                }
                targets.push((Area::new(x, top + cy, card_w, card_h).clip_y(top, room.bottom), spoken, i));
            }
        }
        for (area, label, i) in targets {
            if area.h > self.d(12.0) {
                self.regions.push(Region { area, label, action: Action::App(i) });
            }
        }
        self.scrollbar(c, top, view, rows * pitch - gap);
    }
}

/// An icon `s` square at (x, y), over what is there (its own transparency kept).
fn paint_icon(c: &mut Canvas, icon: &AppIcon, x: i32, y: i32, s: i32) {
    for dy in 0..s {
        let sy = (dy as u32 * icon.h / s as u32) as usize;
        for dx in 0..s {
            let p = icon.px[sy * icon.w as usize + (dx as u32 * icon.w / s as u32) as usize];
            let a = (p >> 24) as u8;
            if a > 0 {
                c.blend(x + dx, y + dy, p & 0xff_ffff, a);
            }
        }
    }
}

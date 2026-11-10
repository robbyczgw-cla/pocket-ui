//! Android presentation controller. Media, box faces, typography, palette, rows, search and Curator are shared.
use pocket_ui::{
    cabinet_art as art,
    document_library::Game,
    game_card_model::CardData,
    gfx::Canvas,
    image::Image,
    keyboard::{Key, Keyboard, Outcome},
    kit,
    media::{self, Medium},
    pad::Button,
    palette::{self, pal},
    settings::Theme,
    text::{Face, Text},
    touch::{Area, HitMap},
};
use std::{collections::HashSet, sync::Arc};
#[path = "check_page.rs"]
mod check_page;
#[path = "onboarding.rs"]
mod onboarding;
#[path = "phone.rs"]
mod phone;
#[path = "play_cards.rs"]
mod play_cards;

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub light: bool,
    pub selected_uri: String,
    pub selected_key: String,
    pub theme: Theme,
    pub favorites: HashSet<String>,
    /// Settings → Language, once chosen (`chosen_language`).
    pub language: Option<pocket_ui::i18n::Lang>,
    /// What an older version wrote for the same choice; read, and gone once a language is chosen again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub german: Option<bool>,
    pub text_size: pocket_ui::text_size::TextSize,
    pub muted: bool,
    pub haptics: bool,
    pub remind_where: Option<bool>,
    pub gameboy_gambatte: bool,
    pub onboarding_done: bool,
    pub onboarding_step: usize,
    pub cozy: Option<bool>,
    pub download_media: Option<bool>,
    /// The look settings of the shared Linux pages that exist on a phone (`apply_look`); a wrong value is the start value.
    #[serde(deserialize_with = "lenient")]
    pub colours: pocket_ui::palette::Colours,
    #[serde(deserialize_with = "lenient")]
    pub font: pocket_ui::font_style::FontStyle,
    #[serde(deserialize_with = "lenient")]
    pub retro_colours: pocket_ui::retro::Colours,
    #[serde(deserialize_with = "lenient")]
    pub gba_model: pocket_ui::system_art::GbaModel,
    #[serde(deserialize_with = "lenient")]
    pub prompts: pocket_ui::button_prompts::PromptStyle,
    #[serde(deserialize_with = "lenient")]
    pub motion: pocket_ui::settings::Motion,
    #[serde(deserialize_with = "lenient")]
    pub extras: pocket_ui::cozy_extras::Extras,
    /// When each game was last started (unix seconds by game key): Rediscover's input.
    #[serde(deserialize_with = "lenient")]
    pub played: std::collections::HashMap<String, u64>,
    /// Seconds played by game key, from the launch to the launcher coming back (Decorations' input); the game left
    /// open (key, start) until then.
    #[serde(deserialize_with = "lenient")]
    pub play_secs: std::collections::HashMap<String, u64>,
    #[serde(deserialize_with = "lenient")]
    pub playing: Option<(String, u64)>,
    /// The case stickers per game key, without the star: that is `favorites`.
    #[serde(deserialize_with = "lenient")]
    pub stickers: std::collections::HashMap<String, pocket_ui::case_stickers::CaseStickers>,
}

impl Preferences {
    /// The language chosen in Settings or the first start, however it was written down; `None`: the phone's.
    pub fn chosen_language(&self) -> Option<pocket_ui::i18n::Lang> {
        self.language.or(self.german.map(|g| if g { pocket_ui::i18n::Lang::De } else { pocket_ui::i18n::Lang::En }))
    }
    /// Keeps `lang` and paints in it from the next frame on.
    pub fn choose_language(&mut self, lang: pocket_ui::i18n::Lang) {
        self.language = Some(lang);
        self.german = None;
        pocket_ui::i18n::set(lang);
    }
    /// The stickers a game has, the star included (a phone's Favourite sticker is the game being a favourite).
    pub fn stickers_of(&self, key: &str) -> pocket_ui::case_stickers::CaseStickers {
        let mut stickers = self.stickers.get(key).copied().unwrap_or_default();
        stickers.favourite = self.favorites.contains(key);
        stickers
    }
    /// What the pictures show: nothing with Cozy extras → Stickers on cases off.
    pub fn stickers_shown(&self, key: &str) -> pocket_ui::case_stickers::CaseStickers {
        if self.extras.case_stickers {
            self.stickers_of(key)
        } else {
            Default::default()
        }
    }
}
/// One wrong value in launcher.json costs that setting only, never the rest of the file.
fn lenient<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    Ok(<serde_json::Value as serde::Deserialize>::deserialize(d).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default())
}
impl Preferences {
    /// Hands the look settings to the shared painters (they read process-wide switches, as pocket-ui's Linux pages do).
    pub fn apply_look(&self) {
        pocket_ui::palette::set_colours(self.colours);
        pocket_ui::font_style::set(self.font);
        pocket_ui::retro::set_colours(self.retro_colours);
        pocket_ui::button_prompts::set(self.prompts);
        pocket_ui::motion::set_level(self.motion);
        pocket_ui::motion::set_cozy(self.cozy.unwrap_or(true));
        pocket_ui::cozy_extras::set(self.extras);
        pocket_ui::system_art::set_gba_model(self.gba_model);
    }
}
/// The rows of the Cozy extras sheet, in the Linux page's order (`Extra::PAGE_ROWS`): every one has its feature on a
/// phone. Case sounds, the old-TV switch-on and the playful cases are rows of Settings itself, beside Animations, Motion
/// and UI sounds, as in Linux's Sound & feel; so is the slideshow, which Linux keeps in Sleep & power (`Extra::home`).
pub const PHONE_EXTRAS: [pocket_ui::cozy_extras::Extra; 8] = pocket_ui::cozy_extras::Extra::PAGE_ROWS;
/// The rows of Settings in page order; only what exists on a phone (no LEDs, fan, sleep or Tailscale rows).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Setting {
    Look,
    Appearance,
    Colours,
    Font,
    TextSize,
    Language,
    Animations,
    Motion,
    CozyExtras,
    /// A cozy extra you hear or that moves: a switch of its own (`Extra::home` is Sound & feel on Linux).
    Extra(pocket_ui::cozy_extras::Extra),
    Sounds,
    Haptics,
    RemindWhere,
    ButtonSymbols,
    GameBoy,
    Folder,
    GameCheck,
    Media,
    Updates,
    FirstStart,
}
const SETTINGS: [Setting; 23] = [
    Setting::Look,
    Setting::Appearance,
    Setting::Colours,
    Setting::Font,
    Setting::TextSize,
    Setting::CozyExtras,
    Setting::Language,
    Setting::Animations,
    Setting::Extra(pocket_ui::cozy_extras::Extra::Playful),
    Setting::Extra(pocket_ui::cozy_extras::Extra::CrtOn),
    Setting::Motion,
    Setting::Sounds,
    Setting::Haptics,
    Setting::RemindWhere,
    Setting::Extra(pocket_ui::cozy_extras::Extra::CaseSounds),
    Setting::Extra(pocket_ui::cozy_extras::Extra::IdleSlideshow),
    Setting::ButtonSymbols,
    Setting::GameBoy,
    Setting::Folder,
    Setting::GameCheck,
    Setting::Media,
    Setting::Updates,
    Setting::FirstStart,
];
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    Library,
    Favorites,
    Search,
    Curator,
    Results,
    /// A fact's other games (Connections) or Rediscover: a list opened from a card or from Curator.
    Connection,
    Facet,
    Settings,
    /// Settings → Cozy extras.
    Extras,
    /// Settings → Game check.
    GameCheck,
    Quick,
    Box,
    /// The open card's stickers.
    Stickers,
}
#[derive(Clone)]
pub enum Action {
    /// A fact on the open card that leads to the player's other games.
    Link(usize),
    JustPlay,
    Controls,
    /// The landscape shelf's arrows: the shelf above (-1) or below (1).
    Shelf(i32),
    /// The Game of the year sticker's year, back (-1) or on (1).
    Year(i32),
    Setup(usize),
    /// Words that are read, not tapped (a title, a status).
    Label,
    /// The same as the controller's key.
    Press(Button),
    /// The search field: opens the keyboard.
    Keyboard,
    /// Android's folder picker (an empty page's button).
    Folder,
    Tab(Screen),
    Row(usize),
    Key(usize, usize),
}
pub struct Region {
    pub area: Area,
    pub label: String,
    pub action: Action,
}
pub enum Command {
    Notice(&'static str),
    PersistSetup,
    None,
    Launch,
    Folder,
    Persist,
    /// Start the Game check on a worker.
    Check,
    /// Stop reading this chosen folder (it can't be read any more).
    Forget(String),
}
pub struct Library {
    pub games: Vec<Game>,
    pub cards: Vec<CardData>,
    pub index: pocket_ui::curator::Index,
    pub search: pocket_ui::search::Index,
    pub connections: pocket_ui::connections::Index,
}
/// The games a Connections or Rediscover list holds (indexes into the library).
#[derive(Clone)]
struct OpenList {
    title: String,
    games: Vec<usize>,
}
/// Where back from a list goes: the screen the card was opened from, the card's game, and the list it was in.
struct ListBack {
    prior: Screen,
    game: usize,
    list: Option<OpenList>,
}
/// The room's light and what was earned, as the shelves show them (`room_sync`), and the props' pictures.
#[derive(Default)]
struct RoomUi {
    light: Option<(std::time::Instant, pocket_ui::room_light::Light)>,
    earned: Option<(std::time::Instant, std::rc::Rc<pocket_ui::room_deco::Earned>)>,
    pics: pocket_ui::room_props::PropPics,
    /// What the cached shelf pictures were made in (light and what was earned).
    made_for: u64,
}
/// How long a reading of the clock or of the play time holds.
const ROOM_LOOK_AGAIN: std::time::Duration = std::time::Duration::from_secs(20);
/// How many lists deep back remembers its way (a card, its list, a game's card there, its list...).
const LIST_BACK_MAX: usize = 8;
pub struct Ui {
    pub prefs: Preferences,
    pub library: Option<Library>,
    pub screen: Screen,
    pub selected: usize,
    pub visible: Vec<usize>,
    pub regions: Vec<Region>,
    pub hits: HitMap,
    pub message: String,
    pub system_font_scale: f32,
    pub controls_open: bool,
    controls_scroll: usize,
    control_names: Vec<pocket_ui::controls::Target>,
    pub keyboard: Option<Keyboard>,
    pub query: String,
    pub lens: pocket_ui::curator::Lens,
    setup_row: usize,
    setup_preview: Option<((Theme, bool, i32, i32), art::Pic)>,
    pub scan_running: bool,
    pub controller_connected: bool,
    pub wifi_ready: bool,
    pub snapshot: Option<Image>,
    /// The games that stand open (`frozen_phone::stand_open`), by `Game::key`.
    frozen: HashSet<String>,
    /// Which stand open is to be read again (the host does, on a thread).
    pub frozen_stale: bool,
    menu: usize,
    connection: Option<OpenList>,
    list_back: Vec<ListBack>,
    room: RoomUi,
    check: check_page::CheckUi,
    facet: pocket_ui::curator::Facet,
    positions: std::collections::HashMap<Screen, usize>,
    box_view: pocket_ui::game_box::BoxView,
    info_view: pocket_ui::game_info::InfoView,
    info_cover: Option<((usize, i32, i32), std::rc::Rc<Image>)>,
    tv: Option<((usize, i32, i32), art::Pic)>,
    prior: Screen,
    cards: std::collections::HashMap<(usize, bool), art::Pic>,
    /// Canvas pixels per dp (Android density), set by the host before each paint.
    pub dp: f32,
    /// The last input was a touch: no controller hints, no selection band on sheet rows.
    pub touch_mode: bool,
    /// The page's vertical scroll in pixels, its end, and the fling's speed (pixels a second).
    scroll: f32,
    scroll_max: f32,
    velocity: (f32, f32),
    /// Scroll so the selection is in view at the next paint (after a controller move or a page change).
    follow: bool,
    /// What a drag of this many pixels moves by one game: (x along the landscape shelf, y in a list).
    step: (f32, f32),
    drag_rest: f32,
    shelves: Vec<((i32, i32, usize), art::Pic)>,
    /// Games a board of the portrait shelves holds (0 when the page is not that grid).
    grid_cols: usize,
    /// The landscape game card's own canvas (narrower than the page, beside its buttons).
    card_buf: Vec<u32>,
    /// The next paint is the page under a game's start (`insert`): the selected case isn't drawn, `front_at` says where
    /// it stood.
    pub launching: bool,
    /// Where the selected case stood in that paint (canvas pixels) and its front; None when the page shows no case.
    pub front_at: Option<((i32, i32, i32, i32), art::Pic)>,
    /// Where the card's canvas starts on the page.
    card_origin: (i32, i32),
    /// The Games page's shelves (Shelf, Cabinet and Cards looks with more than one), positions in `visible`.
    rows: Option<pocket_ui::phone_shelves::Shelves>,
    /// How far each portrait shelf is scrolled along, and where each landscape shelf's selection was left.
    row_scroll: std::collections::HashMap<pocket_ui::cabinet::ShelfKey, f32>,
    row_left_at: std::collections::HashMap<pocket_ui::cabinet::ShelfKey, usize>,
    /// Where each portrait shelf lies on the page (top, bottom, its shelf, how far it can scroll), from the last paint.
    row_bands: Vec<(i32, i32, pocket_ui::cabinet::ShelfKey, f32)>,
    /// One game's width on a portrait shelf (0 when the page is not that).
    row_pitch: i32,
    /// This finger's page height, and what it drags: along a shelf (true) or across them, and which portrait shelf.
    finger_y: f32,
    drag_along: Option<bool>,
    drag_row: Option<pocket_ui::cabinet::ShelfKey>,
    /// Finger travel towards changing the landscape shelf; None once this finger has changed it (one a swipe).
    drag_across: Option<f32>,
    /// The Game of the year sticker's year on the open sheet.
    sticker_year: u16,
}
impl Ui {
    pub fn new(prefs: Preferences) -> Self {
        Self {
            prefs,
            library: None,
            screen: Screen::Library,
            selected: 0,
            visible: vec![],
            regions: vec![],
            hits: HitMap::default(),
            message: String::new(),
            system_font_scale: 1.0,
            controls_open: false,
            controls_scroll: 0,
            control_names: Vec::new(),
            keyboard: None,
            query: String::new(),
            lens: Default::default(),
            setup_row: 0,
            setup_preview: None,
            scan_running: false,
            controller_connected: false,
            wifi_ready: false,
            snapshot: None,
            frozen: HashSet::new(),
            frozen_stale: true,
            menu: 0,
            connection: None,
            list_back: Vec::new(),
            room: RoomUi::default(),
            check: Default::default(),
            facet: pocket_ui::curator::Facet::System,
            positions: Default::default(),
            box_view: Default::default(),
            info_view: Default::default(),
            info_cover: None,
            tv: None,
            prior: Screen::Library,
            cards: Default::default(),
            dp: 2.2,
            touch_mode: true,
            scroll: 0.0,
            scroll_max: 0.0,
            velocity: (0.0, 0.0),
            follow: true,
            step: (0.0, 0.0),
            drag_rest: 0.0,
            shelves: Vec::new(),
            card_buf: Vec::new(),
            grid_cols: 0,
            launching: false,
            front_at: None,
            card_origin: (0, 0),
            rows: None,
            row_scroll: Default::default(),
            row_left_at: Default::default(),
            row_bands: Vec::new(),
            row_pitch: 0,
            finger_y: 0.0,
            drag_along: None,
            drag_row: None,
            drag_across: Some(0.0),
            sticker_year: 0,
        }
    }
    pub fn is_frozen(&self, key: &str) -> bool {
        self.frozen.contains(key)
    }
    /// The answer of the read: the cases are made again when it changed.
    pub fn frozen_got(&mut self, open: Vec<String>) {
        let open: HashSet<String> = open.into_iter().collect();
        if open != self.frozen {
            self.frozen = open;
            self.cards.clear();
            self.reshelve();
        }
    }
    pub fn game(&self) -> Option<&Game> {
        self.library.as_ref()?.games.get(*self.visible.get(self.selected)?)
    }
    pub fn install(&mut self, library: Library) {
        let restored = library
            .games
            .iter()
            .position(|g| g.key == self.prefs.selected_key || g.uri == self.prefs.selected_uri);
        self.library = Some(library);
        self.frozen_stale = true;
        self.message.clear();
        self.filter();
        // Nothing remembered (a fresh start): the page's first game, so several shelves open at the top with Recent,
        // not on the shelf of the library's alphabetically first title.
        self.selected = restored.and_then(|r| self.visible.iter().position(|i| *i == r)).unwrap_or(0);
        self.cards.clear();
    }
    pub fn filter(&mut self) {
        let Some(lib) = &self.library else {
            self.visible.clear();
            self.rows = None;
            return;
        };
        if !matches!(self.screen, Screen::Settings | Screen::Extras | Screen::GameCheck | Screen::Quick | Screen::Curator | Screen::Facet | Screen::Box | Screen::Stickers) {
            self.rows = None;
        }
        self.visible = match self.screen {
            Screen::Favorites => lib
                .games
                .iter()
                .enumerate()
                .filter(|(_, g)| self.prefs.favorites.contains(&g.key))
                .map(|(i, _)| i)
                .collect(),
            Screen::Search => lib.search.search(&self.query, &|_| true).iter().map(|h| h.game).collect(),
            Screen::Results => lib.index.list(&lib.index.matches(&self.lens), false),
            Screen::Connection => self.connection.as_ref().map(|l| l.games.clone()).unwrap_or_default(),
            Screen::Settings | Screen::Extras | Screen::GameCheck | Screen::Quick | Screen::Curator | Screen::Facet | Screen::Box | Screen::Stickers => return,
            _ => {
                let shelves = (!matches!(self.prefs.theme, Theme::Minimal | Theme::Split | Theme::Big)).then(|| shelves_of(lib, &self.prefs, &self.frozen)).filter(|s| s.rows.len() > 1);
                self.rows = shelves;
                self.rows.as_ref().map_or_else(|| (0..lib.games.len()).collect(), |s| s.games.clone())
            }
        };
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
    }
    pub fn list_screen(screen: Screen) -> bool {
        matches!(screen, Screen::Library | Screen::Favorites | Screen::Search | Screen::Results | Screen::Connection)
    }
    fn facets(&self) -> Vec<pocket_ui::curator::Facet> {
        self.library.as_ref().map(|l| l.index.shown_facets()).unwrap_or_default()
    }
    pub fn navigate(&mut self, screen: Screen) {
        if Self::list_screen(self.screen) {
            self.positions.insert(self.screen, self.selected);
        }
        // Back from Cozy extras (B, Android Back or its Back row): its own row in Settings, not the first.
        let back_to_row = if self.screen == Screen::Extras && screen == Screen::Settings {
            SETTINGS.iter().position(|s| matches!(s, Setting::CozyExtras))
        } else if self.screen == Screen::GameCheck && screen == Screen::Settings {
            SETTINGS.iter().position(|s| matches!(s, Setting::GameCheck))
        } else {
            None
        };
        self.screen = screen;
        self.menu = back_to_row.unwrap_or(0);
        self.scroll = 0.0;
        self.velocity = (0.0, 0.0);
        self.follow = true;
        self.keyboard = if screen == Screen::Search {
            let mut keyboard = Keyboard::for_search();
            keyboard.text = self.query.clone();
            Some(keyboard)
        } else {
            None
        };
        if Self::list_screen(screen) {
            self.selected = self.positions.get(&screen).copied().unwrap_or(0);
        }
        self.filter();
    }
    /// The page's scroll follows the selection again: the case to start from comes into view.
    pub fn follow_selection(&mut self) {
        self.follow = true;
        self.velocity = (0.0, 0.0);
    }
    /// What was painted with the old colours or type (case fronts, shelves, the TV): made again at the next paint.
    fn drop_pictures(&mut self) {
        self.cards.clear();
        self.shelves.clear();
        self.tv = None;
    }
    fn count(&self) -> usize {
        match self.screen {
            Screen::Settings => SETTINGS.len(),
            Screen::Extras => PHONE_EXTRAS.len() + 2,
            Screen::GameCheck => self.check_rows().len(),
            Screen::Quick => 3,
            Screen::Stickers => STICKER_ROWS,
            Screen::Curator => 2 + self.facets().len() + self.rediscover_row() as usize,
            Screen::Facet => self.library.as_ref().map_or(2, |l| 2 + l.index.facet(self.facet).values.len()),
            _ => self.visible.len(),
        }
    }
    pub fn move_by(&mut self, delta: i32) {
        if self.screen == Screen::Box {
            if matches!(self.prefs.theme, Theme::Minimal | Theme::Cards | Theme::Split) {
                self.info_view.scroll_by(delta * 64);
            } else {
                self.box_view.scroll_by(delta * 64);
            }
            return;
        }
        let count = self.count();
        let selected = if self.sheet() {
            &mut self.menu
        } else {
            &mut self.selected
        };
        *selected = (*selected as i32 + delta).clamp(0, count.saturating_sub(1) as i32) as usize;
        self.follow = true;
    }
    /// The page is a sheet of rows, not a shelf of games.
    fn sheet(&self) -> bool {
        matches!(self.screen, Screen::Settings | Screen::Extras | Screen::GameCheck | Screen::Quick | Screen::Curator | Screen::Facet | Screen::Stickers)
    }
    fn open_controls(&mut self) {
        // Read once at the explicit open, like the game card's save details; never in the painter.
        self.control_names = self.game().map(|g| play_cards::remembered_names(&super::data_dir(), &game_key(g,self.prefs.gameboy_gambatte))).unwrap_or_default();
        self.controls_open = true;
        self.controls_scroll = 0;
    }
    pub fn press(&mut self, b: Button) -> Command {
        if self.controls_open {
            match b {
                Button::South | Button::East => self.controls_open = false,
                Button::Up | Button::Left => self.controls_scroll = self.controls_scroll.saturating_sub(1),
                Button::Down | Button::Right => self.controls_scroll += 1,
                _ => (),
            }
            return Command::None;
        }
        if self.screen == Screen::Box && b == Button::R2 && self.game().is_some_and(|g| super::bundled_core(g.system_id).is_some()) {
            self.open_controls();
            return Command::None;
        }
        if !self.prefs.onboarding_done {
            return self.setup_press(b);
        }
        if let Some(keyboard) = &mut self.keyboard {
            let result = keyboard.press(b);
            self.query = keyboard.text.clone();
            self.filter();
            if matches!(result, Outcome::Done(_) | Outcome::Cancelled) {
                self.keyboard = None;
            }
            return Command::None;
        }
        if self.screen == Screen::GameCheck {
            return self.check_press(b);
        }
        if self.screen == Screen::Box && self.card_link_press(b) {
            return Command::None;
        }
        if self.screen == Screen::Stickers {
            if let Some(command) = self.sticker_press(b) {
                return command;
            }
        }
        if b == Button::South {
            if self.screen == Screen::Box {
                self.screen = self.prior;
            } else if self.screen == Screen::Connection {
                self.leave_list();
            } else if self.screen == Screen::Facet {
                self.navigate(Screen::Curator);
            } else if self.screen == Screen::Extras {
                self.navigate(Screen::Settings);
            } else if self.screen != Screen::Library {
                self.navigate(Screen::Library);
            }
            return Command::None;
        }
        if b == Button::Start {
            if self.screen == Screen::Settings {
                self.navigate(Screen::Library);
            } else {
                self.navigate(Screen::Settings);
            }
            return Command::None;
        }
        if b == Button::Select && self.screen == Screen::Box {
            self.open_stickers();
            return Command::None;
        }
        if b == Button::Select {
            self.navigate(Screen::Quick);
            return Command::None;
        }
        if b == Button::L1 || b == Button::R1 {
            let tabs = [Screen::Library, Screen::Favorites, Screen::Search, Screen::Curator];
            let i = tabs.iter().position(|s| *s == self.screen).unwrap_or(0);
            self.navigate(tabs[(i + if b == Button::R1 { 1 } else { 3 }) % 4]);
            return Command::None;
        }
        if b == Button::West {
            if let Some(g) = self.game() {
                let key = g.key.clone();
                if !self.prefs.favorites.remove(&key) {
                    self.prefs.favorites.insert(key);
                }
                self.cards.clear();
                // On the shelves `reshelve` reads the selection before it rebuilds them; `filter` first would lose it.
                if self.on_shelves() {
                    self.reshelve();
                } else {
                    self.filter();
                }
                return Command::Persist;
            }
        }
        if b == Button::North {
            self.refresh_card();
            if self.screen == Screen::Box {
                self.box_view.flip();
            } else if self.game().is_some() {
                self.prior = self.screen;
                self.screen = Screen::Box;
                self.box_view = Default::default();
                if !self.prefs.cozy.unwrap_or(true) {
                    self.box_view.snapshot_settle();
                }
                self.info_view = Default::default();
            }
            return Command::None;
        }
        if self.screen == Screen::Search && b == Button::Up {
            self.keyboard = Some(Keyboard::for_search());
            if let Some(k) = &mut self.keyboard {
                k.text = self.query.clone();
            }
            return Command::None;
        }
        if matches!(b, Button::Up | Button::Down) && self.rows.is_some() && self.screen == Screen::Library {
            self.step_shelf(if b == Button::Up { -1 } else { 1 });
            return Command::None;
        }
        if matches!(b, Button::Up | Button::Down) && self.grid_cols > 1 && Self::list_screen(self.screen) {
            // The portrait shelves: Up/Down go a board up or down, Left/Right along it.
            let cols = self.grid_cols as i32;
            let target = self.selected as i32 + if b == Button::Up { -cols } else { cols };
            if (0..self.visible.len() as i32).contains(&target) {
                self.move_by(target - self.selected as i32);
            }
            return Command::None;
        }
        if matches!(b, Button::Up | Button::Left | Button::Down | Button::Right) {
            self.move_by(if matches!(b, Button::Up | Button::Left) { -1 } else { 1 });
            return Command::None;
        }
        if b != Button::East {
            return Command::None;
        }
        match self.screen {
            Screen::Settings => {
                match SETTINGS[self.menu.min(SETTINGS.len() - 1)] {
                    Setting::Look => {
                        let i = Theme::PHONE.iter().position(|t| *t == self.prefs.theme).unwrap_or(0);
                        self.prefs.theme = Theme::PHONE[(i + 1) % Theme::PHONE.len()];
                        self.cards.clear();
                    }
                    Setting::Appearance => {
                        self.prefs.light = !self.prefs.light;
                        self.cards.clear();
                    }
                    Setting::Colours => {
                        if self.prefs.font == pocket_ui::font_style::FontStyle::Retro {
                            self.prefs.retro_colours = self.prefs.retro_colours.step(true);
                        } else { self.prefs.colours = self.prefs.colours.step(true); }
                        self.drop_pictures();
                    }
                    Setting::Font => {
                        self.prefs.font = self.prefs.font.next();
                        self.drop_pictures();
                    }
                    Setting::Language => {
                        self.prefs.choose_language(pocket_ui::i18n::current().next());
                    }
                    Setting::TextSize => self.prefs.text_size = self.prefs.text_size.next(),
                    Setting::Sounds => self.prefs.muted = !self.prefs.muted,
                    Setting::Haptics => self.prefs.haptics = !self.prefs.haptics,
                    Setting::RemindWhere => self.prefs.remind_where = Some(!self.prefs.remind_where.unwrap_or(true)),
                    Setting::Motion => self.prefs.motion = self.prefs.motion.next(),
                    Setting::ButtonSymbols => self.prefs.prompts = self.prefs.prompts.step(true),
                    Setting::GameBoy => self.prefs.gameboy_gambatte = !self.prefs.gameboy_gambatte,
                    Setting::Folder => return Command::Folder,
                    Setting::GameCheck => return self.open_check(),
                    Setting::Updates => return Command::Notice("Install a newer Android APK to update. Your games and settings stay."),
                    Setting::Media => self.prefs.download_media = Some(!self.prefs.download_media.unwrap_or(false)),
                    Setting::Animations => self.prefs.cozy = Some(!self.prefs.cozy.unwrap_or(true)),
                    Setting::CozyExtras => {
                        self.navigate(Screen::Extras);
                        return Command::None;
                    }
                    Setting::Extra(e) => {
                        let on = !self.prefs.extras.get(e);
                        self.prefs.extras.set(e, on);
                        self.cards.clear();
                    }
                    Setting::FirstStart => {
                        self.prefs.onboarding_done = false;
                        self.prefs.onboarding_step = 0;
                        self.setup_row = 0;
                        return Command::PersistSetup;
                    }
                };
                self.prefs.apply_look();
                self.frozen_stale = true;
                Command::Persist
            }
            Screen::Extras => {
                if let Some(e) = PHONE_EXTRAS.get(self.menu).copied() {
                    let on = !self.prefs.extras.get(e);
                    self.prefs.extras.set(e, on);
                    self.prefs.apply_look();
                    self.tv = None;
                    self.cards.clear();
                    Command::Persist
                } else if self.menu == PHONE_EXTRAS.len() {
                    self.prefs.gba_model = self.prefs.gba_model.next();
                    self.prefs.apply_look();
                    self.drop_pictures();
                    Command::Persist
                } else {
                    self.navigate(Screen::Settings);
                    Command::None
                }
            }
            Screen::Quick => {
                if self.menu == 0 {
                    self.prefs.muted = !self.prefs.muted;
                    Command::Persist
                } else if self.menu == 1 {
                    self.just_play()
                } else {
                    self.navigate(Screen::Settings);
                    Command::None
                }
            }
            Screen::Curator => {
                if self.menu == 0 {
                    let seed = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64;
                    let pick = self
                        .library
                        .as_ref()
                        .and_then(|l| l.index.surprise(&mut pocket_ui::curator::surprise::Rng::new(seed), &[]));
                    if let Some(pick) = pick {
                        self.navigate(Screen::Library);
                        self.selected = self.visible.iter().position(|i| *i == pick.game).unwrap_or(0);
                        if let Some(lib) = &mut self.library {
                            lib.cards[pick.game].surprise = Some(pick.reason);
                        }
                        self.prior = Screen::Library;
                        self.screen = Screen::Box;
                    }
                } else if self.menu == self.count() - 1 {
                    self.navigate(Screen::Results);
                } else if self.rediscover_row() && self.menu == self.count() - 2 {
                    self.open_rediscover();
                } else if let Some(facet) = self.facets().get(self.menu - 1).copied() {
                    self.facet = facet;
                    self.navigate(Screen::Facet);
                }
                Command::None
            }
            Screen::Facet => {
                if self.menu == 0 {
                    self.lens.clear(self.facet);
                } else if self.menu == self.count() - 1 {
                    self.navigate(Screen::Results);
                } else if let Some(value) = self.library.as_ref().and_then(|l| l.index.facet(self.facet).values.get(self.menu - 1)) {
                    self.lens.toggle(self.facet, &value.key);
                }
                Command::None
            }
            _ => {
                if self.game().is_some() {
                    Command::Launch
                } else if self.library.is_none() || self.screen == Screen::Library {
                    Command::Folder
                } else {
                    Command::None
                }
            }
        }
    }
    /// The room's light now and what the player has earned, for the Shelf and Cabinet looks: neutral light with the
    /// extra off, None for the decorations with theirs off. The caches of tinted shelf pictures are dropped when
    /// either changed since they were made. Looks at the clock and the play time every 20 s at most.
    pub(super) fn room_sync(&mut self) -> (pocket_ui::room_light::Light, Option<std::rc::Rc<pocket_ui::room_deco::Earned>>) {
        use pocket_ui::room_light::{self, Light};
        let now = std::time::Instant::now();
        let light = if self.prefs.extras.time_of_day_light {
            match self.room.light {
                Some((at, l)) if now.duration_since(at) < ROOM_LOOK_AGAIN => l,
                _ => {
                    let l = room_light::at(room_light::minute_now(), self.prefs.light);
                    self.room.light = Some((now, l));
                    l
                }
            }
        } else {
            Light::NEUTRAL
        };
        let earned = if self.prefs.extras.decorations {
            match &self.room.earned {
                Some((at, e)) if now.duration_since(*at) < ROOM_LOOK_AGAIN => Some(e.clone()),
                _ => {
                    let metas: Vec<(String, pocket_ui::library_meta::GameMeta)> = self
                        .library
                        .iter()
                        .flat_map(|l| l.games.iter())
                        .filter_map(|g| {
                            let secs = self.prefs.play_secs.get(&g.key).copied();
                            let case = self.prefs.stickers.get(&g.key).copied();
                            if secs.is_none() && case.is_none() {
                                return None;
                            }
                            let meta = pocket_ui::library_meta::GameMeta { secs: secs.unwrap_or(0), case: case.unwrap_or_default(), ..Default::default() };
                            Some((format!("{}/{}", g.system_id, g.name), meta))
                        })
                        .collect();
                    let e = std::rc::Rc::new(pocket_ui::room_deco::earned(metas.iter().map(|(p, m)| (p.as_str(), m))));
                    self.room.earned = Some((now, e.clone()));
                    Some(e)
                }
            }
        } else {
            None
        };
        let key = light.key().wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ earned.as_ref().map_or(1, |e| e.key());
        if key != self.room.made_for {
            self.room.made_for = key;
            self.shelves.clear();
        }
        (light, earned)
    }
    /// The launcher is back: the time since the game was started counts as played (at most a day, so a game left
    /// running overnight does not grow the plant by itself).
    pub fn game_returned(&mut self) {
        self.frozen_stale = true;
        let Some((key, start)) = self.prefs.playing.take() else { return };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let secs = now.saturating_sub(start).min(86_400);
        *self.prefs.play_secs.entry(key).or_default() += secs;
        self.room.earned = None;
        // The game goes to the top of the recent games.
        self.reshelve();
    }
    /// The Games page's shelves after the favourites or the recent games changed: the selection stays on its game, on
    /// the same shelf while it is there.
    fn on_shelves(&self) -> bool {
        self.screen == Screen::Library || self.screen == Screen::Box && self.prior == Screen::Library
    }
    fn reshelve(&mut self) {
        if !self.on_shelves() {
            return;
        }
        let game = self.visible.get(self.selected).copied();
        let key = self.shelf_row().map(|r| r.key);
        let screen = std::mem::replace(&mut self.screen, Screen::Library);
        self.filter();
        self.screen = screen;
        if let Some(game) = game {
            let place = self.rows.as_ref().map_or_else(|| self.visible.iter().position(|g| *g == game), |s| s.place_of(game, key));
            self.selected = place.unwrap_or(0);
        }
    }
    /// The shelf the selection stands on.
    pub(super) fn shelf_row(&self) -> Option<pocket_ui::phone_shelves::Row> {
        let rows = self.rows.as_ref()?;
        rows.rows.get(rows.row_of(self.selected)?).copied()
    }
    /// Up or Down, the shelf's arrows or a swipe across: the case on the next shelf that stands under this one in
    /// portrait, in landscape (one shelf at a time on the page) the one left selected there.
    pub(super) fn step_shelf(&mut self, delta: i32) -> bool {
        let Some(rows) = &self.rows else { return false };
        let Some(at) = rows.row_of(self.selected) else { return false };
        let Some(target) = at.checked_add_signed(delta as isize).and_then(|t| rows.rows.get(t)).copied() else { return false };
        let here = rows.rows[at];
        let offset = if self.row_pitch > 0 {
            let first = |key| (self.row_scroll.get(&key).copied().unwrap_or(0.0) as i32 + self.row_pitch / 2) / self.row_pitch;
            ((self.selected - here.start) as i32 - first(here.key)).max(0) as usize + first(target.key) as usize
        } else {
            self.row_left_at.get(&target.key).copied().unwrap_or(0)
        };
        self.row_left_at.insert(here.key, self.selected - here.start);
        self.selected = (target.start + offset).min(target.end - 1);
        self.follow = true;
        true
    }
    /// The open card's sticker sheet.
    fn open_stickers(&mut self) {
        let Some(&index) = self.visible.get(self.selected) else { return };
        let Some(lib) = &self.library else { return };
        let key = &lib.games[index].key;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let year = pocket_ui::civil_date::civil(pocket_ui::civil_date::local_day(now, 0)).0;
        self.sticker_year = self
            .prefs
            .stickers_of(key)
            .goty
            .unwrap_or_else(|| pocket_ui::case_stickers::default_year(lib.cards[index].year, year as u16));
        self.screen = Screen::Stickers;
        self.menu = 0;
        self.scroll = 0.0;
        self.follow = true;
    }
    /// A key on the sticker sheet; None: the same as on every sheet (moving up and down).
    fn sticker_press(&mut self, b: Button) -> Option<Command> {
        use pocket_ui::case_stickers::Kind;
        let on_year = self.menu == Kind::ALL.len() || Kind::ALL.get(self.menu) == Some(&Kind::Goty);
        match b {
            Button::South => {
                self.screen = Screen::Box;
                Some(Command::None)
            }
            Button::Left | Button::Right if on_year => Some(self.sticker_year_by(if b == Button::Left { -1 } else { 1 })),
            Button::Left | Button::Right => Some(Command::None),
            Button::East => match Kind::ALL.get(self.menu) {
                Some(&kind) => Some(self.sticker_toggle(kind)),
                None if self.menu == Kind::ALL.len() => Some(Command::None),
                None => {
                    self.screen = Screen::Box;
                    Some(Command::None)
                }
            },
            Button::Up | Button::Down => None,
            _ => Some(Command::None),
        }
    }
    /// Puts sticker `kind` on the open game or takes it off. The star is the game being a favourite.
    fn sticker_toggle(&mut self, kind: pocket_ui::case_stickers::Kind) -> Command {
        use pocket_ui::case_stickers::Kind;
        let Some(key) = self.game().map(|g| g.key.clone()) else { return Command::None };
        let has = self.prefs.stickers_of(&key).has(kind);
        if kind == Kind::Favourite {
            if has {
                self.prefs.favorites.remove(&key);
            } else {
                self.prefs.favorites.insert(key);
            }
            self.reshelve_from_sheet();
        } else {
            let year = self.sticker_year;
            self.edit_stickers(key, |s| s.set(kind, !has, year));
        }
        self.sticker_changed();
        Command::Persist
    }
    /// Changes the year; a sticker that is on keeps the new year at once.
    fn sticker_year_by(&mut self, delta: i32) -> Command {
        use pocket_ui::case_stickers::{Kind, YEAR_FIRST, YEAR_LAST};
        self.sticker_year = (self.sticker_year as i32 + delta).clamp(YEAR_FIRST as i32, YEAR_LAST as i32) as u16;
        let Some(key) = self.game().map(|g| g.key.clone()) else { return Command::None };
        if self.prefs.stickers_of(&key).has(Kind::Goty) {
            let year = self.sticker_year;
            self.edit_stickers(key, |s| s.set(Kind::Goty, true, year));
            self.sticker_changed();
            return Command::Persist;
        }
        Command::None
    }
    fn edit_stickers(&mut self, key: String, edit: impl FnOnce(&mut pocket_ui::case_stickers::CaseStickers)) {
        let stickers = self.prefs.stickers.entry(key.clone()).or_default();
        edit(stickers);
        if stickers.is_empty() {
            self.prefs.stickers.remove(&key);
        }
    }
    /// The pictures with stickers on them, and what they earn on the shelf, are made again.
    fn sticker_changed(&mut self) {
        self.cards.clear();
        self.room.earned = None;
    }
    /// The shelves under the sheet, opened from the Games page's card.
    fn reshelve_from_sheet(&mut self) {
        let sheet = std::mem::replace(&mut self.screen, Screen::Box);
        self.reshelve();
        self.screen = sheet;
    }
    /// Curator lists Rediscover (Cozy extras → Rediscover).
    fn rediscover_row(&self) -> bool {
        self.prefs.extras.rediscover && self.library.is_some()
    }
    /// Rediscover's games: played a year ago today or not for a year, by when each was last started on this phone
    /// (`connections::rediscover`). A phone has no older history, so this is empty for the first year.
    fn rediscover_games(&self) -> Vec<usize> {
        let Some(lib) = &self.library else { return Vec::new() };
        let played: Vec<_> = lib
            .games
            .iter()
            .filter_map(|g| Some((std::path::PathBuf::from(&g.key), *self.prefs.played.get(&g.key)?, 1)))
            .collect();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let at: std::collections::HashMap<&str, usize> = lib.games.iter().enumerate().map(|(i, g)| (g.key.as_str(), i)).collect();
        pocket_ui::connections::rediscover(&played, &[], now, 0)
            .into_iter()
            .filter_map(|(p, _)| at.get(p.to_string_lossy().as_ref()).copied())
            .collect()
    }
    fn open_rediscover(&mut self) {
        self.list_back.clear();
        self.connection = Some(OpenList { title: "Rediscover".into(), games: self.rediscover_games() });
        self.navigate(Screen::Connection);
        self.selected = 0;
    }
    /// The open list is Rediscover (from Curator), not a fact's games.
    pub(super) fn list_from_curator(&self) -> bool {
        self.list_back.is_empty()
    }
    /// The title of the open Connections or Rediscover list.
    pub(super) fn connection_title(&self) -> String {
        self.connection.as_ref().map(|l| tr(&l.title)).unwrap_or_default()
    }
    /// A on a fact of the open card that leads to the player's other games: their list, as a shelf. B comes back to
    /// the card.
    fn open_link(&mut self, i: usize) {
        let Some(&game) = self.visible.get(self.selected) else { return };
        let Some((_, link)) = self.library.as_ref().and_then(|l| l.cards[game].links.get(i)) else { return };
        let list = OpenList { title: link.title.clone(), games: link.games.iter().map(|(at, _)| at.1).collect() };
        if self.list_back.len() >= LIST_BACK_MAX {
            self.list_back.remove(0);
        }
        self.list_back.push(ListBack { prior: self.prior, game, list: self.connection.take() });
        self.connection = Some(list);
        self.navigate(Screen::Connection);
        self.selected = 0;
    }
    /// B on a list: back to the card it was opened from (and that card's list), or to Curator for Rediscover.
    fn leave_list(&mut self) {
        let Some(back) = self.list_back.pop() else {
            self.navigate(Screen::Curator);
            return;
        };
        self.connection = back.list;
        self.navigate(back.prior);
        self.selected = self.visible.iter().position(|g| *g == back.game).unwrap_or(0);
        self.prior = back.prior;
        self.screen = Screen::Box;
        self.box_view = Default::default();
        if !self.prefs.cozy.unwrap_or(true) {
            self.box_view.snapshot_settle();
        }
        self.info_view = Default::default();
    }
    /// The card's facts that lead to other games (`CardData::links`), from the index: only while Cozy extras → Game
    /// connections is on; a series the titles give is added as a fact where the data has none.
    fn fill_links(&mut self, index: usize) {
        let on = self.prefs.extras.connections;
        let Some(lib) = &mut self.library else { return };
        let path = std::path::PathBuf::from(&lib.games[index].key);
        let Library { cards, connections, .. } = lib;
        let card = &mut cards[index];
        card.links.clear();
        if !on {
            return;
        }
        if !card.facts.iter().any(|(k, _)| *k == "Series") {
            if let Some(series) = connections.derived_series(&path) {
                card.facts.push(("Series", series.to_string()));
            }
        }
        card.links = card
            .facts
            .iter()
            .enumerate()
            .filter_map(|(i, (k, v))| Some((i, connections.link(pocket_ui::connections::Kind::of_label(k)?, v, &path)?)))
            .map(|(i, l)| {
                let games = l.games.iter().map(|&g| (connections.games[g].at, connections.games[g].path.clone())).collect();
                (i, pocket_ui::game_card_model::CardLink { title: l.title(), games })
            })
            .collect();
    }
    /// Up, Down and A on the open card's facts that lead to other games, as on the Linux card; false: not for them.
    fn card_link_press(&mut self, b: Button) -> bool {
        let Some(&game) = self.visible.get(self.selected) else { return false };
        let links = self.library.as_ref().map_or(0, |l| l.cards[game].links.len());
        if links == 0 || !matches!(b, Button::Up | Button::Down | Button::East) {
            return false;
        }
        let down = b == Button::Down;
        let picked = if matches!(self.prefs.theme, Theme::Minimal | Theme::Cards | Theme::Split) {
            if b != Button::East && self.info_view.link_step(down, links) {
                return true;
            }
            self.info_view.link
        } else {
            if !self.box_view.back {
                return false;
            }
            if b != Button::East {
                self.box_view.link_step(down, links, 64);
                return true;
            }
            self.box_view.link
        };
        match picked.filter(|l| b == Button::East && *l < links) {
            Some(l) => {
                self.open_link(l);
                true
            }
            None => false,
        }
    }
    /// The card's arrows as tap targets: where the shared page painted them (its canvas starts at `origin` on the
    /// page).
    pub(super) fn link_targets(&mut self, (ox, oy): (i32, i32)) {
        let Some(&game) = self.visible.get(self.selected) else { return };
        let Some(card) = self.library.as_ref().map(|l| &l.cards[game]) else { return };
        let info = matches!(self.prefs.theme, Theme::Minimal | Theme::Cards | Theme::Split);
        let found: Vec<_> = card
            .links
            .iter()
            .enumerate()
            .filter_map(|(l, (f, _))| {
                let rect = if info { self.info_view.link_rects.get(l).copied() } else { self.box_view.link_rect(l) }.filter(|r| r.2 > 0)?;
                Some((rect, card.facts.get(*f).map(|(k, v)| format!("{k}: {v}")).unwrap_or_default(), l))
            })
            .collect();
        for ((x, y, w, h), label, l) in found {
            self.region(Area::new(x + ox, y + oy, w, h), label, Action::Link(l));
        }
    }
    fn refresh_card(&mut self) {
        let Some(&index) = self.visible.get(self.selected) else { return };
        self.fill_links(index);
        let Some(lib) = &mut self.library else { return };
        let game = &lib.games[index];
        lib.cards[index].favorite = self.prefs.favorites.contains(&game.key);
        if super::bundled_core(game.system_id).is_none() {
            return;
        }
        let key = game_key(game, self.prefs.gameboy_gambatte);
        let Some(stem) = std::path::Path::new(&game.relative).file_stem() else {
            return;
        };
        let base = super::data_dir().join("states").join(key).join(stem);
        let times: Vec<_> = (0..pocket_ui::slots::SLOTS).map(|i| pocket_ui::slots::saved_at(&base, i)).collect();
        lib.cards[index].saves = Some(pocket_ui::game_card_model::CardSaves {
            system: game.system_id.into(),
            stripe: game.color,
            filled: times.iter().map(Option::is_some).collect(),
            newest: times
                .into_iter()
                .flatten()
                .max()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|t| t.as_secs()),
        });
    }
    /// The shared choice policy; limit automatic picks to our bundled player. External
    /// apps remain available through an explicit Play, without probing them on the UI thread.
    fn just_play(&mut self) -> Command {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let seed = now.as_nanos() as u64;
        let prefs = &self.prefs;
        let pick = self.library.as_mut().and_then(|lib| {
            lib.index.user.now = now.as_secs();
            // The index was made at scan time; choices must see stars, stickers and
            // sessions changed since then, including the game just returned from.
            for (game, user) in lib.games.iter().zip(&mut lib.index.user.games) {
                let case = prefs.stickers_of(&game.key);
                user.starred = case.favourite;
                user.status = if case.completed || case.hundred { pocket_ui::library_meta::Status::Completed }
                    else if case.playing { pocket_ui::library_meta::Status::Playing }
                    else { pocket_ui::library_meta::Status::NotStarted };
                user.last_played = prefs.played.get(&game.key).copied();
                user.secs = prefs.play_secs.get(&game.key).copied().unwrap_or(0);
            }
            let allowed = |g: usize| lib.games.get(g).is_some_and(|game| super::bundled_core(game.system_id).is_some());
            let heavy = |_| false;
            let mut startable = |g: usize| lib.games.get(g).is_some_and(|game| game.size != 0 && (game.system_id == "psp" || game.size <= 512 << 20) && game.missing.is_empty());
            lib.index.just_play(&mut pocket_ui::curator::surprise::Rng::new(seed),
                &pocket_ui::curator::justplay::Offer { allowed: &allowed, heavy: &heavy }, &mut startable).ok()
        });
        if let Some((game, _)) = pick {
            self.navigate(Screen::Library);
            self.selected = self.visible.iter().position(|i| *i == game).unwrap_or(0);
            self.follow_selection();
            Command::Launch
        } else {
            Command::Notice("No games to pick from yet")
        }
    }
    pub fn tap(&mut self, x: f32, y: f32) -> Command {
        self.touch_mode = true;
        if !self.prefs.onboarding_done {
            return self.setup_tap(x, y);
        }
        let action = self.regions.iter().rev().find(|r| r.area.contains(x, y)).map(|r| r.action.clone());
        match action {
            Some(Action::Controls) => {
                self.open_controls();
                Command::None
            }
            Some(Action::JustPlay) => self.just_play(),
            Some(Action::Tab(screen)) => {
                if self.screen == screen && Self::list_screen(screen) {
                    // The tab again goes back to the top, as on every phone.
                    self.scroll = 0.0;
                    self.velocity = (0.0, 0.0);
                } else {
                    self.navigate(screen);
                    if screen == Screen::Search && !self.query.is_empty() {
                        self.keyboard = None;
                    }
                }
                Command::None
            }
            Some(Action::Key(row, col)) => {
                if let Some(k) = &mut self.keyboard {
                    k.row = row;
                    k.col = col;
                }
                self.press(Button::East)
            }
            Some(Action::Keyboard) => {
                if self.keyboard.is_none() {
                    let mut keyboard = Keyboard::for_search();
                    keyboard.text = self.query.clone();
                    self.keyboard = Some(keyboard);
                }
                Command::None
            }
            Some(Action::Press(b)) => self.press(b),
            Some(Action::Link(i)) => {
                self.open_link(i);
                Command::None
            }
            Some(Action::Shelf(delta)) => {
                self.step_shelf(delta);
                Command::None
            }
            Some(Action::Year(delta)) => self.sticker_year_by(delta),
            Some(Action::Folder) => Command::Folder,
            Some(Action::Row(i)) => {
                if self.sheet() {
                    // A sheet row does what it says at the first tap.
                    self.menu = i;
                    self.press(Button::East)
                } else if i == self.selected {
                    self.press(Button::East)
                } else {
                    self.selected = i;
                    Command::None
                }
            }
            _ => Command::None,
        }
    }
    /// A finger held still on a game opens its card.
    pub fn long_press(&mut self, x: f32, y: f32) -> Command {
        self.touch_mode = true;
        if !self.prefs.onboarding_done || !Self::list_screen(self.screen) {
            return Command::None;
        }
        let row = self.regions.iter().rev().find(|r| r.area.contains(x, y)).and_then(|r| match r.action {
            Action::Row(i) => Some(i),
            _ => None,
        });
        if let Some(i) = row {
            self.selected = i;
            self.keyboard = None;
            return self.press(Button::North);
        }
        Command::None
    }
    /// Back from the system (gesture or key): closes the keyboard first, then goes up like B.
    pub fn back(&mut self) -> Command {
        if self.prefs.onboarding_done && self.keyboard.is_some() {
            self.keyboard = None;
            return Command::None;
        }
        self.press(Button::South)
    }
    /// A finger moved by (dx, dy) canvas pixels: the page follows it (the landscape shelf moves a game per step).
    pub fn drag(&mut self, dx: f32, dy: f32) -> bool {
        if self.controls_open {
            self.drag_rest += dy;
            if self.drag_rest.abs() > 35.0 {
                self.controls_scroll = (self.controls_scroll as i32 + if self.drag_rest < 0.0 {1} else {-1}).max(0) as usize;
                self.drag_rest = 0.0;
            }
            return true;
        }
        self.touch_mode = true;
        if !self.prefs.onboarding_done {
            return false;
        }
        if self.screen == Screen::Box {
            let d = -dy.round() as i32;
            if matches!(self.prefs.theme, Theme::Minimal | Theme::Cards | Theme::Split) {
                self.info_view.scroll_by(d);
            } else {
                self.box_view.scroll_by(d);
            }
            return true;
        }
        let shelves = self.rows.is_some() && self.screen == Screen::Library;
        if shelves {
            // A finger goes along a shelf or across the shelves, whichever way it set off.
            let along = *self.drag_along.get_or_insert(dx.abs() >= dy.abs());
            if self.step.0 > 0.0 && !along {
                let Some(across) = self.drag_across.map(|d| d + dy) else { return false };
                self.drag_across = Some(across);
                if across.abs() < self.dp * 48.0 {
                    return false;
                }
                self.drag_across = None;
                return self.step_shelf(if across < 0.0 { 1 } else { -1 });
            }
            if self.step.0 == 0.0 && along {
                return self.drag_shelf(dx);
            }
        }
        if self.step.0 > 0.0 && Self::list_screen(self.screen) {
            self.drag_rest += dx;
            let before = self.selected;
            let range = self.shelf_row().filter(|_| shelves).map(|r| r.start as i32..r.end as i32);
            while self.drag_rest.abs() >= self.step.0 {
                let one = if self.drag_rest > 0.0 { -1 } else { 1 };
                self.drag_rest -= -one as f32 * self.step.0;
                // A finger stays on its shelf.
                if range.as_ref().is_some_and(|r| !r.contains(&(self.selected as i32 + one))) {
                    self.drag_rest = 0.0;
                    break;
                }
                self.move_by(one);
            }
            self.follow = false;
            return before != self.selected;
        }
        let before = self.scroll;
        self.scroll = (self.scroll - dy).clamp(0.0, self.scroll_max);
        self.follow = false;
        before != self.scroll
    }
    /// A portrait shelf follows the finger along it: the one the finger came down on.
    fn drag_shelf(&mut self, dx: f32) -> bool {
        let y = self.finger_y as i32;
        let band = match self.drag_row {
            Some(key) => self.row_bands.iter().find(|b| b.2 == key),
            None => self.row_bands.iter().find(|b| (b.0..b.1).contains(&y)),
        };
        let Some(&(_, _, key, most)) = band else { return false };
        self.drag_row = Some(key);
        self.follow = false;
        let scroll = self.row_scroll.entry(key).or_default();
        let before = *scroll;
        *scroll = (*scroll - dx).clamp(0.0, most);
        before != *scroll
    }
    /// The finger left at (vx, vy) canvas pixels a second: the page keeps going and slows down.
    pub fn fling(&mut self, vx: f32, vy: f32) {
        self.drag_rest = 0.0;
        self.velocity = if self.step.0 > 0.0 && Self::list_screen(self.screen) { (vx, 0.0) } else { (0.0, vy) };
        if self.screen == Screen::Box {
            self.velocity = (0.0, vy);
        }
        if self.rows.is_some() && self.screen == Screen::Library {
            self.velocity = match (self.drag_along, self.step.0 > 0.0) {
                (Some(true), _) => (vx, 0.0),
                (_, true) => (0.0, 0.0),
                (_, false) => (0.0, vy),
            };
        }
    }
    /// Stops a fling (a finger came down).
    pub fn hold(&mut self) {
        self.velocity = (0.0, 0.0);
        self.drag_rest = 0.0;
        self.drag_along = None;
        self.drag_row = None;
        self.drag_across = Some(0.0);
    }
    /// A finger came down at this height of the page.
    pub fn hold_at(&mut self, y: f32) {
        self.hold();
        self.finger_y = y;
    }
    /// Advances a fling by `dt` seconds; false once it has stopped.
    pub fn tick(&mut self, dt: f32) -> bool {
        let (vx, vy) = self.velocity;
        if vx.abs() < 40.0 && vy.abs() < 40.0 {
            self.velocity = (0.0, 0.0);
            return false;
        }
        let moved = self.drag(vx * dt, vy * dt);
        // About 0.3 s to lose two thirds of the speed: a light flick crosses a page, a hard one many.
        let keep = (-dt * 3.2).exp();
        self.velocity = (vx * keep, vy * keep);
        if !moved && self.step.0 == 0.0 && self.screen != Screen::Box {
            self.velocity = (0.0, 0.0);
        }
        true
    }
    pub fn selected_region(&self, region: &Region) -> bool {
        match region.action {
            Action::Setup(i) => i == self.setup_row,
            Action::Row(i) => {
                i == if self.sheet() {
                    self.menu
                } else {
                    self.selected
                }
            }
            Action::Tab(screen) => screen == self.screen,
            Action::Key(r, c) => self.keyboard.as_ref().is_some_and(|k| k.row == r && k.col == c),
            _ => false,
        }
    }
    fn region(&mut self, area: Area, label: String, action: Action) {
        self.regions.push(Region {
            area,
            label: tr(&label),
            action,
        });
    }
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, cover: Option<&Image>) {
        self.prefs.apply_look();
        palette::set_blend(if self.prefs.light { 255 } else { 0 });
        pocket_ui::type_scale::set_system_scale(self.system_font_scale);
        pocket_ui::type_scale::set(play_cards::text_size(self.prefs.text_size, self.system_font_scale));
        pocket_ui::game_card_model::set_touch(self.touch_mode);
        c.clear(pal().void);
        self.hits.clear();
        self.regions.clear();
        self.front_at = None;
        if !self.prefs.onboarding_done {
            self.paint_setup(c, text);
            return;
        }
        if self.controls_open {
            let system = self.game().map_or("", |g| g.system_id).to_string();
            let (back, max) = play_cards::paint_controls(c, text, &system, &self.control_names, self.controls_scroll);
            self.controls_scroll = self.controls_scroll.min(max);
            self.target(back, "Back", Action::Press(Button::South));
            self.target(Area::new(36, 120, c.w - 72, back.y - 140), &play_cards::control_lines(&system, &self.control_names).join(". "), Action::Label);
            return;
        }
        self.paint_phone(c, text, cover);
    }
    /// The rows of the sheet pages: Settings, quick settings, Curator and a facet's values.
    fn menu_rows(&self) -> Vec<phone::MenuRow> {
        let row = |label: &str, value: &str| phone::MenuRow {
            label: tr(label),
            value: tr(value),
            check: None,
            stepper: false,
        };
        let on_off = |on: bool| if on { "On" } else { "Off" };
        match self.screen {
            Screen::Settings => SETTINGS
                .iter()
                .map(|setting| match setting {
                    Setting::Look => row("Look", self.prefs.theme.name()),
                    Setting::Appearance => row("Appearance", if self.prefs.light { "Light" } else { "Dark" }),
                    Setting::Colours => row("Colours", if self.prefs.font == pocket_ui::font_style::FontStyle::Retro { self.prefs.retro_colours.label() } else { self.prefs.colours.label() }),
                    Setting::Font => row("Style", self.prefs.font.label()),
                    Setting::TextSize => row("Text size", self.prefs.text_size.label()),
                    Setting::Language => row("Language", pocket_ui::i18n::current().name()),
                    Setting::Animations => row("Animations", if self.prefs.cozy.unwrap_or(true) { "Cozy" } else { "Quick" }),
                    Setting::Motion => row("Motion", self.prefs.motion.name()),
                    Setting::CozyExtras => row("Cozy extras", ""),
                    Setting::Extra(e) => row(e.label(), on_off(self.prefs.extras.get(*e))),
                    Setting::Sounds => row("UI sounds", on_off(!self.prefs.muted)),
                    Setting::Haptics => row("Haptics", on_off(self.prefs.haptics)),
                    Setting::RemindWhere => row("Remind me where I was", on_off(self.prefs.remind_where.unwrap_or(true))),
                    Setting::ButtonSymbols => row("Button symbols", self.prefs.prompts.label()),
                    Setting::GameBoy => row("Game Boy core", if self.prefs.gameboy_gambatte { "Gambatte" } else { "mGBA" }),
                    Setting::Folder => row("Choose your ROM folder", ""),
                    Setting::GameCheck => row("Game check", &self.check_value()),
                    Setting::Media => row("Covers and pictures", on_off(self.prefs.download_media.unwrap_or(false))),
                    Setting::Updates => row("Updates", "Android APK"),
                    Setting::FirstStart => row("First start", ""),
                })
                .collect(),
            Screen::Extras => {
                let mut rows: Vec<_> = PHONE_EXTRAS.iter().map(|e| row(e.label(), on_off(self.prefs.extras.get(*e)))).collect();
                rows.push(row("GBA console", self.prefs.gba_model.label()));
                rows.push(row("Back", ""));
                rows
            }
            Screen::Quick => vec![row("UI sounds", on_off(!self.prefs.muted)), row("Just Play", ""), row("Settings", "")],
            Screen::Stickers => {
                use pocket_ui::case_stickers::Kind;
                let on = self.game().map(|g| self.prefs.stickers_of(&g.key)).unwrap_or_default();
                let mut rows: Vec<_> = Kind::ALL.iter().map(|k| phone::MenuRow { label: tr(k.label()), value: String::new(), check: Some(on.has(*k)), stepper: false }).collect();
                rows.push(phone::MenuRow { label: tr("Year"), value: self.sticker_year.to_string(), check: None, stepper: true });
                rows.push(row("Back", ""));
                rows
            }
            Screen::Facet => {
                let mut rows = vec![row("Clear filters", "")];
                if let Some(lib) = &self.library {
                    let counts = lib.index.counts(&self.lens, self.facet, false);
                    for (i, value) in lib.index.facet(self.facet).values.iter().enumerate() {
                        rows.push(phone::MenuRow {
                            label: tr(&value.label),
                            value: counts[i].to_string(),
                            check: Some(self.lens.picked(self.facet, &value.key)),
                            stepper: false,
                        });
                    }
                }
                rows.push(row("Show games", ""));
                rows
            }
            _ => {
                let mut rows = vec![row("Surprise me", "")];
                for facet in self.facets() {
                    let picked = self
                        .library
                        .as_ref()
                        .map(|l| l.index.picks_line(&self.lens, facet))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| tr("All"));
                    rows.push(phone::MenuRow {
                        label: tr(facet.label()),
                        value: picked,
                        check: None,
                        stepper: false,
                    });
                }
                if self.rediscover_row() {
                    let n = self.rediscover_games().len();
                    rows.push(row("Rediscover", &if n == 0 { tr("None yet") } else { tr(&format!("{n} games")) }));
                }
                rows.push(row("Show games", ""));
                rows
            }
        }
    }
    fn paint_box(&mut self, c: &mut Canvas, text: &mut Text, cover: Option<&Image>) {
        let Some(lib) = &self.library else { return };
        let Some(&index) = self.visible.get(self.selected) else { return };
        let game = &lib.games[index];
        let m = Medium::of(game.system_id);
        let look = art::Look {
            color: game.color,
            band: game.color,
            plastic: art::plastic(game.system_id),
        };
        if matches!(self.prefs.theme, Theme::Minimal | Theme::Cards | Theme::Split) {
            let (cw, ch) = pocket_ui::game_info::cover_room(c.w, c.h, &lib.cards[index]);
            let key = (index, cw, ch);
            if self.info_cover.as_ref().is_none_or(|(old, _)| *old != key) {
                self.info_cover = cover.map(|image| {
                    let (iw, ih) = image.fit(cw, ch);
                    (key, std::rc::Rc::new(image.resized(iw, ih)))
                });
            }
            self.info_view.paint(
                c,
                text,
                &lib.cards[index],
                self.info_cover.as_ref().map(|(_, image)| image),
                pocket_ui::system_art::get(game.system_id),
            );
            return;
        }
        let k = pocket_ui::game_box::case_height(c.w, c.h, m) as f32 / m.size().h as f32;
        let console = pocket_ui::system_art::get(game.system_id);
        let mut front = media::front(text, m, k, look, &game.name, cover, console, game.system);
        let stickers = self.prefs.stickers_shown(&game.key);
        pocket_ui::case_stickers::on_front(text, &mut front, &stickers, media::label_rect(m, k), index as u64);
        let inside = game_inside(text, game, (m, front.h), look, cover, self.prefs.extras.printed_discs);
        self.box_view.hide_case = self.launching;
        self.box_view.front_rect = None;
        self.box_view.paint(
            c,
            text,
            &lib.cards[index],
            &[],
            &pocket_ui::game_box::Inputs {
                medium: m,
                look,
                front: Some(&front),
                game: inside.as_ref(),
                cover,
                console,
            },
        );
        if self.launching {
            let (ox, oy) = self.card_origin;
            self.front_at = self.box_view.front_rect.map(|(x, y, w, h)| ((x + ox, y + oy, w, h), front));
        }
    }
    pub fn cover_changed(&mut self) {
        self.cards.clear();
        self.tv = None;
        self.info_cover = None;
    }
    pub fn animating(&self) -> bool {
        self.screen == Screen::Box && self.box_view.animating(std::time::Instant::now())
    }
}

/// The rows of the sticker sheet: the stickers, the year, Back.
const STICKER_ROWS: usize = pocket_ui::case_stickers::Kind::ALL.len() + 2;
/// Continue holds only verified open moments, newest played first; then favourites and systems.
fn shelves_of(lib: &Library, prefs: &Preferences, open: &HashSet<String>) -> pocket_ui::phone_shelves::Shelves {
    let games: Vec<(&str, &str)> = lib.games.iter().map(|g| (g.system_id, g.system)).collect();
    let mut played: Vec<(u64, usize)> = lib.games.iter().enumerate().filter(|(_, g)| open.contains(&g.key)).filter_map(|(i, g)| Some((*prefs.played.get(&g.key)?, i))).collect();
    played.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let recent: Vec<usize> = played.into_iter().map(|(_, i)| i).collect();
    let favourites: Vec<usize> = lib.games.iter().enumerate().filter(|(_, g)| prefs.favorites.contains(&g.key)).map(|(i, _)| i).collect();
    pocket_ui::phone_shelves::build(&games, &recent, &favourites)
}

/// The cartridge, card or disc in a case of `front_h`, beside it.
fn game_inside(text: &mut Text, game: &Game, (m, front_h): (Medium, i32), look: art::Look, cover: Option<&Image>, printed: bool) -> Option<art::Pic> {
    let h = m.game().size_beside(m, front_h).1;
    if m.game().is_round() {
        // A disc or a UMD: printed with the cover's art and the title, or with Printed discs off a plain one in the
        // system's colour (`app_cabinet::printed_disc` on the shelf).
        let kind = media::DiscKind::of(game.system_id);
        Some(if printed {
            media::disc_pic(text, kind, h, look, &game.name, cover)
        } else {
            media::disc_pic(text, kind, h, art::Look { color: game.color, ..look }, "", None)
        })
    } else {
        media::game_pic(text, m.game(), h, look, &game.name, cover, pocket_ui::system_art::get(game.system_id))
    }
}
/// Same provider identity and alternative-core namespace as the player's save directory.
pub fn game_key(game: &Game, gambatte: bool) -> String {
    let mut hash = key_hash(&game.key);
    if gambatte && matches!(game.system_id, "gb" | "gbc") {
        hash = "gambatte".bytes().fold(hash, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    }
    format!("{hash:016x}")
}

fn key_hash(key: &str) -> u64 {
    key.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

/// [`game_key`] of a game no Game Boy choice touches: the big ones.
pub fn game_key_of(key: &str) -> String {
    format!("{:016x}", key_hash(key))
}

pub(crate) fn tr(s: &str) -> String {
    pocket_ui::i18n::tr(s)
}

pub fn library(games: Vec<Game>) -> Library {
    use pocket_ui::{
        curator::{
            features::{features_for, CuratorDbFeel, FeelSource, GameRef},
            Catalog, Index, UserState,
        },
        gamedb::GameDb,
    };
    let mut db = GameDb::embedded();
    let feel = CuratorDbFeel;
    let mut entries = Vec::new();
    let mut features = Vec::new();
    let mut cards = Vec::new();
    let mut systems = std::collections::BTreeMap::new();
    for (i, g) in games.iter().enumerate() {
        let stem = std::path::Path::new(&g.relative).file_stem().unwrap_or_default().to_string_lossy();
        let info = db.lookup(g.system_id, &stem);
        let tags = info.as_ref().map(|d| feel.tags(g.system_id, d)).unwrap_or_default();
        let identity = std::path::PathBuf::from(&g.key);
        // The facts that can lead to other games (Connections), by `connections::Kind`.
        entries.push(pocket_ui::connections::Entry {
            at: (0, i),
            path: identity.clone(),
            name: g.name.clone(),
            year: info.as_ref().and_then(|d| d.year),
            values: info
                .as_ref()
                .map(|d| [d.developers.clone(), d.publishers.clone(), d.series.clone(), d.genres.clone()])
                .unwrap_or_default(),
        });
        features.push(features_for(
            GameRef {
                at: (0, i),
                system: g.system_id,
                path: &identity,
                name: &g.name,
            },
            info.as_ref(),
            &tags,
        ));
        systems.entry(g.system_id).or_insert(pocket_ui::curator::SystemName {
            id: g.system_id.into(),
            name: g.system.into(),
            short: g.system.into(),
        });
        let mut card = CardData {
            title: g.name.clone(),
            system: g.system.into(),
            stripe: g.color,
            ..Default::default()
        };
        if let Some(info) = info {
            card.matched = true;
            card.year = info.year;
            card.players = info.players.clone();
            card.scores = Some(pocket_ui::gamedb::top_scores(&info, 2).join(" · "));
            for (label, values) in [
                ("Developer", info.developers),
                ("Publisher", info.publishers),
                ("Genre", info.genres),
                ("Series", info.series),
            ] {
                if !values.is_empty() {
                    card.facts.push((label, values.join(", ")));
                }
            }
        }
        if !g.missing.is_empty() {
            card.note = format!("Missing disc files: {}", g.missing.join(", "));
        }
        cards.push(card);
    }
    let index = Index::build(Arc::new(Catalog::build(features, systems.into_values().collect())), UserState::default());
    let search = pocket_ui::search::Index::build(games.iter().enumerate().map(|(i, g)| (0, i, g.name.as_str())));
    let connections = pocket_ui::connections::Index::build(entries);
    Library { games, cards, index, search, connections }
}

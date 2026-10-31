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
#[path = "onboarding.rs"]
mod onboarding;
#[path = "phone.rs"]
mod phone;

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub light: bool,
    pub selected_uri: String,
    pub selected_key: String,
    pub theme: Theme,
    pub favorites: HashSet<String>,
    pub german: Option<bool>,
    pub text_size: pocket_ui::text_size::TextSize,
    pub muted: bool,
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
        pocket_ui::button_prompts::set(self.prompts);
        pocket_ui::motion::set_level(self.motion);
        pocket_ui::motion::set_cozy(self.cozy.unwrap_or(true));
        pocket_ui::cozy_extras::set(self.extras);
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
    ButtonSymbols,
    GameBoy,
    Folder,
    Media,
    FirstStart,
}
const SETTINGS: [Setting; 19] = [
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
    Setting::Extra(pocket_ui::cozy_extras::Extra::CaseSounds),
    Setting::Extra(pocket_ui::cozy_extras::Extra::IdleSlideshow),
    Setting::ButtonSymbols,
    Setting::GameBoy,
    Setting::Folder,
    Setting::Media,
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
    Quick,
    Box,
}
#[derive(Clone)]
pub enum Action {
    /// A fact on the open card that leads to the player's other games.
    Link(usize),
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
    PersistSetup,
    None,
    Launch,
    Folder,
    Persist,
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
    pub keyboard: Option<Keyboard>,
    pub query: String,
    pub lens: pocket_ui::curator::Lens,
    setup_row: usize,
    setup_preview: Option<((Theme, bool, i32, i32), art::Pic)>,
    pub scan_running: bool,
    pub controller_connected: bool,
    pub wifi_ready: bool,
    pub snapshot: Option<Image>,
    menu: usize,
    connection: Option<OpenList>,
    list_back: Vec<ListBack>,
    room: RoomUi,
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
            keyboard: None,
            query: String::new(),
            lens: Default::default(),
            setup_row: 0,
            setup_preview: None,
            scan_running: false,
            controller_connected: false,
            wifi_ready: false,
            snapshot: None,
            menu: 0,
            connection: None,
            list_back: Vec::new(),
            room: RoomUi::default(),
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
        }
    }
    pub fn game(&self) -> Option<&Game> {
        self.library.as_ref()?.games.get(*self.visible.get(self.selected)?)
    }
    pub fn install(&mut self, library: Library) {
        let restored = library
            .games
            .iter()
            .position(|g| g.key == self.prefs.selected_key || g.uri == self.prefs.selected_uri)
            .unwrap_or(0);
        self.library = Some(library);
        self.message.clear();
        self.filter();
        self.selected = self.visible.iter().position(|i| *i == restored).unwrap_or(0);
        self.cards.clear();
    }
    pub fn filter(&mut self) {
        let Some(lib) = &self.library else {
            self.visible.clear();
            return;
        };
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
            Screen::Settings | Screen::Extras | Screen::Quick | Screen::Curator | Screen::Facet | Screen::Box => return,
            _ => (0..lib.games.len()).collect(),
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
    /// What was painted with the old colours or type (case fronts, shelves, the TV): made again at the next paint.
    fn drop_pictures(&mut self) {
        self.cards.clear();
        self.shelves.clear();
        self.tv = None;
    }
    fn count(&self) -> usize {
        match self.screen {
            Screen::Settings => SETTINGS.len(),
            Screen::Extras => PHONE_EXTRAS.len() + 1,
            Screen::Quick => 2,
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
        let selected = if matches!(self.screen, Screen::Settings | Screen::Extras | Screen::Quick | Screen::Curator | Screen::Facet) {
            &mut self.menu
        } else {
            &mut self.selected
        };
        *selected = (*selected as i32 + delta).clamp(0, count.saturating_sub(1) as i32) as usize;
        self.follow = true;
    }
    pub fn press(&mut self, b: Button) -> Command {
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
        if self.screen == Screen::Box && self.card_link_press(b) {
            return Command::None;
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
                self.filter();
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
                        let i = Theme::ALL.iter().position(|t| *t == self.prefs.theme).unwrap_or(0);
                        self.prefs.theme = Theme::ALL[(i + 1) % Theme::ALL.len()];
                        self.cards.clear();
                    }
                    Setting::Appearance => {
                        self.prefs.light = !self.prefs.light;
                        self.cards.clear();
                    }
                    Setting::Colours => {
                        self.prefs.colours = self.prefs.colours.step(true);
                        self.drop_pictures();
                    }
                    Setting::Font => {
                        self.prefs.font = self.prefs.font.next();
                        self.drop_pictures();
                    }
                    Setting::Language => {
                        let german = self.prefs.german.unwrap_or(pocket_ui::i18n::current() == pocket_ui::i18n::Lang::De);
                        self.prefs.german = Some(!german);
                        pocket_ui::i18n::set(if german { pocket_ui::i18n::Lang::En } else { pocket_ui::i18n::Lang::De });
                    }
                    Setting::TextSize => self.prefs.text_size = self.prefs.text_size.next(),
                    Setting::Sounds => self.prefs.muted = !self.prefs.muted,
                    Setting::Motion => self.prefs.motion = self.prefs.motion.next(),
                    Setting::ButtonSymbols => self.prefs.prompts = self.prefs.prompts.step(true),
                    Setting::GameBoy => self.prefs.gameboy_gambatte = !self.prefs.gameboy_gambatte,
                    Setting::Folder => return Command::Folder,
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
                } else {
                    self.navigate(Screen::Settings);
                    Command::None
                }
            }
            Screen::Quick => {
                if self.menu == 0 {
                    self.prefs.muted = !self.prefs.muted;
                    Command::Persist
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
                            let secs = *self.prefs.play_secs.get(&g.key)?;
                            Some((format!("{}/{}", g.system_id, g.name), pocket_ui::library_meta::GameMeta { secs, ..Default::default() }))
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
        let Some((key, start)) = self.prefs.playing.take() else { return };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let secs = now.saturating_sub(start).min(86_400);
        *self.prefs.play_secs.entry(key).or_default() += secs;
        self.room.earned = None;
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
        let base = pocket_ui::platform::data_dir().join("states").join(key).join(stem);
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
    pub fn tap(&mut self, x: f32, y: f32) -> Command {
        self.touch_mode = true;
        if !self.prefs.onboarding_done {
            return self.setup_tap(x, y);
        }
        let action = self.regions.iter().rev().find(|r| r.area.contains(x, y)).map(|r| r.action.clone());
        match action {
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
            Some(Action::Folder) => Command::Folder,
            Some(Action::Row(i)) => {
                if matches!(self.screen, Screen::Settings | Screen::Extras | Screen::Quick | Screen::Curator | Screen::Facet) {
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
        if self.step.0 > 0.0 && Self::list_screen(self.screen) {
            self.drag_rest += dx;
            let before = self.selected;
            while self.drag_rest.abs() >= self.step.0 {
                let one = if self.drag_rest > 0.0 { -1 } else { 1 };
                self.drag_rest -= -one as f32 * self.step.0;
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
    /// The finger left at (vx, vy) canvas pixels a second: the page keeps going and slows down.
    pub fn fling(&mut self, vx: f32, vy: f32) {
        self.drag_rest = 0.0;
        self.velocity = if self.step.0 > 0.0 && Self::list_screen(self.screen) { (vx, 0.0) } else { (0.0, vy) };
        if self.screen == Screen::Box {
            self.velocity = (0.0, vy);
        }
    }
    /// Stops a fling (a finger came down).
    pub fn hold(&mut self) {
        self.velocity = (0.0, 0.0);
        self.drag_rest = 0.0;
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
                i == if matches!(self.screen, Screen::Settings | Screen::Extras | Screen::Quick | Screen::Curator | Screen::Facet) {
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
        let text_size = if self.system_font_scale >= 1.25 {
            pocket_ui::text_size::TextSize::Large
        } else if self.system_font_scale >= 1.12 && self.prefs.text_size == pocket_ui::text_size::TextSize::Normal {
            pocket_ui::text_size::TextSize::Medium
        } else {
            self.prefs.text_size
        };
        pocket_ui::type_scale::set(text_size);
        pocket_ui::game_card_model::set_touch(self.touch_mode);
        c.clear(pal().void);
        self.hits.clear();
        self.regions.clear();
        if !self.prefs.onboarding_done {
            self.paint_setup(c, text);
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
        };
        let on_off = |on: bool| if on { "On" } else { "Off" };
        match self.screen {
            Screen::Settings => SETTINGS
                .iter()
                .map(|setting| match setting {
                    Setting::Look => row("Look", self.prefs.theme.name()),
                    Setting::Appearance => row("Appearance", if self.prefs.light { "Light" } else { "Dark" }),
                    Setting::Colours => row("Colours", self.prefs.colours.label()),
                    Setting::Font => row("Font", self.prefs.font.label()),
                    Setting::TextSize => row("Text size", self.prefs.text_size.label()),
                    Setting::Language => row("Language", pocket_ui::i18n::current().name()),
                    Setting::Animations => row("Animations", if self.prefs.cozy.unwrap_or(true) { "Cozy" } else { "Quick" }),
                    Setting::Motion => row("Motion", self.prefs.motion.name()),
                    Setting::CozyExtras => row("Cozy extras", ""),
                    Setting::Extra(e) => row(e.label(), on_off(self.prefs.extras.get(*e))),
                    Setting::Sounds => row("UI sounds", on_off(!self.prefs.muted)),
                    Setting::ButtonSymbols => row("Button symbols", self.prefs.prompts.label()),
                    Setting::GameBoy => row("Game Boy core", if self.prefs.gameboy_gambatte { "Gambatte" } else { "mGBA" }),
                    Setting::Folder => row("Choose your ROM folder", ""),
                    Setting::Media => row("Covers and pictures", on_off(self.prefs.download_media.unwrap_or(false))),
                    Setting::FirstStart => row("First start", ""),
                })
                .collect(),
            Screen::Extras => {
                let mut rows: Vec<_> = PHONE_EXTRAS.iter().map(|e| row(e.label(), on_off(self.prefs.extras.get(*e)))).collect();
                rows.push(row("Back", ""));
                rows
            }
            Screen::Quick => vec![row("UI sounds", on_off(!self.prefs.muted)), row("Settings", "")],
            Screen::Facet => {
                let mut rows = vec![row("Clear filters", "")];
                if let Some(lib) = &self.library {
                    let counts = lib.index.counts(&self.lens, self.facet, false);
                    for (i, value) in lib.index.facet(self.facet).values.iter().enumerate() {
                        rows.push(phone::MenuRow {
                            label: tr(&value.label),
                            value: counts[i].to_string(),
                            check: Some(self.lens.picked(self.facet, &value.key)),
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
        let mut stickers = pocket_ui::case_stickers::CaseStickers::default();
        stickers.set(pocket_ui::case_stickers::Kind::Favourite, self.prefs.extras.case_stickers && self.prefs.favorites.contains(&game.key), 2026);
        pocket_ui::case_stickers::on_front(text, &mut front, &stickers, media::label_rect(m, k), index as u64);
        let h = m.game().size_beside(m, front.h).1;
        let inside = if m.game().is_round() {
            // A disc or a UMD: printed with the cover's art and the title, or with Printed discs off a plain one in the
            // system's colour (`app_cabinet::printed_disc` on the shelf).
            let kind = media::DiscKind::of(game.system_id);
            Some(if self.prefs.extras.printed_discs {
                media::disc_pic(text, kind, h, look, &game.name, cover)
            } else {
                media::disc_pic(text, kind, h, art::Look { color: game.color, ..look }, "", None)
            })
        } else {
            media::game_pic(text, m.game(), h, look, &game.name, cover, console)
        };
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

/// Same provider identity and alternative-core namespace as the player's save directory.
pub fn game_key(game: &Game, gambatte: bool) -> String {
    let mut hash = game
        .key
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    if gambatte && matches!(game.system_id, "gb" | "gbc") {
        hash = "gambatte".bytes().fold(hash, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    }
    format!("{hash:016x}")
}

fn tr(s: &str) -> String {
    if pocket_ui::i18n::current() == pocket_ui::i18n::Lang::De {
        pocket_ui::i18n::german().translate(s).map(|s| s.into_owned()).unwrap_or_else(|| s.into())
    } else {
        s.into()
    }
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

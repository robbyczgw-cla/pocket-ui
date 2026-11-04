//! The feature registry: every optional part of pocket-ui, with the switch that turns it on (the same one its
//! Settings page sets), what the first start's profiles do with it, the version it came in and its card (title, one
//! line, a preview). The first start's "How do you like to play?", Settings → Tour and "New in this version" read
//! only this list, so a new feature adds one entry to `FEATURES` (docs/DEVELOPING.md, "Adding a feature to the tour").
//!
//! Nothing here touches a file: `Switches` is a copy of the App's own settings, the cards change the copy, and the App
//! takes it back and saves it the way each Settings page does (`app_tour.rs`). So a profile never keeps a state of its
//! own; every choice is a switch in Settings afterwards.
//!
//! The cards (`Cards`) are a small state machine with their painting. A card's preview is painted the first time the
//! card is shown (`feature_previews`), at the screen's size and scaled down, and kept while the cards are open.

use std::collections::HashMap;

use crate::curator::prefs::Prefs;
use crate::feature_previews as pv;
use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::leds;
use crate::on_start::OnStart;
use crate::pad::Button;
use crate::palette::*;
use crate::settings::{Frames, Motion, TextSize};
use crate::text::{Face, Text};
use crate::touch::Area;
use crate::ui_sound;
use crate::update::newer;

/// The switches the cards and profiles set: copies of what Settings keeps, each in its own file (settings.json,
/// curator.json, sounds.json, haptics.json, leds.json). The App fills it and takes it back (`App::adopt_switches`).
#[derive(Clone, Debug, PartialEq)]
pub struct Switches {
    pub motion: Motion,
    /// Settings › Look › Text size (settings.json's `text_size`).
    pub text_size: TextSize,
    pub on_start: OnStart,
    /// Settings › Continue where I quit (settings.json's `forget_where_quit`, the other way round).
    pub remember_quit: bool,
    pub frames: Frames,
    /// Settings › Playing › Over the game (`over_game`).
    pub over_game: crate::over_game::Level,
    pub curator: Prefs,
    pub sound: ui_sound::Config,
    pub haptics: bool,
    pub leds: leds::Config,
}

impl Default for Switches {
    /// The start settings of a new device.
    fn default() -> Switches {
        Switches {
            motion: Motion::default(),
            text_size: TextSize::default(),
            on_start: OnStart::default(),
            remember_quit: true,
            frames: Frames::default(),
            over_game: crate::over_game::Level::Off,
            curator: Prefs::default(),
            sound: ui_sound::Config::default(),
            haptics: false,
            leds: leds::Config::default(),
        }
    }
}

/// Whether Easy to read is on: Large text and a Motion that isn't Full. The Tour card's switch, and what puts Zoom
/// into the pause menu's main page (`app_zoom::zoom_row_on`).
pub fn reading_on(text_size: TextSize, motion: Motion) -> bool {
    text_size == TextSize::Large && motion != Motion::Full
}

/// A feature's switch: on or off, read from and written to `Switches`.
#[derive(Clone, Copy)]
pub struct Switch {
    pub get: fn(&Switches) -> bool,
    pub set: fn(&mut Switches, bool),
}

/// What a profile does with a switch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    On,
    Off,
    /// Leaves it as it is.
    Keep,
}

/// The first start's answer to "How do you like to play?".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Profile {
    /// "The shelf": the look and switches as they ship.
    Simple,
    /// The start settings with the Easy look (`Profile::look`).
    JustPlay,
    Enthusiast,
    /// The cards, one by one.
    Choose,
    /// Kids Mode's own steps (`app_kids`): no switch of this list changes.
    Child,
}

impl Profile {
    pub const ALL: [Profile; 5] = [Profile::Simple, Profile::JustPlay, Profile::Enthusiast, Profile::Choose, Profile::Child];

    /// What this device offers: no "For a child" where the host owns buttons a child can reach (the Steam Deck's Steam button).
    pub fn offered() -> Vec<Profile> {
        Profile::ALL.into_iter().filter(|p| *p != Profile::Child || !crate::device::hides_here("kids")).collect()
    }

    /// Settings › Help & tools › How you like to play: the first start's answers, without Kids Mode (it has its own row).
    pub fn offered_in_settings() -> Vec<Profile> {
        Profile::offered().into_iter().filter(|p| *p != Profile::Child).collect()
    }

    /// The profile the switches stand at: Simple or Enthusiast where they are exactly what it sets, otherwise the
    /// player's own choice.
    pub fn of(switches: &Switches) -> Profile {
        [Profile::Simple, Profile::Enthusiast]
            .into_iter()
            .find(|p| {
                let mut s = switches.clone();
                apply(*p, &mut s);
                s == *switches
            })
            .unwrap_or(Profile::Choose)
    }

    pub fn name(self) -> &'static str {
        match self {
            Profile::Simple => "The shelf",
            Profile::JustPlay => "Easy",
            Profile::Enthusiast => "Enthusiast",
            Profile::Choose => "Choose for yourself",
            Profile::Child => "For a child",
        }
    }

    /// The look this answer comes with.
    pub fn look(self) -> Option<crate::settings::Theme> {
        (self == Profile::JustPlay).then_some(crate::settings::Theme::Big)
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Profile::Simple => "Your games on a shelf, as it comes.",
            Profile::JustPlay => "One big game at a time, simple menus.",
            Profile::Enthusiast => "Suggestions, play statistics, picture details and frames, all on.",
            Profile::Choose => "One card per feature, with a preview: on, off, or later.",
            Profile::Child => "A big, simple shelf with the games you pick, behind your PIN.",
        }
    }
}

/// A feature's card painter: the feature's own screen (or the part of it that shows the feature) at the screen's
/// size, 1240 x 1080, in the palette in use, with made-up data. No file, no network.
pub type Preview = fn(&mut Canvas, &mut Text);

/// One entry of the registry.
pub struct Feature {
    /// Stable name, kept in settings.json (`tour_later`) and used by the snapshots.
    pub id: &'static str,
    /// The card's title.
    pub title: &'static str,
    /// What it does in everyday words, for the first start's summary; no feature names.
    pub plain: &'static str,
    /// One line under it.
    pub line: &'static str,
    /// Where it is changed or found later.
    pub place: &'static str,
    /// Its section in the user guide, `page.md#anchor` (checked by `doc_checks`).
    pub guide: &'static str,
    /// The version it came in; "New in this version" shows the entries newer than the last version seen.
    pub since: &'static str,
    /// None: nothing to switch, the card says where it is (Settings → Tour and "New in this version" only).
    pub switch: Option<Switch>,
    pub simple: Pick,
    pub enthusiast: Pick,
    pub preview: Preview,
    /// Not wired yet: which branch brings it. Such entries are never shown.
    pub todo: Option<&'static str>,
}

impl Feature {
    pub fn pick(&self, p: Profile) -> Pick {
        match p {
            Profile::Simple | Profile::JustPlay => self.simple,
            Profile::Enthusiast => self.enthusiast,
            Profile::Choose | Profile::Child => Pick::Keep,
        }
    }

    pub fn is_on(&self, s: &Switches) -> Option<bool> {
        self.switch.map(|w| (w.get)(s))
    }
}

/// The version every device set up before the cards existed counts as having seen (`Settings::seen_version` unset):
/// the version of the first entries.
pub const BASELINE: &str = "0.1.0";

/// At most this many cards after an update.
pub const NEW_MAX: usize = 3;

fn curator(s: &mut Switches) -> &mut Prefs {
    &mut s.curator
}

/// Every feature, in the order of the cards. The switch cards come first: they are the first start's "Choose
/// yourself". Within each group the order is what a new player needs first (continuing a game, quitting, the feel of
/// the menus), the Curator's cards last: the first-start walk of 2026-11-04 opened the tour on "Lens & smart shelves".
pub static FEATURES: &[Feature] = &[
    Feature {
        // Was "Quit without saving", a pause-menu entry of its own (its id is kept: settings.json's `tour_later`).
        id: "quit_without_saving",
        title: "Continue where I quit",
        plain: "continue where you quit",
        line: "On: Quit keeps the moment and the next start continues there. Off: only the game's own saves stay.",
        place: "Settings › Playing › Continue where I quit",
        guide: "settings.md#continue-where-i-quit",
        since: "0.1.0",
        switch: Some(Switch { get: |s| s.remember_quit, set: |s, on| s.remember_quit = on }),
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::remember_quit,
        todo: None,
    },
    Feature {
        id: "last_game",
        title: "Continue your last game",
        plain: "continue your last game at start",
        line: "When the Pocket starts, the game you played last continues where you left off.",
        place: "Settings › Playing › On start",
        guide: "settings.md#on-start",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.on_start == OnStart::Continue,
            set: |s, on| s.on_start = if on { OnStart::Continue } else { OnStart::Home },
        }),
        simple: Pick::Off,
        enthusiast: Pick::On,
        preview: pv::last_game,
        todo: None,
    },
    Feature {
        id: "motion",
        title: "Motion",
        plain: "gentle animations",
        line: "Selections glide and pages slide in. Off shows every press at once, without animation.",
        place: "Settings › Sound & feel › Motion",
        guide: "settings.md#motion",
        since: "0.1.0",
        // Off is Motion Off; Reduced counts as not Full, and a card left on Off keeps it.
        switch: Some(Switch {
            get: |s| s.motion == Motion::Full,
            set: |s, on| s.motion = if on { Motion::Full } else { Motion::Off },
        }),
        simple: Pick::On,
        enthusiast: Pick::On,
        preview: pv::motion,
        todo: None,
    },
    Feature {
        id: "easy_to_read",
        title: "Easy to read",
        plain: "larger text and calmer menus",
        line: "Larger text, menus that only fade, and Zoom in the pause menu. Switches Text size and Motion together.",
        place: "Settings › Look › Text size, Settings › Sound & feel › Motion",
        guide: "easy-to-read.md#easy-to-read",
        since: "0.2.0",
        // On is Large text and Reduced motion (an Off motion stays Off); off is the design's Normal text and Full motion.
        switch: Some(Switch {
            get: |s| reading_on(s.text_size, s.motion),
            set: |s, on| {
                if on {
                    s.text_size = TextSize::Large;
                    if s.motion == Motion::Full {
                        s.motion = Motion::Reduced;
                    }
                } else {
                    s.text_size = TextSize::Normal;
                    s.motion = Motion::Full;
                }
            },
        }),
        // A choice of its own, not part of a profile: Simple is the defaults, and larger text is for whoever needs it.
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::easy_to_read,
        todo: None,
    },
    Feature {
        id: "sounds",
        title: "Menu sounds",
        plain: "soft clicks in the menus",
        line: "Soft clicks as you move through the menus. Never in games.",
        place: "Settings › Sound & feel › Sounds",
        guide: "playing.md#menu-sounds",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.sound.on,
            set: |s, on| s.sound.on = on,
        }),
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::sounds,
        todo: None,
    },
    Feature {
        id: "haptics",
        title: "Haptics",
        plain: "a light tick as you move",
        line: "A very short tick in your hands as the selection moves.",
        place: "Settings › Sound & feel › Haptics",
        guide: "settings.md#open-settings",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.haptics,
            set: |s, on| s.haptics = on,
        }),
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::haptics,
        todo: None,
    },
    Feature {
        id: "leds",
        title: "Stick lights",
        plain: "lights around the sticks",
        line: "The lights around the sticks glow in one calm colour. Colour and brightness in Settings.",
        place: "Settings › Sound & feel › LEDs",
        guide: "settings.md#leds",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.leds.mode != leds::Mode::Off,
            set: |s, on| {
                // On keeps a choice made before (Battery, a colour); from Off it is the colour.
                if !on {
                    s.leds.mode = leds::Mode::Off;
                } else if s.leds.mode == leds::Mode::Off {
                    s.leds.mode = leds::Mode::Colour;
                }
            },
        }),
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::leds,
        todo: None,
    },
    // ---- nothing to switch: where it is ----
    Feature {
        id: "frames",
        title: "Screen frames on every console",
        plain: "frames around the picture on every console",
        line: "Auto frames the picture on the home consoles too, not only on the handhelds.",
        place: "Settings › Playing › Screen frames",
        guide: "settings.md#screen-frames",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.frames == Frames::Every,
            set: |s, on| s.frames = if on { Frames::Every } else { Frames::Off },
        }),
        simple: Pick::Off,
        enthusiast: Pick::On,
        preview: pv::frames,
        todo: None,
    },
    Feature {
        id: "echoes",
        title: "Echoes",
        plain: "suggestions for similar games",
        line: "More like this, on every game card: games you own that play alike, each with a reason.",
        place: "Settings › Games › Curator",
        guide: "curator.md#echoes",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.curator.echoes,
            set: |s, on| curator(s).echoes = on,
        }),
        simple: Pick::On,
        enthusiast: Pick::On,
        preview: pv::echoes,
        todo: None,
    },
    Feature {
        id: "versions",
        title: "Versions",
        plain: "other versions of a game",
        line: "A game you own on several systems shows once in mixed lists and starts the version you prefer.",
        place: "Settings › Games › Curator",
        guide: "curator.md#versions",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.curator.merge_versions,
            set: |s, on| curator(s).merge_versions = on,
        }),
        simple: Pick::On,
        enthusiast: Pick::On,
        preview: pv::versions,
        todo: None,
    },
    Feature {
        id: "stickers",
        title: "Labels",
        plain: "your own labels for games",
        line: "Your own labels for games, such as Couch co-op. Pin one to see it on Home.",
        place: "Settings › Games › Curator",
        guide: "curator.md#labels",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.curator.stickers,
            set: |s, on| curator(s).stickers = on,
        }),
        simple: Pick::On,
        enthusiast: Pick::On,
        preview: pv::stickers,
        todo: None,
    },
    Feature {
        id: "up_next",
        title: "Up next after a game",
        plain: "a next game to try after you quit",
        line: "When you quit a game, one line on the shelf suggests a game of yours like it. X plays it; nothing starts by itself.",
        place: "Settings › Games › Curator",
        guide: "curator.md#up-next",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.curator.up_next,
            set: |s, on| curator(s).up_next = on,
        }),
        // Off by default and in Simple: for a few seconds Up next makes A play the suggestion.
        simple: Pick::Off,
        enthusiast: Pick::On,
        preview: pv::up_next,
        todo: None,
    },
    Feature {
        id: "play_history",
        title: "Play history",
        plain: "your play time by week and year",
        line: "Curator › History: what you played this week, month and year, and a calm Year in review.",
        place: "Settings › Games › Curator",
        guide: "curator.md#play-history",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.curator.history,
            set: |s, on| curator(s).history = on,
        }),
        // On from the start for everyone: it takes no room until something was played, and never pops up.
        simple: Pick::Keep,
        enthusiast: Pick::On,
        preview: pv::play_history,
        todo: None,
    },
    Feature {
        id: "echoes_elsewhere",
        title: "Echoes beyond this device",
        plain: "suggestions from games you don't own yet",
        line: "Echoes also names a few games you don't have, from the offline game list. Names only.",
        place: "Settings › Games › Curator",
        guide: "curator.md#echoes",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.curator.not_on_device,
            set: |s, on| curator(s).not_on_device = on,
        }),
        simple: Pick::Off,
        enthusiast: Pick::On,
        preview: pv::echoes_elsewhere,
        todo: None,
    },
    Feature {
        id: "pixel_path",
        title: "Pixel Path",
        plain: "picture and sound details in the pause menu",
        line: "In the pause menu: how a game's picture and sound reach you, stage by stage, graded in colour.",
        place: "Settings › Games › Curator",
        guide: "pause-menu.md#pixel-path",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.curator.pixel_path,
            set: |s, on| curator(s).pixel_path = on,
        }),
        simple: Pick::Off,
        enthusiast: Pick::On,
        preview: pv::pixel_path,
        todo: None,
    },
    Feature {
        id: "pixel_path_live",
        title: "Pixel Path over every game",
        plain: "a line of picture details over every game",
        line: "One quiet line beside the picture: the emulator, the scale, the frame rate, the latency, the sound.",
        place: "Settings › Playing › Over the game",
        guide: "pause-menu.md#pixel-path-over-the-game",
        since: "0.1.0",
        // Over the game's Pixel Path level (below a console's or a game's own choice); Off takes only that level
        // away, so the FPS line stays where it was chosen. It doesn't need the pause menu's Pixel Path (2026-10-25).
        switch: Some(Switch {
            get: |s| s.over_game == crate::over_game::Level::PixelPath,
            set: |s, on| {
                use crate::over_game::Level;
                if on {
                    s.over_game = Level::PixelPath;
                } else if s.over_game == Level::PixelPath {
                    s.over_game = Level::Off;
                }
            },
        }),
        simple: Pick::Keep,
        // A line over every game is a choice of its own, not part of a profile (Alex, 2026-10-31, on the RG28XX).
        enthusiast: Pick::Keep,
        preview: pv::pixel_path_live,
        todo: None,
    },
    Feature {
        id: "lens",
        title: "Lens & smart shelves",
        plain: "pick games by mood and length",
        line: "A Curator row on Home: combine filters, and keep one as a shelf that updates itself.",
        place: "Settings › Games › Curator",
        guide: "curator.md#lens",
        since: "0.1.0",
        switch: Some(Switch {
            get: |s| s.curator.lens,
            set: |s, on| curator(s).lens = on,
        }),
        // Simple is the defaults (2026-10-25), and Lens is one of them; pressing START on the profiles gives the same.
        simple: Pick::On,
        enthusiast: Pick::On,
        preview: pv::lens,
        todo: None,
    },
    Feature {
        id: "search",
        title: "Search and the letter jump",
        plain: "Search and the letter jump",
        line: "Search is the row above Continue. L2 and R2 jump through any list by first letter.",
        place: "Home: one press up. L2 / R2 in any list",
        guide: "playing.md#finding-a-game",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::search,
        todo: None,
    },
    Feature {
        id: "console_credits",
        title: "Console credits",
        plain: "Console credits",
        line: "Who made a console, its chips and its family, with links to your own games for it.",
        place: "X on a console page (X on a system on Home)",
        guide: "playing.md#credits--links",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::console_credits,
        todo: None,
    },
    Feature {
        id: "players",
        title: "Players",
        plain: "Players",
        line: "Up to four players on pocket-ui's own host: each controller you connect is the next player.",
        place: "Pause menu (HOME) › Players",
        guide: "pause-menu.md#players",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::players,
        todo: None,
    },
    Feature {
        id: "run_ahead",
        title: "Run-ahead",
        plain: "less input delay in older games",
        line: "The game responds to your buttons a frame or two sooner, on the lighter consoles.",
        place: "Pause menu › Options › Speed, rumble, fan",
        guide: "options.md#run-ahead",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::run_ahead,
        todo: None,
    },
    Feature {
        id: "runs_on",
        title: "Runs on",
        plain: "Runs on",
        line: "Which emulator plays a console, or a single game, and RetroArch's own picture settings.",
        place: "START on a game card or a console page",
        guide: "options.md#runs-on",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::runs_on,
        todo: None,
    },
    Feature {
        id: "achievements",
        title: "RetroAchievements",
        plain: "RetroAchievements",
        line: "Sign in once and earn achievements in the games on pocket-ui's own host.",
        place: "Settings › Playing › RetroAchievements",
        guide: "achievements.md#sign-in",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::achievements,
        todo: None,
    },
    Feature {
        id: "sync",
        title: "Sync",
        plain: "Sync",
        line: "Keeps saves and save states in step with your other devices, with Syncthing.",
        place: "Settings › Connections › Sync",
        guide: "sync.md#turn-it-on",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::sync,
        todo: None,
    },
    Feature {
        id: "tailscale",
        title: "Tailscale",
        plain: "Tailscale",
        line: "Reach the Pocket from your phone or computer, even away from home.",
        place: "Settings › Connections › Tailscale",
        guide: "remote.md#turn-it-on-and-sign-in",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::tailscale,
        todo: None,
    },
    Feature {
        id: "tools",
        title: "Tools",
        plain: "Tools",
        line: "The emulators' own menus, gamepad test and calibration, a file manager, a terminal.",
        place: "Settings › Help & tools › Tools",
        guide: "extras.md#tools",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::tools,
        todo: None,
    },
    Feature {
        id: "cozy_extras",
        title: "Cozy extras",
        plain: "Cozy extras",
        line: "The little touches, each on its own: real cases, the memory card, stickers, printed discs and more.",
        place: crate::cozy_extras::PAGE,
        guide: "settings.md#cozy-extras",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::cozy_extras,
        todo: None,
    },
    Feature {
        id: "perf_fan",
        title: "Performance & fan",
        plain: "Performance & fan",
        line: "How fast games run, and a fan that speeds up and slows down gently instead of switching on and off.",
        place: "Settings › Playing › Performance & fan",
        guide: "performance.md#performance-and-fan",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::perf_fan,
        todo: None,
    },
];

/// The entries that are wired (not `todo`) and that this device has (the same list as Settings' rows,
/// `device::hides`: no Stick lights without sticks, no Haptics, Sync, Tailscale, Tools or Performance & fan on a
/// Steam Deck), with their index in `FEATURES`.
pub fn available() -> impl Iterator<Item = (usize, &'static Feature)> {
    available_on(crate::device::current().has_sticks(), crate::device::current().host_owned())
}

pub fn available_on(sticks: bool, host_owned: bool) -> impl Iterator<Item = (usize, &'static Feature)> {
    FEATURES.iter().enumerate().filter(move |(_, f)| f.todo.is_none() && !crate::device::hides(f.id, sticks, host_owned))
}

pub fn by_id(id: &str) -> Option<usize> {
    available().find(|(_, f)| f.id == id).map(|(i, _)| i)
}

/// One of the Tour’s six basic cards, explained without a switch. The reading choice follows these.
pub struct Basic {
    /// Stable name, used by the snapshots (`--snapshot tour:<id>`).
    pub id: &'static str,
    pub title: &'static str,
    /// What to know, in one or two sentences.
    pub text: &'static str,
    /// Where to press or look.
    pub place: &'static str,
    /// Its section in the user guide, `page.md#anchor` (checked by `doc_checks`).
    pub guide: &'static str,
}

/// The Tour's six basic cards, in order, before the reading choice.
pub static BASICS: &[Basic] = &[
    Basic {
        id: "play",
        title: "Play, and HOME is the menu",
        text: "Pick a game and press A. While you play, tap HOME to pause and open the menu: Continue, Save, Load, Quit.",
        place: "Tap HOME in a game",
        guide: "pause-menu.md#save-slots-where-i-quit-and-before-load",
    },
    Basic {
        id: "continue",
        title: "Continue where you stopped",
        text: "Home opens on the game you played last. A continues it, and quitting keeps the moment, so the next start picks it up.",
        place: "Home › Continue",
        guide: "playing.md#home",
    },
    Basic {
        id: "saving",
        title: "Saving",
        text: "Save and Load are in the pause menu. Earlier brings back a moment from before, if a game went wrong.",
        place: "Pause menu › Save, Load, Earlier",
        guide: "pause-menu.md#earlier",
    },
    Basic {
        id: "quick",
        title: "Quick settings",
        text: "Brightness, volume, Wi-Fi, Bluetooth, fan and power in one panel, over the shelf and over a game.",
        place: "HOME on the shelf, START in the pause menu",
        guide: "quick-settings.md#quick-settings",
    },
    Basic {
        id: "add_games",
        title: "Add games",
        text: "Games go in each system's folder on the SD card. Add games shows the folders and BIOS files, and can share them over Wi-Fi.",
        place: "START › Settings › Games › Add games",
        guide: "getting-started.md#5-add-games",
    },
    Basic {
        id: "settings",
        title: "Settings",
        text: "Everything on these cards, and everything you meet along the way, can be switched under START › Settings.",
        place: "START on the shelf",
        guide: "settings.md#settings",
    },
];

/// The six basics followed by Easy to read. Tour indices after BASICS refer to FEATURES.
pub fn tour_list() -> Vec<usize> {
    (0..BASICS.len()).chain(std::iter::once(BASICS.len() + by_id("easy_to_read").expect("reading card"))).collect()
}

/// The Tour's heading: six basic cards followed by the reading choice.
pub const TOUR_TITLE: &str = "The basics in 7 cards";

pub fn basic_by_id(id: &str) -> Option<usize> {
    BASICS.iter().position(|b| b.id == id)
}

/// Sets every switch the profile has an answer for (Choose: none).
pub fn apply(profile: Profile, s: &mut Switches) {
    for (_, f) in available() {
        if let Some(w) = f.switch {
            match f.pick(profile) {
                Pick::On => (w.set)(s, true),
                Pick::Off => (w.set)(s, false),
                Pick::Keep => {}
            }
        }
    }
}

/// What a profile switches on, and off, in everyday words (`Feature::plain`; for the line under the profiles).
pub fn profile_summary(p: Profile) -> (Vec<&'static str>, Vec<&'static str>) {
    let titles = |want: Pick| {
        available()
            .filter(|(_, f)| f.switch.is_some() && f.pick(p) == want)
            .map(|(_, f)| f.plain)
            .collect()
    };
    (titles(Pick::On), titles(Pick::Off))
}

/// The cards of the first start's "Choose for yourself": every feature with a switch.
pub fn choose_list() -> Vec<usize> {
    available().filter(|(_, f)| f.switch.is_some()).map(|(i, _)| i).collect()
}

/// "New in this version": the features newer than the version last seen (`BASELINE` when none was noted) and not
/// newer than this one, the newest first, at most `NEW_MAX`. Empty when this version was seen already (also after
/// going back to an older one).
pub fn due(seen: Option<&str>, current: &str) -> Vec<usize> {
    let seen = seen.unwrap_or(BASELINE);
    if !newer(current, seen) {
        return Vec::new();
    }
    let mut list: Vec<usize> = available()
        .filter(|(_, f)| newer(f.since, seen) && !newer(f.since, current))
        .map(|(i, _)| i)
        .collect();
    // Newest first; the registry's order among the same version.
    list.sort_by(|a, b| {
        let (a, b) = (FEATURES[*a].since, FEATURES[*b].since);
        if newer(a, b) {
            std::cmp::Ordering::Less
        } else if newer(b, a) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    list.truncate(NEW_MAX);
    list
}

/// Whether this version is one the device has not noted yet (so `Settings::seen_version` is written once).
pub fn unseen(seen: Option<&str>, current: &str) -> bool {
    newer(current, seen.unwrap_or(BASELINE))
}

// ---- the cards ----------------------------------------------------------------------------------------------------

/// Which cards are open, and what B does there.
#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    /// The first start's "Choose for yourself": B goes back a card, from the first one back to the profiles.
    First,
    /// Settings → Tour: six basics and the reading choice; B goes back to Settings.
    Tour,
    /// After an update: B puts them all away.
    New { version: String },
}

/// The three choices of a switch card.
pub const CHOICES: [&str; 3] = ["On", "Off", "Later"];
const LATER: usize = 2;

/// What a press on the cards asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Out {
    Stay,
    /// A switch changed: the App takes `switches` (`take_switches`).
    Changed,
    /// The cards are done (the last card, START, B in the Tour or the new ones).
    Done,
    /// B on the first card of the first start: back to the profiles.
    Back,
}

/// The preview's size: the screen at half its size, so the scaling is one 2 x 2 average per pixel (`half`).
pub const PREVIEW: (i32, i32) = (620, 540);
const SCREEN: (u32, u32) = (1240, 1080);

pub struct Cards {
    pub mode: Mode,
    list: Vec<usize>,
    at: usize,
    choice: usize,
    pub switches: Switches,
    /// Ids put off with Later (settings.json `tour_later`).
    pub later: Vec<String>,
    changed: bool,
    /// The previews painted so far: (feature, light).
    previews: HashMap<(usize, bool), Image>,
}

impl Cards {
    pub fn new(mode: Mode, list: Vec<usize>, switches: Switches, later: Vec<String>) -> Cards {
        let mut c = Cards {
            mode,
            list,
            at: 0,
            choice: 0,
            switches,
            later,
            changed: false,
            previews: HashMap::new(),
        };
        c.show(0);
        c
    }

    /// Opens at card `i` of the list.
    pub fn show(&mut self, i: usize) {
        self.at = i.min(self.list.len().saturating_sub(1));
        self.choice = match self.feature().and_then(|f| f.is_on(&self.switches)) {
            Some(false) => 1,
            _ => 0,
        };
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The index of the card shown, in the list.
    pub fn at(&self) -> usize {
        self.at
    }

    fn feature_index(&self) -> Option<usize> {
        let i = *self.list.get(self.at)?;
        if self.mode == Mode::Tour { i.checked_sub(BASICS.len()) } else { Some(i) }
    }

    /// The feature card shown (the Tour ends with Easy to read after its basics).
    pub fn feature(&self) -> Option<&'static Feature> {
        FEATURES.get(self.feature_index()?)
    }

    /// The basic card shown (the Tour only).
    pub fn basic(&self) -> Option<&'static Basic> {
        if self.mode != Mode::Tour {
            return None;
        }
        self.list.get(self.at).and_then(|i| BASICS.get(*i))
    }

    pub fn choice(&self) -> usize {
        self.choice
    }

    /// The switches after a change, once.
    pub fn take_switches(&mut self) -> Option<Switches> {
        std::mem::take(&mut self.changed).then(|| self.switches.clone())
    }

    /// What the screen shows, for the App's redraw check.
    pub fn mark(&self) -> (usize, usize, usize) {
        (self.at, self.choice, self.later.len())
    }

    fn next(&mut self) -> Out {
        if self.at + 1 >= self.list.len() {
            return Out::Done;
        }
        self.show(self.at + 1);
        Out::Stay
    }

    /// Takes the choice on the card shown: switches it, or puts it off.
    fn take(&mut self) -> bool {
        let Some(f) = self.feature() else { return false };
        let Some(w) = f.switch else { return false };
        if self.choice == LATER {
            if !self.later.iter().any(|l| l == f.id) {
                self.later.push(f.id.to_string());
            }
            return false;
        }
        self.later.retain(|l| l != f.id);
        let on = self.choice == 0;
        if (w.get)(&self.switches) == on {
            return false;
        }
        (w.set)(&mut self.switches, on);
        self.changed = true;
        true
    }

    pub fn press(&mut self, b: Button) -> Out {
        let switch = self.feature().is_some_and(|f| f.switch.is_some());
        match b {
            Button::Up if switch => self.choice = (self.choice + CHOICES.len() - 1) % CHOICES.len(),
            Button::Down if switch => self.choice = (self.choice + 1) % CHOICES.len(),
            Button::East => {
                let changed = self.take();
                let out = self.next();
                return if changed && out == Out::Stay { Out::Changed } else { out };
            }
            // Through the cards without choosing.
            Button::R1 | Button::Right => {
                if self.at + 1 < self.list.len() {
                    self.show(self.at + 1);
                }
            }
            Button::L1 | Button::Left => self.show(self.at.saturating_sub(1)),
            Button::South => match self.mode {
                Mode::First if self.at == 0 => return Out::Back,
                Mode::First => self.show(self.at - 1),
                _ => return Out::Done,
            },
            Button::Start => return Out::Done,
            _ => {}
        }
        Out::Stay
    }

    /// The preview of the card shown, painted the first time it is needed (in the appearance in use) and kept.
    fn preview(&mut self, text: &mut Text) -> Option<&Image> {
        let i = self.feature_index()?;
        let light = pal().is_light();
        // A picture of the screen at the design's sizes, scaled down onto the card: Text size doesn't change it.
        Some(self.previews.entry((i, light)).or_insert_with(|| crate::type_scale::unscaled(|| render_preview(&FEATURES[i], text, light))))
    }

    /// A basic card: the title, what to know, and where to press, on the page's own paper. No picture and no switch.
    fn paint_basic(&self, c: &mut Canvas, text: &mut Text, b: &Basic) -> Hits {
        let mut hits = Hits::default();
        text.draw(c, Face::Body, 28, MARGIN, 72, pal().dust, TOUR_TITLE);
        text.draw_right(c, Face::Body, 28, c.w - MARGIN, 72, pal().dust, &format!("{} of {}", self.at + 1, self.list.len()));
        let title = text.fit(Face::Title, 67, b.title, c.w - 2 * MARGIN);
        let lines: Vec<String> = text.wrap_prose(Face::Body, 32, b.text, c.w - 2 * MARGIN).into_iter().take(4).collect();
        let line_h = crate::type_scale::line(32, 14);
        let strip_h = crate::type_scale::row(72, 28);
        // The block (title, words, the strip) stands a little above the middle of the page, as an empty page's does.
        let block = 80 + 36 + lines.len() as i32 * line_h + 36 + strip_h;
        let mut y = ((c.h - block) * 2 / 5).max(150) + 80;
        text.draw(c, Face::Title, 67, MARGIN, y, pal().chalk, &title);
        y += 36 + line_h;
        for line in &lines {
            text.draw(c, Face::Body, 32, MARGIN, y, pal().chalk, line);
            y += line_h;
        }
        // Where, on a paper strip: the one thing to do with a finger or a thumb.
        y += 8;
        c.round_rect(MARGIN, y, c.w - 2 * MARGIN, strip_h, 16, pal().keycap);
        let place = text.fit(Face::Body, 28, b.place, c.w - 2 * MARGIN - 56);
        text.draw(c, Face::Body, 28, MARGIN + 28, y + strip_h / 2 + crate::type_scale::base(10, 28), pal().chalk, &place);
        let last = self.at + 1 >= self.list.len();
        hits.buttons = crate::setup::hints(c, text, &[("A", if last { "Done" } else { "Next" }), ("B", "Close")]);
        hits
    }

    /// The card page. Returns the choice rows and the hint pills for touch.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text) -> Hits {
        c.clear(pal().void);
        let mut hits = Hits::default();
        if let Some(b) = self.basic() {
            return self.paint_basic(c, text, b);
        }
        let Some(f) = self.feature() else { return hits };
        let heading = match &self.mode {
            Mode::First => "Choose for yourself".to_string(),
            Mode::Tour => TOUR_TITLE.to_string(),
            Mode::New { version } => format!("New in pocket-ui {version}"),
        };
        text.draw(c, Face::Body, 28, MARGIN, 72, pal().dust, &heading);
        if self.list.len() > 1 {
            text.draw_right(
                c,
                Face::Body,
                28,
                c.w - MARGIN,
                72,
                pal().dust,
                &format!("{} of {}", self.at + 1, self.list.len()),
            );
        }
        let title = text.fit(Face::Title, 67, f.title, c.w - 2 * MARGIN);
        text.draw(c, Face::Title, 67, MARGIN, 160, pal().chalk, &title);
        for (k, line) in text.wrap_prose(Face::Body, 28, f.line, c.w - 2 * MARGIN).iter().take(2).enumerate() {
            text.draw(c, Face::Body, 28, MARGIN, 208 + k as i32 * 38, pal().dust, line);
        }

        // On a short screen (768 lines) the preview is smaller and the choices lower-key, so that the card ends above the
        // hint bar; the column beside the preview gets the width the preview gave up.
        let short = crate::kit::short_screen(c.h);
        let (pw, ph) = if short { (360 * PREVIEW.0 / PREVIEW.1, 360) } else { PREVIEW };
        let (px, py) = (MARGIN + 8, if short { 280 } else { 300 });
        c.round_rect(px - 8, py - 8, pw + 16, ph + 16, 16, pal().keycap);
        if let Some(img) = self.preview(text) {
            c.blit_scaled(img, px, py, pw, ph);
        }

        let x = px + pw + 8 + 44;
        let w = c.w - MARGIN - x;
        let f = self.feature().expect("shown above");
        let put_off = self.later.iter().any(|l| l == f.id);
        let lead = match (f.switch.is_some(), self.choice == LATER) {
            (false, _) => "Nothing to switch. Find it here:",
            (true, true) => "Left as it is for now. Changed any time in",
            (true, false) => "Changed any time in",
        };
        // The way to the setting can be long (German, two settings, Large text) and must stay readable whole, above the hint
        // bar: when it doesn't fit, its type goes 28, 24, 21 px, and last the rows are lower, before anything is cut.
        let (lead_lines, rows) = (text.wrap_prose(Face::Body, 24, lead, w), if f.switch.is_some() { CHOICES.len() as i32 } else { 0 });
        let tiers = if short { [(72, 10, 20, 28), (72, 10, 20, 24), (72, 10, 20, 21), (60, 8, 22, 21)] } else { [(88, 14, 30, 28), (88, 14, 30, 24), (88, 14, 30, 21), (60, 8, 22, 21)] };
        let top = py - 8;
        let limit = crate::kit::hint_bar_top(c.h) - 8;
        let mut chosen = None;
        for (row, gap, extra, font) in tiers {
            let lines = text.wrap_prose(Face::Body, font, f.place, w);
            let high = rows * (row + gap) + if rows > 0 { extra } else { 30 } + lead_lines.len() as i32 * crate::type_scale::line(24, 10) + 16
                + lines.len() as i32 * crate::type_scale::line(font, 12);
            chosen = Some((row, gap, extra, font, lines));
            if top + high <= limit {
                break;
            }
        }
        let (row, gap, extra, font, place_lines) = chosen.expect("tiers is not empty");
        let mut y = top;
        if let Some(sw) = f.switch {
            let now = if (sw.get)(&self.switches) { 0 } else { 1 };
            for (k, label) in CHOICES.iter().enumerate() {
                let selected = k == self.choice;
                c.rect(x, y, w, row, if selected { pal().label } else { pal().spine });
                if selected {
                    crate::game_card::lift_shadow(c, x, y + row, w, pal().t);
                }
                let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
                text.draw(c, Face::Title, 37, x + 28, y + row / 2 + crate::type_scale::base(13, 37), fg, label);
                // What it is now, at the right end, as the status picker marks its rows.
                let note = match k {
                    _ if k == now => "Now",
                    LATER if put_off => "Put off",
                    _ => "",
                };
                if !note.is_empty() {
                    text.draw_right(c, Face::Body, 24, x + w - 24, y + row / 2 + 9, dim, note);
                }
                hits.choices.push((Area::new(x, y, w, row), k));
                y += row + gap;
            }
            y += extra;
        } else {
            y += 30;
        }
        for line in &lead_lines {
            text.draw(c, Face::Body, 24, x, y, pal().dust, line);
            y += crate::type_scale::line(24, 10);
        }
        y += 16;
        for line in &place_lines {
            text.draw(c, Face::Body, font, x, y, pal().chalk, line);
            y += crate::type_scale::line(font, 12);
        }

        let last = self.at + 1 >= self.list.len();
        let a = if last { "Done" } else { "Next" };
        let mut h: Vec<(&str, &str)> = Vec::new();
        if f.switch.is_some() {
            h.push(("Up/Down", "Choose"));
        }
        h.push(("A", a));
        h.push(("B", if self.mode == Mode::First { "Back" } else { "Close" }));
        // In the Tour and the new cards B closes already; START does the same there, without a hint of its own.
        if self.mode == Mode::First {
            h.push(("START", "Skip the rest"));
        }
        hits.buttons = crate::setup::hints(c, text, &h);
        hits.sel = self.choice;
        hits
    }
}

/// What a page of cards or the profiles offers to touch: rows (with their index) and hint pills.
#[derive(Default, Debug)]
pub struct Hits {
    pub choices: Vec<(Area, usize)>,
    pub sel: usize,
    pub buttons: Vec<(Area, Button)>,
}

/// A feature's preview: its painter at the screen's size, in the appearance asked for, scaled to `PREVIEW`.
pub fn render_preview(f: &Feature, text: &mut Text, light: bool) -> Image {
    let (w, h) = SCREEN;
    let mut px = vec![0u32; (w * h) as usize];
    crate::palette::scoped(if light { 255 } else { 0 }, || {
        let mut c = Canvas::new(&mut px, w, h);
        (f.preview)(&mut c, text);
    });
    half(&px, w as usize, h as usize)
}

/// `px` (`w` x `h`) at half the size, each pixel the average of four: red and blue are added in one word, green in
/// another, so a preview scales in about a millisecond instead of the three of `Image::resized`.
fn half(px: &[u32], w: usize, h: usize) -> Image {
    let (hw, hh) = (w / 2, h / 2);
    let mut out = Vec::with_capacity(hw * hh);
    for y in 0..hh {
        let (r0, r1) = (&px[2 * y * w..2 * y * w + w], &px[(2 * y + 1) * w..(2 * y + 1) * w + w]);
        for x in 0..hw {
            let q = [r0[2 * x], r0[2 * x + 1], r1[2 * x], r1[2 * x + 1]];
            let rb = (q.iter().map(|p| p & 0xff00ff).sum::<u32>() >> 2) & 0xff00ff;
            let g = (q.iter().map(|p| p & 0x00ff00).sum::<u32>() >> 2) & 0x00ff00;
            out.push(rb | g);
        }
    }
    Image {
        w: hw as u32,
        h: hh as u32,
        px: out,
    }
}

// ---- the profiles (the first start) -------------------------------------------------------------------------------

/// Width of the preview beside the profiles on a large screen.
const PROFILE_PREVIEW_W: i32 = 320;

/// "How do you like to play?": the three profiles as rows, the selected one paper, and under them what it turns on
/// and off (from the registry, so it can't say anything the switches don't do).
/// `later`: opened from Settings → Help & tools, where choosing a way sets its switches over what is there now.
pub fn paint_profiles(c: &mut Canvas, text: &mut Text, sel: usize, profiles: &[Profile], later: bool, previews: &[Image]) -> Hits {
    c.clear(pal().void);
    let mut hits = Hits { sel, ..Default::default() };
    // A short screen (768 lines) takes the heading and the five rows closer, so the last row clears the hints (it ran 14 px
    // under them at pitch 108).
    let short = crate::kit::short_screen(c.h);
    let (head, sub, first, pitch, row_h) = if short { (100, 148, 172, 100, 92) } else { (140, 192, 230, 124, 112) };
    text.draw(c, Face::Title, 67, MARGIN, head, pal().chalk, "How do you like to play?");
    text.draw(
        c,
        Face::Body,
        28,
        MARGIN,
        sub,
        pal().dust,
        "A starting point for the extras. Every switch stays in Settings.",
    );
    let full = c.w - 2 * MARGIN;
    // A small live preview beside the rows (the look step's pictures: Easy's case for its row, the shelf for
    // The shelf and Enthusiast): only where there's room. On a small screen (drawn on 960 lines at 640x480) the rows fill it, so the preview is left out.
    let (pw, ph) = (PROFILE_PREVIEW_W, PROFILE_PREVIEW_W * c.h / c.w.max(1));
    let preview = c.w >= 1000 && c.h >= 1000 && previews.len() >= 2;
    let width = if preview { full - pw - 24 } else { full };
    if preview {
        let (px, py) = (c.w - MARGIN - pw, 230);
        c.round_rect(px - 8, py - 8, pw + 16, ph + 16, 12, pal().keycap);
        match profiles[sel.min(profiles.len() - 1)] {
            // Look order: Shelf, Easy, ... (`Theme::ALL`).
            Profile::JustPlay => c.blit_scaled(&previews[1], px, py, pw, ph),
            Profile::Simple | Profile::Enthusiast => c.blit_scaled(&previews[0], px, py, pw, ph),
            // Nothing to show for these two: the choice's own name, not an empty frame.
            other => {
                c.rect(px, py, pw, ph, pal().spine);
                let name = text.fit(Face::Title, 37, other.name(), pw - 24);
                text.draw(c, Face::Title, 37, px + 12, py + ph / 2, pal().dust, &name);
            }
        }
    }
    for (i, p) in profiles.iter().enumerate() {
        let y = first + i as i32 * pitch;
        let selected = i == sel;
        c.rect(MARGIN, y, width, row_h, if selected { pal().label } else { pal().spine });
        if selected {
            crate::game_card::lift_shadow(c, MARGIN, y + row_h, width, pal().t);
        }
        let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
        text.draw(c, Face::Title, 46, MARGIN + 28, y + row_h * 50 / 112, fg, p.name());
        let blurb = text.fit(Face::Body, 24, p.blurb(), width - 56);
        text.draw(c, Face::Body, 24, MARGIN + 28, y + row_h * 90 / 112, dim, &blurb);
        hits.choices.push((Area::new(MARGIN, y, width, row_h), i));
    }
    let mut y = first + profiles.len() as i32 * pitch + if short { 22 } else { 30 };
    let p = profiles[sel.min(profiles.len() - 1)];
    let lines: Vec<(String, u32)> = match p {
        Profile::Child => vec![
            ("Next: a PIN, the games, Blue or Pink, and a time and volume limit if you like.".to_string(), pal().chalk),
            ("Your settings and saves stay as they are for when the mode goes off.".to_string(), pal().dust),
        ],
        Profile::Choose => vec![
            (
                format!("{} cards, each with a small picture of what it does.", choose_list().len()),
                pal().chalk,
            ),
            ("START skips the rest; what you haven't chosen stays as it is.".to_string(), pal().dust),
        ],
        _ => {
            let (on, off) = profile_summary(p);
            let mut v = vec![(format!("On: {}.", on.join(", ")), pal().chalk)];
            // Not the same as skipping once the first start is over: it sets the switches back (On start and Screen frames
            // among them, which the Off line only counts as "and more").
            let back = later && p == Profile::Simple;
            if !off.is_empty() && !back {
                // Two of them, then "and more": a long list ran off the page on 480 lines.
                let mut shown: Vec<&str> = off.iter().copied().take(2).collect();
                if off.len() > 2 {
                    shown.push("and more");
                }
                v.push((format!("Off: {}.", shown.join(", ")), pal().dust));
            }
            if back {
                v.push(("Picked now, it puts every extra back as pocket-ui ships it, also On start and Screen frames.".to_string(), pal().chalk));
            }
            v
        }
    };
    // As many lines as stay clear of the hints, the last cut with "…" (larger text on the Mini V1's 960 lines,
    // docs/DEVICES.md; on 1080 lines everything fits).
    let (step, last) = (crate::type_scale::line(24, 10), c.h - 100 - crate::type_scale::grow(24));
    for (line, colour) in lines {
        // Five rows on 960 lines leave no room: the rows say enough.
        if y > last {
            break;
        }
        let room = ((last - y) / step + 1).max(0) as usize;
        for l in text.wrap_prose_at_most(Face::Body, 24, &line, width, room) {
            text.draw(c, Face::Body, 24, MARGIN, y, colour, &l);
            y += step;
        }
        y += 10;
    }
    hits.buttons = crate::setup::hints(c, text, &[("Up/Down", "Choose"), ("A", "Take this"), ("B", "Back"), ("START", "Skip")]);
    hits
}

/// A spine row of the previews: the spine or, selected, the paper label with its lift and the stripe at the left.
/// Returns (text, secondary text) colours. As Curator's pages draw their rows (`app_curator::row_back`).
pub fn row_back(c: &mut Canvas, (x, y, w, h): (i32, i32, i32, i32), stripe: u32, selected: bool) -> (u32, u32) {
    c.rect(x, y, w, h, if selected { pal().label } else { pal().spine });
    if selected {
        crate::game_card::lift_shadow(c, x, y + h, w, pal().t);
    }
    c.rect(x, y, STRIPE, h, if selected { stripe } else { mix(stripe, pal().spine, 150) });
    if selected {
        (pal().ink, pal().label_dust)
    } else {
        (pal().chalk, pal().dust)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plain_words_on_a_card_are_ordinary_words() {
        // A new player reads these first (finding 15 of the new-user test): no tool words, and the Lens one says what it does.
        let lens = FEATURES.iter().find(|f| f.id == "lens").unwrap();
        assert_eq!(lens.plain, "pick games by mood and length");
        for f in FEATURES {
            let p = f.plain.to_lowercase();
            assert!(!p.contains("filter") && !p.contains("signal"), "{}: {}", f.id, f.plain);
        }
    }

    /// What the pause menu reads to list Zoom (`app_zoom::zoom_row_on`) is what the Tour card shows as On: turning the card
    /// on puts Zoom on the main page, turning it off takes it away again.
    #[test]
    fn the_reading_card_decides_whether_the_pause_menu_lists_zoom() {
        let card = FEATURES.iter().find(|f| f.id == "easy_to_read").unwrap().switch.unwrap();
        let mut s = Switches::default();
        assert!(!(card.get)(&s) && !reading_on(s.text_size, s.motion), "a new device: no Zoom on the main page");
        (card.set)(&mut s, true);
        assert!(reading_on(s.text_size, s.motion) && (card.get)(&s));
        // Large text alone, or calmer motion alone, is not Easy to read.
        assert!(!reading_on(TextSize::Large, Motion::Full) && !reading_on(TextSize::Normal, Motion::Reduced));
        (card.set)(&mut s, false);
        assert!(!reading_on(s.text_size, s.motion));
    }

    #[test]
    fn ids_are_unique_and_every_entry_is_complete() {
        let mut ids: Vec<&str> = FEATURES.iter().map(|f| f.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), FEATURES.len(), "ids are unique");
        for f in FEATURES {
            assert!(!f.title.is_empty() && !f.line.is_empty() && !f.place.is_empty(), "{}", f.id);
            assert!(f.guide.contains(".md#"), "{}: a page and an anchor", f.id);
            assert!(f.since.split('.').all(|p| p.parse::<u32>().is_ok()), "{}: a version", f.id);
            if f.switch.is_none() {
                assert_eq!(
                    (f.simple, f.enthusiast),
                    (Pick::Keep, Pick::Keep),
                    "{}: a profile can't set what has no switch",
                    f.id
                );
            }
            assert!(!f.line.ends_with(' ') && f.line.ends_with('.'), "{}: one sentence", f.id);
        }
        // Up next and Pixel Path over every game came from other branches and are wired now.
        for id in ["up_next", "pixel_path_live"] {
            assert!(by_id(id).is_some_and(|i| FEATURES[i].switch.is_some()), "{id}");
        }
    }

    #[test]
    fn a_profile_sets_only_switches_and_each_one_reads_back() {
        let start = Switches::default();
        for (_, f) in available() {
            let Some(w) = f.switch else { continue };
            for on in [true, false] {
                let mut s = start.clone();
                (w.set)(&mut s, on);
                assert_eq!((w.get)(&s), on, "{}", f.id);
            }
        }
        let mut simple = start.clone();
        apply(Profile::Simple, &mut simple);
        let mut enthusiast = start.clone();
        apply(Profile::Enthusiast, &mut enthusiast);
        let mut chosen = start.clone();
        apply(Profile::Choose, &mut chosen);
        assert_eq!(chosen, start, "Choose yourself starts from what is there");
        let c = &enthusiast.curator;
        assert!(
            c.lens && c.echoes && c.merge_versions && c.stickers && c.not_on_device && c.pixel_path,
            "Curator in full"
        );
        assert_eq!(
            (enthusiast.frames, enthusiast.on_start, enthusiast.motion),
            (Frames::Every, OnStart::Continue, Motion::Full)
        );
        let c = &simple.curator;
        assert!(c.lens && c.stickers && c.echoes && c.merge_versions && !c.not_on_device && !c.pixel_path && !c.up_next);
        assert!(enthusiast.curator.up_next, "Enthusiast keeps Up next");
        assert_eq!((simple.over_game, enthusiast.over_game), (crate::over_game::Level::Off, crate::over_game::Level::Off), "no line over the game from a profile");
        assert_eq!(
            (simple.frames, simple.on_start, simple.motion),
            (Frames::Off, OnStart::Home, Motion::Full)
        );
        // Simple is the defaults: it changes nothing that START on the profiles (skip) leaves, and the other way round.
        assert_eq!(simple, start, "Simple = Skip = the defaults");
        // What no profile touches stays as it was.
        for s in [&simple, &enthusiast] {
            assert_eq!((s.sound, s.haptics, s.leds), (start.sound, start.haptics, start.leds));
        }
        // Profiles are idempotent and say exactly what they do.
        let mut again = simple.clone();
        apply(Profile::Simple, &mut again);
        assert_eq!(again, simple);
        let (on, off) = profile_summary(Profile::Enthusiast);
        assert!(!on.contains(&"a line of picture details over every game") && off.is_empty(), "{on:?} {off:?}");
    }

    #[test]
    fn the_first_starts_summary_speaks_plainly() {
        // The names a new user can't know yet: the summary says what a switch does, not what it is called.
        const JARGON: [&str; 10] =
            ["echoes", "lens", "pixel", "curator", "up next", "stickers", "haptics", "run-ahead", "shelves", "tour"];
        for (_, f) in available().filter(|(_, f)| f.switch.is_some()) {
            let p = f.plain;
            assert!(!p.is_empty() && p.len() <= 60, "{}: short", f.id);
            assert!(p.starts_with(|c: char| c.is_lowercase()) && !p.ends_with('.'), "{}: {p:?} is a lower-case phrase", f.id);
            assert_ne!(p, f.title, "{}: not just the title", f.id);
            for j in JARGON {
                assert!(!p.to_lowercase().contains(j), "{}: {p:?} uses the name {j:?}", f.id);
            }
        }
        for p in [Profile::Simple, Profile::Enthusiast] {
            let (on, off) = profile_summary(p);
            for t in on.iter().chain(&off) {
                assert!(FEATURES.iter().all(|f| f.title != *t), "{t:?} is a title");
            }
        }
    }

    #[test]
    fn pixel_path_over_every_game_is_over_the_games_pixel_path_level() {
        use crate::over_game::Level;
        let w = FEATURES[by_id("pixel_path_live").unwrap()].switch.unwrap();
        let mut s = Switches::default();
        (w.set)(&mut s, true);
        assert_eq!(s.over_game, Level::PixelPath);
        assert!(!s.curator.pixel_path, "the pause menu's Pixel Path is a switch of its own");
        (w.set)(&mut s, false);
        assert_eq!(s.over_game, Level::Off);
        s.over_game = Level::Sensors;
        assert!(!(w.get)(&s));
        (w.set)(&mut s, false);
        assert_eq!(s.over_game, Level::Sensors, "Off leaves a chosen FPS line alone");
    }

    #[test]
    fn new_cards_are_the_entries_newer_than_the_version_seen_at_most_three_newest_first() {
        // Easy to read is the only entry newer than the baseline: an update to 0.2.0 or later shows it, once, and nothing before.
        let easy = by_id("easy_to_read").unwrap();
        assert_eq!(due(None, "0.3.0"), vec![easy]);
        assert_eq!(due(Some("0.1.14"), "0.2.0"), vec![easy]);
        assert!(due(None, "0.1.15").is_empty(), "not in a version before its own");
        assert!(due(Some("0.2.0"), "0.3.0").is_empty());
        assert!(unseen(None, "0.3.0") && unseen(Some("0.2.0"), "0.3.0"));
        assert!(
            !unseen(Some("0.3.0"), "0.3.0") && !unseen(Some("0.4.0"), "0.3.0"),
            "seen, or an older version again"
        );
        assert!(!unseen(None, BASELINE), "a dev build of the baseline");
    }

    #[test]
    fn the_rule_of_due_on_made_up_versions() {
        // `due` reads FEATURES; the rule itself, on versions: newer than seen, not newer than current.
        let pick = |since: &str, seen: &str, current: &str| newer(since, seen) && !newer(since, current);
        assert!(pick("0.3.0", "0.2.0", "0.3.0"));
        assert!(!pick("0.3.0", "0.3.0", "0.4.0"), "seen already");
        assert!(!pick("0.4.0", "0.2.0", "0.3.0"), "not in this version yet");
        assert!(pick("0.2.1", "0.2.0", "0.10.0"));
    }

    fn cards(mode: Mode, ids: &[&str]) -> Cards {
        let list = ids.iter().map(|id| by_id(id).unwrap()).collect();
        Cards::new(mode, list, Switches::default(), Vec::new())
    }

    #[test]
    fn a_card_starts_on_what_is_set_and_a_changes_only_what_differs() {
        let mut c = cards(Mode::First, &["pixel_path", "echoes", "search"]);
        assert_eq!(c.choice(), 1, "Pixel Path is off: Off is selected");
        assert_eq!(c.press(Button::East), Out::Stay, "Off again changes nothing");
        assert!(c.take_switches().is_none());
        assert_eq!(c.choice(), 0, "Echoes is on");
        c.press(Button::Down);
        assert_eq!(c.press(Button::East), Out::Changed);
        let s = c.take_switches().unwrap();
        assert!(!s.curator.echoes && !s.curator.pixel_path);
        assert!(c.take_switches().is_none(), "once");
        // A card without a switch: Up/Down do nothing, A ends the last card.
        assert_eq!(c.feature().unwrap().id, "search");
        c.press(Button::Down);
        assert_eq!(c.choice(), 0);
        assert_eq!(c.press(Button::East), Out::Done);
    }

    #[test]
    fn later_is_remembered_until_the_card_is_decided() {
        let mut c = cards(Mode::First, &["stickers", "lens"]);
        c.press(Button::Down);
        c.press(Button::Down);
        assert_eq!(c.choice(), LATER);
        assert_eq!(c.press(Button::East), Out::Stay);
        assert_eq!(c.later, ["stickers"]);
        assert!(c.take_switches().is_none(), "Later changes no switch");
        c.press(Button::L1);
        assert_eq!(c.feature().unwrap().id, "stickers");
        c.press(Button::Down);
        c.press(Button::East);
        assert!(c.later.is_empty(), "decided now");
        assert!(!c.take_switches().unwrap().curator.stickers);
    }

    #[test]
    fn b_and_start_per_mode() {
        let mut first = cards(Mode::First, &["lens", "echoes"]);
        assert_eq!(first.press(Button::South), Out::Back, "first card: back to the profiles");
        first.press(Button::R1);
        assert_eq!(first.press(Button::South), Out::Stay);
        assert_eq!(first.at(), 0);
        assert_eq!(first.press(Button::Start), Out::Done, "START skips the rest");
        let mut tour = Cards::new(Mode::Tour, tour_list(), Switches::default(), Vec::new());
        tour.press(Button::R1);
        assert_eq!(tour.press(Button::South), Out::Done);
        let mut new = cards(Mode::New { version: "0.3.0".into() }, &["lens"]);
        assert_eq!(new.press(Button::South), Out::Done, "B puts them away");
        // Left/Right and L1/R1 stop at the ends.
        let mut c = cards(Mode::New { version: "0.3.0".into() }, &["lens", "echoes"]);
        c.press(Button::Left);
        assert_eq!(c.at(), 0);
        c.press(Button::Right);
        c.press(Button::Right);
        assert_eq!(c.at(), 1);
    }

    #[test]
    fn choose_yourself_is_the_switch_cards_and_the_tour_has_the_reading_choice() {
        assert!(choose_list().iter().all(|i| FEATURES[*i].switch.is_some()));
        assert_eq!(tour_list().len(), BASICS.len() + 1);
        assert_eq!(TOUR_TITLE, format!("The basics in {} cards", tour_list().len()));
        let mut ids: Vec<&str> = BASICS.iter().map(|b| b.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), BASICS.len(), "ids are unique");
        for b in BASICS {
            assert!(!b.title.is_empty() && b.text.ends_with('.') && !b.place.is_empty(), "{}", b.id);
            assert!(b.guide.contains(".md#"), "{}: a page and an anchor", b.id);
        }
    }

    #[test]
    fn the_six_basics_lead_to_a_reading_choice_that_applies_both_settings() {
        let mut text = Text::new();
        let (w, h) = SCREEN;
        let mut px = vec![0u32; (w * h) as usize];
        let mut c = Cards::new(Mode::Tour, tour_list(), Switches::default(), Vec::new());
        for _ in 0..BASICS.len() - 1 {
            let hits = c.paint(&mut Canvas::new(&mut px, w, h), &mut text);
            assert!(hits.choices.is_empty() && hits.buttons.iter().any(|(_, b)| *b == Button::East));
            c.press(Button::Down);
            assert_eq!(c.choice(), 0, "Up/Down do nothing here");
            assert_eq!(c.press(Button::East), Out::Stay);
        }
        assert_eq!(c.basic().unwrap().id, "settings");
        assert_eq!(c.press(Button::East), Out::Stay);
        assert!(c.take_switches().is_none());
        assert_eq!(c.feature().unwrap().id, "easy_to_read");
        let hits = c.paint(&mut Canvas::new(&mut px, w, h), &mut text);
        assert_eq!(hits.choices.len(), 3, "the reading card can be switched from Tour");
        c.press(Button::Up); // Off is selected initially; Up chooses On.
        assert_eq!(c.press(Button::East), Out::Done);
        let enabled = c.take_switches().unwrap();
        assert_eq!((enabled.text_size, enabled.motion), (TextSize::Large, Motion::Reduced));
        assert_eq!(enabled.curator, Switches::default().curator);
        let mut again = Cards::new(Mode::Tour, tour_list(), enabled, Vec::new());
        again.show(again.len() - 1);
        again.press(Button::Down); // On -> Off.
        again.press(Button::East);
        let disabled = again.take_switches().unwrap();
        assert_eq!((disabled.text_size, disabled.motion), (TextSize::Normal, Motion::Full));
    }

    #[test]
    fn half_averages_each_two_by_two_block() {
        let px = [0xff0000, 0x000000, 0x0000ff, 0xffffff, 0x00ff00, 0x00ff00, 0x101010, 0x303030];
        let img = half(&px, 4, 2);
        assert_eq!((img.w, img.h), (2, 1));
        assert_eq!(img.px, vec![0x3f7f00, 0x4f4f8f]);
    }

    #[test]
    fn every_preview_paints_something_in_both_appearances_quickly() {
        let mut text = Text::new();
        for (_, f) in available() {
            for light in [false, true] {
                let t0 = std::time::Instant::now();
                let img = render_preview(f, &mut text, light);
                let took = t0.elapsed();
                assert_eq!((img.w as i32, img.h as i32), PREVIEW);
                let page = if light { LIGHT.void } else { DARK.void };
                let other = img.px.iter().filter(|p| **p != page).count();
                assert!(
                    other > img.px.len() / 50,
                    "{} ({}): the preview is not empty",
                    f.id,
                    if light { "light" } else { "dark" }
                );
                // Generous for a debug build; the release numbers are in docs/DEVELOPING.md.
                assert!(took.as_millis() < 2000, "{}: {took:?}", f.id);
            }
        }
    }

    #[test]
    fn the_card_page_and_the_profiles_paint_with_their_hints() {
        let mut text = Text::new();
        let (w, h) = SCREEN;
        let mut px = vec![0u32; (w * h) as usize];
        let mut c = cards(Mode::First, &["frames", "tools"]);
        let hits = c.paint(&mut Canvas::new(&mut px, w, h), &mut text);
        assert_eq!(hits.choices.len(), 3, "On, Off, Later");
        assert!(hits.buttons.iter().any(|(_, b)| *b == Button::Start));
        c.press(Button::R1);
        let hits = c.paint(&mut Canvas::new(&mut px, w, h), &mut text);
        assert!(hits.choices.is_empty(), "nothing to switch");
        let hits = paint_profiles(&mut Canvas::new(&mut px, w, h), &mut text, 1, &Profile::offered(), false, &[]);
        assert_eq!((hits.choices.len(), hits.sel), (5, 1));
    }
}

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
use crate::settings::{Frames, Motion};
use crate::text::{Face, Text};
use crate::touch::Area;
use crate::ui_sound;
use crate::update::newer;

/// The switches the cards and profiles set: copies of what Settings keeps, each in its own file (settings.json,
/// curator.json, sounds.json, haptics.json, leds.json). The App fills it and takes it back (`App::adopt_switches`).
#[derive(Clone, Debug, PartialEq)]
pub struct Switches {
    pub motion: Motion,
    pub on_start: OnStart,
    /// Settings › Remember where I quit (settings.json's `forget_where_quit`, the other way round).
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
    Simple,
    Enthusiast,
    /// The cards, one by one.
    Choose,
}

impl Profile {
    pub const ALL: [Profile; 3] = [Profile::Simple, Profile::Enthusiast, Profile::Choose];

    pub fn name(self) -> &'static str {
        match self {
            Profile::Simple => "Simple",
            Profile::Enthusiast => "Enthusiast",
            Profile::Choose => "Choose for yourself",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Profile::Simple => "As pocket-ui ships, the same as skipping this step.",
            Profile::Enthusiast => "Suggestions, play statistics, picture details and frames, all on.",
            Profile::Choose => "One card per feature, with a preview: on, off, or later.",
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
            Profile::Simple => self.simple,
            Profile::Enthusiast => self.enthusiast,
            Profile::Choose => Pick::Keep,
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
/// yourself".
pub static FEATURES: &[Feature] = &[
    Feature {
        id: "lens",
        title: "Lens & smart shelves",
        plain: "filter games by mood and length",
        line: "A Curator row on Home: combine filters, and keep one as a shelf that updates itself.",
        place: "Settings › Playing › Curator",
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
        id: "echoes",
        title: "Echoes",
        plain: "suggestions for similar games",
        line: "More like this, on every game card: games you own that play alike, each with a reason.",
        place: "Settings › Playing › Curator",
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
        id: "up_next",
        title: "Up next after a game",
        plain: "a next game to try after you quit",
        line: "When you quit a game, one line on the shelf suggests a game of yours like it. A plays it; nothing starts by itself.",
        place: "Settings › Playing › Curator",
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
        id: "echoes_elsewhere",
        title: "Echoes beyond this device",
        plain: "suggestions from games you don't own yet",
        line: "Echoes also names a few games you don't have, from the offline game list. Names only.",
        place: "Settings › Playing › Curator",
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
        id: "versions",
        title: "Versions",
        plain: "other versions of a game",
        line: "A game you own on several systems shows once in mixed lists and starts the version you prefer.",
        place: "Settings › Playing › Curator",
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
        title: "Stickers",
        plain: "your own labels for games",
        line: "Your own labels for games, such as Couch co-op. Pin one to see it on Home.",
        place: "Settings › Playing › Curator",
        guide: "curator.md#stickers",
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
        id: "play_history",
        title: "Play history",
        plain: "your play time by week and year",
        line: "Curator › History: what you played this week, month and year, and a calm Year in review.",
        place: "Settings › Playing › Curator",
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
        id: "pixel_path",
        title: "Pixel Path",
        plain: "picture and sound details in the pause menu",
        line: "In the pause menu: how a game's picture and sound reach you, stage by stage, graded in colour.",
        place: "Settings › Playing › Curator",
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
        enthusiast: Pick::On,
        preview: pv::pixel_path_live,
        todo: None,
    },
    Feature {
        id: "frames",
        title: "Screen frames on every console",
        plain: "frames around the picture on every console",
        line: "Auto frames the picture on the home consoles too, not only on the handhelds.",
        place: "Settings › Look & feel › Screen frames",
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
        id: "last_game",
        title: "Straight back into your game",
        plain: "start straight into your last game",
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
        // Was "Quit without saving", a pause-menu entry of its own (its id is kept: settings.json's `tour_later`).
        id: "quit_without_saving",
        title: "Remember where I quit",
        plain: "continue where you quit",
        line: "On: Quit keeps the moment and the next start continues there. Off: only the game's own saves stay.",
        place: "Settings › Playing › Remember where I quit",
        guide: "settings.md#remember-where-i-quit",
        since: "0.1.0",
        switch: Some(Switch { get: |s| s.remember_quit, set: |s, on| s.remember_quit = on }),
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::remember_quit,
        todo: None,
    },
    Feature {
        id: "motion",
        title: "Motion",
        plain: "gentle animations",
        line: "Selections glide and pages slide in. Off shows every press at once, without animation.",
        place: "Settings › Look & feel › Look › Motion (R1)",
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
        id: "sounds",
        title: "Menu sounds",
        plain: "soft clicks in the menus",
        line: "Soft clicks as you move through the menus. Never in games.",
        place: "Settings › Look & feel › Sounds",
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
        place: "Settings › Look & feel › Haptics",
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
        place: "Settings › Look & feel › LEDs",
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
        place: "Pause menu › Options › Extras",
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
        place: "Settings › Games › Tools",
        guide: "extras.md#tools",
        since: "0.1.0",
        switch: None,
        simple: Pick::Keep,
        enthusiast: Pick::Keep,
        preview: pv::tools,
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

/// The entries that are wired (not `todo`), with their index in `FEATURES`.
pub fn available() -> impl Iterator<Item = (usize, &'static Feature)> {
    FEATURES.iter().enumerate().filter(|(_, f)| f.todo.is_none())
}

pub fn by_id(id: &str) -> Option<usize> {
    available().find(|(_, f)| f.id == id).map(|(i, _)| i)
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

/// The cards of Settings → Tour: every feature.
pub fn tour_list() -> Vec<usize> {
    available().map(|(i, _)| i).collect()
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
    /// Settings → Tour: B goes back to Settings.
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

    pub fn feature(&self) -> Option<&'static Feature> {
        self.list.get(self.at).map(|i| &FEATURES[*i])
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
        let i = *self.list.get(self.at)?;
        let light = pal().is_light();
        // A picture of the screen at the design's sizes, scaled down onto the card: Text size doesn't change it.
        Some(self.previews.entry((i, light)).or_insert_with(|| crate::type_scale::unscaled(|| render_preview(&FEATURES[i], text, light))))
    }

    /// The card page. Returns the choice rows and the hint pills for touch.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text) -> Hits {
        c.clear(pal().void);
        let mut hits = Hits::default();
        let Some(f) = self.feature() else { return hits };
        let heading = match &self.mode {
            Mode::First => "Choose for yourself".to_string(),
            Mode::Tour => "Tour".to_string(),
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
        for (k, line) in text.wrap(Face::Body, 28, f.line, c.w - 2 * MARGIN).iter().take(2).enumerate() {
            text.draw(c, Face::Body, 28, MARGIN, 208 + k as i32 * 38, pal().dust, line);
        }

        let (pw, ph) = PREVIEW;
        let (px, py) = (MARGIN + 8, 300);
        c.round_rect(px - 8, py - 8, pw + 16, ph + 16, 16, pal().keycap);
        if let Some(img) = self.preview(text) {
            c.blit_scaled(img, px, py, pw, ph);
        }

        let x = px + pw + 8 + 44;
        let w = c.w - MARGIN - x;
        let f = self.feature().expect("shown above");
        let put_off = self.later.iter().any(|l| l == f.id);
        let mut y = py - 8;
        if let Some(sw) = f.switch {
            let now = if (sw.get)(&self.switches) { 0 } else { 1 };
            let (row, gap) = (88, 14);
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
            y += 30;
            let later = if self.choice == LATER {
                "Asked again in Settings › System › Tour. Changed any time in"
            } else {
                "Changed any time in"
            };
            for line in text.wrap(Face::Body, 24, later, w) {
                text.draw(c, Face::Body, 24, x, y, pal().dust, &line);
                y += crate::type_scale::line(24, 10);
            }
        } else {
            y += 30;
            for line in text.wrap(Face::Body, 24, "Nothing to switch. Find it here:", w) {
                text.draw(c, Face::Body, 24, x, y, pal().dust, &line);
                y += crate::type_scale::line(24, 10);
            }
        }
        y += 16;
        for line in text.wrap(Face::Body, 28, f.place, w).iter().take(3) {
            text.draw(c, Face::Body, 28, x, y, pal().chalk, line);
            y += crate::type_scale::line(28, 12);
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

/// "How do you like to play?": the three profiles as rows, the selected one paper, and under them what it turns on
/// and off (from the registry, so it can't say anything the switches don't do).
pub fn paint_profiles(c: &mut Canvas, text: &mut Text, sel: usize) -> Hits {
    c.clear(pal().void);
    let mut hits = Hits { sel, ..Default::default() };
    text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "How do you like to play?");
    text.draw(
        c,
        Face::Body,
        28,
        MARGIN,
        192,
        pal().dust,
        "A starting point for the extras. Every switch stays in Settings.",
    );
    let width = c.w - 2 * MARGIN;
    for (i, p) in Profile::ALL.iter().enumerate() {
        let y = 240 + i as i32 * 150;
        let selected = i == sel;
        c.rect(MARGIN, y, width, 132, if selected { pal().label } else { pal().spine });
        if selected {
            crate::game_card::lift_shadow(c, MARGIN, y + 132, width, pal().t);
        }
        let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
        text.draw(c, Face::Title, 50, MARGIN + 28, y + 62, fg, p.name());
        let blurb = text.fit(Face::Body, 24, p.blurb(), width - 56);
        text.draw(c, Face::Body, 24, MARGIN + 28, y + 104, dim, &blurb);
        hits.choices.push((Area::new(MARGIN, y, width, 132), i));
    }
    let mut y = 240 + 3 * 150 + 40;
    let p = Profile::ALL[sel.min(2)];
    let lines: Vec<(String, u32)> = match p {
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
            if !off.is_empty() {
                v.push((format!("Off: {}.", off.join(", ")), pal().dust));
            }
            v
        }
    };
    // As many lines as stay clear of the hints, the last cut with "…" (larger text on the Mini V1's 960 lines,
    // docs/DEVICES.md; on 1080 lines everything fits).
    let (step, last) = (crate::type_scale::line(24, 10), c.h - 100 - crate::type_scale::grow(24));
    for (line, colour) in lines {
        let room = ((last - y) / step + 1).max(0) as usize;
        for l in text.wrap_at_most(Face::Body, 24, &line, width, room) {
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
        assert_eq!((simple.over_game, enthusiast.over_game), (crate::over_game::Level::Off, crate::over_game::Level::PixelPath));
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
        assert!(on.contains(&"a line of picture details over every game") && off.is_empty(), "{on:?} {off:?}");
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
        // With every entry at the baseline, an update shows nothing.
        assert!(due(None, "0.3.0").is_empty());
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
        let mut c = cards(Mode::Tour, &["stickers", "lens"]);
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
        let mut tour = cards(Mode::Tour, &["lens", "echoes"]);
        tour.press(Button::R1);
        assert_eq!(tour.press(Button::South), Out::Done);
        let mut new = cards(Mode::New { version: "0.3.0".into() }, &["lens"]);
        assert_eq!(new.press(Button::South), Out::Done, "B puts them away");
        // Left/Right and L1/R1 stop at the ends.
        let mut c = cards(Mode::Tour, &["lens", "echoes"]);
        c.press(Button::Left);
        assert_eq!(c.at(), 0);
        c.press(Button::Right);
        c.press(Button::Right);
        assert_eq!(c.at(), 1);
    }

    #[test]
    fn choose_yourself_is_the_switch_cards_and_the_tour_is_all() {
        let choose = choose_list();
        assert!(choose.iter().all(|i| FEATURES[*i].switch.is_some()));
        assert_eq!(tour_list().len(), available().count());
        assert!(tour_list().len() > choose.len(), "the tour also shows where the rest is");
        // The switch cards come first, so the tour starts with what can be changed.
        assert_eq!(&tour_list()[..choose.len()], &choose[..]);
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
        let hits = paint_profiles(&mut Canvas::new(&mut px, w, h), &mut text, 1);
        assert_eq!((hits.choices.len(), hits.sel), (3, 1));
    }
}

//! The **Cozy extras** (docs/user/settings.md "Cozy extras"): the little touches that make the shelf feel like a real
//! one, each switched on its own. Not everyone likes them, so each can go without the others; most are on until the
//! player turns one off. Each has one home in Settings, where it is seen or heard ([`Extra::home`]): the ones you see
//! on the page Settings → Look → Cozy extras ([`Extra::PAGE_ROWS`]), the case sounds, the playful cases and the
//! old-TV switch-on as rows of Sound & feel beside Animations and Sounds, the slideshow in System → Sleep & power.
//!
//! - Kept in `settings.json` under `"extras"` ([`Extras`]), one `true`/`false` per [`Extra::key`]. A file from before
//!   them, a key this version doesn't know or a value that isn't a bool leaves that extra on. Only extras that are off
//!   are written, so a new extra starts on for everyone. Reset settings starts from `Extras::default()`: all on.
//! - Read anywhere with [`on`]: one relaxed atomic load, no lock, no file. pocket-ui sets it at the start and on every
//!   switch ([`set`]). pocket-core doesn't read it: what it needs comes in the start's environment (the old-TV
//!   switch-on and its sound, `crt_on::ENV`, `crt_on::SILENT_ENV`).
//! - The existing switches stay where they are and are not repeated here: Animations (Cozy/Quick) and Motion decide
//!   whether anything moves at all, Sounds whether anything is heard. An extra that needs one of them says so under
//!   its row ([`Extra::needs`]).
//!
//! A new extra: a variant in [`Extra`] (with `ALL`, `key`, `label`, `blurb`, `only`, `needs`), a field in [`Extras`],
//! a row in docs/user/settings.md's "Cozy extras" table and its German lines in `assets/i18n/de.tsv`. While its
//! feature is still being built, [`Extra::built`] keeps its row off the page; the key and [`on`] work already.

use std::sync::atomic::{AtomicU32, Ordering};

/// Where the page is, as the Tour card and the guide say it: the one place to change when the page moves
/// (`doc_checks` holds it to Settings' categories and rows).
pub const PAGE: &str = "Settings › Look › Cozy extras";

/// Where an extra that is not on the page lives ([`Extra::home`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Home {
    /// A row of the Cozy extras page ([`PAGE`]).
    Page,
    /// A row of Settings → Sound & feel, beside Animations and Sounds.
    SoundAndFeel,
    /// A row of Settings → System → Sleep & power, before Screen off after's effect.
    SleepAndPower,
    /// A row of Settings → System, under Sleep & power: Playing, beside Remember where I quit, has its eight.
    System,
}

/// One switch of the page, in the page's order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Extra {
    /// The sounds of the cases and what is in them: a case or box opening, the cartridge, disc or card going in, the
    /// old TV switching on (`ui_sound::Sound::CaseOpen` to `CrtOn`, and the Quick launch's click). Menu sounds are
    /// Settings → Sound & feel → Sounds.
    CaseSounds,
    /// The old-TV switch-on over a game's first moments at the end of the Cozy launch (`crt_on`).
    CrtOn,
    /// The TV on the Cabinet's top board (`cabinet_tv`).
    CabinetTv,
    /// The stickers on the cases (`case_stickers`): hidden from the pictures, kept in `library.json`.
    Stickers,
    /// The pause menu's Save and Load show the game's whole memory card or cartridge with its grid (`save_media`); off
    /// (the default): a slim row of eight blocks in the game's colour. The info page and the box's back always show it.
    PauseMemoryCard,
    /// Discs printed with their cover's art and title (`media::disc_pic`); off: a plain disc in its system's colour.
    PrintedDiscs,
    /// A game's developer, publisher, series and genre leading to the player's other games on the info page and the
    /// box's back (`connections`).
    Connections,
    /// Rediscover in Curator's list (`app_connections`).
    Rediscover,
    /// The room's light following the time of day (Shelf and Cabinet, `room_light`, `app_room_extras`).
    TimeOfDayLight,
    /// Decorations on the shelf that grow with the player's play (Shelf and Cabinet, `room_deco`, `app_room_extras`).
    Decorations,
    /// A slideshow of the player's games on the shelf after a quiet minute, before the screen goes off (`slideshow`,
    /// `app_slideshow`). Any button ends it and does nothing else.
    IdleSlideshow,
    /// Cases that spring, wiggle and lean, and each medium going into its console its own way (`playful`). Its start
    /// follows Animations: on with Cozy, off with Quick, until the player switches it ([`Extras::playful_animations`]).
    Playful,
    /// The last games with a moment to go back to stand a little open on the shelf (`frozen`, `app_frozen`).
    FrozenGames,
}

impl Extra {
    pub const ALL: [Extra; 13] = [
        Extra::CaseSounds,
        Extra::CrtOn,
        Extra::CabinetTv,
        Extra::Stickers,
        Extra::PauseMemoryCard,
        Extra::PrintedDiscs,
        Extra::Connections,
        Extra::Rediscover,
        Extra::TimeOfDayLight,
        Extra::Decorations,
        Extra::IdleSlideshow,
        Extra::Playful,
        Extra::FrozenGames,
    ];

    /// The page's rows, in its order: the things you see on the shelf, the cabinet, the case, the disc and the game
    /// card. (`ALL`'s order is the bits' and stays.)
    pub const PAGE_ROWS: [Extra; 8] = [
        Extra::CabinetTv,
        Extra::Stickers,
        Extra::PrintedDiscs,
        Extra::PauseMemoryCard,
        Extra::TimeOfDayLight,
        Extra::Decorations,
        Extra::Connections,
        Extra::Rediscover,
    ];

    /// Where its switch is in Settings: one place each.
    pub const fn home(self) -> Home {
        match self {
            Extra::CaseSounds | Extra::CrtOn | Extra::Playful => Home::SoundAndFeel,
            Extra::IdleSlideshow => Home::SleepAndPower,
            Extra::FrozenGames => Home::System,
            _ => Home::Page,
        }
    }

    /// Its key in `settings.json`'s `"extras"`.
    pub fn key(self) -> &'static str {
        match self {
            Extra::CaseSounds => "case_sounds",
            Extra::CrtOn => "tv_switch_on",
            Extra::CabinetTv => "cabinet_tv",
            Extra::Stickers => "case_stickers",
            Extra::PauseMemoryCard => "pause_memory_card",
            Extra::PrintedDiscs => "printed_discs",
            Extra::Connections => "connections",
            Extra::Rediscover => "rediscover",
            Extra::TimeOfDayLight => "time_of_day_light",
            Extra::Decorations => "decorations",
            Extra::IdleSlideshow => "idle_slideshow",
            Extra::Playful => "playful_animations",
            Extra::FrozenGames => "frozen_games",
        }
    }

    /// The row's name.
    pub const fn label(self) -> &'static str {
        match self {
            Extra::CaseSounds => "Case sounds",
            Extra::CrtOn => "Old-TV switch-on",
            Extra::CabinetTv => "TV on the cabinet",
            Extra::Stickers => "Stickers on cases",
            Extra::PauseMemoryCard => "Memory card in the pause menu",
            Extra::PrintedDiscs => "Printed discs",
            Extra::Connections => "Game connections",
            Extra::Rediscover => "Rediscover",
            Extra::TimeOfDayLight => "Light by time of day",
            Extra::Decorations => "Decorations",
            Extra::IdleSlideshow => "Slideshow before screen off",
            Extra::Playful => "Playful cases",
            Extra::FrozenGames => "Frozen games",
        }
    }

    /// What it does, in one plain line, under the list when its row is selected.
    pub const fn blurb(self) -> &'static str {
        match self {
            Extra::CaseSounds => "Cases click open, cartridges and discs go in with a sound. Menu sounds stay as they are.",
            Extra::CrtOn => "The game's picture comes on like an old TV when it starts.",
            Extra::CabinetTv => "A small old TV on top of the cabinet shows the selected game.",
            Extra::Stickers => "Show your stickers on the cases. Off hides them; they stay on the game's card.",
            Extra::PauseMemoryCard => "The game's memory card or cartridge with its blocks under Save and Load, instead of the slim row.",
            Extra::PrintedDiscs => "Discs printed with their cover's art and title. Off: a plain disc in its console's colour.",
            Extra::Connections => "Same developer, series or genre on the info page and the box's back lead to your other games.",
            Extra::Rediscover => "Games you haven't played for a long time, as a row in Curator.",
            Extra::TimeOfDayLight => "The room's light follows the time of day: morning sun, evening lamp.",
            Extra::Decorations => "Little things appear on your shelf the more you play.",
            Extra::IdleSlideshow => "When you put the Pocket down for a while, your games show one after another.",
            Extra::Playful => "Cases spring and wiggle, discs drop into trays, cartridges click in, spines lean as you scroll.",
            Extra::FrozenGames => "Your last five games stand a little open. A goes straight back to the moment you left.",
        }
    }

    /// The looks it is seen in, when that is not every look.
    pub fn only(self) -> Option<&'static str> {
        match self {
            Extra::CaseSounds | Extra::CrtOn | Extra::Stickers | Extra::PrintedDiscs | Extra::TimeOfDayLight | Extra::Decorations | Extra::Playful => {
                Some("Shelf and Cabinet looks only.")
            }
            Extra::CabinetTv => Some("Cabinet look only."),
            Extra::PauseMemoryCard | Extra::Connections | Extra::Rediscover | Extra::IdleSlideshow | Extra::FrozenGames => None,
        }
    }

    /// The other switch it needs on to be seen or heard at all.
    pub fn needs(self) -> Option<Needs> {
        match self {
            Extra::CaseSounds => Some(Needs::Sounds),
            Extra::CrtOn => Some(Needs::CozyFull),
            Extra::Playful => Some(Needs::MotionFull),
            Extra::FrozenGames => Some(Needs::RememberQuit),
            _ => None,
        }
    }

    /// How it starts: on, except the pause menu's whole memory card and the idle slideshow (not everyone wants it);
    /// Playful animations as Animations is (on with Cozy, `motion::cozy`).
    pub fn default_on(self) -> bool {
        match self {
            Extra::PauseMemoryCard | Extra::IdleSlideshow => false,
            Extra::Playful => crate::motion::cozy(),
            _ => true,
        }
    }

    /// Whether its feature is there to switch: the row is on the page only then. The key and [`on`] work either way,
    /// so a feature can read its switch before its row is shown.
    pub fn built(self) -> bool {
        // Every extra has its feature now; a new one starts as `!matches!(self, Extra::<New>)` until it is built.
        let _ = self;
        true
    }

    /// The extras with a row on the page, in its order.
    pub fn shown() -> Vec<Extra> {
        Extra::PAGE_ROWS.into_iter().filter(|e| e.built()).collect()
    }

    fn bit(self) -> u32 {
        1 << Extra::ALL.iter().position(|e| *e == self).unwrap_or(0)
    }
}

/// A switch elsewhere in Settings that an extra needs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Needs {
    /// Settings → Sound & feel → Sounds on.
    Sounds,
    /// Animations on Cozy and Motion on Full.
    CozyFull,
    /// Motion on Full.
    MotionFull,
    /// Settings → Playing → Remember where I quit on.
    RememberQuit,
}

impl Needs {
    /// What the page says while the other switch keeps the extra from showing.
    pub fn note(self) -> &'static str {
        match self {
            Needs::Sounds => "Silent now: Sounds is off (Sound & feel \u{203a} Sounds).",
            Needs::CozyFull => "Not shown now: it needs Animations on Cozy and Motion on Full (Sound & feel).",
            Needs::MotionFull => "Not shown now: needs Motion on Full (Sound & feel).",
            Needs::RememberQuit => "Remember where I quit is off: a Quit freezes nothing new. Moments from before and from sleep stay.",
        }
    }
}

fn yes() -> bool {
    true
}

/// Written only when off.
fn is_on(b: &bool) -> bool {
    *b
}

/// A switch off by default that stays off when its value isn't a bool.
fn lenient_off<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(v.as_bool().unwrap_or(false))
}

/// A switch that stays on when its value isn't a bool, so one wrong value never turns an extra off.
fn lenient_on<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(v.as_bool().unwrap_or(true))
}

/// Playful animations' value: missing or not a bool, it follows Animations.
fn lenient_choice<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(v.as_bool())
}

/// `settings.json`'s `"extras"`: every switch on unless written `false` (the memory card in the pause menu and the idle
/// slideshow: off unless written `true`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct Extras {
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub case_sounds: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub tv_switch_on: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub cabinet_tv: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub case_stickers: bool,
    /// Off by default: written only when on.
    #[serde(default, deserialize_with = "lenient_off", skip_serializing_if = "std::ops::Not::not")]
    pub pause_memory_card: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub printed_discs: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub connections: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub rediscover: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub time_of_day_light: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub decorations: bool,
    /// Off by default: written only when on.
    #[serde(default, deserialize_with = "lenient_off", skip_serializing_if = "std::ops::Not::not")]
    pub idle_slideshow: bool,
    /// None until the player switches it: then on with Cozy, off with Quick (`Extra::default_on`); written once chosen.
    #[serde(default, deserialize_with = "lenient_choice", skip_serializing_if = "Option::is_none")]
    pub playful_animations: Option<bool>,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub frozen_games: bool,
}

impl Default for Extras {
    fn default() -> Self {
        Extras::from_bits(DEFAULT_BITS)
    }
}

impl Extras {
    /// The switch of `e`; None for Playful animations, which is kept apart (`playful_animations`).
    fn slot(&mut self, e: Extra) -> Option<&mut bool> {
        Some(match e {
            Extra::CaseSounds => &mut self.case_sounds,
            Extra::CrtOn => &mut self.tv_switch_on,
            Extra::CabinetTv => &mut self.cabinet_tv,
            Extra::Stickers => &mut self.case_stickers,
            Extra::PauseMemoryCard => &mut self.pause_memory_card,
            Extra::PrintedDiscs => &mut self.printed_discs,
            Extra::Connections => &mut self.connections,
            Extra::Rediscover => &mut self.rediscover,
            Extra::TimeOfDayLight => &mut self.time_of_day_light,
            Extra::Decorations => &mut self.decorations,
            Extra::IdleSlideshow => &mut self.idle_slideshow,
            Extra::FrozenGames => &mut self.frozen_games,
            Extra::Playful => return None,
        })
    }

    pub fn get(&self, e: Extra) -> bool {
        let mut x = *self;
        match x.slot(e) {
            Some(b) => *b,
            None => x.playful_animations.unwrap_or_else(|| e.default_on()),
        }
    }

    /// Switches `e`; Playful animations is then the player's choice and no longer follows Animations.
    pub fn set(&mut self, e: Extra, on: bool) {
        match self.slot(e) {
            Some(b) => *b = on,
            None => self.playful_animations = Some(on),
        }
    }

    /// Whether every extra is as it starts (`settings.json` leaves `"extras"` out then).
    pub fn is_default(&self) -> bool {
        self.bits() == DEFAULT_BITS
    }

    fn bits(&self) -> u32 {
        let plain = Extra::ALL.iter().filter(|e| **e != Extra::Playful && self.get(**e)).fold(0, |b, e| b | e.bit());
        match self.playful_animations {
            Some(on) => plain | CHOSEN | if on { Extra::Playful.bit() } else { 0 },
            None => plain,
        }
    }

    fn from_bits(bits: u32) -> Extras {
        let mut x = Extras {
            case_sounds: false,
            tv_switch_on: false,
            cabinet_tv: false,
            case_stickers: false,
            pause_memory_card: false,
            printed_discs: false,
            connections: false,
            rediscover: false,
            time_of_day_light: false,
            decorations: false,
            idle_slideshow: false,
            playful_animations: None,
            frozen_games: false,
        };
        for e in Extra::ALL.into_iter().filter(|e| *e != Extra::Playful) {
            x.set(e, bits & e.bit() != 0);
        }
        x.playful_animations = (bits & CHOSEN != 0).then_some(bits & Extra::Playful.bit() != 0);
        x
    }
}

/// Every extra as it starts: on, except the pause menu's whole memory card (bit 4, `Extra::PauseMemoryCard`), the idle
/// slideshow (bit 10, `Extra::IdleSlideshow`) and Playful animations (bit 11, `Extra::Playful`), which follows
/// Animations until it is [`CHOSEN`]; a test keeps this equal to `default_on`.
const DEFAULT_BITS: u32 = ((1 << Extra::ALL.len()) - 1) & !(1 << 4) & !(1 << 10) & !(1 << 11);
/// Playful animations was switched by the player: its bit holds the choice.
const CHOSEN: u32 = 1 << 31;

/// The switches in use: as they start until `set` (pocket-core, tools, tests).
static ON: AtomicU32 = AtomicU32::new(DEFAULT_BITS);

/// Whether extra `e` is on.
pub fn on(e: Extra) -> bool {
    let bits = ON.load(Ordering::Relaxed);
    if e == Extra::Playful && bits & CHOSEN == 0 {
        return e.default_on();
    }
    bits & e.bit() != 0
}

/// The switches in use, as `settings.json` keeps them.
pub fn get() -> Extras {
    Extras::from_bits(ON.load(Ordering::Relaxed))
}

/// From `settings.json` at the start, and after each switch on the page.
pub fn set(x: Extras) {
    ON.store(x.bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_extra_has_its_own_key_and_bit_and_a_field() {
        let keys: std::collections::BTreeSet<_> = Extra::ALL.iter().map(|e| e.key()).collect();
        assert_eq!(keys.len(), Extra::ALL.len());
        // The serialised field names are the keys: one switched from its start writes exactly its key.
        for e in Extra::ALL {
            assert_eq!(Extras::default().get(e), e.default_on(), "{e:?}");
            let mut x = Extras::default();
            x.set(e, !e.default_on());
            let v = serde_json::to_value(x).unwrap();
            assert_eq!(v, serde_json::json!({ e.key(): !e.default_on() }), "{e:?}");
            assert_eq!(Extras::from_bits(x.bits()), x);
        }
    }

    #[test]
    fn missing_unknown_and_wrong_values_leave_an_extra_as_it_starts() {
        let x: Extras = serde_json::from_str("{}").unwrap();
        assert!(x.is_default());
        assert!(!x.pause_memory_card && x.case_sounds);
        let x: Extras =
            serde_json::from_str(r#"{"cabinet_tv": false, "printed_discs": "no", "rediscover": 0, "pause_memory_card": "yes", "something_new": false}"#).unwrap();
        assert!(!x.get(Extra::CabinetTv));
        assert!(x.get(Extra::PrintedDiscs) && x.get(Extra::Rediscover) && !x.get(Extra::PauseMemoryCard));
        assert_eq!(Extra::ALL.iter().filter(|e| x.get(**e) != e.default_on()).count(), 1);
        assert_eq!(serde_json::to_string(&Extras::default()).unwrap(), "{}", "the start writes nothing");
        let x: Extras = serde_json::from_str(r#"{"pause_memory_card": true}"#).unwrap();
        assert!(x.pause_memory_card && !x.is_default());
    }

    #[test]
    fn the_start_value_is_the_one_in_use_before_settings_are_read() {
        assert_eq!(DEFAULT_BITS, Extra::ALL.iter().filter(|e| e.default_on()).fold(0, |b, e| b | e.bit()));
        assert_eq!(Extras::from_bits(DEFAULT_BITS), Extras::default());
    }

    /// Every extra has exactly one home: a row of the page, or one of Sound & feel or Sleep & power (2026-10-30), and the
    /// page holds at most eight rows.
    #[test]
    fn every_extra_has_one_home() {
        for e in Extra::ALL {
            assert_eq!(Extra::PAGE_ROWS.contains(&e), e.home() == Home::Page, "{e:?}");
        }
        let mut seen = std::collections::BTreeSet::new();
        assert!(Extra::PAGE_ROWS.iter().all(|e| seen.insert(e.key())), "no row twice");
        assert!(Extra::PAGE_ROWS.len() <= 8);
        assert_eq!(Extra::ALL.iter().filter(|e| e.home() == Home::SleepAndPower).collect::<Vec<_>>(), [&Extra::IdleSlideshow]);
        // What needs another switch lives beside it: none of the page's rows does.
        assert!(Extra::PAGE_ROWS.iter().all(|e| e.needs().is_none()));
    }

    #[test]
    fn the_idle_slideshow_is_off_until_switched_on_in_every_look() {
        let x: Extras = serde_json::from_str("{}").unwrap();
        assert!(!x.get(Extra::IdleSlideshow) && x.is_default());
        assert_eq!(serde_json::to_string(&x).unwrap(), "{}", "off is not written");
        let x: Extras = serde_json::from_str(r#"{"idle_slideshow": true}"#).unwrap();
        assert!(x.get(Extra::IdleSlideshow) && !x.is_default());
        assert_eq!(serde_json::to_string(&x).unwrap(), r#"{"idle_slideshow":true}"#);
        // A value that is no bool leaves it off.
        let x: Extras = serde_json::from_str(r#"{"idle_slideshow": "sure"}"#).unwrap();
        assert!(!x.get(Extra::IdleSlideshow));
    }

    #[test]
    fn playful_animations_follow_animations_until_switched() {
        let x = Extras::default();
        assert_eq!(x.playful_animations, None);
        crate::motion::set_cozy(true);
        assert!(x.get(Extra::Playful), "on with Cozy");
        crate::motion::set_cozy(false);
        assert!(!x.get(Extra::Playful), "off with Quick");
        let mut y = x;
        y.set(Extra::Playful, true);
        assert!(y.get(Extra::Playful) && !y.is_default());
        assert_eq!(serde_json::to_value(y).unwrap(), serde_json::json!({ "playful_animations": true }));
        crate::motion::set_cozy(true);
        y.set(Extra::Playful, false);
        assert!(!y.get(Extra::Playful), "off in Cozy once chosen");
        assert_eq!(Extras::from_bits(y.bits()), y);
        assert_eq!(Extras::from_bits(x.bits()), x);
        let z: Extras = serde_json::from_str(r#"{"playful_animations": "yes"}"#).unwrap();
        assert_eq!(z.playful_animations, None, "not a bool: follows Animations");
        crate::motion::set_cozy(false);
    }

    #[test]
    fn the_placeholders_are_kept_off_the_page_until_built() {
        let shown = Extra::shown();
        assert_eq!(shown, Extra::PAGE_ROWS, "all of them are built");
        assert!(!shown.contains(&Extra::IdleSlideshow), "its row is in Sleep & power");
        assert_eq!(Extra::IdleSlideshow.only(), None, "the slideshow works in every look");
        assert_eq!(Extra::CabinetTv.only(), Some("Cabinet look only."));
    }
}

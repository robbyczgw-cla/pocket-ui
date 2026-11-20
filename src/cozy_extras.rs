//! The **Cozy extras** (docs/user/settings.md "Cozy extras"): the little touches that make the shelf feel like a real
//! one, each switched on its own. Not everyone likes them, so each can go without the others; most are on until the
//! player turns one off. Each has one home in Settings, where it is seen or heard ([`Extra::home`]): the ones you see
//! or that move on the page Settings → Look → Cozy extras ([`Extra::PAGE_ROWS`], playful cases and old-TV switch-on
//! since the tree of 2026-11-13); case sounds live in Screen & sound, beside Menu sounds (Settings v2).
//!
//! - Kept in `settings.json` under `"extras"` ([`Extras`]), one `true`/`false` per [`Extra::key`]. A file from before
//!   them, a key this version doesn't know or a value that isn't a bool leaves that extra on. Only extras that are off
//!   are written, so a new extra starts on for everyone, except the one that costs power, `TvPlaysGame`: off until written `true`. Reset settings starts from `Extras::default()`: all on.
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
    /// A row of Settings → Screen & sound, beside Menu sounds.
    ControlsAndSound,
}

/// One switch of the page, in the page's order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Extra {
    /// The sounds of the cases and what is in them: a case or box opening, the cartridge, disc or card going in, the
    /// old TV switching on (`ui_sound::Sound::CaseOpen` to `CrtOn`, and the Quick launch's click). Menu sounds are
    /// Settings → Screen & sound → Menu sounds.
    CaseSounds,
    /// The old-TV switch-on over a game's first moments at the end of the Cozy launch (`crt_on`).
    CrtOn,
    /// The TV on the Cabinet's top board (`cabinet_tv`).
    CabinetTv,
    /// The stickers on the cases (`case_stickers`): hidden from the pictures, kept in `library.json`.
    Stickers,
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
    /// Cases that spring, wiggle and lean, and each medium going into its console its own way (`playful`). Its start
    /// follows Animations: on with Cozy, off with Quick, until the player switches it ([`Extras::playful_animations`]).
    Playful,
    /// Games never played stand in the shop's plastic film, torn off at their first start (`shrink_wrap`).
    ShrinkWrap,
    /// The line read from a game's own battery save ("8 badges · Charizard Lv. 45 · 31:12 h", `case_knows`): on the box's
    /// back, beside Continue and on "Where was I?". Off: no line anywhere and no save read at all, so it stays spoiler-free.
    ProgressFromSaves,
    /// The TV on the cabinet plays a game you never played, live, in a second player without window and sound
    /// (`showcase`). **Off at the start**, unlike the others: it costs CPU and battery while it plays. Off: no player
    /// is ever started.
    TvPlaysGame,
    /// A window on the room's wall with the sky as it is now and the season's piece on its sill (Shelf and Cabinet,
    /// `room_window`).
    Window,
    /// After Quit the TV's picture pulls together to a line and a dot that glows on a moment (`eject::tv_off`); off: the
    /// way back starts from black.
    CrtOff,
    /// After dark, while the cabinet's TV is on, the room dims a little and the TV's warm light falls on the shelf and
    /// the cases (`tv_glow`).
    EveningGlow,
    /// The power light of a handheld frame in the game and of the handhelds in the Cabinet, as the original's: it shows the
    /// battery (red, dim, blinking at the "Battery low" level) and blinks when a state is saved (`frame_led`). Off: the
    /// light stays steadily on as the frame is drawn.
    PowerLights,
    /// A handheld's display switching on and off as the real one's (the Game Boy's green rise, the SP's front light, the
    /// PSP's fade, `lcd_on`). Off: the display just comes up out of the dark and goes dark, as before.
    HandheldOn,
}

impl Extra {
    pub const ALL: [Extra; 18] = [
        Extra::CaseSounds,
        Extra::CrtOn,
        Extra::CabinetTv,
        Extra::Stickers,
        Extra::PrintedDiscs,
        Extra::Connections,
        Extra::Rediscover,
        Extra::TimeOfDayLight,
        Extra::Decorations,
        Extra::Playful,
        Extra::ShrinkWrap,
        Extra::ProgressFromSaves,
        Extra::TvPlaysGame,
        Extra::Window,
        Extra::CrtOff,
        Extra::EveningGlow,
        Extra::PowerLights,
        Extra::HandheldOn,
    ];

    /// The page's rows, in its order: the things you see on the shelf, the cabinet, the case, the disc, a game
    /// starting and the game card. (`ALL`'s order is the bits' and stays.)
    pub const PAGE_ROWS: [Extra; 17] = [
        Extra::CabinetTv,
        Extra::TvPlaysGame,
        Extra::Stickers,
        Extra::PrintedDiscs,
        Extra::Playful,
        Extra::ShrinkWrap,
        Extra::CrtOn,
        Extra::CrtOff,
        Extra::HandheldOn,
        Extra::PowerLights,
        Extra::TimeOfDayLight,
        Extra::EveningGlow,
        Extra::Window,
        Extra::Decorations,
        Extra::ProgressFromSaves,
        Extra::Connections,
        Extra::Rediscover,
    ];

    /// Where its switch is in Settings: one place each.
    pub const fn home(self) -> Home {
        match self {
            Extra::CaseSounds => Home::ControlsAndSound,
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
            Extra::PrintedDiscs => "printed_discs",
            Extra::Connections => "connections",
            Extra::Rediscover => "rediscover",
            Extra::TimeOfDayLight => "time_of_day_light",
            Extra::Decorations => "decorations",
            Extra::Playful => "playful_animations",
            Extra::ShrinkWrap => "sealed_new_games",
            Extra::ProgressFromSaves => "progress_from_saves",
            Extra::TvPlaysGame => "tv_plays_game",
            Extra::Window => "window",
            Extra::CrtOff => "tv_switch_off",
            Extra::EveningGlow => "evening_tv_glow",
            Extra::PowerLights => "power_lights",
            Extra::HandheldOn => "handheld_switch_on",
        }
    }

    /// The row's name.
    pub const fn label(self) -> &'static str {
        match self {
            Extra::CaseSounds => "Case sounds",
            Extra::CrtOn => "Old-TV switch-on",
            Extra::CabinetTv => "TV on the cabinet",
            Extra::Stickers => "Stickers on cases",
            Extra::PrintedDiscs => "Printed discs",
            Extra::Connections => "Game connections",
            Extra::Rediscover => "Rediscover",
            Extra::TimeOfDayLight => "Light by time of day",
            Extra::Decorations => "Decorations",
            Extra::Playful => "Playful cases",
            Extra::ShrinkWrap => "Sealed new games",
            Extra::ProgressFromSaves => "Progress from saves",
            Extra::TvPlaysGame => "TV plays the game",
            Extra::Window => "Window",
            Extra::CrtOff => "Old-TV switch-off",
            Extra::EveningGlow => "Evening TV glow",
            Extra::PowerLights => "Power lights",
            Extra::HandheldOn => "Handheld switch-on",
        }
    }

    /// What it does, in one plain line, under the list when its row is selected.
    pub const fn blurb(self) -> &'static str {
        match self {
            Extra::CaseSounds => "Cases click open, cartridges and discs go in with a sound. Menu sounds stay as they are.",
            Extra::CrtOn => "The game's picture comes on like an old TV: a dot, a line, the blue channel, then the game.",
            Extra::CabinetTv => "A small old TV on top of the cabinet shows the selected game.",
            Extra::Stickers => "Show your stickers on the cases. Off hides them; they stay on the game's card.",
            Extra::PrintedDiscs => "Discs printed with their cover's art and title. Off: a plain disc in its console's colour.",
            Extra::Connections => "Same developer, series or genre on the info page and the box's back lead to your other games.",
            Extra::Rediscover => "Games you haven't played for a long time, as a row in Curator.",
            Extra::TimeOfDayLight => "The room's light follows the time of day: morning sun, evening lamp.",
            Extra::Decorations => "Little things appear on your shelf the more you play.",
            Extra::Playful => "Cases spring and wiggle, discs drop into trays, cartridges click in, spines lean as you scroll.",
            Extra::ShrinkWrap => "Games you have never played stand in plastic wrap. It tears off the first time you start one.",
            Extra::ProgressFromSaves => "Badges, stars and hearts from your own saves on the case; off keeps it spoiler-free.",
            Extra::TvPlaysGame => "The TV shows a game you haven't played running live, without sound. Uses more power while it plays; off, nothing extra runs.",
            Extra::Window => "A window on the wall shows the sky as it is now, with something for the season on its sill.",
            Extra::CrtOff => "After Quit the picture shrinks to a line and a glowing dot, like an old TV.",
            Extra::EveningGlow => "After dark the room dims a little and the TV's warm light falls on the shelf.",
            Extra::PowerLights => "A handheld's power light shows your battery like the real one and blinks when you save.",
            Extra::HandheldOn => "A handheld's screen comes on and goes off like the real one: a Game Boy's green glow, a PSP's soft fade.",
        }
    }

    /// The looks it is seen in, when that is not every look.
    pub fn only(self) -> Option<&'static str> {
        match self {
            Extra::CaseSounds | Extra::CrtOn | Extra::CrtOff | Extra::HandheldOn | Extra::Stickers | Extra::PrintedDiscs | Extra::TimeOfDayLight | Extra::Decorations | Extra::Window | Extra::Playful => {
                Some("Shelf and Cabinet looks only.")
            }
            Extra::CabinetTv | Extra::TvPlaysGame | Extra::EveningGlow => Some("Cabinet look only."),
            Extra::Connections | Extra::Rediscover | Extra::ShrinkWrap | Extra::ProgressFromSaves | Extra::PowerLights => None,
        }
    }

    /// The other switch it needs on to be seen or heard at all.
    pub fn needs(self) -> Option<Needs> {
        match self {
            Extra::CaseSounds => Some(Needs::Sounds),
            Extra::CrtOn | Extra::CrtOff | Extra::HandheldOn => Some(Needs::CozyFull),
            Extra::Playful => Some(Needs::MotionFull),
            _ => None,
        }
    }

    /// Playful animations as Animations is (on with Cozy, `motion::cozy`); the TV playing the game starts off.
    pub fn default_on(self) -> bool {
        match self {
            Extra::Playful => crate::motion::cozy(),
            Extra::TvPlaysGame => false,
            _ => true,
        }
    }

    /// Whether its feature is there to switch: the row is on the page only then. The key and [`on`] work either way,
    /// so a feature can read its switch before its row is shown.
    pub fn built(self) -> bool {
        // A new one starts as `!matches!(self, Extra::<New>)` until it is built.
        // Sealed new games too on Android's handheld view (14.11.): it has its own play history, and the Mobile view's comes
        // in (`android_handheld::take_mobile_history`, `mobile_played`).
        // The Window is taken off for 0.6 (19.11.: out of place in the room): no row, nothing drawn
        // (`App::room_window`); its key is still read, so a choice made before stays for when it comes back.
        !matches!(self, Extra::Window)
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
    /// Settings → Screen & sound → Menu sounds on.
    Sounds,
    /// Animations on Cozy and Motion on Full.
    CozyFull,
    /// Motion on Full.
    MotionFull,
}

impl Needs {
    /// What the page says while the other switch keeps the extra from showing.
    pub fn note(self) -> &'static str {
        match self {
            Needs::Sounds => "Silent now: Menu sounds is off (Screen & sound \u{203a} Menu sounds).",
            Needs::CozyFull => "Not shown now: it needs Animations on Cozy (Playing) and Motion on Full (Look).",
            Needs::MotionFull => "Not shown now: needs Motion on Full (Look).",
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

/// A switch that stays on when its value isn't a bool, so one wrong value never turns an extra off.
fn lenient_on<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(v.as_bool().unwrap_or(true))
}

/// A switch that stays off when its value isn't a bool: one wrong value never starts work the player didn't choose.
fn lenient_off<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(v.as_bool().unwrap_or(false))
}

/// Written only when on (the extras that start off).
fn is_off(b: &bool) -> bool {
    !*b
}

/// Playful animations' value: missing or not a bool, it follows Animations.
fn lenient_choice<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(v.as_bool())
}

/// `settings.json`'s `"extras"`: visual and sound switches are on unless written `false`; Playful follows Animations;
/// the TV playing the game is off unless written `true`.
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
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub sealed_new_games: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub progress_from_saves: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub window: bool,
    /// Off until the player switches it on: it costs power while it plays (`showcase`).
    #[serde(default, deserialize_with = "lenient_off", skip_serializing_if = "is_off")]
    pub tv_plays_game: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub tv_switch_off: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub evening_tv_glow: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub power_lights: bool,
    #[serde(default = "yes", deserialize_with = "lenient_on", skip_serializing_if = "is_on")]
    pub handheld_switch_on: bool,
    /// None until the player switches it: then on with Cozy, off with Quick (`Extra::default_on`); written once chosen.
    #[serde(default, deserialize_with = "lenient_choice", skip_serializing_if = "Option::is_none")]
    pub playful_animations: Option<bool>,
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
            Extra::PrintedDiscs => &mut self.printed_discs,
            Extra::Connections => &mut self.connections,
            Extra::Rediscover => &mut self.rediscover,
            Extra::TimeOfDayLight => &mut self.time_of_day_light,
            Extra::Decorations => &mut self.decorations,
            Extra::ShrinkWrap => &mut self.sealed_new_games,
            Extra::ProgressFromSaves => &mut self.progress_from_saves,
            Extra::TvPlaysGame => &mut self.tv_plays_game,
            Extra::Window => &mut self.window,
            Extra::CrtOff => &mut self.tv_switch_off,
            Extra::EveningGlow => &mut self.evening_tv_glow,
            Extra::PowerLights => &mut self.power_lights,
            Extra::HandheldOn => &mut self.handheld_switch_on,
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
            printed_discs: false,
            connections: false,
            rediscover: false,
            time_of_day_light: false,
            decorations: false,
            sealed_new_games: false,
            progress_from_saves: false,
            tv_plays_game: false,
            window: false,
            tv_switch_off: false,
            evening_tv_glow: false,
            power_lights: false,
            handheld_switch_on: false,
            playful_animations: None,
        };
        for e in Extra::ALL.into_iter().filter(|e| *e != Extra::Playful) {
            x.set(e, bits & e.bit() != 0);
        }
        x.playful_animations = (bits & CHOSEN != 0).then_some(bits & Extra::Playful.bit() != 0);
        x
    }
}

/// Animations until it is [`CHOSEN`], the TV playing the game off; a test keeps this equal to `default_on`.
const DEFAULT_BITS: u32 = ((1 << Extra::ALL.len()) - 1) & !(1 << (Extra::Playful as u32)) & !(1 << (Extra::TvPlaysGame as u32));
/// Playful animations was switched by the player: its bit holds the choice.
const CHOSEN: u32 = 1 << 31;

/// The switches in use: as they start until `set` (pocket-core, tools, tests).
static ON: AtomicU32 = AtomicU32::new(DEFAULT_BITS);

/// The line under Playful cases while it still follows Animations: what it is, and that Cozy switches it on (the Brick,
/// 2026-11-07: Animations flipping Playful cases as well looked like a fault).
pub const PLAYFUL_FOLLOWS: &str = "Cases spring and wiggle. Follows Animations: on with Cozy, off with Quick, until you switch it.";

/// The line under an extra's row in Settings: `line`, or for Playful cases that it follows Animations until switched.
pub fn row_line(e: Extra, x: &Extras, line: &'static str) -> &'static str {
    if e == Extra::Playful && x.playful_animations.is_none() { PLAYFUL_FOLLOWS } else { line }
}

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
        assert!(x.case_sounds);
        let x: Extras =
            serde_json::from_str(r#"{"cabinet_tv": false, "printed_discs": "no", "rediscover": 0, "pause_memory_card": "yes", "something_new": false}"#).unwrap();
        assert!(!x.get(Extra::CabinetTv));
        assert_eq!(Extra::ALL.iter().filter(|e| x.get(**e) != e.default_on()).count(), 1);
        assert_eq!(serde_json::to_string(&Extras::default()).unwrap(), "{}", "the start writes nothing");
        let x: Extras = serde_json::from_str(r#"{"pause_memory_card": true}"#).unwrap();
        assert!(x.is_default(), "retired pause-menu preference keeps the default slim row");
    }

    #[test]
    fn the_start_value_is_the_one_in_use_before_settings_are_read() {
        assert_eq!(DEFAULT_BITS, Extra::ALL.iter().filter(|e| e.default_on()).fold(0, |b, e| b | e.bit()));
        assert_eq!(Extras::from_bits(DEFAULT_BITS), Extras::default());
    }

    /// Every extra has exactly one home: a row of the page, or one of Screen & sound (2026-10-30, the tree of
    /// 2026-11-13), and the page with its GBA console row holds at most eighteen rows (ten until Sealed new games, eleven
    /// until Progress from saves, twelve with the TV playing the game, 14.11.2026, fifteen with the Window, the TV's
    /// switch-off and evening glow, 19.11.2026, the Window's row hidden for now, seventeen
    /// with Handheld switch-on and Power lights; the page scrolls).
    #[test]
    fn every_extra_has_one_home() {
        for e in Extra::ALL {
            assert_eq!(Extra::PAGE_ROWS.contains(&e), e.home() == Home::Page, "{e:?}");
        }
        let mut seen = std::collections::BTreeSet::new();
        assert!(Extra::PAGE_ROWS.iter().all(|e| seen.insert(e.key())), "no row twice");
        assert!(Extra::PAGE_ROWS.len() < 18);
        // What needs another switch says which (`Needs::note`), wherever its row is.
        assert!(Extra::ALL.iter().filter_map(|e| e.needs()).all(|n| !n.note().is_empty()));
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
        // And its row says so, until it is switched: Cozy turning it on is no surprise then.
        assert_eq!(row_line(Extra::Playful, &x, Extra::Playful.blurb()), PLAYFUL_FOLLOWS);
        assert_eq!(row_line(Extra::Playful, &y, Extra::Playful.blurb()), Extra::Playful.blurb(), "chosen: its own line");
        assert_eq!(row_line(Extra::CrtOn, &x, Extra::CrtOn.blurb()), Extra::CrtOn.blurb());
    }

    /// Lead, 14.11.: Sealed new games was off on Android with "no play history of its own"; the handheld view has one and
    /// takes the Mobile view's. Same row, same switch, on by default, whatever the target.
    #[test]
    fn sealed_new_games_has_its_row_and_is_on_by_default_on_android_too() {
        assert!(Extra::ShrinkWrap.built());
        assert!(Extra::shown().contains(&Extra::ShrinkWrap));
        assert!(Extra::ShrinkWrap.default_on());
        let src = include_str!("cozy_extras.rs");
        let body = src.split("pub fn built(self) -> bool {").nth(1).and_then(|b| b.split("\n    }").next()).unwrap_or_default();
        assert!(!body.contains("target_os"), "no platform takes a row off the page: {body}");
    }

    /// The TV playing the game costs power: off at the start and after anything unreadable, written only when on.
    #[test]
    fn the_tv_playing_the_game_starts_off_and_only_a_true_turns_it_on() {
        assert!(!Extras::default().get(Extra::TvPlaysGame));
        assert!(!Extra::TvPlaysGame.default_on());
        for json in ["{}", r#"{"tv_plays_game": "yes"}"#, r#"{"tv_plays_game": 1}"#, r#"{"tv_plays_game": null}"#, r#"{"tv_plays_game": false}"#] {
            let x: Extras = serde_json::from_str(json).unwrap();
            assert!(!x.get(Extra::TvPlaysGame) && x.is_default(), "{json}");
        }
        let x: Extras = serde_json::from_str(r#"{"tv_plays_game": true}"#).unwrap();
        assert!(x.get(Extra::TvPlaysGame) && !x.is_default());
        assert_eq!(serde_json::to_value(x).unwrap(), serde_json::json!({ "tv_plays_game": true }), "kept across restarts");
        assert_eq!(Extras::from_bits(x.bits()), x);
        // Its row follows the cabinet's own TV on the page.
        assert_eq!(Extra::PAGE_ROWS[..2], [Extra::CabinetTv, Extra::TvPlaysGame]);
    }

    #[test]
    fn the_placeholders_are_kept_off_the_page_until_built() {
        let shown = Extra::shown();
        // All of them are built but the Window, taken off for 0.6: its row is not on the page.
        assert_eq!(shown, Extra::PAGE_ROWS.into_iter().filter(|e| *e != Extra::Window).collect::<Vec<_>>());
        assert!(!Extra::Window.built() && !shown.contains(&Extra::Window));
        assert_eq!(Extra::CabinetTv.only(), Some("Cabinet look only."));
    }
}

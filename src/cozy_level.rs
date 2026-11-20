//! "How cozy should it be?": one page of the first start (after the look) that sets the cozy switches together, instead
//! of each new touch hiding in Settings (Alex, 19.11.2026: 0.6 brings many of them). Three levels:
//!
//! - **Plain**: Quick starts, none of the cozy extras that are seen or heard (sounds of the cases, light by time of day,
//!   the evening TV glow, decorations, stickers, printed discs, playful cases, sealed new games, the old-TV switch-on and
//!   switch-off, the handhelds' switch-on and power lights). What informs stays (game connections, progress from saves, Rediscover).
//! - **Cozy**: as pocket-ui ships: Cozy starts, every extra on except the TV that plays a game (it costs power), the
//!   shelf tidy.
//! - **Everything**: Cozy, every extra on (the cabinet's TV playing games too).
//!
//! The lived-in shelf is off in every level (Alex, 19.11.2026: a messy shelf is a choice of its own, Settings › Look ›
//! Lived-in shelf only).
//!
//! Every switch stays in Settings on its own (Look › Cozy extras, Look › Game start animation);
//! the page only sets them together, once. The level the switches stand at now is `of`, so the page opens on it.
//!
//! New 0.6 touches join here as they land on main (shelf space, RomM, photo album off, the ritual's switches): one line in
//! `apply` and `of` each, and their word in `includes`.

use crate::cozy_extras::{Extra, Extras};
use crate::settings::Animations;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CozyLevel {
    Plain,
    Cozy,
    Everything,
}

/// The page's title and the sentence under it.
pub const TITLE: &str = "How cozy should it be?";
pub const SENTENCE: &str = "One choice for all the little touches. Each one stays in Settings › Look.";

/// The extras that are only seen or heard (Plain leaves them out); the others inform and stay on in every level.
/// The Window is not among them: it is taken off for 0.6 (`Extra::built`), and a level leaves a choice made for it alone.
const SEEN_OR_HEARD: [Extra; 14] = [
    Extra::CaseSounds,
    Extra::CrtOn,
    Extra::CabinetTv,
    Extra::Stickers,
    Extra::PrintedDiscs,
    Extra::TimeOfDayLight,
    Extra::Decorations,
    Extra::Playful,
    Extra::ShrinkWrap,
    Extra::TvPlaysGame,
    Extra::CrtOff,
    Extra::EveningGlow,
    Extra::HandheldOn,
    Extra::PowerLights,
];

/// What a level sets: the start's animation, the lived-in shelf and the cozy extras.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cozy {
    pub animations: Animations,
    pub lived_in: bool,
    pub extras: Extras,
}

impl CozyLevel {
    pub const ALL: [CozyLevel; 3] = [CozyLevel::Plain, CozyLevel::Cozy, CozyLevel::Everything];

    pub fn name(self) -> &'static str {
        match self {
            CozyLevel::Plain => "Plain",
            CozyLevel::Cozy => "Cozy",
            CozyLevel::Everything => "Everything",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            CozyLevel::Plain => "Quick starts, no sounds or decorations: just your games.",
            CozyLevel::Cozy => "Cases open, games go in with a sound, the room follows the day.",
            CozyLevel::Everything => "All of it, plus a TV in the Cabinet that plays new games.",
        }
    }

    /// What it brings, in the touches' own names, for the line under the rows: the new ones first, so they are seen.
    pub fn includes(self) -> &'static str {
        match self {
            CozyLevel::Plain => "Nothing extra. Game connections and progress from your saves stay on.",
            CozyLevel::Cozy => "Light by time of day, a warm TV glow after dark, case sounds, stickers, printed discs, sealed new games, an old TV switching on and off.",
            CozyLevel::Everything => "Cozy's touches, plus the Cabinet's TV playing games you haven't tried (uses more power).",
        }
    }

    /// The switches of this level, the others of `extras` as they are.
    pub fn apply(self, extras: Extras) -> Cozy {
        let mut x = extras;
        for e in SEEN_OR_HEARD {
            let on = match self {
                CozyLevel::Plain => false,
                CozyLevel::Cozy => e != Extra::TvPlaysGame,
                CozyLevel::Everything => true,
            };
            x.set(e, on);
        }
        // Playful cases follow the start's animation unless the level turns them off (`Extras::playful_animations`).
        if self != CozyLevel::Plain {
            x.playful_animations = None;
        }
        Cozy {
            animations: if self == CozyLevel::Plain { Animations::Quick } else { Animations::Cozy },
            // Never by a level: only Settings › Look › Lived-in shelf turns it on.
            lived_in: false,
            extras: x,
        }
    }

    /// The level the switches stand at exactly, else None (the player's own mix).
    pub fn of(now: &Cozy) -> Option<CozyLevel> {
        // By what each extra reads as (`Extras::get`): Playful following Animations counts as the value it follows.
        let same = |a: &Cozy, b: &Cozy| a.animations == b.animations && a.lived_in == b.lived_in && Extra::ALL.iter().all(|e| a.extras.get(*e) == b.extras.get(*e));
        CozyLevel::ALL.into_iter().find(|l| same(&l.apply(now.extras), now))
    }

    /// The level the page opens on: the one the switches stand at, else Cozy (as pocket-ui ships).
    pub fn start(now: &Cozy) -> CozyLevel {
        CozyLevel::of(now).unwrap_or(CozyLevel::Cozy)
    }
}

// ---- the page ---------------------------------------------------------------------------------------------------------

use crate::features::Hits;
use crate::gfx::Canvas;
use crate::touch::Area;
use crate::palette::{pal, MARGIN};
use crate::text::{Face, Text};

/// The page, `sel` the selected level: the title, its sentence, a row a level (name and line), and what the selected one
/// brings. Rows as "How do you like to play?" has them (`features::paint_profiles`), so the two read as one family.
pub fn paint(c: &mut Canvas, text: &mut Text, sel: usize) -> Hits {
    c.clear(pal().void);
    let mut hits = Hits { sel, ..Default::default() };
    let short = crate::kit::short_screen(c.h);
    // On a short screen the title stands under the first start's top right (L2 Aa and the dots, `Setup::paint_step_count`),
    // not beside it; every word whole in the column's full width, the sentence on two lines where it needs them.
    let (head, pitch, row_h) = if short && c.h < 768 { (120, 90, 82) } else if short { (128, 96, 86) } else { (140, 136, 118) };
    // Large text grows the name and the line: the row grows with them, so the name never touches the row's top.
    let extra = crate::type_scale::grow(46) / 2 + crate::type_scale::grow(24);
    let (pitch, row_h) = (pitch + extra, row_h + extra);
    let full = c.w - 2 * MARGIN;
    let (px, title) = text.fit_sizes(Face::Title, &[67, 60, 54, 46], TITLE, full);
    text.draw(c, Face::Title, px, MARGIN, head, pal().chalk, &title);
    let mut y = head + if short { 40 } else { 52 };
    for l in text.wrap_prose_at_most(Face::Body, 25, SENTENCE, full, 2) {
        text.draw(c, Face::Body, 25, MARGIN, y, pal().dust, &l);
        y += crate::type_scale::line(25, 8);
    }
    let first = y + if short { 6 } else { 24 };
    for (i, level) in CozyLevel::ALL.iter().enumerate() {
        let y = first + i as i32 * pitch;
        let selected = i == sel;
        c.rect(MARGIN, y, full, row_h, if selected { pal().label } else { pal().spine });
        if selected {
            crate::game_card::lift_shadow(c, MARGIN, y + row_h, full, pal().t);
        }
        let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
        text.draw(c, Face::Title, 46, MARGIN + 28, y + (row_h - extra) * 50 / 112 + extra / 2, fg, level.name());
        let (bpx, blurb) = text.fit_sizes(Face::Body, &[24, 22, 20], level.blurb(), full - 56);
        text.draw(c, Face::Body, bpx, MARGIN + 28, y + row_h * 90 / 112 + crate::type_scale::grow(24), dim, &blurb);
        hits.choices.push((Area::new(MARGIN, y, full, row_h), i));
        if selected {
            crate::nav_probe::focus(c, (MARGIN, y, full, row_h));
        }
    }
    // What the selected level brings, in three lines at most above the hint bar (Large text in German needs them).
    let level = CozyLevel::ALL[sel.min(2)];
    let mut y = first + 3 * pitch + if short { 18 } else { 30 };
    let bottom = crate::kit::hint_bar_top(c.h) - 8;
    for l in text.wrap_prose_at_most(Face::Body, 24, level.includes(), full, 3) {
        if y + crate::type_scale::line(24, 8) > bottom {
            break;
        }
        text.draw(c, Face::Body, 24, MARGIN, y + 24, pal().chalk, &l);
        y += crate::type_scale::line(24, 8);
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> Cozy {
        Cozy { animations: Animations::Cozy, lived_in: false, extras: Extras::default() }
    }

    #[test]
    fn each_level_sets_the_cozy_switches_together_and_is_read_back() {
        let plain = CozyLevel::Plain.apply(Extras::default());
        assert_eq!(plain.animations, Animations::Quick);
        assert!(!plain.lived_in);
        for e in SEEN_OR_HEARD {
            assert!(!plain.extras.get(e), "{e:?} off in Plain");
        }
        // What informs stays in every level.
        for e in [Extra::Connections, Extra::ProgressFromSaves, Extra::Rediscover] {
            for l in CozyLevel::ALL {
                assert!(l.apply(Extras::default()).extras.get(e), "{l:?} keeps {e:?}");
            }
        }
        let cozy = CozyLevel::Cozy.apply(Extras::default());
        assert_eq!(cozy.animations, Animations::Cozy);
        assert!(cozy.extras.get(Extra::EveningGlow) && cozy.extras.get(Extra::CrtOff) && !cozy.extras.get(Extra::TvPlaysGame) && !cozy.lived_in);
        let all = CozyLevel::Everything.apply(Extras::default());
        assert!(all.extras.get(Extra::TvPlaysGame) && all.extras.get(Extra::EveningGlow));
        // Alex's look at it (19.11.2026): the messy shelf is no level's, not even Everything's.
        for l in CozyLevel::ALL {
            assert!(!l.apply(Extras::default()).lived_in, "{l:?} leaves the lived-in shelf off");
        }
        for l in CozyLevel::ALL {
            assert_eq!(CozyLevel::of(&l.apply(Extras::default())), Some(l), "{l:?} is read back");
        }
    }

    /// A new device stands at Cozy (the page opens there); a mix of the player's own opens there too and is no level.
    /// The page says only what a level really switches (19.11.2026: Cozy still promised the Window after it was taken off
    /// for 0.6): no extra that isn't built is named or set, and every seen-or-heard extra that is built is one a level sets.
    #[test]
    fn the_levels_name_and_set_only_extras_that_are_there() {
        for e in Extra::ALL.into_iter().filter(|e| !e.built()) {
            assert!(!SEEN_OR_HEARD.contains(&e), "{e:?} is not built: no level sets it");
            let word = e.label().to_lowercase();
            for l in CozyLevel::ALL {
                for line in [l.blurb(), l.includes()] {
                    assert!(!line.to_lowercase().contains(&word), "{l:?} names {e:?}, which isn't there: {line}");
                }
            }
            // A choice made for it before stays as it was, whatever the level.
            let mut x = Extras::default();
            x.set(e, false);
            for l in CozyLevel::ALL {
                assert!(!l.apply(x).extras.get(e), "{l:?} leaves {e:?} alone");
            }
        }
        let informs = [Extra::Connections, Extra::Rediscover, Extra::ProgressFromSaves];
        for e in Extra::ALL.into_iter().filter(|e| e.built() && !informs.contains(e)) {
            assert!(SEEN_OR_HEARD.contains(&e), "{e:?} is seen or heard: the levels set it");
        }
    }

    #[test]
    fn a_new_device_is_cozy_and_a_mix_of_ones_own_opens_on_cozy() {
        assert_eq!(CozyLevel::of(&shipped()), Some(CozyLevel::Cozy));
        let mut mix = shipped();
        mix.extras.set(Extra::EveningGlow, false);
        assert_eq!(CozyLevel::of(&mix), None);
        assert_eq!(CozyLevel::start(&mix), CozyLevel::Cozy);
        let quick = Cozy { animations: Animations::Quick, ..shipped() };
        assert_eq!(CozyLevel::of(&quick), None, "Quick with every extra on is the player's own");
    }

    #[test]
    fn every_word_of_the_page_is_translated() {
        let mut words = vec![TITLE, SENTENCE];
        for l in CozyLevel::ALL {
            words.extend([l.name(), l.blurb(), l.includes()]);
        }
        for lang in crate::i18n::Lang::ALL.into_iter().filter(|l| *l != crate::i18n::Lang::En) {
            for w in &words {
                assert_ne!(&crate::i18n::tr_in(lang, w), w, "{lang:?}: {w}");
            }
        }
    }
}

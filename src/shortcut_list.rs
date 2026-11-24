//! The pause menu's list of HOME shortcuts in a RetroArch game (Options → Runs on → Shortcuts, `app_shortcut_list`):
//! what HOME and another button do there, as `ra_hotkeys` set RetroArch up for this game. Only what works is listed:
//! a shortcut switched off, rewind while RetroArch's Rewind is off, nothing of it in Kids Mode.
//!
//! pocket-core's games have the same list with their switches on Options → Shortcuts (`option_table`).

use crate::hotkeys::Enabled;

/// One line: what it does, the keys, a short explanation under the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub what: &'static str,
    pub keys: &'static str,
    pub desc: &'static str,
}

const fn line(what: &'static str, keys: &'static str, desc: &'static str) -> Line {
    Line { what, keys, desc }
}

/// The page's title.
pub const TITLE: &str = "Shortcuts";
/// The Runs on page's row that opens it, and its explanation.
pub const ROW_DESC: &str = "What HOME and another button do in this game.";

/// What the list is made from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Game {
    /// Options → Shortcuts of the game or its console, as `ra_hotkeys::enabled` gives them (Kids Mode and Easy applied).
    pub on: Enabled,
    /// RetroArch's Rewind (Runs on) is on for this game: holding HOME + L2 does something.
    pub rewind: bool,
    pub kids: bool,
    /// Easy: no game switcher (`app_switcher`).
    pub easy: bool,
}

/// The lines for a RetroArch game, in the order of pocket-core's Options → Shortcuts.
pub fn retroarch(g: Game) -> Vec<Line> {
    let mut v = vec![line("Pause menu", "HOME", "Tap HOME alone: this menu.")];
    if g.kids {
        return v;
    }
    if !g.easy {
        v.push(line("Recent games", "HOME + SELECT", "Your last games, to switch to another one."));
    }
    let on = g.on;
    let rows = [
        (on.save, line("Save state", "HOME + R1", "Saves a quick state right now. Use Load here to jump back.")),
        (on.load, line("Load state", "HOME + L1", "Loads your last save state.")),
        (on.fast_forward, line("Fast-forward", "HOME + R2", "Turns fast-forward on and off.")),
        (on.rewind && g.rewind, line("Rewind", "Hold HOME + L2", "Rewinds gameplay while held.")),
        (on.screenshot, line("Screenshot", "HOME + A", "A picture of the game, in Settings › Games › Screenshots.")),
        (on.over_game, line("Show FPS", "HOME + Y", "Shows RetroArch's frame rate in the corner, or hides it.")),
    ];
    v.extend(rows.into_iter().filter(|(on, _)| *on).map(|(_, l)| l));
    v.push(line("RetroArch's menu", "HOME + X", "RetroArch's own settings."));
    v.push(line("Quit", "HOME + START", "Press twice to quit. RetroArch saves your spot for Continue."));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        Game { on: Enabled::default(), rewind: true, kids: false, easy: false }
    }

    fn keys(v: &[Line]) -> Vec<&'static str> {
        v.iter().map(|l| l.keys).collect()
    }

    /// The keys `ra_hotkeys` gives RetroArch, and pocket-ui's own two (HOME, HOME + SELECT): every one listed.
    #[test]
    fn a_retroarch_game_lists_what_works_there() {
        assert_eq!(
            keys(&retroarch(game())),
            ["HOME", "HOME + SELECT", "HOME + R1", "HOME + L1", "HOME + R2", "Hold HOME + L2", "HOME + A", "HOME + Y", "HOME + X", "HOME + START"]
        );
    }

    /// Honest: a shortcut switched off isn't there, nor rewind while RetroArch's Rewind is off; no slots (one save) and
    /// no Zoom (pocket-core's picture) in RetroArch.
    #[test]
    fn what_is_off_or_missing_in_retroarch_is_not_listed() {
        let off = Game { on: Enabled { save: false, fast_forward: false, ..Enabled::default() }, rewind: false, ..game() };
        let k = keys(&retroarch(off));
        for gone in ["HOME + R1", "HOME + R2", "Hold HOME + L2"] {
            assert!(!k.contains(&gone), "{gone} is off: {k:?}");
        }
        assert!(k.contains(&"HOME + L1"));
        let all = keys(&retroarch(game()));
        assert!(!all.iter().any(|k| k.contains("Left") || k.contains("Right")), "no slots in RetroArch games");
        assert!(!retroarch(game()).iter().any(|l| l.what == "Zoom"));
    }

    #[test]
    fn kids_mode_and_easy() {
        assert_eq!(keys(&retroarch(Game { kids: true, ..game() })), ["HOME"]);
        assert!(!keys(&retroarch(Game { easy: true, ..game() })).contains(&"HOME + SELECT"), "Easy has no game switcher");
    }
}

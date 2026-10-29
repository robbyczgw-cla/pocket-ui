//! What the pause menu can honestly offer for saving, per emulator (docs/user/pause-menu.md, "Saving with RetroArch",
//! "Other emulators"). The menu has Save and Load only where they do something; elsewhere it says, in one calm line,
//! what is true instead, beside Continue, Options and Quit (`main.rs` `paint_menu`).
//!
//! | Emulator | Save and Load |
//! |---|---|
//! | pocket-core | eight slots, Auto, Before load |
//! | pocket-core for ScummVM, PICO-8, TIC-80 (`hostsys::HostSystem::no_states`) | none: the console's own line |
//! | ARMSX2, Dolphin, Azahar, once their state names are known (`standalone::states`) | slots (Azahar one) |
//! | hakuX (Xbox) while its core has no save states (`standalone::no_states_line`) | none: `hakux::NO_STATES` |
//! | ARMSX2 before it has named the game (PINE): nothing could be written | none: [`NOT_READY`] |
//! | RetroArch | its one save |
//! | PortMaster port | none: [`PORT_SAVES`] |
//! | any other emulator | none: [`OWN_SAVES`] |

use crate::quit::Saver;

/// Other emulators: pocket-ui can't save or continue them; Quit only closes them.
pub const OWN_SAVES: &str = "This emulator keeps its own saves. Save in the game before you quit.";
/// A PortMaster port: it keeps its own saves; Quit ends the port without asking it for one.
pub const PORT_SAVES: &str = "This game saves by itself. Save in the game before you quit.";
/// A PS2 game whose emulator hasn't said yet which game runs: a Save would have no file to go to, a Quit no
/// resume state to write.
pub const NOT_READY: &str = "Save and Load aren't ready yet. Save in the game before you quit.";
/// pocket-core with a system without save states that `hostsys` has no line for.
pub const NO_STATES: &str = "This game has no save states. Save in the game before you quit.";

/// What saving offers in the pause menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saves {
    /// pocket-ui's slots: Save, and Load with Auto and Before load (pocket-core, ARMSX2, Dolphin, Azahar).
    Slots,
    /// RetroArch's one save.
    One,
    /// No Save or Load: the emulator saves only inside the game. The line says so.
    InGame(&'static str),
}

impl Saves {
    /// For the running game's emulator. `states_known`: its state files have names (`standalone::states`; always
    /// for pocket-core); `no_states`: the system's own line when pocket-core runs it without states (`hostsys`).
    pub fn of(saver: Saver, states_known: bool, no_states: Option<&'static str>) -> Saves {
        match saver {
            Saver::PocketCore { states: true } => Saves::Slots,
            Saver::PocketCore { states: false } => Saves::InGame(no_states.unwrap_or(NO_STATES)),
            Saver::Standalone if states_known => Saves::Slots,
            // Xbox (hakuX) without save states says so; ARMSX2 before it has named the game says it isn't ready.
            Saver::Standalone => Saves::InGame(no_states.unwrap_or(NOT_READY)),
            Saver::RetroArch => Saves::One,
            Saver::Port => Saves::InGame(PORT_SAVES),
            Saver::Other => Saves::InGame(OWN_SAVES),
        }
    }

    /// Whether the menu lists Save and Load.
    pub fn rows(self) -> bool {
        !matches!(self, Saves::InGame(_))
    }

    /// The line the menu shows instead of Save and Load.
    pub fn line(self) -> Option<&'static str> {
        match self {
            Saves::InGame(line) => Some(line),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_only_where_they_do_something() {
        assert_eq!(Saves::of(Saver::PocketCore { states: true }, true, None), Saves::Slots);
        assert_eq!(Saves::of(Saver::Standalone, true, None), Saves::Slots);
        assert_eq!(Saves::of(Saver::RetroArch, false, None), Saves::One, "RetroArch's one save needs no state names of ours");
        // ScummVM: the menu's Save only said "no save states" over the game, Load was always greyed out.
        let scummvm = crate::hostsys::find("scummvm").and_then(|h| h.no_states);
        assert!(scummvm.is_some());
        assert_eq!(Saves::of(Saver::PocketCore { states: false }, true, scummvm), Saves::InGame(scummvm.unwrap()));
        // ARMSX2 before PINE named the game: Save went nowhere and Quit wrote no resume state.
        assert_eq!(Saves::of(Saver::Standalone, false, None), Saves::InGame(NOT_READY));
        assert_eq!(Saves::of(Saver::Other, true, None), Saves::InGame(OWN_SAVES));
        assert_eq!(Saves::of(Saver::Port, false, None), Saves::InGame(PORT_SAVES), "a port saves by itself, with or without state names");
        assert!(!Saves::of(Saver::Port, true, None).rows());
        for s in [Saves::Slots, Saves::One] {
            assert!(s.rows() && s.line().is_none());
        }
    }

    #[test]
    fn a_system_without_its_own_line_still_gets_one() {
        let s = Saves::of(Saver::PocketCore { states: false }, true, None);
        assert!(!s.rows());
        assert_eq!(s.line(), Some(NO_STATES));
    }
}

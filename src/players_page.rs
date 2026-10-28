//! The pause menu's Players page (pocket-core games, with a second controller connected): Player 1 to 4 and the
//! controller of each. A on a controller makes that controller the selected player, one press per controller: the
//! selection then moves on to the next player, so everyone presses A in turn. Painting and the reader of pocket-core's
//! seats are in `app_players.rs`; who is which player is decided in `players`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::pad::{Button, Controller};
use crate::players::{self, Named, SeatsView, MAX};

pub enum Action {
    Nothing,
    /// Back to the pause menu.
    Close,
    /// The seats changed: save them as the order and tell pocket-core (`order`).
    Assign,
}

pub struct PlayersPage {
    /// The selected player (0-based).
    pub sel: usize,
    /// What pocket-core said last (`players::SEATS_FILE`), or the change just made on this page.
    pub seats: Vec<Option<Named>>,
    /// Gamepads connected without a seat (a fifth one).
    pub spare: Vec<Named>,
    /// The reader thread runs while this is true; dropping the page stops it.
    pub alive: Arc<AtomicBool>,
}

impl Drop for PlayersPage {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

impl PlayersPage {
    pub fn new(view: SeatsView) -> PlayersPage {
        let mut p = PlayersPage { sel: 0, seats: vec![None; MAX], spare: Vec::new(), alive: Arc::new(AtomicBool::new(true)) };
        p.receive(view);
        p
    }

    /// pocket-core's seats: true when the page changes.
    pub fn receive(&mut self, mut view: SeatsView) -> bool {
        view.seats.resize(MAX, None);
        if view.seats == self.seats && view.spare == self.spare {
            return false;
        }
        (self.seats, self.spare) = (view.seats, view.spare);
        true
    }

    /// A button from controller `from`. A gives the selected player to `from`: whoever had it gets `from`'s old place
    /// (or none, for a controller that had none).
    pub fn press(&mut self, b: Button, from: &Controller, confirm: Button, cancel: Button) -> Action {
        match b {
            Button::Up => self.sel = (self.sel + MAX - 1) % MAX,
            Button::Down => self.sel = (self.sel + 1) % MAX,
            b if b == cancel => return Action::Close,
            b if b == confirm && players::seatable(from) => {
                let had = self.seats.iter().position(|s| s.as_ref().is_some_and(|n| n.key == from.key));
                let changed = had != Some(self.sel);
                if changed {
                    let me = Named { key: from.key.clone(), name: players::short_name(from) };
                    let there = self.seats[self.sel].replace(me);
                    match had {
                        Some(i) => self.seats[i] = there,
                        None => {
                            self.spare.retain(|n| n.key != from.key);
                            self.spare.extend(there);
                        }
                    }
                }
                // On to the next player, for the next controller's A.
                self.sel = (self.sel + 1).min(MAX - 1);
                return if changed { Action::Assign } else { Action::Nothing };
            }
            _ => {}
        }
        Action::Nothing
    }

    /// The seats as the order for `players.json` (`order_of`).
    pub fn order(&self, old: &[String]) -> Vec<String> {
        order_of(&self.seats, old)
    }

    /// A fingerprint for the menu sounds: a move or a change is one.
    pub fn mark(&self) -> (usize, Vec<Option<String>>) {
        (self.sel, self.seats.iter().map(|s| s.as_ref().map(|n| n.key.clone())).collect())
    }
}

/// Seats as the order for `players.json`: these players exactly, then the controllers `old` remembers that are not
/// seated now (they keep their turn for the next game), without trailing gaps.
pub fn order_of(seats: &[Option<Named>], old: &[String]) -> Vec<String> {
    let mut order: Vec<String> = seats.iter().map(|s| s.as_ref().map_or(String::new(), |n| n.key.clone())).collect();
    let rest: Vec<String> = old.iter().filter(|k| !k.is_empty() && !order.contains(k)).cloned().collect();
    order.extend(rest);
    while order.last().is_some_and(String::is_empty) {
        order.pop();
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::BUILTIN;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn named(key: &str, name: &str) -> Option<Named> {
        Some(Named { key: key.into(), name: name.into() })
    }

    fn pad(key: &str) -> Controller {
        Controller { key: key.into(), name: "Xbox Wireless Controller".into(), gamepad: true, node: String::new() }
    }

    fn page() -> PlayersPage {
        PlayersPage::new(SeatsView { seats: vec![named(BUILTIN, "Pocket"), named("bt:a", "DualSense")], spare: vec![] })
    }

    fn keys(p: &PlayersPage) -> Vec<String> {
        p.seats.iter().map(|s| s.as_ref().map_or(String::new(), |n| n.key.clone())).collect()
    }

    #[test]
    fn one_press_per_controller_in_player_order() {
        let mut p = page();
        assert_eq!(keys(&p), [BUILTIN, "bt:a", "", ""]);
        // The DualSense's owner wants to be player 1: A on it with Player 1 selected; the Pocket takes its place.
        assert!(matches!(p.press(A, &pad("bt:a"), A, B), Action::Assign));
        assert_eq!(keys(&p), ["bt:a", BUILTIN, "", ""]);
        assert_eq!(p.sel, 1, "on to Player 2");
        // The Pocket presses A on Player 2, its own already: nothing changes, the selection moves on.
        assert!(matches!(p.press(A, &Controller::builtin(), A, B), Action::Nothing));
        assert_eq!(p.sel, 2);
        // A third controller nobody seated yet takes Player 3.
        p.spare.push(Named { key: "bt:c".into(), name: "Xbox".into() });
        assert!(matches!(p.press(A, &pad("bt:c"), A, B), Action::Assign));
        assert_eq!(keys(&p), ["bt:a", BUILTIN, "bt:c", ""]);
        assert!(p.spare.is_empty());
        assert_eq!(p.seats[2].as_ref().unwrap().name, "Xbox");
        assert_eq!(p.sel, 3);
        assert!(matches!(p.press(A, &pad("bt:a"), A, B), Action::Assign), "and the last one, Player 4");
        assert_eq!(keys(&p), ["", BUILTIN, "bt:c", "bt:a"]);
        assert_eq!(p.sel, 3, "stays on the last player");
    }

    #[test]
    fn up_down_wrap_b_goes_back_and_keys_are_no_players() {
        let mut p = page();
        p.press(Button::Up, &Controller::builtin(), A, B);
        assert_eq!(p.sel, 3);
        p.press(Button::Down, &Controller::builtin(), A, B);
        assert_eq!(p.sel, 0);
        let headset = Controller { gamepad: false, ..pad("bt:h") };
        assert!(matches!(p.press(A, &headset, A, B), Action::Nothing));
        assert_eq!(keys(&p), [BUILTIN, "bt:a", "", ""]);
        assert!(matches!(p.press(B, &pad("bt:a"), A, B), Action::Close));
    }

    #[test]
    fn the_order_keeps_absent_controllers_after_the_players() {
        let mut p = page();
        p.press(A, &pad("bt:a"), A, B);
        let old = vec![BUILTIN.to_string(), "bt:z".into(), "".into(), "bt:a".into()];
        // Players 3 and 4 have nobody (an exact seat each), the absent one waits after them.
        assert_eq!(p.order(&old), ["bt:a", BUILTIN, "", "", "bt:z"]);
        p.seats[1] = None;
        assert_eq!(p.order(&[]), ["bt:a"]);
    }

    #[test]
    fn only_a_changed_view_repaints_and_dropping_stops_the_reader() {
        let mut p = page();
        assert!(!p.receive(SeatsView { seats: vec![named(BUILTIN, "Pocket"), named("bt:a", "DualSense"), None, None], spare: vec![] }));
        assert!(p.receive(SeatsView { seats: vec![named(BUILTIN, "Pocket")], spare: vec![] }));
        assert_eq!(p.seats.len(), MAX);
        let alive = p.alive.clone();
        drop(p);
        assert!(!alive.load(Ordering::Relaxed));
    }
}

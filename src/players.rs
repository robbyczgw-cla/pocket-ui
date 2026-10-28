//! Players of pocket-core games: which controller is player 1 to 4 (couch co-op with Bluetooth controllers), and the
//! input of each player as pocket-core reads it (`Inputs`). See docs/user/pause-menu.md#players and
//! docs/HOST.md ("Players").
//!
//! - **Order** (`<data>/players.json`, `{"order": [...]}`): controller keys (`pad::Controller::key`) in player order,
//!   as last chosen on the pause menu's Players page; `""` for a player left without a controller. Only pocket-ui
//!   writes it. Without one the Pocket's own controls are player 1.
//! - **Seats**: who is which player in the running game, kept by pocket-core. At the start everyone connected is
//!   seated by the order, closing gaps (a player whose controller is off is skipped), then the controllers the order
//!   doesn't name: the Pocket first, then in the order they connected. A controller that connects during the game
//!   gets the seat it had in this game if that is free, else the lowest free one; one that goes away leaves its seat
//!   empty (that player gets no input). The Players page's choice is taken exactly as chosen (`Mode::Exact`).
//! - pocket-core writes the seats to `SEATS_FILE` whenever they change; the Players page shows that file.
//!
//! Only gamepads (and the Pocket) are players: a headset's volume keys are a controller of their own but take no
//! seat. More than four gamepads: the fifth gets no player until a seat is free.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::controls::{self, Sticks};
use crate::pad::{Axis, Controller, Mux, PadEvent};
use crate::rumble;

/// Players pocket-core gives input to (libretro ports 0 to 3).
pub const MAX: usize = 4;
/// The order, in pocket-ui's data directory.
pub const FILE: &str = "players.json";
/// pocket-core's seats of the running game (`write_seats`), read by the Players page.
pub const SEATS_FILE: &str = "/tmp/pocket-core.players.json";
/// pocket-core's command FIFO: read the order again and seat everyone exactly by it (`Inputs::reorder`).
pub const COMMAND: &str = "players";

/// `<data>/players.json`.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Order {
    #[serde(default)]
    pub order: Vec<String>,
}

impl Order {
    pub fn load(data_dir: &Path) -> Order {
        std::fs::read(data_dir.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string(self).map_err(std::io::Error::other)?;
        crate::fsutil::write_atomic(&data_dir.join(FILE), format!("{json}\n").as_bytes())
    }
}

/// A controller can be a player: the Pocket, or a gamepad.
pub fn seatable(c: &Controller) -> bool {
    c.is_builtin() || c.gamepad
}

/// How `Seats::arrange` reads the order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// The start of a game: in order, gaps closed.
    Compact,
    /// The Players page's choice: everyone at the place given.
    Exact,
}

/// Who is which player in the running game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seats {
    seat: [Option<Controller>; MAX],
    /// The seat each controller had in this game, so one that drops out and comes back is the same player.
    had: Vec<(String, usize)>,
}

impl Seats {
    /// Everyone in `connected` (connection order) that can play, seated by `order`.
    pub fn arrange(order: &[String], connected: &[Controller], mode: Mode) -> Seats {
        let players: Vec<&Controller> = connected.iter().filter(|c| seatable(c)).collect();
        let mut s = Seats::default();
        let find = |k: &str| players.iter().find(|c| c.key == k).copied();
        let mut next = 0;
        for (i, key) in order.iter().enumerate() {
            let Some(c) = find(key).filter(|c| s.player_of(&c.key).is_none()) else { continue };
            let at = match mode {
                Mode::Exact if i < MAX => i,
                Mode::Exact => continue,
                Mode::Compact if next < MAX => next,
                Mode::Compact => break,
            };
            s.seat[at] = Some(c.clone());
            next += 1;
        }
        // The rest: the Pocket first, then in the order they connected.
        let mut rest: Vec<&Controller> = players.into_iter().filter(|c| s.player_of(&c.key).is_none()).collect();
        rest.sort_by_key(|c| !c.is_builtin());
        for c in rest {
            if let Some(free) = s.seat.iter().position(Option::is_none) {
                s.seat[free] = Some(c.clone());
            }
        }
        s.had = s.seat.iter().enumerate().filter_map(|(i, c)| c.as_ref().map(|c| (c.key.clone(), i))).collect();
        s
    }

    /// A controller (re)connected during the game: its player, None when it can't play or all seats are taken.
    pub fn connect(&mut self, c: &Controller) -> Option<usize> {
        if !seatable(c) {
            return None;
        }
        if let Some(at) = self.player_of(&c.key) {
            // Known already; a device of it may have come later (the Pocket's pad after its keys).
            self.seat[at] = Some(c.clone());
            return Some(at);
        }
        let before = self.had.iter().find(|(k, _)| *k == c.key).map(|(_, i)| *i).filter(|i| self.seat[*i].is_none());
        let at = before.or_else(|| self.seat.iter().position(Option::is_none))?;
        self.seat[at] = Some(c.clone());
        self.had.retain(|(k, _)| *k != c.key);
        self.had.push((c.key.clone(), at));
        Some(at)
    }

    /// A controller went away: the player it was; the seat stays empty.
    pub fn disconnect(&mut self, key: &str) -> Option<usize> {
        let at = self.player_of(key)?;
        self.seat[at] = None;
        Some(at)
    }

    /// 0-based player of a controller.
    pub fn player_of(&self, key: &str) -> Option<usize> {
        self.seat.iter().position(|c| c.as_ref().is_some_and(|c| c.key == key))
    }

    pub fn get(&self, player: usize) -> Option<&Controller> {
        self.seat.get(player).and_then(Option::as_ref)
    }

    /// The controller keys by player, as an order for `players.json` (`""`: nobody).
    pub fn keys(&self) -> Vec<String> {
        self.seat.iter().map(|c| c.as_ref().map_or(String::new(), |c| c.key.clone())).collect()
    }
}

/// What a controller is called on screen: "Pocket" for the built-in controls, else its own name without the maker
/// ("Sony Interactive Entertainment DualSense Wireless Controller" is "DualSense").
pub fn short_name(c: &Controller) -> String {
    if c.is_builtin() {
        return "Pocket".into();
    }
    let mut n = c.name.trim();
    for maker in ["Sony Interactive Entertainment ", "Sony Computer Entertainment ", "Microsoft ", "Nintendo ", "Nintendo Co., Ltd. "] {
        n = n.strip_prefix(maker).unwrap_or(n);
    }
    let n = n.strip_suffix(" Wireless Controller").filter(|s| !s.is_empty()).unwrap_or(n);
    if n.is_empty() { "Controller".into() } else { n.to_string() }
}

/// One seat as `SEATS_FILE` has it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Named {
    pub key: String,
    /// `short_name`.
    pub name: String,
}

/// `SEATS_FILE`: the seats of the running game, and controllers connected without a seat.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SeatsView {
    pub seats: Vec<Option<Named>>,
    #[serde(default)]
    pub spare: Vec<Named>,
}

impl SeatsView {
    pub fn of(seats: &Seats, connected: &[Controller]) -> SeatsView {
        let named = |c: &Controller| Named { key: c.key.clone(), name: short_name(c) };
        SeatsView {
            seats: (0..MAX).map(|i| seats.get(i).map(named)).collect(),
            spare: connected.iter().filter(|c| seatable(c) && seats.player_of(&c.key).is_none()).map(named).collect(),
        }
    }

    pub fn read(path: &Path) -> Option<SeatsView> {
        let mut v: SeatsView = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        v.seats.resize(MAX, None);
        Some(v)
    }

    pub fn write(&self, path: &Path) {
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = crate::fsutil::replace_file(path, &json);
        }
    }
}

/// The toast when a controller comes or goes during a game.
pub fn connected_note(c: &Controller, player: Option<usize>) -> String {
    match player {
        Some(p) => format!("{} connected — Player {}", short_name(c), p + 1),
        None => format!("{} connected — all four players are taken", short_name(c)),
    }
}

pub fn disconnected_note(c: &Controller, player: usize) -> String {
    format!("{} disconnected — Player {}", short_name(c), player + 1)
}

/// Where a player's rumble goes (`rumble::set_targets`): the Pocket's motor, the controller's own, or nowhere.
fn rumble_target(c: Option<&Controller>) -> rumble::Target {
    match c {
        Some(c) if c.is_builtin() => rumble::Target::Motor,
        Some(c) if !c.node.is_empty() => rumble::Target::Node(c.node.clone()),
        _ => rumble::Target::None,
    }
}

/// pocket-core's input: every device through one `Mux`, each player's buttons and sticks from the controller in its
/// seat. The pad threads feed it (`feed`), the game thread reads one atomic per player and frame (`buttons`,
/// `sticks`); the lock is only taken by the pad threads and for the rare roster changes.
pub struct Inputs {
    inner: Mutex<Inner>,
    /// RetroPad bits of each player, before the Controls page's mapping.
    ports: [AtomicU16; MAX],
    sticks: [Sticks; MAX],
    /// Player 1's buttons pressed by the test command `press` (POCKET_CORE_TEST).
    injected: AtomicU16,
    notes_waiting: AtomicBool,
    /// The player the Pocket's own controls are (0-based; `MAX`: none), for Screen orientation, which turns only
    /// the controls that turn with the Pocket (`is_pocket`).
    pocket_seat: AtomicU8,
}

struct Inner {
    mux: Mux,
    seats: Seats,
    order: Vec<String>,
    /// The first look at the devices is over (`arrange`): connections from now on are toasted.
    started: bool,
    /// The controllers as of the last roster change, to see who came and who went.
    known: Vec<Controller>,
    notes: Vec<String>,
    seats_file: Option<PathBuf>,
}

impl Inputs {
    /// `order` from `players.json`; `seats_file` is `SEATS_FILE` in pocket-core, None in tests.
    pub fn new(order: Vec<String>, seats_file: Option<PathBuf>) -> Inputs {
        Inputs {
            inner: Mutex::new(Inner { mux: Mux::default(), seats: Seats::default(), order, started: false, known: Vec::new(), notes: Vec::new(), seats_file }),
            ports: Default::default(),
            sticks: Default::default(),
            injected: AtomicU16::new(0),
            notes_waiting: AtomicBool::new(false),
            pocket_seat: AtomicU8::new(0),
        }
    }

    /// Whether `player` plays with the Pocket's own controls (player 1 until the seats are known).
    pub fn is_pocket(&self, player: usize) -> bool {
        self.pocket_seat.load(Ordering::Relaxed) as usize == player
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// One event of one device (a pad thread). `merged` gets what changed for all controllers together: the HOME
    /// shortcuts listen to every controller.
    pub fn feed(&self, e: PadEvent, mut merged: impl FnMut(&PadEvent)) {
        let mut g = self.lock();
        let roster = matches!(e, PadEvent::Connected { .. } | PadEvent::Disconnected { .. });
        let source = match e {
            PadEvent::Button { source, .. } | PadEvent::Axis { source, .. } | PadEvent::Raw { source, .. } => Some(source),
            _ => None,
        };
        for m in g.mux.feed(e) {
            merged(&m);
        }
        if roster {
            self.roster_changed(&mut g);
        } else if let Some(p) = source.and_then(|s| g.seats.player_of(g.mux.key_of(s))) {
            self.store(&g, p);
        }
    }

    /// The first look at the devices is done: everyone is seated by the order (`Mode::Compact`).
    pub fn arrange(&self) {
        let mut g = self.lock();
        let connected = g.mux.controllers();
        g.seats = Seats::arrange(&g.order, &connected, Mode::Compact);
        g.known = connected;
        g.started = true;
        self.seated(&g);
    }

    /// The Players page changed the order: everyone exactly where it says.
    pub fn reorder(&self, order: Vec<String>) {
        let mut g = self.lock();
        let connected = g.mux.controllers();
        g.seats = Seats::arrange(&order, &connected, Mode::Exact);
        g.order = order;
        g.known = connected;
        self.seated(&g);
    }

    fn roster_changed(&self, g: &mut Inner) {
        let now = g.mux.controllers();
        let started = g.started;
        for c in &now {
            let was = g.known.iter().find(|k| k.key == c.key);
            if was == Some(c) {
                continue;
            }
            let player = g.seats.connect(c);
            if started && was.is_none() && seatable(c) {
                g.notes.push(connected_note(c, player));
            }
        }
        for gone in g.known.iter().filter(|k| !now.iter().any(|c| c.key == k.key)).cloned().collect::<Vec<_>>() {
            if let Some(p) = g.seats.disconnect(&gone.key) {
                if started {
                    g.notes.push(disconnected_note(&gone, p));
                }
            }
        }
        g.known = now;
        if !g.notes.is_empty() {
            self.notes_waiting.store(true, Ordering::Relaxed);
        }
        self.seated(g);
    }

    /// After the seats changed: every player's input again, the rumble targets, the file for the Players page.
    fn seated(&self, g: &Inner) {
        let pocket = (0..MAX).find(|p| g.seats.get(*p).is_some_and(Controller::is_builtin)).unwrap_or(MAX);
        self.pocket_seat.store(pocket as u8, Ordering::Relaxed);
        for p in 0..MAX {
            self.store(g, p);
        }
        rumble::set_targets(std::array::from_fn(|p| rumble_target(g.seats.get(p))));
        if let Some(path) = &g.seats_file {
            SeatsView::of(&g.seats, &g.known).write(path);
        }
    }

    fn store(&self, g: &Inner, p: usize) {
        let (buttons, axes) = match g.seats.get(p) {
            Some(c) => (controls::retropad(g.mux.buttons_of(&c.key)), g.mux.axes_of(&c.key)),
            None => (0, [0.0; 6]),
        };
        self.ports[p].store(buttons, Ordering::Relaxed);
        for (a, v) in Axis::ALL.iter().zip(axes) {
            self.sticks[p].set(*a, v);
        }
    }

    /// RetroPad bits a player holds (before the Controls mapping).
    pub fn buttons(&self, player: usize) -> u16 {
        let b = self.ports.get(player).map_or(0, |p| p.load(Ordering::Relaxed));
        if player == 0 { b | self.injected.load(Ordering::Relaxed) } else { b }
    }

    pub fn sticks(&self, player: usize) -> &Sticks {
        &self.sticks[player.min(MAX - 1)]
    }

    /// Test aid (`press` on the FIFO): player 1's `bits` down or up.
    pub fn inject(&self, bits: u16, down: bool) {
        if down {
            self.injected.fetch_or(bits, Ordering::Relaxed);
        } else {
            self.injected.fetch_and(!bits, Ordering::Relaxed);
        }
    }

    /// One player holds all of `mask` (SELECT + START to quit: one controller, not two players' buttons together).
    pub fn one_player_holds(&self, mask: u16) -> bool {
        (0..MAX).any(|p| self.buttons(p) & mask == mask)
    }

    /// Toasts for controllers that came or went (`connected_note`), once.
    pub fn take_notes(&self) -> Vec<String> {
        if !self.notes_waiting.swap(false, Ordering::Relaxed) {
            return Vec::new();
        }
        std::mem::take(&mut self.lock().notes)
    }

    pub fn seats(&self) -> Seats {
        self.lock().seats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libretro::joypad;
    use crate::pad::{Button, BUILTIN};
    use crate::testutil::TestDir;

    fn pad(key: &str) -> Controller {
        Controller { key: key.into(), name: "Sony Interactive Entertainment DualSense Wireless Controller".into(), gamepad: true, node: format!("/dev/input/{key}") }
    }

    fn keys(s: &Seats) -> Vec<String> {
        s.keys()
    }

    fn order(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_pocket_is_player_one_unless_a_controller_was_put_first() {
        let connected = [pad("bt:a"), Controller::builtin(), pad("bt:b")];
        assert_eq!(keys(&Seats::arrange(&[], &connected, Mode::Compact)), ["builtin", "bt:a", "bt:b", ""]);
        // Promoted on the Players page: the Bluetooth pad first (docked to a TV).
        assert_eq!(keys(&Seats::arrange(&order(&["bt:b", BUILTIN]), &connected, Mode::Compact)), ["bt:b", "builtin", "bt:a", ""]);
        // A player whose controller is off is skipped at the start; the Pocket still comes before the unnamed ones.
        assert_eq!(keys(&Seats::arrange(&order(&["bt:x", "bt:a"]), &connected, Mode::Compact)), ["bt:a", "builtin", "bt:b", ""]);
    }

    #[test]
    fn exact_seats_keep_gaps_and_only_gamepads_play() {
        let headset = Controller { gamepad: false, ..pad("bt:h") };
        let connected = [Controller::builtin(), headset, pad("bt:a")];
        let s = Seats::arrange(&order(&["builtin", "", "bt:a"]), &connected, Mode::Exact);
        assert_eq!(keys(&s), ["builtin", "", "bt:a", ""]);
        assert_eq!(s.player_of("bt:h"), None, "a headset's keys are no player");
        let five: Vec<Controller> = ["a", "b", "c", "d", "e"].iter().map(|k| pad(k)).collect();
        let s = Seats::arrange(&[], &five, Mode::Compact);
        assert_eq!(keys(&s), ["a", "b", "c", "d"], "four players");
    }

    #[test]
    fn a_controller_that_comes_back_is_the_same_player_if_its_seat_is_free() {
        let mut s = Seats::arrange(&[], &[Controller::builtin(), pad("bt:a"), pad("bt:b")], Mode::Compact);
        assert_eq!(s.disconnect("bt:a"), Some(1));
        assert_eq!(keys(&s), ["builtin", "", "bt:b", ""], "player 3 stays player 3");
        assert_eq!(s.connect(&pad("bt:a")), Some(1), "back as player 2");
        assert_eq!(s.disconnect("bt:a"), Some(1));
        assert_eq!(s.connect(&pad("bt:c")), Some(1), "a new one takes the lowest free seat");
        assert_eq!(s.connect(&pad("bt:a")), Some(3), "its old seat is taken now");
        assert_eq!(s.connect(&pad("bt:d")), None, "all four taken");
        assert_eq!(s.connect(&pad("bt:c")), Some(1), "connecting again changes nothing");
        assert_eq!(s.disconnect("bt:zz"), None);
    }

    #[test]
    fn each_player_rumbles_their_own_controller() {
        assert_eq!(rumble_target(Some(&Controller::builtin())), rumble::Target::Motor);
        assert_eq!(rumble_target(Some(&pad("bt:a"))), rumble::Target::Node("/dev/input/bt:a".into()));
        assert_eq!(rumble_target(Some(&Controller { node: String::new(), ..pad("bt:b") })), rumble::Target::None);
        assert_eq!(rumble_target(None), rumble::Target::None);
    }

    #[test]
    fn short_names_drop_the_maker() {
        let named = |n: &str| short_name(&Controller { name: n.into(), ..pad("bt:x") });
        assert_eq!(named("Sony Interactive Entertainment DualSense Wireless Controller"), "DualSense");
        assert_eq!(named("Xbox Wireless Controller"), "Xbox");
        assert_eq!(named("Wireless Controller"), "Wireless Controller", "a DualShock 4 is called just that");
        assert_eq!(named("Pro Controller"), "Pro Controller");
        assert_eq!(named("8BitDo Ultimate 2C Wireless Controller"), "8BitDo Ultimate 2C");
        assert_eq!(named("  "), "Controller");
        assert_eq!(short_name(&Controller::builtin()), "Pocket");
    }

    /// Two controllers as pocket-core sees them: source 1 the Pocket's pad, source 2 a Bluetooth pad.
    fn two_players() -> Inputs {
        let i = Inputs::new(Vec::new(), None);
        i.feed(PadEvent::Connected { source: 1, controller: Controller { node: "/dev/input/event9".into(), ..Controller::builtin() } }, |_| {});
        i.feed(PadEvent::Connected { source: 2, controller: pad("bt:a") }, |_| {});
        i.arrange();
        i
    }

    fn button(source: usize, button: Button, pressed: bool) -> PadEvent {
        PadEvent::Button { source, button, pressed }
    }

    #[test]
    fn each_player_gets_only_their_own_controllers_buttons_and_sticks() {
        let i = two_players();
        i.feed(button(1, Button::East, true), |_| {});
        i.feed(button(2, Button::South, true), |_| {});
        i.feed(PadEvent::Axis { source: 2, axis: Axis::LeftX, value: -1.0 }, |_| {});
        assert_eq!(i.buttons(0), 1 << joypad::A);
        assert_eq!(i.buttons(1), 1 << joypad::B);
        assert_eq!(i.sticks(1).analog(true)[0], -32767);
        assert_eq!(i.sticks(0).analog(true)[0], 0);
        assert_eq!((i.buttons(2), i.buttons(3)), (0, 0));
        // The same button on both: each holds its own; one letting go leaves the other's down (SOL-09).
        i.feed(button(2, Button::East, true), |_| {});
        i.feed(button(2, Button::East, false), |_| {});
        assert_eq!(i.buttons(0), 1 << joypad::A);
        // Quitting takes SELECT + START on one controller.
        let quit = (1 << joypad::SELECT) | (1 << joypad::START);
        i.feed(button(1, Button::Select, true), |_| {});
        i.feed(button(2, Button::Start, true), |_| {});
        assert!(!i.one_player_holds(quit), "two players' buttons together are no quit");
        i.feed(button(1, Button::Start, true), |_| {});
        assert!(i.one_player_holds(quit));
    }

    #[test]
    fn a_controller_disconnecting_mid_press_lets_go_of_its_player_only_and_is_toasted() {
        let i = two_players();
        assert!(i.take_notes().is_empty(), "the controllers there at the start are not announced");
        i.feed(button(1, Button::Up, true), |_| {});
        i.feed(button(2, Button::Up, true), |_| {});
        i.feed(PadEvent::Axis { source: 2, axis: Axis::RightY, value: 0.9 }, |_| {});
        let mut merged = Vec::new();
        i.feed(PadEvent::Disconnected { source: 2 }, |e| merged.push(e.clone()));
        assert_eq!(i.buttons(0), 1 << joypad::UP, "player 1 still holds Up");
        assert_eq!((i.buttons(1), i.sticks(1).analog(true)[3]), (0, 0), "player 2 has let go");
        assert!(merged.iter().all(|e| !matches!(e, PadEvent::Button { button: Button::Up, .. })), "Up is still down for the shortcuts: {merged:?}");
        assert_eq!(i.take_notes(), ["DualSense disconnected — Player 2"]);
        // Hotplug during the game: back as player 2, a new one as player 3.
        i.feed(PadEvent::Connected { source: 3, controller: pad("bt:a") }, |_| {});
        i.feed(PadEvent::Connected { source: 4, controller: pad("bt:b") }, |_| {});
        assert_eq!(i.take_notes(), ["DualSense connected — Player 2", "DualSense connected — Player 3"]);
        assert!(i.take_notes().is_empty());
        i.feed(button(4, Button::West, true), |_| {});
        assert_eq!(i.buttons(2), 1 << joypad::Y);
    }

    #[test]
    fn the_players_page_reseats_exactly_and_the_pocket_can_be_any_player() {
        let i = two_players();
        i.feed(button(1, Button::North, true), |_| {});
        i.reorder(order(&["bt:a", "builtin"]));
        assert_eq!(keys(&i.seats()), ["bt:a", "builtin", "", ""]);
        assert_eq!(i.buttons(1), 1 << joypad::X, "the Pocket's held button moved with it to player 2");
        assert_eq!(i.buttons(0), 0);
        i.inject(1 << joypad::START, true);
        assert_eq!(i.buttons(0), 1 << joypad::START, "test presses are player 1's");
        i.inject(1 << joypad::START, false);
        assert_eq!(i.buttons(0), 0);
    }

    #[test]
    fn the_pocket_is_known_by_its_seat_wherever_it_sits() {
        let i = Inputs::new(Vec::new(), None);
        assert!(i.is_pocket(0), "player 1 before the seats are known");
        let i = two_players();
        assert!(i.is_pocket(0) && !i.is_pocket(1));
        i.reorder(order(&["bt:a", "builtin"]));
        assert!(!i.is_pocket(0) && i.is_pocket(1), "a turned Pocket turns player 2's controls then");
        i.reorder(order(&["builtin", "bt:a"]));
        assert!(i.is_pocket(0));
    }

    #[test]
    fn the_seats_file_names_who_plays_and_who_waits() {
        let dir = TestDir::new("players");
        let path = dir.join("seats.json");
        let i = Inputs::new(order(&["bt:a"]), Some(path.clone()));
        i.feed(PadEvent::Connected { source: 1, controller: Controller::builtin() }, |_| {});
        i.feed(PadEvent::Connected { source: 2, controller: pad("bt:a") }, |_| {});
        i.arrange();
        let v = SeatsView::read(&path).unwrap();
        assert_eq!(v.seats.len(), MAX);
        assert_eq!(v.seats[0].as_ref().map(|n| n.name.as_str()), Some("DualSense"));
        assert_eq!(v.seats[1].as_ref().map(|n| n.key.as_str()), Some("builtin"));
        assert!(v.seats[2].is_none() && v.spare.is_empty());
        // The order file round-trips.
        let o = Order { order: order(&["bt:a", "", "builtin"]) };
        o.save(&dir).unwrap();
        assert_eq!(Order::load(&dir), o);
        assert_eq!(Order::load(&dir.join("missing")), Order::default());
    }
}

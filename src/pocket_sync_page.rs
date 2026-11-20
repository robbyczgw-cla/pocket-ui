//! Settings > Connections > Pockets: the state of the page and what a press on it asks for (docs/SYNC-OWN.md, merge
//! 3). The App runs the service, pairs, syncs and forgets (`app_pocket_sync.rs`); painting is in the binary too.
//! Nothing here touches the system, so every path is a plain test.
//!
//! The list: the switch, "Pair a Pocket", then one row per paired Pocket (A syncs now, X forgets it after a second X).
//! Pairing: open "Pair a Pocket" on both Pockets; each shows its own code and lists the Pockets nearby that show one.
//! On one of them, choose the other and type its code with the D-pad.

use crate::pad::Button;

/// The page's switch, stored in `<data>/pocket-sync/prefs.json`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    pub enabled: bool,
}

impl Prefs {
    pub fn load(dir: &std::path::Path) -> Prefs {
        let v: serde_json::Value =
            std::fs::read(dir.join("prefs.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Prefs { enabled: v["enabled"].as_bool().unwrap_or(false) }
    }

    pub fn save(&self, dir: &std::path::Path) -> std::io::Result<()> {
        crate::fsutil::write_atomic(&dir.join("prefs.json"), serde_json::json!({"enabled": self.enabled}).to_string().as_bytes())
    }
}

/// A paired Pocket as the page lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    pub id: String,
    pub name: String,
    /// How it stands: "Synced · 3 min ago", "Not reachable since 14:05" and the like ([`status`]).
    pub status: String,
    /// The tick or the cross before it.
    pub mark: Mark,
}

/// The sign before a Pocket's line: a tick for synced, a cross for a problem, none for the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mark {
    #[default]
    None,
    Good,
    Bad,
}

/// A save that came or went, on a Pocket's own page: the game, and "From TrimUI Brick · 3 min ago".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub game: String,
    pub line: String,
}

/// A Pocket nearby that shows a code (found on the Wi-Fi while the pairing view is open).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nearby {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Switch,
    Pair,
    /// "What is synced": what goes between Pockets, what not yet, which Pockets can pair.
    What,
    /// A paired Pocket, by its place in [`PocketPage::pockets`].
    Pocket(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    List,
    /// This Pocket's code, and the Pockets nearby that show one (`sel` among them).
    Pairing { sel: usize },
    /// Typing the code of a Pocket nearby: six digits, `at` is the one Up/Down changes.
    Code { pocket: usize, digits: [u8; 6], at: usize },
    /// What is synced, what not, which Pockets (B or A goes back).
    What,
    /// A paired Pocket's own page: how it stands, A syncs now, the last saves that came and went ([`PocketPage::seen`]).
    Pocket(usize),
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Close,
    /// The switch: the service starts or stops (and the choice is stored).
    SetEnabled(bool),
    /// The pairing view opened: make a code and look for Pockets nearby.
    StartPairing,
    /// The pairing view closed: the code is no longer valid.
    StopPairing,
    /// Pair with the Pocket nearby `id` using `code`.
    Pair { id: String, code: String },
    SyncNow(String),
    Forget(String),
    /// A paired Pocket's page opened: the App fills in what came and went.
    Open(String),
}

/// The row line of a paired Pocket.
pub const IN_STEP: &str = "Synced";
pub const NEVER: &str = "Not synced yet";
pub const UNREACHABLE: &str = "Not reachable";
pub const BUSY_THERE: &str = "Playing a game";
pub const UNPAIRED: &str = "Pair again";
pub const SYNCING: &str = "Syncing…";

pub struct PocketPage {
    pub sel: usize,
    pub view: View,
    pub prefs: Prefs,
    pub pockets: Vec<Known>,
    pub nearby: Vec<Nearby>,
    /// This Pocket's code while the pairing view is open, and its seconds left.
    pub code: Option<String>,
    pub code_left: u64,
    /// The Pocket whose X was pressed once: a second X forgets it.
    pub forget: Option<usize>,
    /// The last thing that happened, shown under the list ("Paired with Brick.", "Wrong code.").
    pub line: Option<String>,
    /// Why the service does not listen although it is on.
    pub problem: Option<String>,
    /// The saves that came from and went to the Pocket whose page is open, newest first.
    pub seen: Vec<Seen>,
}

impl PocketPage {
    pub fn new(prefs: Prefs) -> PocketPage {
        PocketPage {
            sel: 0,
            view: View::List,
            prefs,
            pockets: Vec::new(),
            nearby: Vec::new(),
            code: None,
            code_left: 0,
            forget: None,
            line: None,
            problem: None,
            seen: Vec::new(),
        }
    }

    /// The list's rows: the switch, pairing, the paired Pockets.
    pub fn rows(&self) -> Vec<Row> {
        let mut r = vec![Row::Switch, Row::Pair, Row::What];
        r.extend((0..self.pockets.len()).map(Row::Pocket));
        r
    }

    pub fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.sel.min(rows.len() - 1)]
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        match self.view.clone() {
            View::List => self.press_list(b, confirm, cancel),
            View::What => {
                if b == cancel || b == confirm {
                    self.view = View::List;
                }
                Action::Nothing
            }
            View::Pocket(i) => self.press_pocket(i, b, confirm, cancel),
            View::Pairing { sel } => {
                let n = self.nearby.len();
                match b {
                    b if b == cancel => {
                        self.view = View::List;
                        self.code = None;
                        return Action::StopPairing;
                    }
                    Button::Up if n > 0 => self.view = View::Pairing { sel: (sel + n - 1) % n },
                    Button::Down if n > 0 => self.view = View::Pairing { sel: (sel + 1) % n },
                    b if b == confirm && n > 0 => self.view = View::Code { pocket: sel.min(n - 1), digits: [0; 6], at: 0 },
                    _ => {}
                }
                Action::Nothing
            }
            View::Code { pocket, mut digits, at } => {
                match b {
                    b if b == cancel => self.view = View::Pairing { sel: pocket },
                    Button::Up => digits[at] = (digits[at] + 1) % 10,
                    Button::Down => digits[at] = (digits[at] + 9) % 10,
                    Button::Left => {
                        self.view = View::Code { pocket, digits, at: (at + 5) % 6 };
                        return Action::Nothing;
                    }
                    Button::Right => {
                        self.view = View::Code { pocket, digits, at: (at + 1) % 6 };
                        return Action::Nothing;
                    }
                    b if b == confirm => {
                        let Some(p) = self.nearby.get(pocket) else { return Action::Nothing };
                        let code: String = digits.iter().map(|d| char::from(b'0' + d)).collect();
                        let id = p.id.clone();
                        self.view = View::Pairing { sel: pocket };
                        return Action::Pair { id, code };
                    }
                    _ => {}
                }
                if let View::Code { .. } = self.view {
                    self.view = View::Code { pocket, digits, at };
                }
                Action::Nothing
            }
        }
    }

    fn press_list(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = self.rows().len();
        let was_forget = self.forget.take();
        match b {
            Button::Up => self.sel = (self.sel + n - 1) % n,
            Button::Down => self.sel = (self.sel + 1) % n,
            b if b == cancel => return Action::Close,
            // X on a paired Pocket asks, a second X forgets it.
            Button::North => {
                if let Row::Pocket(i) = self.row() {
                    if was_forget == Some(i) {
                        let id = self.pockets.remove(i).id;
                        self.sel = self.sel.min(self.rows().len() - 1);
                        return Action::Forget(id);
                    }
                    self.forget = Some(i);
                }
            }
            b if b == confirm || b == Button::Left || b == Button::Right => {
                let only_toggle = b != confirm;
                match self.row() {
                    Row::Switch => {
                        self.prefs.enabled = !self.prefs.enabled;
                        return Action::SetEnabled(self.prefs.enabled);
                    }
                    Row::What if !only_toggle => self.view = View::What,
                    Row::Pair if !only_toggle && self.prefs.enabled => {
                        self.view = View::Pairing { sel: 0 };
                        self.line = None;
                        return Action::StartPairing;
                    }
                    Row::Pocket(i) if !only_toggle => {
                        self.view = View::Pocket(i);
                        self.seen.clear();
                        return Action::Open(self.pockets[i].id.clone());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Action::Nothing
    }

    /// On a Pocket's page: A syncs now (when on), X twice forgets it, B goes back to the list.
    fn press_pocket(&mut self, i: usize, b: Button, confirm: Button, cancel: Button) -> Action {
        let was_forget = self.forget.take();
        let Some(p) = self.pockets.get_mut(i) else {
            self.view = View::List;
            return Action::Nothing;
        };
        match b {
            b if b == cancel => self.view = View::List,
            b if b == confirm && self.prefs.enabled => {
                p.status = SYNCING.to_string();
                p.mark = Mark::None;
                return Action::SyncNow(p.id.clone());
            }
            Button::North if was_forget == Some(i) => {
                let id = self.pockets.remove(i).id;
                self.view = View::List;
                self.sel = self.sel.min(self.rows().len() - 1);
                return Action::Forget(id);
            }
            Button::North => self.forget = Some(i),
            _ => {}
        }
        Action::Nothing
    }

    pub fn label(&self, row: Row) -> String {
        match row {
            Row::Switch => "Sync with my Pockets".to_string(),
            Row::Pair => "Pair a Pocket".to_string(),
            Row::What => "What Pocket Sync syncs".to_string(),
            Row::Pocket(i) => self.pockets.get(i).map(|p| p.name.clone()).unwrap_or_default(),
        }
    }

    pub fn value(&self, row: Row) -> String {
        match row {
            Row::Switch => crate::web_page::on_off(self.prefs.enabled).to_string(),
            Row::Pair => if self.prefs.enabled { ">" } else { "" }.to_string(),
            Row::What => ">".to_string(),
            Row::Pocket(i) if self.forget == Some(i) => "X again: forget".to_string(),
            Row::Pocket(i) => self.pockets.get(i).map(|p| p.status.clone()).unwrap_or_default(),
        }
    }

    /// The line for the selected row, in the hint bar.
    pub fn describe(&self, row: Row) -> &'static str {
        match row {
            Row::Switch => "Saves go to your other Pockets after a game. No account.",
            Row::What => "What goes over, and what not yet.",
            Row::Pair if self.prefs.enabled => "Open it on both, then type one's code on the other.",
            Row::Pair | Row::Pocket(_) if !self.prefs.enabled => "Switch it on first.",
            Row::Pair => "",
            Row::Pocket(_) => "A: how it stands, its last saves. X: forget it.",
        }
    }

    /// The hint bar's buttons for the view and row.
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match &self.view {
            View::List => match self.row() {
                Row::Switch => vec![("B", "Back"), ("A", "Change")],
                Row::Pair if self.prefs.enabled => vec![("B", "Back"), ("A", "Open")],
                Row::What => vec![("B", "Back"), ("A", "Open")],
                Row::Pocket(_) => vec![("B", "Back"), ("X", "Forget"), ("A", "Open")],
                Row::Pair => vec![("B", "Back")],
            },
            View::Pairing { .. } if self.nearby.is_empty() => vec![("B", "Done")],
            View::Pairing { .. } => vec![("B", "Done"), ("A", "Type its code")],
            View::Code { .. } => vec![("B", "Back"), ("↑↓", "Digit"), ("←→", "Place"), ("A", "Pair")],
            View::What => vec![("B", "Back")],
            View::Pocket(_) if self.prefs.enabled => vec![("B", "Back"), ("X", "Forget"), ("A", "Sync now")],
            View::Pocket(_) => vec![("B", "Back"), ("X", "Forget")],
        }
    }
}

/// When, for a line about the past: "just now", "3 min ago", "yesterday" (`play_time::ago_from`).
fn ago(at: u64, now: u64) -> String {
    crate::play_time::ago_from(at, now)
}

/// The local day of a Unix time (`offset` seconds east of UTC).
fn day(t: u64, offset: i32) -> i64 {
    (t as i64 + offset as i64).div_euclid(86_400)
}

/// A paired Pocket's line and sign, from its log state (`pocket_sync::log`). `offset`: the local zone, for "since
/// 14:05". A sync running now says so over everything else.
pub fn status(st: &crate::pocket_sync::log::State, syncing: bool, now: u64, offset: i32) -> (Mark, String) {
    use crate::pocket_sync::log::Last;
    if syncing {
        return (Mark::None, SYNCING.into());
    }
    let ok = |what: &str| st.ok_at.map_or_else(|| what.to_string(), |t| format!("{what} · {}", ago(t, now)));
    match st.last {
        Last::Never => (Mark::None, NEVER.into()),
        Last::Synced => (Mark::Good, ok(IN_STEP)),
        Last::Asked => (Mark::None, "Two saves: choose".into()),
        Last::Failed(1) => (Mark::Bad, "1 save did not sync".into()),
        Last::Failed(n) => (Mark::Bad, format!("{n} saves did not sync")),
        Last::Busy => (Mark::None, BUSY_THERE.into()),
        Last::Unpaired => (Mark::Bad, UNPAIRED.into()),
        Last::Bad => (Mark::Bad, "Did not answer right".into()),
        Last::Unreachable => {
            let since = st.down_since.unwrap_or(now);
            let line = match day(now, offset) - day(since, offset) {
                0 => format!("Not reachable since {}", crate::timezone::clock_at(since as i64, offset)),
                1 => "Not reachable since yesterday".into(),
                n => format!("Not reachable for {n} days"),
            };
            (Mark::Bad, line)
        }
    }
}

/// A save on a Pocket's page: "From TrimUI Brick · 3 min ago" or "To TrimUI Brick · 14:05".
pub fn seen_line(dir: crate::pocket_sync::log::Dir, pocket: &str, at: u64, now: u64) -> String {
    match dir {
        crate::pocket_sync::log::Dir::From => format!("From {pocket} · {}", ago(at, now)),
        crate::pocket_sync::log::Dir::To => format!("To {pocket} · {}", ago(at, now)),
    }
}

/// The game card's Pocket Sync line ("Pocket Sync · " before it): "With TrimUI Brick at 14:05" the same day, "With TrimUI Brick, 3 days ago"
/// before.
pub fn card_line(pocket: &str, at: u64, now: u64, offset: i32) -> String {
    if day(now, offset) == day(at, offset) {
        format!("With {pocket} at {}", crate::timezone::clock_at(at as i64, offset))
    } else {
        format!("With {pocket}, {}", ago(at, now))
    }
}

/// The "What is synced" view: (heading, text), in order. Kept with the docs (docs/user/sync.md, "Your other Pockets").
pub const WHAT: [(&str, &str); 4] = [
    ("Synced", "The saves inside your games: battery saves, the cartridge clock and PS1 memory cards beside your games. The same game under another file name is found by its contents."),
    ("Not synced yet", "Continue moments and save states, play time, favourites, stickers and notes, saves of PS2, GameCube, Wii and 3DS, BIOS files."),
    ("Which Pockets", "Pockets with pocket-ui OS, pocket-ui on the TrimUI Brick and on dArkOS, and pocket-ui for Android, on the same Wi-Fi or in your Tailscale."),
    ("Each Pocket", "A paired Pocket is listed by its model. Nothing is overwritten when you played on both: you choose."),
];

/// "Which Pockets" on a handheld without Wi-Fi (`device::no_wifi_chip`): no network to name, and why (docs/DEVICES.md).
pub const WHICH_NO_WIFI: &str = "Pockets with pocket-ui OS, pocket-ui on the TrimUI Brick and on dArkOS, and pocket-ui for Android.";

/// The "What Pocket Sync syncs" paragraphs for this handheld, under their headings: without Wi-Fi, "Which Pockets" names
/// no Wi-Fi to be on, and a paragraph of its own says it needs a network (`device::NEEDS_NETWORK`, translated as it is).
pub fn what(no_wifi: bool) -> Vec<(&'static str, Vec<&'static str>)> {
    WHAT.iter()
        .map(|&(h, t)| match (h, no_wifi) {
            ("Which Pockets", true) => (h, vec![WHICH_NO_WIFI, crate::device::NEEDS_NETWORK]),
            _ => (h, vec![t]),
        })
        .collect()
}

/// Why a sync with every paired Pocket ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// A game ended: this Pocket gives what it saved.
    AfterGame,
    /// The service started, or the Pocket woke: this Pocket fetches what others saved meanwhile.
    Start,
}

/// What one Pocket's sync did, as the shelf line needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub name: String,
    pub sent: usize,
    pub fetched: usize,
    pub asked: usize,
    /// The games of the saves that came, then of those that went (their names on the shelf).
    pub games: Vec<String>,
    /// It answered, but saves did not go or come; or it answered wrong / no longer knows this Pocket.
    pub failed: usize,
    pub fault: Option<Fault>,
}

/// A sync that went wrong in a way the player should hear about (not reachable or a game there is normal, not this).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Unpaired,
    Bad,
}

/// The one line on the shelf after the syncs with every paired Pocket, or none: a save that came is always told (and
/// from where), a save that went only after a game. Nothing for no change, or when no Pocket answered (a game must
/// never be followed by a complaint about a Pocket that is asleep). A question is told by the look for saves with
/// two versions.
pub fn shelf_line(why: Why, done: &[Outcome]) -> Option<String> {
    // A problem first: never silent (a Pocket asleep is not one).
    if let Some(o) = done.iter().find(|o| o.fault.is_some() || o.failed > 0) {
        return Some(match (o.fault, o.failed) {
            (Some(Fault::Unpaired), _) => format!("Not synced with {}: pair again", o.name),
            (Some(Fault::Bad), _) => format!("Not synced with {}: it did not answer right", o.name),
            (None, 1) => format!("1 save not synced with {}", o.name),
            (None, n) => format!("{n} saves not synced with {}", o.name),
        });
    }
    let names = |f: &dyn Fn(&Outcome) -> bool| done.iter().filter(|o| f(o)).map(|o| o.name.clone()).collect::<Vec<_>>();
    let from = names(&|o| o.fetched > 0);
    if let Some(first) = from.first() {
        let n: usize = done.iter().map(|o| o.fetched).sum();
        let game = done.iter().find(|o| o.fetched > 0).and_then(|o| o.games.first());
        return Some(match (n, game) {
            (1, Some(g)) => format!("New save from {first} · {g}"),
            (1, None) => format!("New save from {first}."),
            _ => format!("{n} new saves from {}.", from.join(", ")),
        });
    }
    let to = names(&|o| o.sent > 0);
    let sent: usize = done.iter().map(|o| o.sent).sum();
    let game = done.iter().find(|o| o.sent > 0).and_then(|o| o.games.first());
    match (why, to.is_empty(), game) {
        (Why::AfterGame, false, Some(g)) if sent == 1 || done.iter().filter(|o| o.sent > 0).all(|o| o.sent == 1) => {
            Some(format!("Save given to {} · {g}", to.join(", ")))
        }
        (Why::AfterGame, false, _) => Some(format!("Save given to {}.", to.join(", "))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn page(on: bool) -> PocketPage {
        let mut p = PocketPage::new(Prefs { enabled: on });
        p.pockets = vec![
            Known { id: "P1".into(), name: "RG35XX Plus".into(), status: IN_STEP.into(), mark: Mark::Good },
            Known { id: "P2".into(), name: "Brick".into(), status: UNREACHABLE.into(), mark: Mark::Bad },
        ];
        p
    }

    #[test]
    fn the_switch_and_its_rows() {
        let mut p = page(false);
        assert_eq!(p.rows(), vec![Row::Switch, Row::Pair, Row::What, Row::Pocket(0), Row::Pocket(1)]);
        assert_eq!(p.press(A, A, B), Action::SetEnabled(true));
        assert_eq!(p.press(Button::Right, A, B), Action::SetEnabled(false));
        // Off: pairing and syncing do nothing, and the hint bar offers no A there.
        p.sel = 1;
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.hints(), vec![("B", "Back")]);
        // A Pocket's page opens while off too (how it stood, what came), but A there syncs nothing.
        p.sel = 3;
        assert_eq!(p.press(A, A, B), Action::Open("P1".into()));
        assert_eq!(p.view, View::Pocket(0));
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert!(!p.hints().contains(&("A", "Sync now")));
        p.press(B, A, B);
        assert_eq!(p.view, View::List);
        assert_eq!(p.press(B, A, B), Action::Close);
    }

    #[test]
    fn what_is_synced_opens_also_while_off_and_says_it_plainly() {
        let mut p = page(false);
        p.sel = 2;
        assert_eq!(p.row(), Row::What);
        assert_eq!(p.hints(), vec![("B", "Back"), ("A", "Open")]);
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.view, View::What);
        assert_eq!(p.hints(), vec![("B", "Back")]);
        assert_eq!(p.press(B, A, B), Action::Nothing);
        assert_eq!(p.view, View::List);
        // What it says: what is synced, what not yet, which Pockets.
        let all: String = WHAT.iter().map(|(h, t)| format!("{h} {t} ")).collect();
        for word in ["battery saves", "Continue", "play time", "favourites", "pocket-ui OS", "TrimUI Brick", "dArkOS", "Android"] {
            assert!(all.contains(word), "{word}");
        }
    }

    #[test]
    fn a_paired_pocket_opens_with_a_syncs_there_and_is_forgotten_with_x_twice() {
        let mut p = page(true);
        p.sel = 4;
        assert_eq!(p.hints(), vec![("B", "Back"), ("X", "Forget"), ("A", "Open")]);
        assert_eq!(p.press(A, A, B), Action::Open("P2".into()));
        assert_eq!(p.view, View::Pocket(1));
        assert_eq!(p.hints(), vec![("B", "Back"), ("X", "Forget"), ("A", "Sync now")]);
        assert_eq!(p.press(A, A, B), Action::SyncNow("P2".into()));
        assert_eq!(p.value(Row::Pocket(1)), SYNCING);
        assert_eq!(p.pockets[1].mark, Mark::None, "no cross while it syncs");
        // X twice on its page forgets it, back to the list.
        assert_eq!(p.press(Button::North, A, B), Action::Nothing);
        assert_eq!(p.forget, Some(1));
        assert_eq!(p.press(Button::North, A, B), Action::Forget("P2".into()));
        assert_eq!(p.view, View::List);
        assert_eq!(p.pockets.len(), 1);
        assert_eq!(p.sel, 3, "the selection stays on a row that exists");
        // In the list X twice forgets too.
        let mut p = page(true);
        p.sel = 4;
        assert_eq!(p.press(Button::North, A, B), Action::Nothing);
        assert_eq!(p.value(Row::Pocket(1)), "X again: forget");
        // Anything else in between: no longer asked.
        p.press(Button::Up, A, B);
        p.press(Button::Down, A, B);
        assert_eq!(p.press(Button::North, A, B), Action::Nothing);
        assert_eq!(p.press(Button::North, A, B), Action::Forget("P2".into()));
        assert_eq!(p.pockets.len(), 1);
        assert_eq!(p.sel, 3, "the selection stays on a row that exists");
    }

    #[test]
    fn pairing_lists_pockets_nearby_and_types_a_code_with_the_dpad() {
        let mut p = page(true);
        p.sel = 1;
        assert_eq!(p.press(A, A, B), Action::StartPairing);
        assert_eq!(p.view, View::Pairing { sel: 0 });
        // Nobody nearby yet: A does nothing, B closes.
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.hints(), vec![("B", "Done")]);
        p.nearby = vec![Nearby { id: "N1".into(), name: "Brick".into() }, Nearby { id: "N2".into(), name: "pocket".into() }];
        p.press(Button::Down, A, B);
        p.press(A, A, B);
        assert_eq!(p.view, View::Code { pocket: 1, digits: [0; 6], at: 0 });
        // 8 5 4 3 1 1: up 8 times is down twice; then the next places.
        for (i, d) in [8u8, 5, 4, 3, 1, 1].iter().enumerate() {
            if *d >= 5 {
                for _ in 0..(10 - d) {
                    p.press(Button::Down, A, B);
                }
            } else {
                for _ in 0..*d {
                    p.press(Button::Up, A, B);
                }
            }
            if i < 5 {
                p.press(Button::Right, A, B);
            }
        }
        assert_eq!(p.press(A, A, B), Action::Pair { id: "N2".into(), code: "854311".into() });
        assert_eq!(p.view, View::Pairing { sel: 1 });
        // Left wraps to the last place; B goes back to the list of Pockets nearby, B again closes.
        p.press(A, A, B);
        p.press(Button::Left, A, B);
        assert!(matches!(p.view, View::Code { at: 5, .. }));
        p.press(B, A, B);
        assert_eq!(p.press(B, A, B), Action::StopPairing);
        assert_eq!(p.view, View::List);
    }

    fn o(name: &str, sent: usize, fetched: usize) -> Outcome {
        Outcome { name: name.into(), sent, fetched, ..Outcome::default() }
    }

    fn og(name: &str, sent: usize, fetched: usize, games: &[&str]) -> Outcome {
        Outcome { games: games.iter().map(|g| g.to_string()).collect(), ..o(name, sent, fetched) }
    }

    #[test]
    fn the_shelf_line_names_the_game_and_never_keeps_a_problem_quiet() {
        assert_eq!(shelf_line(Why::Start, &[og("RG35XX Plus", 0, 1, &["Pokémon Gold"])]).as_deref(), Some("New save from RG35XX Plus · Pokémon Gold"));
        assert_eq!(shelf_line(Why::AfterGame, &[og("Brick", 1, 0, &["Golden Sun"])]).as_deref(), Some("Save given to Brick · Golden Sun"));
        assert_eq!(
            shelf_line(Why::AfterGame, &[og("Brick", 1, 0, &["Golden Sun"]), og("pocket", 1, 0, &["Golden Sun"])]).as_deref(),
            Some("Save given to Brick, pocket · Golden Sun")
        );
        assert_eq!(shelf_line(Why::AfterGame, &[og("Brick", 2, 0, &["Golden Sun", "Mother 3"])]).as_deref(), Some("Save given to Brick."));
        // Problems, after a game and after a wake alike; a Pocket asleep is none (it is not in the list at all).
        let f = |fault, failed| Outcome { fault, failed, ..o("Brick", 0, 0) };
        assert_eq!(shelf_line(Why::AfterGame, &[f(Some(Fault::Unpaired), 0)]).as_deref(), Some("Not synced with Brick: pair again"));
        assert_eq!(shelf_line(Why::Start, &[f(Some(Fault::Bad), 0)]).as_deref(), Some("Not synced with Brick: it did not answer right"));
        assert_eq!(shelf_line(Why::Start, &[f(None, 1)]).as_deref(), Some("1 save not synced with Brick"));
        assert_eq!(shelf_line(Why::AfterGame, &[og("pocket", 1, 0, &["Golden Sun"]), f(None, 3)]).as_deref(), Some("3 saves not synced with Brick"));
    }

    #[test]
    fn every_state_of_a_paired_pocket_has_its_line_and_sign() {
        use crate::pocket_sync::log::{Last, State};
        // 13:05 UTC; the zone is UTC here.
        let now = 1_763_557_500;
        let st = |last, ok_at, down_since| status(&State { last, ok_at, down_since }, false, now, 0);
        assert_eq!(st(Last::Never, None, None), (Mark::None, NEVER.into()));
        assert_eq!(st(Last::Synced, Some(now - 180), None), (Mark::Good, "Synced · 3 min ago".into()));
        assert_eq!(st(Last::Synced, Some(now - 20), None), (Mark::Good, "Synced · just now".into()));
        assert_eq!(st(Last::Asked, Some(now), None), (Mark::None, "Two saves: choose".into()));
        assert_eq!(st(Last::Failed(1), Some(now), None), (Mark::Bad, "1 save did not sync".into()));
        assert_eq!(st(Last::Failed(4), Some(now), None), (Mark::Bad, "4 saves did not sync".into()));
        assert_eq!(st(Last::Busy, None, None), (Mark::None, BUSY_THERE.into()));
        assert_eq!(st(Last::Unpaired, None, None), (Mark::Bad, UNPAIRED.into()));
        assert_eq!(st(Last::Bad, None, None), (Mark::Bad, "Did not answer right".into()));
        assert_eq!(st(Last::Unreachable, None, Some(now - 3600)), (Mark::Bad, "Not reachable since 12:05".into()));
        assert_eq!(st(Last::Unreachable, None, Some(now - 86_400)), (Mark::Bad, "Not reachable since yesterday".into()));
        assert_eq!(st(Last::Unreachable, None, Some(now - 4 * 86_400)), (Mark::Bad, "Not reachable for 4 days".into()));
        // In another zone the clock is its own: 14:05 in UTC+2 is 12:05 UTC.
        assert_eq!(status(&State { last: Last::Unreachable, ok_at: None, down_since: Some(now - 3600) }, false, now, 7200).1, "Not reachable since 14:05");
        // A sync running says so over everything.
        assert_eq!(status(&State { last: Last::Bad, ok_at: None, down_since: None }, true, now, 0), (Mark::None, SYNCING.into()));
        // The Pocket's page and the card.
        use crate::pocket_sync::log::Dir;
        assert_eq!(seen_line(Dir::From, "Brick", now - 180, now), "From Brick · 3 min ago");
        assert_eq!(seen_line(Dir::To, "Brick", now - 2 * 86_400 - 10, now), "To Brick · 2 days ago");
        assert_eq!(card_line("Brick", now - 600, now, 0), "With Brick at 12:55");
        assert_eq!(card_line("Brick", now - 3 * 86_400, now, 0), "With Brick, 3 days ago");
    }

    #[test]
    fn the_shelf_line_after_a_game_and_after_a_wake() {
        assert_eq!(shelf_line(Why::AfterGame, &[o("RG35XX Plus", 1, 0), o("Brick", 1, 0)]).as_deref(), Some("Save given to RG35XX Plus, Brick."));
        assert_eq!(shelf_line(Why::Start, &[o("RG35XX Plus", 1, 0)]), None, "after a wake only what came is told");
        assert_eq!(shelf_line(Why::Start, &[o("Brick", 0, 1)]).as_deref(), Some("New save from Brick."));
        assert_eq!(shelf_line(Why::AfterGame, &[o("Brick", 0, 2), o("pocket", 1, 1)]).as_deref(), Some("3 new saves from Brick, pocket."));
        assert_eq!(shelf_line(Why::AfterGame, &[o("Brick", 0, 0)]), None, "nothing changed, nothing said");
        assert_eq!(shelf_line(Why::AfterGame, &[]), None, "nobody answered: no complaint after a game");
    }

    /// Every status line, the Pocket page's lines, the card's line and the shelf lines are in every language (made of
    /// parts: the painter translates each, a game's name stays).
    #[test]
    fn every_new_line_is_translated_in_every_language() {
        use crate::i18n::{learn_names, tr_in, Lang};
        use crate::pocket_sync::log::{Dir, Last, State};
        learn_names(["Golden Sun".to_string()]);
        let now = 1_763_557_500;
        let mut lines: Vec<String> = [Last::Synced, Last::Asked, Last::Failed(1), Last::Failed(3), Last::Busy, Last::Unpaired, Last::Bad, Last::Never]
            .iter()
            .map(|&last| status(&State { last, ok_at: Some(now - 300), down_since: None }, false, now, 0).1)
            .collect();
        for since in [now - 60, now - 86_400, now - 5 * 86_400] {
            lines.push(status(&State { last: Last::Unreachable, ok_at: None, down_since: Some(since) }, false, now, 0).1);
        }
        lines.push(SYNCING.into());
        lines.extend([seen_line(Dir::From, "Brick", now - 200, now), seen_line(Dir::To, "Brick", now - 9_000, now)]);
        lines.extend([card_line("Brick", now - 200, now, 0), card_line("Brick", now - 3 * 86_400, now, 0)]);
        let g = |sent, fetched| Outcome { name: "Brick".into(), sent, fetched, games: vec!["Golden Sun".into()], ..Outcome::default() };
        lines.extend(shelf_line(Why::Start, &[g(0, 1)]));
        lines.extend(shelf_line(Why::AfterGame, &[g(1, 0)]));
        for (fault, failed) in [(Some(Fault::Unpaired), 0), (Some(Fault::Bad), 0), (None, 1), (None, 2)] {
            lines.extend(shelf_line(Why::Start, &[Outcome { fault, failed, ..g(0, 0) }]));
        }
        let page = page(true);
        lines.push(page.describe(Row::Pocket(0)).into());
        lines.extend(["Last saves", "None yet. A syncs now.", "None yet. Switch Pocket Sync on first.", "Synced"].map(String::from));
        lines.extend(WHAT.iter().flat_map(|(h, t)| [h.to_string(), t.to_string()]));
        assert!(lines.len() > 30);
        for lang in [Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It] {
            for l in &lines {
                let t = tr_in(lang, l);
                assert_ne!(&t, l, "{lang:?}: {l}");
                assert!(!t.contains('{'), "{lang:?}: {t}");
                if l.contains("Golden Sun") {
                    assert!(t.contains("Golden Sun"), "{lang:?}: the game's name stays: {t}");
                }
            }
        }
        assert_eq!(tr_in(Lang::De, &card_line("TrimUI Brick", now - 600, now, 0)), "Mit TrimUI Brick um 12:55");
        assert_eq!(tr_in(Lang::De, &shelf_line(Why::Start, &[g(0, 1)]).unwrap()), "Neuer Spielstand von Brick · Golden Sun");
        assert_eq!(tr_in(Lang::De, &status(&State { last: Last::Synced, ok_at: Some(now - 180), down_since: None }, false, now, 0).1), "Abgeglichen · vor 3 min");
    }

    /// A handheld without Wi-Fi (RG28XX, R36S): "Which Pockets" names no Wi-Fi to be on and says it needs a network
    /// (docs/DEVICES.md; tests/no_wifi_walk.rs paints the pages).
    #[test]
    fn without_wifi_the_page_names_no_wifi_to_join() {
        let all = |no_wifi| what(no_wifi).into_iter().flat_map(|(h, ts)| std::iter::once(h).chain(ts)).collect::<Vec<_>>().join(" ");
        assert!(all(false).contains("same Wi-Fi"));
        let without = all(true);
        assert!(!without.contains("same Wi-Fi") && !without.contains("Tailscale"), "{without}");
        assert!(without.contains(crate::device::NEEDS_NETWORK) && without.contains("pocket-ui for Android"), "{without}");
        for lang in [crate::i18n::Lang::De, crate::i18n::Lang::Es, crate::i18n::Lang::Pt, crate::i18n::Lang::Fr, crate::i18n::Lang::It] {
            assert_ne!(crate::i18n::tr_in(lang, WHICH_NO_WIFI), WHICH_NO_WIFI, "{lang:?}");
        }
    }

    /// The D-pad reaches every row and view of the page from the list, and B always leads back (the explorer of the
    /// real binary has no paired Pocket on its fresh card, so the Pocket's page is walked here).
    #[test]
    fn the_dpad_reaches_every_view_and_b_always_leads_back() {
        let mut seen = std::collections::HashSet::new();
        let mut p = page(true);
        p.nearby = vec![Nearby { id: "N1".into(), name: "Brick".into() }];
        let n = p.rows().len();
        for i in 0..n {
            assert_eq!(p.sel, i);
            seen.insert(format!("{:?}", p.row()));
            if !matches!(p.row(), Row::Switch) {
                p.press(A, A, B);
                seen.insert(format!("{:?}", p.view));
                if p.view == (View::Pairing { sel: 0 }) {
                    p.press(A, A, B);
                    seen.insert(format!("{:?}", std::mem::discriminant(&p.view)));
                    assert!(matches!(p.view, View::Code { .. }));
                    p.press(B, A, B);
                }
                p.press(B, A, B);
                assert_eq!(p.view, View::List, "B leads back from {:?}", p.row());
                assert_eq!(p.sel, i, "back on the same row");
            }
            p.press(Button::Down, A, B);
        }
        assert_eq!(p.sel, 0, "the list wraps");
        for v in ["What", "Pocket(0)", "Pocket(1)", "Pairing { sel: 0 }"] {
            assert!(seen.contains(v), "{v} not reached: {seen:?}");
        }
    }

    // The file-fault rule (tests/data_sources.rs, scripts/reader-inventory.py): the switch's file missing, empty, cut,
    // flipped or in another encoding never panics and reads as Off unless it says On.
    #[test]
    fn file_source_rule_pocket_sync_prefs() {
        let d = crate::testutil::TestDir::new("pocket-sync-prefs-faults");
        Prefs { enabled: true }.save(&d).unwrap();
        let seed = std::fs::read(d.join("prefs.json")).unwrap();
        for (label, bytes) in crate::file_faults::faults(&seed) {
            let _ = std::fs::remove_file(d.join("prefs.json"));
            if let Some(b) = &bytes {
                std::fs::write(d.join("prefs.json"), b).unwrap();
            }
            let p = Prefs::load(&d);
            if label == "valid" {
                assert!(p.enabled);
            }
        }
    }

    #[test]
    fn the_switch_is_stored() {
        let d = crate::testutil::TestDir::new("pocket-sync-prefs");
        assert_eq!(Prefs::load(&d), Prefs { enabled: false });
        Prefs { enabled: true }.save(&d).unwrap();
        assert_eq!(Prefs::load(&d), Prefs { enabled: true });
    }
}

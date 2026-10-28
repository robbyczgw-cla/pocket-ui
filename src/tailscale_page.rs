//! Settings > Tailscale: the state of the page and what a press on it asks for. The App turns an `Action` into work
//! on a thread (`tailscale::spawn`) and reports back with an `Event`; painting is in the binary (`app_tailscale.rs`).
//! Nothing here touches the system, so every path is a plain test.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::keyboard::{Keyboard, Outcome};
use crate::pad::Button;
use crate::tailscale::{clean_hostname, Backend, Snapshot, State, NO_NETWORK, SUGGESTED_HOSTNAME};

/// A row of the page, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    /// Tailscale on or off (the service, and `tailscale.up` for the next boot).
    Switch,
    /// Sign in with a QR code; once signed in, log out.
    Account,
    /// Sign in with an auth key typed on the gamepad keyboard.
    AuthKey,
    /// The name of this device in the tailnet.
    Hostname,
}

const NOT_INSTALLED: &str = "Tailscale is not in this system image";
/// Said (calmly, not as an error) when a press needs the network and there is none.
const SIGN_IN_NEEDS_NETWORK: &str = "Signing in needs a network. Connect to Wi-Fi first.";
const LOG_OUT_NEEDS_NETWORK: &str = "Logging out needs a network. Connect to Wi-Fi first.";

pub const ROWS: [Row; 4] = [Row::Switch, Row::Account, Row::AuthKey, Row::Hostname];

/// What the keyboard is typing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Typing {
    AuthKey,
    Hostname,
}

pub enum View {
    List,
    /// The login is running. `url` is the address to approve it at, once `tailscale up` has printed it.
    Login {
        url: Option<String>,
        by_key: bool,
    },
    Keyboard(Typing, Keyboard),
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    SetEnabled(bool),
    /// Start `tailscale up` and show its address; `page.cancel` stops it.
    StartLogin,
    LoginWithKey(String),
    Logout,
    SetHostname(String),
}

/// News for the page from its threads.
#[derive(Debug)]
pub enum Event {
    Poll(Snapshot),
    Switched {
        on: bool,
        result: Result<(), String>,
    },
    /// `tailscale up` printed the address to sign in at.
    LoginUrl(String),
    LoginDone(Result<(), String>),
    LoggedOut(Result<(), String>),
    HostnameSet {
        name: String,
        result: Result<(), String>,
    },
}

pub struct TailscalePage {
    pub sel: usize,
    pub view: View,
    pub snap: Option<Snapshot>,
    /// The name the user chose, kept by the App (`tailscale::Prefs`).
    pub chosen_hostname: Option<String>,
    /// "Turning Tailscale on ..." while the work runs.
    pub busy: Option<&'static str>,
    pub error: Option<String>,
    /// What a press could not do for want of a network: shown calmly, gone with the next press or network change.
    pub note: Option<&'static str>,
    /// The first press on "Log out" asks; the second does it.
    pub confirm_logout: bool,
    /// Ends the running `tailscale up` when set; a new one for each login.
    pub cancel: Arc<AtomicBool>,
    /// Stops the poller thread when the page closes.
    pub alive: Arc<AtomicBool>,
    /// A poll that was already under way when a switch or a logout finished still tells the old state: not believed.
    skip_polls: u8,
}

impl TailscalePage {
    pub fn new(chosen_hostname: Option<String>) -> TailscalePage {
        TailscalePage {
            sel: 0,
            view: View::List,
            snap: None,
            chosen_hostname,
            busy: None,
            error: None,
            note: None,
            confirm_logout: false,
            cancel: Arc::new(AtomicBool::new(false)),
            alive: Arc::new(AtomicBool::new(true)),
            skip_polls: 0,
        }
    }

    pub fn state(&self) -> Option<State> {
        self.snap.as_ref().map(Snapshot::state)
    }

    /// Switched on as far as the user can tell: the service runs.
    pub fn on(&self) -> bool {
        self.snap.as_ref().is_some_and(|s| s.running)
    }

    /// There is no network under Tailscale (Wi-Fi off or not joined, no cable). Unknown before the first poll counts as
    /// online: nothing is held back for a read that has not come.
    pub fn offline(&self) -> bool {
        self.snap.as_ref().is_some_and(|s| s.offline)
    }

    /// Signed in: the account can be logged out. What Tailscale says, with or without a network.
    pub fn signed_in(&self) -> bool {
        matches!(
            self.snap.as_ref().map(Snapshot::backend_state),
            Some(State::Connected | State::Connecting | State::Paused | State::WaitingForApproval)
        )
    }

    pub fn row(&self) -> Row {
        ROWS[self.sel.min(ROWS.len() - 1)]
    }

    /// The name shown: the one Tailscale reports, else the one chosen, else the suggestion.
    pub fn hostname(&self) -> String {
        let reported = self
            .snap
            .as_ref()
            .and_then(|s| s.status.as_ref())
            .map(|s| s.hostname.clone())
            .filter(|h| !h.is_empty());
        reported
            .or_else(|| self.chosen_hostname.clone())
            .unwrap_or_else(|| SUGGESTED_HOSTNAME.to_string())
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        // The keyboard has its own buttons (A types, B deletes, START is done).
        let typed = match &mut self.view {
            View::Keyboard(typing, kb) => Some((*typing, kb.press(b))),
            _ => None,
        };
        if let Some((typing, outcome)) = typed {
            return match outcome {
                Outcome::Editing => Action::Nothing,
                Outcome::Cancelled => {
                    self.view = View::List;
                    Action::Nothing
                }
                Outcome::Done(text) => self.typed(typing, &text),
            };
        }
        if matches!(self.view, View::Login { .. }) {
            // Back ends the login: the thread running `tailscale up` sees the flag and stops it.
            if b == cancel {
                self.cancel.store(true, Ordering::Relaxed);
                self.view = View::List;
            }
            return Action::Nothing;
        }
        let n = ROWS.len();
        if b != confirm {
            self.confirm_logout = false;
        }
        if b == Button::Up {
            self.sel = (self.sel + n - 1) % n;
        } else if b == Button::Down {
            self.sel = (self.sel + 1) % n;
        } else if b == cancel {
            self.alive.store(false, Ordering::Relaxed);
            return Action::Close;
        } else if b == confirm {
            return self.activate();
        }
        Action::Nothing
    }

    /// The keyboard is done with `text`.
    fn typed(&mut self, typing: Typing, text: &str) -> Action {
        self.view = View::List;
        match typing {
            Typing::Hostname => {
                let name = clean_hostname(text);
                if name.is_empty() {
                    self.error = Some("A name needs letters or digits".into());
                    return Action::Nothing;
                }
                self.error = None;
                self.busy = Some("Setting the name");
                Action::SetHostname(name)
            }
            Typing::AuthKey => {
                let key = text.trim();
                if key.is_empty() {
                    return Action::Nothing;
                }
                self.error = None;
                self.cancel = Arc::new(AtomicBool::new(false));
                self.view = View::Login { url: None, by_key: true };
                Action::LoginWithKey(key.to_string())
            }
        }
    }

    fn installed(&self) -> bool {
        self.snap.as_ref().is_none_or(|s| s.installed)
    }

    fn activate(&mut self) -> Action {
        if self.busy.is_some() {
            return Action::Nothing;
        }
        self.error = None;
        self.note = None;
        if !self.installed() && self.row() != Row::Hostname {
            self.error = Some(NOT_INSTALLED.into());
            return Action::Nothing;
        }
        // Signing in and out talk to Tailscale's servers: without a network they would only run into a timeout. The
        // switch, the name and the sign-in screens that need no server stay as they are.
        if self.offline() {
            match self.row() {
                Row::Account if self.signed_in() => {
                    self.note = Some(LOG_OUT_NEEDS_NETWORK);
                    return Action::Nothing;
                }
                Row::Account | Row::AuthKey => {
                    self.note = Some(SIGN_IN_NEEDS_NETWORK);
                    return Action::Nothing;
                }
                Row::Switch | Row::Hostname => {}
            }
        }
        match self.row() {
            Row::Switch => {
                let on = !self.on();
                self.busy = Some(if on { "Turning Tailscale on" } else { "Turning Tailscale off" });
                Action::SetEnabled(on)
            }
            Row::Account if self.signed_in() => {
                if !self.confirm_logout {
                    self.confirm_logout = true;
                    return Action::Nothing;
                }
                self.confirm_logout = false;
                self.busy = Some("Logging out");
                Action::Logout
            }
            Row::Account => {
                self.cancel = Arc::new(AtomicBool::new(false));
                self.view = View::Login { url: None, by_key: false };
                Action::StartLogin
            }
            Row::AuthKey => {
                self.view = View::Keyboard(Typing::AuthKey, Keyboard::default());
                Action::Nothing
            }
            Row::Hostname => {
                self.view = View::Keyboard(
                    Typing::Hostname,
                    Keyboard {
                        text: self.hostname(),
                        ..Default::default()
                    },
                );
                Action::Nothing
            }
        }
    }

    pub fn label(&self, row: Row) -> &'static str {
        match row {
            Row::Switch => "Tailscale",
            Row::Account if self.signed_in() => "Log out",
            Row::Account => "Sign in",
            Row::AuthKey => "Sign in with a key",
            Row::Hostname => "Name",
        }
    }

    /// The value at the right of a row.
    pub fn value(&self, row: Row) -> String {
        match row {
            Row::Switch => match &self.snap {
                None => String::new(),
                Some(s) if !s.installed => "Not installed".into(),
                Some(s) if s.running => "On".into(),
                Some(_) => "Off".into(),
            },
            Row::Account if self.signed_in() => {
                let status = self.snap.as_ref().and_then(|s| s.status.as_ref());
                status
                    .map(|s| if s.tailnet.is_empty() { s.login.clone() } else { s.tailnet.clone() })
                    .unwrap_or_default()
            }
            Row::Account | Row::AuthKey => String::new(),
            Row::Hostname => self.hostname(),
        }
    }

    /// The line under the list for the selected row.
    pub fn describe(&self, row: Row) -> &'static str {
        match row {
            Row::Account | Row::AuthKey if self.offline() => "Needs a network first.",
            Row::Switch => "Reach the Pocket from your other devices.",
            Row::Account if self.confirm_logout => "Press A again to log out. B keeps you signed in.",
            Row::Account if self.signed_in() => "Leaves your tailnet. Sign in again any time.",
            Row::Account => "Scan a QR code with your phone.",
            Row::AuthKey => "For a key from the admin console.",
            Row::Hostname => "How the Pocket is named on your tailnet.",
        }
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Poll(_) if self.skip_polls > 0 => self.skip_polls -= 1,
            Event::Poll(snap) => {
                let connected = snap.state() == State::Connected;
                let address = snap.status.as_ref().and_then(|s| s.auth_url.clone());
                if snap.installed && self.error.as_deref() == Some(NOT_INSTALLED) {
                    self.error = None;
                }
                // The network came or went: what failed before was about the other state of things (a timeout while
                // it was gone, a note that it was needed). The status line says how it stands now.
                if self.snap.as_ref().is_some_and(|before| before.offline != snap.offline) {
                    self.error = None;
                    self.note = None;
                }
                self.snap = Some(snap);
                if let View::Login { url, by_key } = &mut self.view {
                    if connected {
                        self.view = View::List;
                        self.error = None;
                    } else if url.is_none() && !*by_key {
                        *url = address;
                    }
                }
            }
            Event::Switched { on, result } => {
                self.busy = None;
                self.error = result.err();
                if self.error.is_none() {
                    // Shown at once; the polls after it confirm.
                    self.skip_polls = 1;
                    if let Some(s) = self.snap.as_mut() {
                        (s.running, s.boot, s.status, s.error) = (on, on, None, None);
                    }
                }
            }
            Event::LoginUrl(address) => {
                if let View::Login { url, .. } = &mut self.view {
                    *url = Some(address);
                }
            }
            Event::LoginDone(result) => {
                self.busy = None;
                if matches!(self.view, View::Login { .. }) {
                    self.view = View::List;
                }
                self.error = match result {
                    Err(msg) if msg != "Cancelled" && (msg == NO_NETWORK || self.offline()) => {
                        // The network went while it ran: said calmly. A "Timed out" or a dial error would only be noise.
                        eprintln!("tailscale: sign-in ended without a network: {msg}");
                        self.note = Some(SIGN_IN_NEEDS_NETWORK);
                        None
                    }
                    Err(msg) if msg != "Cancelled" => Some(msg),
                    _ => None,
                };
            }
            Event::LoggedOut(result) => {
                self.busy = None;
                if let Err(msg) = result {
                    // Without a network a log out can only fail; the account is still there and the status line says
                    // why nothing happened.
                    if msg == NO_NETWORK || self.offline() {
                        eprintln!("tailscale: log out ended without a network: {msg}");
                        self.note = Some(LOG_OUT_NEEDS_NETWORK);
                    } else {
                        self.error = Some(msg);
                    }
                    return;
                }
                self.error = None;
                self.skip_polls = 1;
                if let Some(st) = self.snap.as_mut().and_then(|s| s.status.as_mut()) {
                    (st.backend, st.auth_url, st.ips) = (Backend::NeedsLogin, None, Vec::new());
                    (st.tailnet, st.login, st.peers_online, st.peers_total) = (String::new(), String::new(), 0, 0);
                }
            }
            Event::HostnameSet { name, result } => {
                self.busy = None;
                match result {
                    Ok(()) => {
                        self.chosen_hostname = Some(name);
                        self.error = None;
                    }
                    Err(msg) => self.error = Some(msg),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tailscale::Status;

    const CONFIRM: Button = Button::East;
    const CANCEL: Button = Button::South;

    const RUNNING: &str = include_str!("fixtures/tailscale-status-running.json");
    const NEEDS_LOGIN: &str = include_str!("fixtures/tailscale-status-needslogin.json");
    const STOPPED: &str = include_str!("fixtures/tailscale-status-stopped.json");

    fn snap(running: bool, json: Option<&str>) -> Snapshot {
        Snapshot {
            installed: true,
            running,
            boot: running,
            status: json.map(|j| Status::parse(j).unwrap()),
            error: None,
            offline: false,
        }
    }

    fn snap_offline(running: bool, json: Option<&str>) -> Snapshot {
        Snapshot { offline: true, ..snap(running, json) }
    }

    fn page_with(s: Snapshot) -> TailscalePage {
        let mut p = TailscalePage::new(None);
        p.event(Event::Poll(s));
        p
    }

    fn go_to(p: &mut TailscalePage, row: Row) {
        while p.row() != row {
            p.press(Button::Down, CONFIRM, CANCEL);
        }
    }

    fn press(p: &mut TailscalePage, b: Button) -> Action {
        p.press(b, CONFIRM, CANCEL)
    }

    #[test]
    fn the_switch_asks_for_on_then_off_and_says_what_it_is_doing() {
        let mut p = page_with(snap(false, None));
        assert_eq!(p.value(Row::Switch), "Off");
        assert_eq!(press(&mut p, CONFIRM), Action::SetEnabled(true));
        assert_eq!(p.busy, Some("Turning Tailscale on"));
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing, "busy: a second press does nothing");
        p.event(Event::Switched { on: true, result: Ok(()) });
        assert_eq!(p.busy, None);
        p.event(Event::Poll(snap(true, Some(RUNNING))));
        assert_eq!(p.value(Row::Switch), "On");
        assert_eq!(press(&mut p, CONFIRM), Action::SetEnabled(false));
        assert_eq!(p.busy, Some("Turning Tailscale off"));
    }

    #[test]
    fn a_failed_switch_says_why_and_a_missing_install_cannot_be_switched() {
        let mut p = page_with(snap(false, None));
        press(&mut p, CONFIRM);
        p.event(Event::Switched {
            on: true,
            result: Err("Failed to start tailscaled.service".into()),
        });
        assert_eq!(p.error.as_deref(), Some("Failed to start tailscaled.service"));
        assert_eq!(p.busy, None);
        press(&mut p, CONFIRM);
        assert_eq!(p.error, None, "a new try clears the old message");
        let mut missing = page_with(Snapshot::default());
        assert_eq!(missing.value(Row::Switch), "Not installed");
        assert_eq!(press(&mut missing, CONFIRM), Action::Nothing);
        assert!(missing.error.is_some(), "it says why nothing happened");
    }

    #[test]
    fn a_switch_shows_at_once_and_a_poll_that_was_already_under_way_does_not_undo_it() {
        let mut p = page_with(snap(true, Some(RUNNING)));
        assert_eq!(press(&mut p, CONFIRM), Action::SetEnabled(false));
        p.event(Event::Switched { on: false, result: Ok(()) });
        assert_eq!(p.value(Row::Switch), "Off", "not after the next poll, now");
        assert_eq!(p.state(), Some(State::Off));
        // The poll that was in flight while the service was stopped still says it runs: not believed.
        p.event(Event::Poll(snap(true, Some(RUNNING))));
        assert_eq!(p.value(Row::Switch), "Off");
        // The next one is the truth again.
        p.event(Event::Poll(snap(false, None)));
        assert_eq!(p.value(Row::Switch), "Off");
        press(&mut p, CONFIRM);
        p.event(Event::Switched { on: true, result: Ok(()) });
        assert_eq!(p.state(), Some(State::Starting), "on, and nothing heard from it yet");
        p.event(Event::Poll(snap(false, None)));
        p.event(Event::Poll(snap(true, Some(NEEDS_LOGIN))));
        assert_eq!(p.state(), Some(State::LoggedOut));
    }

    // --- no network ---

    #[test]
    fn without_a_network_the_page_says_so_calmly_and_keeps_the_switch_as_it_was() {
        // Wi-Fi off, but the node still claims to be running with an address.
        let p = page_with(snap_offline(true, Some(RUNNING)));
        assert_eq!(p.state(), Some(State::NoNetwork));
        assert_eq!(p.snap.as_ref().unwrap().status_line(), "No network yet");
        assert_eq!(p.value(Row::Switch), "On", "the choice is not undone by the network");
        assert!(p.on());
        assert_eq!(p.error, None, "nothing red");
        assert_eq!(p.note, None);
        // Signed in stays signed in: the row still offers the log out.
        assert_eq!(p.label(Row::Account), "Log out");
        assert_eq!(p.value(Row::Account), "alex@example.com");
        // A node that has not answered, or says it is starting, reads the same.
        let mut quiet = snap_offline(true, None);
        quiet.error = Some("tailscale did not answer".into());
        let p = page_with(quiet);
        assert_eq!(p.state(), Some(State::NoNetwork));
        assert_eq!(p.error, None);
    }

    #[test]
    fn without_a_network_signing_in_and_out_say_why_instead_of_running_into_a_timeout() {
        let mut p = page_with(snap_offline(true, Some(NEEDS_LOGIN)));
        go_to(&mut p, Row::Account);
        assert_eq!(p.describe(Row::Account), "Needs a network first.");
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing, "no `tailscale up` that can only time out");
        assert!(matches!(p.view, View::List), "no QR code of a login that cannot finish");
        assert_eq!(p.note, Some("Signing in needs a network. Connect to Wi-Fi first."));
        assert_eq!(p.error, None);
        go_to(&mut p, Row::AuthKey);
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert!(matches!(p.view, View::List), "no keyboard for a key that cannot be used yet");
        assert_eq!(p.note, Some("Signing in needs a network. Connect to Wi-Fi first."));
        // Signed in: no log out either, and it does not even start asking twice.
        let mut p = page_with(snap_offline(true, Some(RUNNING)));
        go_to(&mut p, Row::Account);
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert!(!p.confirm_logout);
        assert_eq!(p.note, Some("Logging out needs a network. Connect to Wi-Fi first."));
        // The switch and the name need no server: they work.
        go_to(&mut p, Row::Switch);
        assert_eq!(press(&mut p, CONFIRM), Action::SetEnabled(false));
        assert_eq!(p.note, None, "a new press clears the note");
        go_to(&mut p, Row::Hostname);
        p.busy = None;
        press(&mut p, CONFIRM);
        assert!(matches!(p.view, View::Keyboard(Typing::Hostname, _)));
    }

    #[test]
    fn with_a_network_the_real_errors_stay_visible() {
        // Logged out: the sign-in is offered.
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        assert_eq!(p.state(), Some(State::LoggedOut));
        go_to(&mut p, Row::Account);
        assert_eq!(press(&mut p, CONFIRM), Action::StartLogin);
        // A sign-in that fails online shows why, and polls (also with the network still there) do not hide it.
        p.event(Event::LoginDone(Err("invalid key: key expired".into())));
        assert_eq!(p.error.as_deref(), Some("invalid key: key expired"));
        p.event(Event::Poll(snap(true, Some(NEEDS_LOGIN))));
        p.event(Event::Poll(snap(true, Some(NEEDS_LOGIN))));
        assert_eq!(p.error.as_deref(), Some("invalid key: key expired"));
        // A failed log out and a failed switch too.
        let mut p = page_with(snap(true, Some(RUNNING)));
        p.event(Event::LoggedOut(Err("logout failed".into())));
        assert_eq!(p.error.as_deref(), Some("logout failed"));
        assert_eq!(p.state(), Some(State::Connected), "still signed in");
        p.event(Event::Switched { on: false, result: Err("Failed to stop tailscaled.service".into()) });
        assert_eq!(p.error.as_deref(), Some("Failed to stop tailscaled.service"));
        // The daemon is down while Wi-Fi is fine: Starting, with its reason from the snapshot.
        let mut down = snap(true, None);
        down.error = Some("failed to connect to local tailscaled".into());
        let p = page_with(down);
        assert_eq!(p.state(), Some(State::Starting));
        assert_eq!(p.snap.as_ref().unwrap().error.as_deref(), Some("failed to connect to local tailscaled"));
        // A service that is stopped is Off with its switch Off, not "No network yet".
        assert_eq!(page_with(snap_offline(false, None)).state(), Some(State::Off));
    }

    #[test]
    fn an_error_from_before_the_network_went_is_gone_when_it_goes_and_when_it_comes_back() {
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        p.event(Event::LoginDone(Err("Timed out".into())));
        assert_eq!(p.error.as_deref(), Some("Timed out"));
        // Wi-Fi goes: the timeout was about that. Nothing red is left.
        p.event(Event::Poll(snap_offline(true, Some(NEEDS_LOGIN))));
        assert_eq!(p.error, None);
        assert_eq!(p.state(), Some(State::NoNetwork));
        // An error that arrives while it is gone (the login ran until the network went) is not shown either.
        p.event(Event::LoginDone(Err("Timed out".into())));
        assert_eq!(p.error, None);
        assert_eq!(p.note, Some("Signing in needs a network. Connect to Wi-Fi first."), "said calmly instead");
        // A note from a press while offline is gone when the network is back.
        go_to(&mut p, Row::Account);
        press(&mut p, CONFIRM);
        assert!(p.note.is_some());
        p.event(Event::Poll(snap_offline(true, Some(NEEDS_LOGIN))));
        assert!(p.note.is_some(), "polls that change nothing leave it");
        p.event(Event::Poll(snap(true, Some(NEEDS_LOGIN))));
        assert_eq!(p.note, None);
        assert_eq!(p.error, None);
        assert_eq!(p.state(), Some(State::LoggedOut), "online again: the real state");
    }

    #[test]
    fn offline_then_online_recovers_by_itself_without_a_press() {
        // Switched on without a network, the service runs and says Starting.
        let mut p = page_with(snap(false, None));
        press(&mut p, CONFIRM);
        p.event(Event::Switched { on: true, result: Ok(()) });
        p.event(Event::Poll(snap(false, None))); // the poll that was under way
        p.event(Event::Poll(snap_offline(true, Some(r#"{"BackendState":"Starting"}"#))));
        assert_eq!(p.state(), Some(State::NoNetwork));
        assert_eq!(p.value(Row::Switch), "On");
        assert_eq!((p.error.as_deref(), p.busy), (None, None));
        // The network comes: the next poll shows what Tailscale does with it. Nobody pressed anything.
        p.event(Event::Poll(snap(true, Some(r#"{"BackendState":"Starting"}"#))));
        assert_eq!(p.state(), Some(State::Connecting));
        p.event(Event::Poll(snap(true, Some(RUNNING))));
        assert_eq!(p.state(), Some(State::Connected));
        assert_eq!(p.error, None);
    }

    #[test]
    fn a_failure_the_worker_found_to_be_the_network_is_calm_even_before_the_next_poll_knows() {
        // The page still believes it is online (the poll is two seconds old) when the work comes back: no network.
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        go_to(&mut p, Row::Account);
        press(&mut p, CONFIRM);
        p.event(Event::LoginDone(Err(NO_NETWORK.into())));
        assert_eq!(p.error, None);
        assert_eq!(p.note, Some("Signing in needs a network. Connect to Wi-Fi first."));
        let mut p = page_with(snap(true, Some(RUNNING)));
        p.event(Event::LoggedOut(Err(NO_NETWORK.into())));
        assert_eq!((p.error.as_deref(), p.note), (None, Some("Logging out needs a network. Connect to Wi-Fi first.")));
        assert!(p.signed_in());
        // The poll that follows knows it, and the status line says so.
        p.event(Event::Poll(snap_offline(true, Some(RUNNING))));
        assert_eq!(p.state(), Some(State::NoNetwork));
    }

    #[test]
    fn a_log_out_that_fails_offline_leaves_the_account_alone_and_says_nothing_red() {
        let mut p = page_with(snap_offline(true, Some(RUNNING)));
        p.event(Event::LoggedOut(Err("Timed out".into())));
        assert_eq!(p.error, None);
        assert_eq!(p.note, Some("Logging out needs a network. Connect to Wi-Fi first."));
        assert!(p.signed_in(), "not pretended to be logged out");
        assert_eq!(p.snap.as_ref().unwrap().backend_state(), State::Connected);
    }

    #[test]
    fn a_logout_shows_at_once_too() {
        let mut p = page_with(snap(true, Some(RUNNING)));
        go_to(&mut p, Row::Account);
        press(&mut p, CONFIRM);
        press(&mut p, CONFIRM);
        p.event(Event::LoggedOut(Ok(())));
        assert_eq!(p.state(), Some(State::LoggedOut));
        assert_eq!(p.label(Row::Account), "Sign in");
        p.event(Event::Poll(snap(true, Some(RUNNING))));
        assert_eq!(p.state(), Some(State::LoggedOut), "the poll that was under way is not believed");
        p.event(Event::Poll(snap(true, Some(NEEDS_LOGIN))));
        assert_eq!(p.state(), Some(State::LoggedOut));
    }

    #[test]
    fn the_not_installed_message_goes_away_when_tailscale_turns_up() {
        let mut p = page_with(Snapshot::default());
        press(&mut p, CONFIRM);
        assert!(p.error.is_some());
        p.event(Event::Poll(snap(false, None)));
        assert_eq!(p.error, None, "it was true a moment ago");
        // Other messages stay until the next try.
        p.event(Event::Switched {
            on: true,
            result: Err("Failed to start tailscaled.service".into()),
        });
        p.event(Event::Poll(snap(false, None)));
        assert!(p.error.is_some());
    }

    #[test]
    fn signing_in_shows_the_login_and_the_address_arrives_later() {
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        go_to(&mut p, Row::Account);
        assert_eq!(p.label(Row::Account), "Sign in");
        assert_eq!(press(&mut p, CONFIRM), Action::StartLogin);
        assert!(matches!(p.view, View::Login { url: None, by_key: false }));
        p.event(Event::LoginUrl("https://login.tailscale.com/a/abc".into()));
        assert!(matches!(&p.view, View::Login { url: Some(u), .. } if u == "https://login.tailscale.com/a/abc"));
        // Approved on the phone: the poller sees the node connected, and the page returns to the list.
        p.event(Event::Poll(snap(true, Some(RUNNING))));
        assert!(matches!(p.view, View::List));
        assert_eq!(p.value(Row::Account), "alex@example.com");
        assert_eq!(p.label(Row::Account), "Log out");
    }

    #[test]
    fn the_poller_can_supply_the_address_when_the_output_was_missed() {
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        go_to(&mut p, Row::Account);
        press(&mut p, CONFIRM);
        p.event(Event::Poll(snap(true, Some(NEEDS_LOGIN))));
        assert!(matches!(&p.view, View::Login { url: Some(u), .. } if u == "https://login.tailscale.com/a/1b2c3d4e5f6a7b"));
    }

    #[test]
    fn back_cancels_a_login_and_stops_tailscale_up() {
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        go_to(&mut p, Row::Account);
        press(&mut p, CONFIRM);
        let flag = p.cancel.clone();
        assert!(!flag.load(Ordering::Relaxed));
        assert_eq!(press(&mut p, Button::Up), Action::Nothing, "other buttons do nothing on the QR code");
        assert_eq!(press(&mut p, CANCEL), Action::Nothing);
        assert!(flag.load(Ordering::Relaxed), "the running `tailscale up` is told to stop");
        assert!(matches!(p.view, View::List));
        // `tailscale up` ends with "Cancelled": that is not an error to show.
        p.event(Event::LoginDone(Err("Cancelled".into())));
        assert_eq!(p.error, None);
        // A new login gets a new flag.
        press(&mut p, CONFIRM);
        assert!(!p.cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn a_login_that_fails_shows_the_reason_and_one_that_ends_well_shows_nothing() {
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        go_to(&mut p, Row::Account);
        press(&mut p, CONFIRM);
        p.event(Event::LoginDone(Err("Timed out".into())));
        assert!(matches!(p.view, View::List));
        assert_eq!(p.error.as_deref(), Some("Timed out"));
        press(&mut p, CONFIRM);
        p.event(Event::LoginDone(Ok(())));
        assert!(matches!(p.view, View::List));
        assert_eq!(p.error, None);
    }

    #[test]
    fn logging_out_asks_twice() {
        let mut p = page_with(snap(true, Some(RUNNING)));
        go_to(&mut p, Row::Account);
        assert_eq!(p.label(Row::Account), "Log out");
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing, "the first press only asks");
        assert!(p.confirm_logout);
        assert!(p.describe(Row::Account).contains("again"));
        press(&mut p, Button::Up);
        assert!(!p.confirm_logout, "moving away takes the question back");
        go_to(&mut p, Row::Account);
        press(&mut p, CONFIRM);
        assert_eq!(press(&mut p, CONFIRM), Action::Logout);
        assert_eq!(p.busy, Some("Logging out"));
        assert!(!p.confirm_logout);
        p.event(Event::LoggedOut(Ok(())));
        assert_eq!(p.busy, None);
        p.event(Event::Poll(snap(true, Some(NEEDS_LOGIN))));
        assert_eq!(p.label(Row::Account), "Sign in");
        // A paused node is signed in too.
        let paused = page_with(snap(true, Some(STOPPED)));
        assert!(paused.signed_in());
    }

    #[test]
    fn the_auth_key_is_typed_on_the_keyboard_and_sent_when_done() {
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        go_to(&mut p, Row::AuthKey);
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert!(matches!(&p.view, View::Keyboard(Typing::AuthKey, kb) if kb.text.is_empty()));
        // Done with nothing typed does not start a login.
        assert_eq!(press(&mut p, Button::Start), Action::Nothing);
        assert!(matches!(p.view, View::List));
        press(&mut p, CONFIRM);
        if let View::Keyboard(_, kb) = &mut p.view {
            kb.text = "tskey-auth-kABC123CNTRL-xyz".into();
        }
        assert_eq!(press(&mut p, Button::Start), Action::LoginWithKey("tskey-auth-kABC123CNTRL-xyz".into()));
        assert!(matches!(p.view, View::Login { by_key: true, url: None }));
        p.event(Event::LoginDone(Err("Error: backend error: invalid key".into())));
        assert!(matches!(p.view, View::List));
        assert_eq!(p.error.as_deref(), Some("Error: backend error: invalid key"));
    }

    #[test]
    fn b_on_an_empty_keyboard_goes_back() {
        let mut p = page_with(snap(true, Some(NEEDS_LOGIN)));
        go_to(&mut p, Row::AuthKey);
        press(&mut p, CONFIRM);
        assert_eq!(press(&mut p, CANCEL), Action::Nothing);
        assert!(matches!(p.view, View::List));
        assert_eq!(press(&mut p, CANCEL), Action::Close, "and from the list it closes the page");
        assert!(!p.alive.load(Ordering::Relaxed), "which stops the poller");
    }

    #[test]
    fn the_host_name_starts_from_the_current_one_and_is_cleaned_before_it_is_sent() {
        let mut p = page_with(snap(true, Some(RUNNING)));
        go_to(&mut p, Row::Hostname);
        assert_eq!(p.value(Row::Hostname), "pocket-mini");
        press(&mut p, CONFIRM);
        assert!(matches!(&p.view, View::Keyboard(Typing::Hostname, kb) if kb.text == "pocket-mini"));
        if let View::Keyboard(_, kb) = &mut p.view {
            kb.text = "Alex Pocket".into();
        }
        assert_eq!(press(&mut p, Button::Start), Action::SetHostname("alex-pocket".into()));
        assert!(matches!(p.view, View::List));
        p.event(Event::HostnameSet {
            name: "alex-pocket".into(),
            result: Ok(()),
        });
        assert_eq!(p.chosen_hostname.as_deref(), Some("alex-pocket"));
        assert_eq!(p.error, None);
        // Nothing usable typed: no action, and the page says so.
        press(&mut p, CONFIRM);
        if let View::Keyboard(_, kb) = &mut p.view {
            kb.text = "???".into();
        }
        assert_eq!(press(&mut p, Button::Start), Action::Nothing);
        assert!(p.error.is_some());
        p.event(Event::HostnameSet {
            name: "x".into(),
            result: Err("tailscale did not answer".into()),
        });
        assert_eq!(p.error.as_deref(), Some("tailscale did not answer"));
        assert_eq!(p.chosen_hostname.as_deref(), Some("alex-pocket"), "unchanged on failure");
    }

    #[test]
    fn the_host_name_before_anything_is_known_is_the_suggestion() {
        let p = TailscalePage::new(None);
        assert_eq!(p.hostname(), "pocket-mini");
        assert_eq!(TailscalePage::new(Some("mini-2".into())).hostname(), "mini-2");
        // What Tailscale reports wins over what we remember.
        let mut q = TailscalePage::new(Some("mini-2".into()));
        q.event(Event::Poll(snap(true, Some(RUNNING))));
        assert_eq!(q.hostname(), "pocket-mini");
    }

    #[test]
    fn up_and_down_wrap_and_every_row_has_words() {
        let mut p = page_with(snap(false, None));
        press(&mut p, Button::Up);
        assert_eq!(p.row(), Row::Hostname);
        press(&mut p, Button::Down);
        assert_eq!(p.row(), Row::Switch);
        for r in ROWS {
            assert!(!p.label(r).is_empty());
            assert!(!p.describe(r).is_empty());
        }
    }

    #[test]
    fn a_poll_while_a_switch_is_working_does_not_end_the_wait() {
        let mut p = page_with(snap(false, None));
        press(&mut p, CONFIRM);
        p.event(Event::Poll(snap(false, None)));
        assert_eq!(p.busy, Some("Turning Tailscale on"), "only the Switched event ends it");
    }
}

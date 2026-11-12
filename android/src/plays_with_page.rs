//! Settings → Plays with in the phone layout. Every system that has pocket-ui's own player and an emulator app on the
//! phone that pocket-ui knows (`EmulatorIntents.java`) gets a row: A steps through the own player and the installed
//! apps. Only installed apps are offered. A game of a system set to an app starts there like the systems that have no
//! player of ours (play time, shelf, start picture and the way back as ever); Continue and the pause menu are the
//! app's then, and the page says so in words above the rows.
//!
//! Later, with testers' numbers: a weak device could suggest the app for a heavy system here (`Ui::plays_suggestion`
//! is the hook; it answers nothing today, so nothing is chosen for anyone).
use super::phone::{MenuRow, Room};
use super::*;

/// What stands above the rows when there is something to choose.
pub const ABOUT: &str = "Another app starts the game and brings you back here. Continue and the pause menu are then that app's own.";
/// What stands there when there is nothing to choose.
pub const NONE: &str = "Nothing to choose yet: none of your systems has another emulator app on this phone (RetroArch, PPSSPP and DuckStation work). Until then pocket-ui's own player starts every game.";
/// The own player's name in a row; the apps keep the name the phone gives them.
const OWN: &str = "pocket-ui";

#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub package: String,
    pub label: String,
}
/// A system with our player and at least one app to play it with.
struct Offer {
    system_id: String,
    system: String,
    apps: Vec<App>,
}
#[derive(Default)]
pub struct PlaysUi {
    /// The apps on the phone by emulator profile (`Game::profile`), as the host last read them.
    installed: std::collections::HashMap<String, Vec<App>>,
    offers: Vec<Offer>,
    /// Each own-player system of the library and its profile: what the host asks the phone about.
    profiles: Vec<(String, String, String)>,
}

/// The name of an app as a row's value: the phone's label without a "(...)" tail, in words that fit beside a system's
/// name on a narrow screen (the package when the phone gave none).
fn short(label: &str, package: &str) -> String {
    let label = label.split('(').next().unwrap_or_default().trim();
    let mut out = String::new();
    for word in label.split_whitespace() {
        if !out.is_empty() && out.chars().count() + 1 + word.chars().count() > 16 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() { package.to_string() } else { out }
}

impl Ui {
    /// The profiles of the library's own-player systems, comma separated: the question for `installedApps`.
    pub fn plays_profiles(&self) -> String {
        let mut profiles: Vec<&str> = self.plays.profiles.iter().map(|(_, _, p)| p.as_str()).collect();
        profiles.sort_unstable();
        profiles.dedup();
        profiles.join(",")
    }
    /// The host read the phone's apps (`installedApps`'s JSON: profile → [{p: package, l: name}]).
    pub fn set_installed_apps(&mut self, json: &str) {
        let read: std::collections::HashMap<String, Vec<serde_json::Value>> = serde_json::from_str(json).unwrap_or_default();
        self.plays.installed = read
            .into_iter()
            .map(|(profile, apps)| {
                let apps = apps
                    .iter()
                    .filter_map(|a| Some(App { package: a["p"].as_str()?.to_string(), label: short(a["l"].as_str().unwrap_or_default(), a["p"].as_str()?) }))
                    .collect();
                (profile, apps)
            })
            .collect();
        self.rebuild_plays();
    }
    /// The library or the phone's apps changed: which systems have a choice now.
    pub(super) fn rebuild_plays(&mut self) {
        let mut profiles: Vec<(String, String, String)> = Vec::new();
        if let Some(lib) = &self.library {
            for g in &lib.games {
                if super::super::bundled_core(g.system_id).is_some() && !profiles.iter().any(|(id, _, _)| id == g.system_id) {
                    profiles.push((g.system_id.to_string(), g.system.to_string(), g.profile.to_string()));
                }
            }
        }
        profiles.sort_by(|a, b| a.1.cmp(&b.1));
        self.plays.offers = profiles
            .iter()
            .filter_map(|(id, name, profile)| {
                let apps = self.plays.installed.get(profile).filter(|a| !a.is_empty())?;
                Some(Offer { system_id: id.clone(), system: name.clone(), apps: apps.clone() })
            })
            .collect();
        self.plays.profiles = profiles;
        // Games that stood open in our player may not be, or the other way round.
        self.frozen_stale = true;
    }
    /// The app this system plays with, when one is chosen and still on the phone; None: our own player.
    pub fn plays_in_app_of(&self, system_id: &str) -> Option<&App> {
        let chosen = self.prefs.plays_with.get(system_id)?;
        self.plays.offers.iter().find(|o| o.system_id == system_id)?.apps.iter().find(|a| &a.package == chosen)
    }
    pub fn plays_in_app(&self, game: &Game) -> Option<&App> {
        self.plays_in_app_of(game.system_id)
    }
    /// Our own player is the one that starts this system and is not yet qualified (`play_cards::experimental`).
    pub fn experimental(&self, system_id: &str) -> bool {
        play_cards::experimental(system_id) && self.plays_in_app_of(system_id).is_none()
    }
    /// The hook for suggesting an app on a weak device (needs testers' numbers first). Nothing is suggested today.
    pub fn plays_suggestion(&self, _system_id: &str) -> Option<&App> {
        None
    }
    /// The Settings row.
    pub(super) fn open_plays_with(&mut self) -> Command {
        self.navigate(Screen::PlaysWith);
        Command::None
    }
    pub(super) fn plays_rows(&self) -> Vec<MenuRow> {
        let mut rows: Vec<MenuRow> = self
            .plays
            .offers
            .iter()
            .map(|o| {
                let value = match self.plays_in_app_of(&o.system_id) {
                    Some(app) => app.label.clone(),
                    None => OWN.to_string(),
                };
                MenuRow { label: o.system.clone(), value, check: None, stepper: false }
            })
            .collect();
        rows.push(MenuRow { label: tr("Back"), value: String::new(), check: None, stepper: false });
        rows
    }
    pub(super) fn plays_words(&self) -> String {
        if self.plays.offers.is_empty() {
            return tr(NONE);
        }
        // The word about our own player, quietly, for the listed systems it is still being tested on (a row is too
        // narrow for it beside a system's name on a phone).
        let testing: Vec<&str> = self.plays.offers.iter().filter(|o| self.experimental(&o.system_id)).map(|o| o.system.as_str()).collect();
        if testing.is_empty() {
            tr(ABOUT)
        } else {
            format!("{}\n{}", tr(ABOUT), tr(&format!("Experimental in pocket-ui's player: {}", testing.join(", "))))
        }
    }
    pub(super) fn plays_press(&mut self, b: Button) -> Command {
        match b {
            Button::Up | Button::Left => self.move_by(-1),
            Button::Down | Button::Right => self.move_by(1),
            Button::South | Button::Start => self.navigate(if b == Button::Start { Screen::Library } else { Screen::Settings }),
            Button::East => {
                let Some(offer) = self.plays.offers.get(self.menu) else {
                    self.navigate(Screen::Settings);
                    return Command::None;
                };
                // Own player → the first app → the next ... → own player again.
                let at = self.plays_in_app_of(&offer.system_id).and_then(|a| offer.apps.iter().position(|x| x == a));
                let next = at.map_or(0, |i| i + 1);
                let id = offer.system_id.clone();
                match offer.apps.get(next) {
                    Some(app) => {
                        self.prefs.plays_with.insert(id, app.package.clone());
                    }
                    None => {
                        self.prefs.plays_with.remove(&id);
                    }
                }
                // The shelf's Continue row and the cases that stand open follow the player that starts the game now.
                self.frozen_stale = true;
                self.cards.clear();
                return Command::Persist;
            }
            _ => (),
        }
        Command::None
    }
    /// The words, whole, then the rows in the room that is left (they scroll when a big type leaves little).
    pub(super) fn paint_plays_with(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let size = self.sp(15.0);
        let step = size as i32 + self.d(7.0);
        let w = c.w - 2 * room.margin;
        let words = self.plays_words();
        let mut y = room.top + self.d(4.0);
        // At most half the page for the words, so the rows always have room.
        let most = ((room.bottom - room.top) / 2 / step).max(2) as usize;
        let lines: Vec<String> = words.lines().flat_map(|line| text.wrap(Face::Body, size, line, w)).collect();
        for line in lines.iter().take(most) {
            text.draw(c, Face::Body, size, room.margin, y + size as i32, pal().dust, line);
            y += step;
        }
        self.cut.extend(lines.iter().skip(most).cloned());
        self.target(Area::new(room.margin, room.top, w, y - room.top), &words.replace('\n', " "), Action::Label);
        let rows = self.plays_rows();
        self.paint_menu_phone(c, text, Room { top: y + self.d(6.0), ..room }, rows);
    }
}

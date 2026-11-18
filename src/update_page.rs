//! Shared decisions and words for Settings → System → Updates; platform hosts own I/O and layout.
use crate::update_channel::{self, Manifest, Release};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { Check, InstallApp, InstallNow, Later, Auto, Card }

impl Action {
    pub fn label(self, version: Option<&str>) -> String {
        match self {
            Self::Check => "Check again".into(),
            Self::InstallApp => format!("Install pocket-ui {}", version.unwrap_or("")),
            Self::InstallNow => "Install now".into(),
            Self::Later => "Later".into(),
            Self::Auto => "Check for updates automatically".into(),
            Self::Card => "Look on the card".into(),
        }
    }
}

pub fn available<'a>(manifest: Option<&'a Manifest>, current: &str, release_build: bool) -> Option<&'a Release> {
    manifest.and_then(|m| m.app.as_ref()).filter(|a| release_build && update_channel::newer(&a.version, current))
}

pub fn actions(available: bool, confirming: bool) -> Vec<Action> {
    if available && confirming { return vec![Action::InstallNow, Action::Later]; }
    let mut rows = Vec::new();
    if available { rows.push(Action::InstallApp); }
    rows.extend([Action::Check, Action::Auto]);
    rows
}

/// The handhelds' page (not Android's): "Look on the card" last, under the switch, for an update file copied onto the card
/// (`update::card`); the rows above keep their places.
pub fn actions_with_card(available: bool, confirming: bool) -> Vec<Action> {
    let mut rows = actions(available, confirming);
    if !(available && confirming) { rows.push(Action::Card); }
    rows
}

pub const CONFIRM: &str = "Update pocket-ui now? Your games and settings stay.";
pub const UP_TO_DATE: &str = "Everything is up to date.";
/// Android's question before the install (the Mobile view's and the handheld view's page): its installer ends the app.
pub const ANDROID_CONFIRM: &str = "After updating, pocket-ui closes. Open it again; your games and settings stay.";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn android_build_counters_offer_only_a_later_version_and_ask_first() {
        let manifest = Manifest { format: 1, app: Some(Release {version: "0.4.0-android.2".into(),url: "app.apk".into(),size: 1,sha256: String::new(),mirrors: vec![],preferred_sources: vec![],notes: String::new()}), ..Default::default() };
        assert!(available(Some(&manifest), "0.4.0-android.1", true).is_some());
        assert!(available(Some(&manifest), "0.4.0-android.2", true).is_none());
        assert!(available(Some(&manifest), "0.4.0-android.3", true).is_none());
        assert_eq!(actions(true, false), vec![Action::InstallApp, Action::Check, Action::Auto]);
        assert_eq!(actions(true, true), vec![Action::InstallNow, Action::Later]);
        assert_eq!(actions(false, true), vec![Action::Check, Action::Auto]);
        assert_eq!(actions_with_card(true, false), vec![Action::InstallApp, Action::Check, Action::Auto, Action::Card]);
        assert_eq!(actions_with_card(false, false), vec![Action::Check, Action::Auto, Action::Card]);
        assert_eq!(actions_with_card(true, true), vec![Action::InstallNow, Action::Later], "the question stays two rows");
    }
}

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

/// A handheld without Wi-Fi leads with the card. Keep the same confirmation on every handheld.
pub fn actions_with_card(available: bool, confirming: bool, has_wifi: bool) -> Vec<Action> {
    if !has_wifi && !(available && confirming) {
        let mut rows = vec![Action::Card];
        if available { rows.push(Action::InstallApp); }
        return rows;
    }
    let mut rows = actions(available, confirming);
    if !(available && confirming) { rows.push(Action::Card); }
    rows
}

pub const CONFIRM: &str = "Update now? Your games and settings stay safe.";
pub const UP_TO_DATE: &str = "Everything is up to date.";
/// Android's question before the install (the Mobile view's and the handheld view's page): its installer ends the app.
pub const ANDROID_CONFIRM: &str = "pocket-ui will close after updating. Reopen it; games and settings stay safe.";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r36s_fix_a_without_wifi_the_card_is_the_first_and_only_check() {
        assert_eq!(actions_with_card(false, false, false), vec![Action::Card]);
        assert_eq!(actions_with_card(true, false, false), vec![Action::Card, Action::InstallApp]);
        assert_eq!(actions_with_card(true, true, false), vec![Action::InstallNow, Action::Later]);
    }

    #[test]
    fn android_build_counters_offer_only_a_later_version_and_ask_first() {
        let manifest = Manifest { format: 1, app: Some(Release {version: "0.4.0-android.2".into(),url: "app.apk".into(),size: 1,sha256: String::new(),mirrors: vec![],preferred_sources: vec![],notes: String::new()}), ..Default::default() };
        assert!(available(Some(&manifest), "0.4.0-android.1", true).is_some());
        assert!(available(Some(&manifest), "0.4.0-android.2", true).is_none());
        assert!(available(Some(&manifest), "0.4.0-android.3", true).is_none());
        assert_eq!(actions(true, false), vec![Action::InstallApp, Action::Check, Action::Auto]);
        assert_eq!(actions(true, true), vec![Action::InstallNow, Action::Later]);
        assert_eq!(actions(false, true), vec![Action::Check, Action::Auto]);
        assert_eq!(actions_with_card(true, false, true), vec![Action::InstallApp, Action::Check, Action::Auto, Action::Card]);
        assert_eq!(actions_with_card(false, false, true), vec![Action::Check, Action::Auto, Action::Card]);
        assert_eq!(actions_with_card(true, true, true), vec![Action::InstallNow, Action::Later], "the question stays two rows");
    }
}

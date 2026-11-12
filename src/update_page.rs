//! Shared decisions and words for Settings → System → Updates; platform hosts own I/O and layout.
use crate::update_channel::{self, Manifest, Release};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { Check, InstallApp, InstallNow, Later, Auto }

impl Action {
    pub fn label(self, version: Option<&str>) -> String {
        match self {
            Self::Check => "Check again".into(),
            Self::InstallApp => format!("Install pocket-ui {}", version.unwrap_or("")),
            Self::InstallNow => "Install now".into(),
            Self::Later => "Later".into(),
            Self::Auto => "Check for updates automatically".into(),
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

pub const CONFIRM: &str = "Update pocket-ui now? Your games and settings stay.";
pub const UP_TO_DATE: &str = "Everything is up to date.";

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
    }
}

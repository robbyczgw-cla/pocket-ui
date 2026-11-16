//! The RetroAchievements account on this Pocket: the user name and the token the server gave at login. The password
//! is never written anywhere; it lives only for the one login request (`api::login`).
//!
//! `<data>/retroachievements.json`, readable and writable by its owner only (0600), replaced in one step like the
//! other settings files (`fsutil::write_atomic`'s way, but created with those permissions from the start, so there is
//! never a moment in which another user could read it).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FILE: &str = "retroachievements.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Account {
    /// The user name as the server spells it.
    pub username: String,
    /// What the server answered the password with; it stands for the password in every later request.
    pub token: String,
    /// The name shown on the site, when it differs from the user name.
    #[serde(default)]
    pub display_name: String,
    /// Hardcore (Settings > RetroAchievements > Hardcore): no save-state loads, rewind, fast-forward or cheats, and the
    /// unlocks count as hardcore on the site (`super::hardcore`). Off for an account signed in before it existed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hardcore: bool,
}

/// Where the account lives when something else owns it (Android's handheld view: the app's folder, which the player and
/// the Mobile view read, not the handheld view's own).
static SHARED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// From now on the account is in `dir`, whatever data folder a page asks with. Before the first page is opened.
pub fn share_in(dir: &Path) {
    let _ = SHARED.set(dir.to_path_buf());
}

/// The account's file: in `shared` when there is one, else in `data_dir`.
pub fn path_with(shared: Option<&Path>, data_dir: &Path) -> PathBuf {
    shared.unwrap_or(data_dir).join(FILE)
}

impl Account {
    pub fn path(data_dir: &Path) -> PathBuf {
        path_with(SHARED.get().map(PathBuf::as_path), data_dir)
    }

    /// The account, if one is signed in (a file without a token counts as signed out).
    pub fn load(data_dir: &Path) -> Option<Account> {
        let bytes = fs::read(Self::path(data_dir)).ok()?;
        let a: Account = serde_json::from_slice(&bytes).ok()?;
        (!a.username.is_empty() && !a.token.is_empty()).then_some(a)
    }

    /// The name to show: the display name, else the user name.
    pub fn shown_name(&self) -> &str {
        if self.display_name.is_empty() {
            &self.username
        } else {
            &self.display_name
        }
    }

    /// Writes the account, owner-only (0600), atomically.
    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        let path = Self::path(data_dir);
        fs::create_dir_all(data_dir)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        let _ = fs::remove_file(&tmp);
        let written = (|| {
            let mut opts = OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut f = opts.open(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, &path)
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        if let Ok(d) = fs::File::open(data_dir) {
            let _ = d.sync_all();
        }
        Ok(())
    }

    /// A new sign-in of the same user keeps what they chose before it (Hardcore): signing in again after a password
    /// change must not quietly turn Hardcore off.
    pub fn keeping_choices(mut self, data_dir: &Path) -> Account {
        let before = fs::read(Self::path(data_dir)).ok().and_then(|b| serde_json::from_slice::<Account>(&b).ok());
        if before.is_some_and(|b| b.username.eq_ignore_ascii_case(&self.username) && b.hardcore) {
            self.hardcore = true;
        }
        self
    }

    /// Signs out: the token is deleted. True when no account is left.
    pub fn remove(data_dir: &Path) -> bool {
        let path = Self::path(data_dir);
        fs::remove_file(&path).is_ok() || !path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn scratch(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("ra-account-{name}"))
    }

    #[test]
    fn the_token_is_kept_owner_only_and_never_a_password() {
        let dir = scratch("save");
        assert_eq!(Account::load(&dir), None, "nothing signed in yet");
        let a = Account {
            username: "Alex".into(),
            token: "Tk123".into(),
            display_name: String::new(),
            hardcore: false,
        };
        a.save(&dir).unwrap();
        assert_eq!(Account::load(&dir), Some(a.clone()));
        let text = fs::read_to_string(Account::path(&dir)).unwrap();
        assert!(text.contains("Tk123") && !text.to_lowercase().contains("password"), "{text}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(Account::path(&dir)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "only the owner may read the token");
        }
        // Saving again replaces it (the temporary file of a crashed write is no obstacle).
        fs::write(Account::path(&dir).with_extension("json.tmp"), "junk").unwrap();
        let b = Account {
            display_name: "Rob".into(),
            ..a
        };
        b.save(&dir).unwrap();
        assert_eq!(Account::load(&dir).unwrap().shown_name(), "Rob");
        assert!(!Account::path(&dir).with_extension("json.tmp").exists());
        assert!(Account::remove(&dir));
        assert_eq!(Account::load(&dir), None);
        assert!(Account::remove(&dir), "signing out twice is fine");
    }

    #[test]
    fn signing_in_again_keeps_hardcore_for_the_same_user_only() {
        let dir = scratch("again");
        let fresh = |name: &str| Account { username: name.into(), token: "New".into(), display_name: String::new(), hardcore: false };
        assert!(!fresh("Alex").keeping_choices(&dir).hardcore, "nothing before");
        Account { hardcore: true, ..fresh("Alex") }.save(&dir).unwrap();
        assert!(fresh("alex").keeping_choices(&dir).hardcore);
        assert!(!fresh("Someone").keeping_choices(&dir).hardcore, "another account starts with its own choice");
    }

    #[test]
    fn a_file_without_a_token_is_signed_out() {
        let dir = scratch("empty");
        fs::create_dir_all(&dir).unwrap();
        fs::write(Account::path(&dir), r#"{"username":"Alex","token":""}"#).unwrap();
        assert_eq!(Account::load(&dir), None);
        fs::write(Account::path(&dir), "not json").unwrap();
        assert_eq!(Account::load(&dir), None);
    }
}

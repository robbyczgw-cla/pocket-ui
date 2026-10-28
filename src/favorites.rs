//! Favourite games: Y on a game stars it. Stored as JSON in the data directory, sorted by name,
//! shown as a "Favourites" row on Home while there is at least one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Favorite {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
}

pub struct Favorites {
    file: PathBuf,
    pub list: Vec<Favorite>,
}

impl Favorites {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("favorites.json");
        let list = crate::fsutil::read_json_or_backup(&file).unwrap_or_default();
        Self { file, list }
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.list.iter().any(|f| f.path == path)
    }

    /// Stars or unstars the game and saves; returns whether it is a favourite now.
    pub fn toggle(&mut self, system: &str, name: &str, path: &Path) -> bool {
        let now = match self.list.iter().position(|f| f.path == path) {
            Some(i) => {
                self.list.remove(i);
                false
            }
            None => {
                self.list.push(Favorite {
                    system: system.to_string(),
                    name: name.to_string(),
                    path: path.to_path_buf(),
                });
                self.list.sort_by_key(|f| f.name.to_lowercase());
                true
            }
        };
        self.save();
        now
    }

    /// Takes a game off the list (it was deleted) and saves; false when it was not on it.
    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.list.len();
        self.list.retain(|f| f.path != path);
        let removed = self.list.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    fn save(&self) {
        let _ = crate::fsutil::write_json(&self.file, &self.list);
    }

    /// Stars (`on`) or unstars the game without saving, for `library_sync`'s batch; true when that changed the list.
    pub fn set(&mut self, system: &str, name: &str, path: &Path, on: bool) -> bool {
        if self.contains(path) == on {
            return false;
        }
        if on {
            self.list.push(Favorite { system: system.to_string(), name: name.to_string(), path: path.to_path_buf() });
            self.list.sort_by_key(|f| f.name.to_lowercase());
        } else {
            self.list.retain(|f| f.path != path);
        }
        true
    }

    /// Writes the list now (after a batch of [`set`](Self::set)).
    pub fn save_now(&self) -> std::io::Result<()> {
        crate::fsutil::write_json(&self.file, &self.list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::fs;

    #[test]
    fn toggle_stars_sorts_and_survives_a_restart() {
        let dir = TestDir::new("favs");
        let mut f = Favorites::load(&dir);
        assert!(f.list.is_empty());
        assert!(f.toggle("snes", "Contra III", Path::new("/r/snes/c.smc")));
        assert!(f.toggle("gbc", "uCity", Path::new("/r/gbc/u.gbc")));
        assert!(f.toggle("nes", "alter ego", Path::new("/r/nes/a.nes")));
        let names: Vec<_> = f.list.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["alter ego", "Contra III", "uCity"], "by name, ignoring case");
        let again = Favorites::load(&dir);
        assert_eq!(again.list, f.list);
        assert!(!f.toggle("snes", "Contra III", Path::new("/r/snes/c.smc")), "a second Y unstars");
        assert!(!f.contains(Path::new("/r/snes/c.smc")) && f.contains(Path::new("/r/gbc/u.gbc")));
        assert_eq!(Favorites::load(&dir).list.len(), 2);
    }

    #[test]
    fn a_zeroed_favorites_file_comes_back_from_the_backup() {
        let dir = TestDir::new("favs-zeroed");
        let mut f = Favorites::load(&dir);
        f.toggle("gbc", "uCity", Path::new("/r/gbc/u.gbc"));
        fs::write(dir.join("favorites.json"), b"").unwrap();
        let again = Favorites::load(&dir);
        assert_eq!(again.list, f.list, "the star survives a 0-byte file");
    }
}

//! A Brick app update unpacks only what changed. The SD card is mounted `sync`, every cluster written costs time, and a release
//! changes the two programs (~25 MB) while ~45 MB of the 47 MB folder (zoneinfo, licences, sources, message pictures) stays the
//! same. So before unpacking, the signed tarball is compared with the running `app/`: a folder whose every file is byte for byte
//! the one in the tarball, and a big file the same, are not unpacked but renamed from `app/` into `app.new/` after the build
//! has been checked (`Plan::move_reused`, the machinery of the small asset packages: a manifest in `app/.asset-moves` written
//! first, which deploy/brick/boot.sh reads to finish or undo a cut-off update). Nothing here can fail an update: if anything is
//! odd the plan is empty and everything is unpacked as before.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

/// A single unchanged file is only worth a rename from here on; smaller ones cost about as much to move as to write.
const MIN_FILE: u64 = 64 << 10;
/// Beyond this a tarball (or the folder on the card) is not one we make: unpack it the plain way.
const MAX_ENTRIES: usize = 20_000;

/// What the plan decided: paths in the tarball (files or whole folders) that are moved instead of unpacked.
#[derive(Default, Debug, PartialEq)]
pub(super) struct Delta {
    pub(super) moves: Vec<String>,
}

impl Delta {
    /// Whether `path` (an entry of the tarball) is, or is inside, something that is moved.
    pub(super) fn skips(&self, path: &str) -> bool {
        self.moves.iter().any(|m| path == m || path.strip_prefix(m.as_str()).is_some_and(|rest| rest.starts_with('/')))
    }
}

/// A name that boot.sh's `move_path` and the one-name-per-line manifest can carry.
pub(super) fn plain_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|c| !c.is_empty() && !c.starts_with('.') && !c.contains(['\\', '\n', '\r', '\0']))
        && !matches!(path, "start.sh" | "RELEASE" | "VERSION" | "asset-packages.json")
        && path != "storage" && !path.starts_with("storage/")
}

struct TarFile {
    size: u64,
    /// The same bytes (and a mode no weaker) as the file at that path in `app/`.
    same: bool,
}

/// Compares the tarball at `archive` with `card/app` (read only) and says what can be moved. `keep` are the paths of the tarball
/// that are always unpacked (what the start check opens before the switch).
pub(super) fn plan(archive: &Path, card: &Path, keep: &[&str]) -> Delta {
    scan(archive, &card.join("app"), keep).unwrap_or_default()
}

fn scan(archive: &Path, app: &Path, keep: &[&str]) -> Option<Delta> {
    if !app.join("start.sh").is_file() {
        return None;
    }
    let mut files: BTreeMap<String, TarFile> = BTreeMap::new();
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    let mut gz = crate::untar::Gz::new(std::fs::File::open(archive).ok()?, 2 << 30).ok()?;
    crate::untar::walk(&mut gz, |e, mode, data| {
        if files.len() + dirs.len() > MAX_ENTRIES {
            return Err(crate::untar::Refused::Unsafe);
        }
        if e.dir {
            dirs.insert(e.path.clone());
            return Ok(());
        }
        let old = app.join(&e.path);
        let mut same = false;
        let mut size = 0;
        if let Some(m) = std::fs::symlink_metadata(&old).ok().filter(|m| m.is_file()) {
            use std::os::unix::fs::PermissionsExt;
            let (hash, n) = hash_reader(data)?;
            same = m.len() == n
                && (mode & 0o111 == 0 || m.permissions().mode() & 0o111 != 0)
                && crate::store::sha256_file(&old).is_ok_and(|h| h == hash);
            size = n;
        }
        files.insert(e.path.clone(), TarFile { size, same });
        Ok(())
    })
    .ok()?;
    // The folders the files imply, besides the ones the tarball lists.
    for path in files.keys().cloned().collect::<Vec<_>>() {
        let mut at = path.as_str();
        while let Some((parent, _)) = at.rsplit_once('/') {
            dirs.insert(parent.to_string());
            at = parent;
        }
    }
    let old = Listing::of(app)?;
    let kept = |p: &str| keep.contains(&p) || p.starts_with('.');
    let mut moves = Vec::new();
    // A folder is moved whole when it is the same all through: every file of it in the tarball is unchanged, nothing else is in it
    // on the card (not a file, a folder, or a link), and the start check does not open anything in it.
    let whole = |dir: &str| -> bool {
        let prefix = format!("{dir}/");
        let mine: Vec<(&String, &TarFile)> = files.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)).collect();
        !mine.is_empty()
            && old.dirs.contains(dir)
            && mine.iter().all(|(k, f)| f.same && !kept(k) && plain_path(k))
            && dirs.range(prefix.clone()..).take_while(|k| k.starts_with(&prefix)).all(|d| plain_path(d))
            && old.under(&old.files, &prefix).eq(mine.iter().map(|(k, _)| k.as_str()))
            && old.under(&old.dirs, &prefix).eq(dirs.range(prefix.clone()..).take_while(|k| k.starts_with(&prefix)).map(String::as_str))
            && !old.odd.iter().any(|o| o.starts_with(&prefix))
    };
    // From the top: an unchanged folder goes whole; otherwise look inside, where a big unchanged file goes by itself.
    let mut todo: Vec<String> = dirs.iter().filter(|d| !d.contains('/')).cloned().collect();
    while let Some(dir) = todo.pop() {
        if !plain_path(&dir) {
            continue;
        }
        if whole(&dir) {
            moves.push(dir);
            continue;
        }
        let prefix = format!("{dir}/");
        for sub in dirs.range(prefix.clone()..).take_while(|k| k.starts_with(&prefix)) {
            if !sub[prefix.len()..].contains('/') {
                todo.push(sub.clone());
            }
        }
        for (k, f) in files.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)) {
            if !k[prefix.len()..].contains('/') && f.same && f.size >= MIN_FILE && plain_path(k) && !kept(k) && !old.odd.contains(k) {
                moves.push(k.clone());
            }
        }
    }
    moves.sort();
    Some(Delta { moves })
}

fn hash_reader(data: &mut dyn Read) -> Result<(String, u64), crate::untar::Refused> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 << 10];
    let mut n = 0u64;
    loop {
        let k = data.read(&mut buf)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((h.finalize().iter().map(|b| format!("{b:02x}")).collect(), n))
}

/// Everything below `app/`, by relative path: files, folders, and what is neither (links, devices).
struct Listing {
    files: BTreeSet<String>,
    dirs: BTreeSet<String>,
    odd: BTreeSet<String>,
}

impl Listing {
    fn of(app: &Path) -> Option<Self> {
        let mut l = Listing { files: BTreeSet::new(), dirs: BTreeSet::new(), odd: BTreeSet::new() };
        l.walk(app, "")?;
        Some(l)
    }
    fn walk(&mut self, dir: &Path, rel: &str) -> Option<()> {
        for e in std::fs::read_dir(dir).ok()? {
            let e = e.ok()?;
            let name = e.file_name().into_string().ok()?;
            let path = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            let meta = std::fs::symlink_metadata(e.path()).ok()?;
            if self.files.len() + self.dirs.len() + self.odd.len() > MAX_ENTRIES {
                return None;
            }
            if meta.is_dir() {
                self.dirs.insert(path.clone());
                self.walk(&e.path(), &path)?;
            } else if meta.is_file() {
                self.files.insert(path);
            } else {
                self.odd.insert(path);
            }
        }
        Some(())
    }
    /// The entries of `set` below `prefix` (ending in "/"), in order.
    fn under<'a>(&'a self, set: &'a BTreeSet<String>, prefix: &str) -> impl Iterator<Item = &'a str> + 'a {
        let prefix = prefix.to_string();
        set.range(prefix.clone()..).take_while(move |k| k.starts_with(&prefix)).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::process::Command;

    fn tar_of(src: &Path, to: &Path) {
        assert!(Command::new("tar").arg("-czf").arg(to).arg("-C").arg(src).arg(".").status().unwrap().success());
    }
    /// `app/` with a start script and the given files, and the same files (or `then`) in a tarball.
    fn card_and_tar(dir: &Path, now: &[(&str, usize, u32)], then: &[(&str, usize, u32)]) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let card = dir.join("card");
        let write = |root: &Path, files: &[(&str, usize, u32)]| {
            for (path, len, mode) in files {
                std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
                std::fs::write(root.join(path), vec![(*len % 251) as u8; *len]).unwrap();
                std::fs::set_permissions(root.join(path), std::fs::Permissions::from_mode(*mode)).unwrap();
            }
        };
        write(&card.join("app"), now);
        std::fs::write(card.join("app/start.sh"), "exit 0\n").unwrap();
        write(&dir.join("src"), then);
        tar_of(&dir.join("src"), &dir.join("app.tar.gz"));
        (card, dir.join("app.tar.gz"))
    }

    #[test]
    fn a_move_covers_the_path_and_what_is_below_it_and_nothing_with_a_similar_name() {
        let d = Delta { moves: vec!["sources".into(), "docs/big.bin".into()] };
        for yes in ["sources", "sources/a.zip", "sources/deep/er", "docs/big.bin"] {
            assert!(d.skips(yes), "{yes}");
        }
        for no in ["sources2", "sources2/a", "source", "docs", "docs/big.bin.old", "docs/big.bi", "x/sources", ""] {
            assert!(!d.skips(no), "{no}");
        }
    }

    #[test]
    fn only_plain_relative_names_can_be_in_a_manifest() {
        for ok in ["sources", "msg/big.raw", "licences/cores", "zoneinfo/Europe/Berlin", "a b/ü"] {
            assert!(plain_path(ok), "{ok}");
        }
        for bad in ["", ".hidden", "a/.hidden", "storage", "storage/x", "start.sh", "RELEASE", "asset-packages.json", "a\\b", "a\nb", "a\rb", "a\0b", "a//b"] {
            assert!(!plain_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_file_is_worth_a_move_from_64_kib_and_only_when_it_is_the_same() {
        let dir = TestDir::new("delta-size-rule");
        let same = [("d/small", MIN_FILE as usize - 1, 0o644), ("d/exact", MIN_FILE as usize, 0o644), ("d/changes", MIN_FILE as usize + 5, 0o644), ("d/extra-old", 70_000, 0o644)];
        let then = [("d/small", MIN_FILE as usize - 1, 0o644), ("d/exact", MIN_FILE as usize, 0o644), ("d/changes", MIN_FILE as usize + 6, 0o644), ("d/new", 70_000, 0o644)];
        let (card, tar) = card_and_tar(&dir, &same, &then);
        assert_eq!(plan(&tar, &card, &[]).moves, ["d/exact"]);
    }

    #[test]
    fn a_program_that_the_card_has_without_its_execute_bit_is_unpacked_not_moved() {
        let dir = TestDir::new("delta-exec-rule");
        let (card, tar) = card_and_tar(&dir, &[("d/tool", 70_000, 0o644), ("d/data", 70_000, 0o644)], &[("d/tool", 70_000, 0o755), ("d/data", 70_000, 0o644)]);
        assert_eq!(plan(&tar, &card, &[]).moves, ["d/data"]);
    }

    #[test]
    fn what_the_start_check_opens_is_never_moved_and_a_folder_with_it_goes_file_by_file() {
        let dir = TestDir::new("delta-keep-rule");
        let files = [("msg/starting.png", 80_000, 0o644), ("msg/other.raw", 80_000, 0o644), ("msg/tiny", 10, 0o644)];
        let (card, tar) = card_and_tar(&dir, &files, &files);
        assert_eq!(plan(&tar, &card, &["msg/starting.png"]).moves, ["msg/other.raw"]);
        assert_eq!(plan(&tar, &card, &[]).moves, ["msg"], "without the rule the folder is the same all through");
    }

    #[test]
    fn nothing_is_moved_when_the_card_or_the_tarball_cannot_be_compared() {
        let dir = TestDir::new("delta-no-plan");
        let files = [("d/big", 80_000, 0o644)];
        let (card, tar) = card_and_tar(&dir, &files, &files);
        // No running program folder (a card that never had one).
        std::fs::remove_file(card.join("app/start.sh")).unwrap();
        assert_eq!(plan(&tar, &card, &[]), Delta::default());
        std::fs::write(card.join("app/start.sh"), "exit 0\n").unwrap();
        assert_eq!(plan(&tar, &card, &[]).moves, ["d"]);
        // Not a tarball, and one that is cut short.
        std::fs::write(dir.join("junk.tar.gz"), b"not a tar").unwrap();
        assert_eq!(plan(&dir.join("junk.tar.gz"), &card, &[]), Delta::default());
        let whole = std::fs::read(&tar).unwrap();
        std::fs::write(dir.join("cut.tar.gz"), &whole[..whole.len() / 2]).unwrap();
        assert_eq!(plan(&dir.join("cut.tar.gz"), &card, &[]), Delta::default());
        assert_eq!(plan(&dir.join("missing.tar.gz"), &card, &[]), Delta::default());
    }

    #[test]
    fn a_link_anywhere_below_a_folder_keeps_the_folder_where_it_is() {
        let dir = TestDir::new("delta-link-rule");
        let files = [("d/a", 80_000, 0o644), ("d/sub/b", 80_000, 0o644)];
        let (card, tar) = card_and_tar(&dir, &files, &files);
        std::os::unix::fs::symlink("a", card.join("app/d/sub/link")).unwrap();
        // The folder stays; the big files that are the same still go on their own, the link stays behind with `d/sub`.
        assert_eq!(plan(&tar, &card, &[]).moves, ["d/a", "d/sub/b"]);
    }
}

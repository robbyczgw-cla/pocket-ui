//! The Android player's session-local Before load moment.
use std::path::{Path, PathBuf};

use super::{preview_path, transaction, undo_path, with_suffix};
use transaction::{Failure, States};

pub struct Undo {
    active: PathBuf,
    staging: PathBuf,
}

impl Undo {
    pub fn new(base: &Path) -> Self {
        Self { active: undo_path(base), staging: with_suffix(base, ".pstate.undo.alt") }
    }

    pub fn path(&self) -> &Path { &self.active }

    pub fn clear(&self) {
        clear(&self.active);
        clear(&self.staging);
    }

    pub fn load(&mut self, core: &mut impl States, bytes: &[u8], preview: impl FnOnce(&Path)) -> Result<(), Failure> {
        let result = transaction::load(core, bytes, |before| {
            let picture = preview_path(&self.staging);
            let unavailable = |_| crate::save_trouble::UNDO_FAILED.to_string();
            match std::fs::remove_file(&picture) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(unavailable(error)),
            }
            crate::state_file::write(&self.staging, before).map_err(unavailable)?;
            preview(&picture);
            Ok(())
        });
        // A successful load replaces Before load. A failed rollback needs the same new moment for recovery.
        // Switch the picker's path only when the staged file is needed, without another disk write.
        if result.is_ok() || result.as_ref().is_err_and(|error| error.recovery) {
            std::mem::swap(&mut self.active, &mut self.staging);
        }
        clear(&self.staging);
        result
    }
}

fn clear(path: &Path) {
    let _ = std::fs::remove_file(preview_path(path));
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Core {
        state: u8,
        refuse: Option<bool>,
        rollback_fails: bool,
        blocked: bool,
    }

    impl States for Core {
        fn snapshot(&mut self) -> Option<Vec<u8>> { Some(vec![self.state]) }
        fn restore(&mut self, data: &[u8]) -> Result<(), (bool, String)> {
            let reached = match self.refuse.take() {
                Some(reached) => reached,
                None if self.rollback_fails => true,
                None => {
                    self.state = data[0];
                    return Ok(());
                }
            };
            if reached { self.state = 99; }
            Err((reached, "Refused".into()))
        }
        fn block_saves(&mut self, blocked: bool) { self.blocked = blocked; }
    }

    #[test]
    fn successful_loads_and_undo_swap_the_current_moment() {
        let dir = tempfile::tempdir().unwrap();
        let mut undo = Undo::new(&dir.path().join("swap"));
        let mut core = Core { state: 1, ..Core::default() };
        undo.load(&mut core, &[2], |_| {}).unwrap();
        undo.load(&mut core, &[3], |_| {}).unwrap();
        let before = crate::state_file::read(undo.path()).unwrap();
        assert_eq!(before, [2]);
        undo.load(&mut core, &before, |_| {}).unwrap();
        assert_eq!(core.state, 2);
        assert_eq!(crate::state_file::read(undo.path()).unwrap(), [3]);
    }

    #[test]
    fn refused_load_keeps_the_previous_undo_and_preview() {
        let dir = tempfile::tempdir().unwrap();
        let mut undo = Undo::new(&dir.path().join("game"));
        let mut core = Core { state: 1, ..Core::default() };
        undo.load(&mut core, &[2], |png| std::fs::write(png, b"first").unwrap()).unwrap();
        let previous = undo.path().to_owned();

        for reached in [false, true] {
            core.refuse = Some(reached);
            let failure = undo.load(&mut core, &[3], |png| std::fs::write(png, b"second").unwrap()).unwrap_err();
            assert!(!failure.recovery && !core.blocked);
            assert_eq!(core.state, 2);
            assert_eq!(undo.path(), previous);
            assert_eq!(crate::state_file::read(undo.path()).unwrap(), [1]);
            assert_eq!(std::fs::read(preview_path(undo.path())).unwrap(), b"first");
        }

        std::fs::create_dir(&undo.staging).unwrap();
        assert!(!undo.load(&mut core, &[3], |_| panic!("a failed snapshot has no preview")).unwrap_err().recovery);
        assert_eq!(core.state, 2);
        assert_eq!(crate::state_file::read(undo.path()).unwrap(), [1]);
        std::fs::remove_dir(&undo.staging).unwrap();

        core.refuse = Some(true);
        core.rollback_fails = true;
        let failure = undo.load(&mut core, &[3], |_| {}).unwrap_err();
        assert!(failure.recovery && core.blocked);
        let recovery = crate::state_file::read(undo.path()).unwrap();
        assert_eq!(recovery, [2]);
        assert!(!preview_path(undo.path()).exists(), "a missing preview must not show an older picture");
        core.rollback_fails = false;
        transaction::recover(&mut core, &recovery).unwrap();
        assert_eq!(core.state, 2);
        assert!(!core.blocked);
        undo.clear();
        assert!(!undo.path().exists() && !previous.exists());
    }
}

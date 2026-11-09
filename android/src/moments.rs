//! Android Load transaction. Refusing a partially applied state must not resume or save an unverified game.
pub const RECOVERY_MESSAGE: &str = "Could not load or restore the game. Load another moment or return to the library. Your earlier saves are kept.";
pub trait States {
    fn snapshot(&mut self) -> Option<Vec<u8>>;
    fn restore(&mut self, bytes: &[u8]) -> Result<(), (bool, String)>;
    fn block_saves(&mut self, blocked: bool);
}
#[derive(Debug)]
pub struct Failure { pub message: String, pub recovery: bool }
/// The caller reads the requested file before this, including when loading Before load itself.
pub fn load(core: &mut impl States, bytes: &[u8], keep: impl FnOnce(&[u8]) -> Result<(), String>) -> Result<(), Failure> {
    let before = core.snapshot().ok_or_else(|| Failure {message:"Cannot preserve the current state".into(), recovery:false})?;
    keep(&before).map_err(|message| Failure {message, recovery:false})?;
    match core.restore(bytes) {
        Ok(()) => { core.block_saves(false); Ok(()) },
        Err((reached, message)) => {
            if reached && core.restore(&before).is_err() {
                core.block_saves(true);
                return Err(Failure {message:RECOVERY_MESSAGE.into(), recovery:true});
            }
            Err(Failure {message, recovery:false})
        }
    }
}
/// After a failed rollback, keep the last verified Before load; never replace it with the damaged core state.
pub fn recover(core: &mut impl States, bytes: &[u8]) -> Result<(), Failure> {
    if core.restore(bytes).is_err() { return Err(Failure {message:RECOVERY_MESSAGE.into(), recovery:true}); }
    core.block_saves(false);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Core { game: u8, calls: usize, rollback_fails: bool, blocked: bool }
    impl States for Core {
        fn snapshot(&mut self) -> Option<Vec<u8>> {Some(vec![self.game])}
        fn restore(&mut self, bytes: &[u8]) -> Result<(), (bool, String)> {
            self.calls += 1;
            self.game = bytes[0];
            if bytes[0] == 9 || self.rollback_fails {Err((true, "Refused".into()))} else {Ok(())}
        }
        fn block_saves(&mut self, blocked: bool) {self.blocked=blocked;}
    }
    #[test]
    fn a_failed_before_load_write_never_touches_the_running_game() {
        let mut core=Core {game:3,calls:0,rollback_fails:false,blocked:false};
        assert!(load(&mut core,&[9], |_| Err("Full".into())).is_err());
        assert_eq!((core.game,core.calls),(3,0));
    }
    #[test]
    fn a_refused_partially_applied_load_restores_the_running_moment() {
        let mut core=Core {game:3,calls:0,rollback_fails:false,blocked:false};
        assert!(load(&mut core,&[9], |before| {assert_eq!(before,[3]);Ok(())}).is_err());
        assert_eq!((core.game,core.calls,core.blocked),(3,2,false));
    }
    #[test]
    fn a_failed_rollback_stays_paused_and_protects_saves_until_a_successful_load() {
        let mut core=Core {game:3,calls:0,rollback_fails:true,blocked:false};
        let error=load(&mut core,&[9], |_| Ok(())).unwrap_err();
        assert!(error.recovery && core.blocked,"A failed rollback cannot be reported as an ordinary refused load");
        core.rollback_fails=false;
        recover(&mut core,&[4]).unwrap();
        assert_eq!((core.game,core.blocked),(4,false));
    }
}

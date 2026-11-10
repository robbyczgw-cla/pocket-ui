//! One cancellable background lane for optional data and its resumable checkpoints.
use std::sync::{Mutex, MutexGuard, TryLockError};

static JOB: Mutex<()> = Mutex::new(());

/// Workers only: wait until the previous writer has finished checkpointing. Never call on the UI thread.
pub fn lock(cancel: &crate::update::Cancel) -> Option<MutexGuard<'static, ()>> {
    loop {
        if cancel.is_cancelled() { return None; }
        match JOB.try_lock() {
            Ok(guard) => return Some(guard),
            Err(TryLockError::Poisoned(error)) => return Some(error.into_inner()),
            Err(TryLockError::WouldBlock) => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaving_a_queued_download_does_not_wait_for_another_packs_checkpoint() {
        let cancel = crate::update::Cancel::new();
        let guard = lock(&crate::update::Cancel::new()).unwrap();
        let request = cancel.clone();
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || { send.send(lock(&request).is_none()).unwrap(); });
        cancel.cancel();
        assert!(receive.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(3))).unwrap(), "B leaves a queued optional download before the active pack has finished");
        drop(guard);
        worker.join().unwrap();
        assert!(lock(&crate::update::Cancel::new()).is_some(), "a later explicit retry can still install");
    }
}

//! Regression for the snapshot gate's ETXTBSY: a previous worker can still be executing a fake program.
#![cfg(target_os = "linux")]

#[path = "common/executable.rs"]
mod executable;

use std::os::unix::fs::MetadataExt;
use std::process::{Child, Command};
use std::sync::{Arc, Barrier};

struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_fixture_can_publish_again_while_its_previous_program_is_running() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sleep");
    let original = std::fs::read("/bin/sleep").unwrap();
    executable::publish(&path, &original).unwrap();
    let mut running = Running(Command::new(&path).arg("60").spawn().unwrap());
    assert!(running.0.try_wait().unwrap().is_none());
    let inode = std::fs::metadata(&path).unwrap().ino();
    // Repeated shots of one fixture write its identical fake curl only once.
    executable::publish(&path, &original).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
    // A changed service answer is a new inode; the old worker keeps running its original program.
    executable::publish(&path, std::fs::read("/bin/true").unwrap()).unwrap();
    assert_ne!(std::fs::metadata(&path).unwrap().ino(), inode);
    assert!(Command::new(&path).status().unwrap().success());
    assert!(running.0.try_wait().unwrap().is_none());
}

#[test]
fn parallel_fixture_updates_do_not_interrupt_fake_programs() {
    let dir = tempfile::tempdir().unwrap();
    let path = Arc::new(dir.path().join("curl"));
    let barrier = Arc::new(Barrier::new(4));
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let (path, barrier) = (Arc::clone(&path), Arc::clone(&barrier));
            scope.spawn(move || {
                barrier.wait();
                for round in 0..32 {
                    executable::publish(&path, format!("#!/bin/sh\n# {worker}:{round}\nexit 7\n")).unwrap();
                    for _ in 0..4 {
                        assert_eq!(Command::new(path.as_ref()).status().unwrap().code(), Some(7));
                    }
                }
            });
        }
    });
}

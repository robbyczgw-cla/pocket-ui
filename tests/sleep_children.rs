//! Exercise the power-key inhibitor across its startup-thread handoff.
#![cfg(target_os = "linux")]
#[path = "../src/sleep_child.rs"]
mod sleep_child;

use std::process::Command;

#[test]
fn the_power_key_inhibitor_stays_held_after_its_startup_thread_hands_it_to_the_ui() {
    use std::{io::{BufRead, BufReader, Write}, process::Stdio};
    let mut child = std::thread::spawn(|| {
        sleep_child::pipe_inhibitor(Command::new("cat").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())).unwrap()
    }).join().unwrap();
    // A completed round-trip proves the inhibitor is alive after the handoff,
    // without relying on a sampled scheduling delay.
    let sent = child.stdin.as_mut().unwrap().write_all(b"still held\n");
    let mut echoed = String::new();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let received = output.read_line(&mut echoed);
    drop(child.stdin.take());
    let status = child.wait().unwrap();
    drop(output);
    assert!(sent.is_ok() && received.is_ok() && echoed == "still held\n" && status.success(),
            "the startup thread's exit must not release the power-key inhibitor: sent={sent:?}, received={received:?}, echo={echoed:?}, status={status}");
}

//! Both hosts must check asynchronous writes and report autosave failures after returning to the shelf.
#[test]
fn android_connects_each_save_warning_surface() {
    let player = include_str!("../android/src/player.rs");
    let shell = include_str!("../android/src/lib.rs");
    let handheld = include_str!("../android/handheld/app_android.rs");
    assert!(player.contains("self.note_save_trouble(self.core.save_trouble())"), "background flush result must be checked");
    assert!(player.contains("save_trouble::report_moment("), "autosave failures must be published");
    assert!(player.contains("trouble.line()"), "the warning must be painted over the game");
    assert!(shell.contains("save_trouble::take_moment("), "the mobile shelf must consume the failure");
    assert!(handheld.contains("save_trouble::take_moment("), "the handheld shelf must consume the failure");
}

/// Every host-side battery flush must consume its result, including future exit/sleep paths.
#[test]
fn every_player_flush_checks_whether_the_save_reached_storage() {
    for (host, source) in [
        ("Linux", include_str!("../src/bin/pocket_core.rs")),
        ("Android", include_str!("../android/src/player.rs")),
    ] {
        for (at, _) in source.match_indices("self.core.write_sram") {
            let after = &source[at.saturating_sub(300)..source.len().min(at + 900)];
            assert!(after.contains("note_save_trouble("), "{} flush silently ignores failures: {}", host, after.lines().next().unwrap());
        }
    }
}

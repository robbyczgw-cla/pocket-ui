#[test]
fn mobile_card_and_player_receive_save_progress() {
    let shell = include_str!("../android/src/lib.rs");
    let ui = include_str!("../android/src/ui.rs");
    let player = include_str!("../android/src/player.rs");
    assert!(ui.contains("Extra::ProgressFromSaves"));
    assert!(shell.contains("reading.progress_from_save("));
    assert!(shell.contains("DirectGame::for_reading("), "a case read must not compute the core's large-disc identity");
    for wire in ["card.progress =", "card.case_art =", "card.case_meter =", "\"progress\":card.progress"] { assert!(shell.contains(wire), "missing {}", wire); }
    assert!(player.contains("progress:r[\"progress\"].as_str()"));
}

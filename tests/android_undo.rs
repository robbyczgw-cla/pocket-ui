#[test]
fn before_load_is_offered_with_picture_and_removed_on_exit() {
    let p = include_str!("../android/src/player.rs");
    assert!(p.contains("slots::clear_undo(&base)"), "a new session discards a crashed session's undo");
    assert!(p.contains("slots::clear_undo(&self.base)"), "Quit must remove Before load");
    assert!(p.contains("\"Before load\""), "slot picker must name Before load");
    assert!(p.contains("let name = v.place_name;"), "the touch picker must use the same Before load label");
    assert!(p.contains("state_file::write(&undo_path, before)"), "undo uses shared state writer");
    assert!(p.contains("slots::preview_path(&undo_path)"), "undo has a preview");
    assert!(p.contains("self.refresh_slots();\n        self.recovery_required = false"), "successful loads expose the new undo immediately");
}

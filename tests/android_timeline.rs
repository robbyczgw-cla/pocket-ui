#[test]
fn android_uses_the_shared_timeline_for_each_capture_and_load_path() {
    let p = include_str!("../android/src/player.rs");
    assert!(p.contains("timeline: pocket_ui::timeline::Timeline"));
    assert!(!p.contains("android_earlier::Writer"));
    for hook in ["timeline::Row", "timeline::note(", "timeline::chip(", "self.earlier_row.step(1)"] {
        assert!(p.contains(hook), "the touch and handheld menus must share the timeline row: {}", hook);
    }
    assert_eq!(p.matches("self.load_earlier()").count(), 2, "both menus must load through the same validated timeline row");
    assert!(!p.contains("with_extension(\"earlier") && !p.contains("with_extension(format!(\"earlier"));
    for hook in ["timeline_play()", "timeline_quiet()", "timeline::to_load(", "timeline::Kind::Started", "timeline::migrate_android("] {
        assert!(p.contains(hook), "missing {hook}");
    }
}

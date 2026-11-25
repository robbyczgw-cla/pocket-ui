//! Which way up the Android app asks for: one rule, `android/java/xyz/pocketui/app/Orientation.java`. With a controller
//! the picture stays the long way round; a square panel (the Anbernic RG Rotate's 720x720) has no long way round, and
//! Android's landscape on it was a quarter turn against the pad (24.11.2026), so there the app follows the system's turn.
//! The rule's own cases are a JVM test (`scripts/android/test/OrientationTest.java`); this test keeps every caller on it.

use std::path::Path;

fn java(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("android/java/xyz/pocketui/app").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn android_square_panels_follow_the_system_turn_through_one_rule() {
    let rule = java("Orientation.java");
    assert!(rule.contains("if (square) return UNSPECIFIED;"), "a square panel asks for nothing of its own");
    assert!(rule.contains("hi * 100L <= lo * 105L"), "square = within 5 %");
    assert!(rule.contains("getRealSize"), "square is the panel's own size, not the window's");
    for name in ["PocketActivity.java", "PlayerActivity.java"] {
        let text = java(name);
        assert!(text.contains("Orientation.wanted(") && text.contains("Orientation.squareDisplay(this)"), "{name} asks Orientation");
        // Nobody picks landscape by itself any more: the only SCREEN_ORIENTATION left is the starting value.
        let picks = text.matches("SCREEN_ORIENTATION_SENSOR_LANDSCAPE").count() + text.matches("SCREEN_ORIENTATION_SENSOR_PORTRAIT").count();
        assert_eq!(picks, 0, "{name} decides the orientation itself instead of Orientation");
    }
    for name in ["AppDrawer.java", "HandheldActivity.java", "Views.java"] {
        assert!(!java(name).contains("setRequestedOrientation"), "{name} turns the screen beside the rule");
    }
}

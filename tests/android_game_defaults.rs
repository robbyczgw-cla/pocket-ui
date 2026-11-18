#[test]
fn every_core_load_host_applies_game_defaults_before_loading() {
    for (host, source) in [
        ("Linux", include_str!("../src/bin/pocket_core.rs")),
        ("Showcase", include_str!("../src/showcase.rs")),
        ("Android", include_str!("../android/src/player.rs")),
    ] {
        let load = source.find("Core::load(").expect(host);
        let defaults = source[..load].rfind("add_game_defaults(");
        assert!(defaults.is_some(), "{} must apply game defaults before Core::load", host);
    }
}

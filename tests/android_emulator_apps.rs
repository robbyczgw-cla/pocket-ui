//! The emulator apps the Android views start (`EmulatorIntents.java`): Android 11 and newer show an app only to an app that
//! names it in its manifest's `<queries>`, so a recipe whose package is missing there is never found, and the game goes to
//! the next app or to none. 26.11.2026 (RG556): ARMSX2 from the Google Play Store is `come.nanodata.armsx2`, not the GitHub
//! build's `com.armsx2`; PS2 games went to NetherSX2, which said "no disc".

const INTENTS: &str = include_str!("../android/java/xyz/pocketui/app/EmulatorIntents.java");
const KEYS: &str = include_str!("../android/java/xyz/pocketui/app/EmulatorKeys.java");
const MANIFEST: &str = include_str!("../android/AndroidManifest.xml");

/// The packages of one profile's recipes, in the order they are tried (`case "<profile>": return new String[][]{{"pkg", …`).
fn recipes(profile: &str) -> Vec<String> {
    let start = INTENTS.find(&format!("case \"{profile}\": return")).unwrap_or_else(|| panic!("no recipes for {profile}"));
    let body = &INTENTS[start..];
    let body = &body[..body.find("};").expect("end of the recipes")];
    body.split("{\"").skip(1).map(|r| r.split('"').next().unwrap().to_string()).collect()
}

/// Every package a recipe names, of every profile (the body of `apps`).
fn all_packages() -> Vec<String> {
    let start = INTENTS.find("private static String[][] apps(").expect("apps()");
    let body = &INTENTS[start..];
    let body = &body[..body.find("default: return").expect("end of apps()")];
    let mut out: Vec<String> = body.split("{\"").skip(1).filter_map(|r| r.split_once("\", ").map(|(p, _)| p.to_string())).collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn both_armsx2_builds_start_ps2_games_before_nethersx2() {
    let ps2 = recipes("ps2");
    let at = |p: &str| ps2.iter().position(|x| x == p).unwrap_or_else(|| panic!("{p} doesn't start PS2 games: {ps2:?}"));
    let (github, play, nether) = (at("com.armsx2"), at("come.nanodata.armsx2"), at("xyz.aethersx2.android"));
    assert!(github < nether && play < nether, "an ARMSX2 is tried before NetherSX2: {ps2:?}");
}

#[test]
fn every_emulator_app_pocket_ui_starts_is_visible_to_it_and_has_the_pause_menu() {
    let packages = all_packages();
    assert!(packages.len() >= 12, "the recipes were not read: {packages:?}");
    for p in &packages {
        assert!(MANIFEST.contains(&format!("<package android:name=\"{p}\"/>")), "{p} is missing from the manifest's <queries>: Android hides it");
        assert!(KEYS.contains(&format!("\"{p}\"")), "{p} is missing from EmulatorKeys.APPS: no pause menu over it");
    }
}

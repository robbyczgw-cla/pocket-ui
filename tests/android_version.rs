//! The Android manifest's versionCode follows its versionName (`scripts/android/build-apk.sh` derives both from Cargo.toml
//! and rewrites them at a version bump, so the build can't abort on drift): MAJOR*1_000_000 + MINOR*10_000 + PATCH*100 + build.

#[test]
fn the_manifest_code_is_derived_from_its_name() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/android/AndroidManifest.xml")).unwrap();
    let attr = |key: &str| manifest.split(&format!("android:{key}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or_default().to_string();
    let (name, code) = (attr("versionName"), attr("versionCode").parse::<u64>().expect("versionCode"));
    let (version, build) = name.split_once("-android.").expect("versionName is <version>-android.<build>");
    let parts: Vec<u64> = version.split('.').map(|n| n.parse().expect("number")).collect();
    let build: u64 = build.parse().expect("build number");
    assert!(parts.len() == 3 && (1..100).contains(&build), "{name}");
    assert_eq!(code, parts[0] * 1_000_000 + parts[1] * 10_000 + parts[2] * 100 + build, "versionCode of {name}; run scripts/android/build-apk.sh or fix by hand");
}

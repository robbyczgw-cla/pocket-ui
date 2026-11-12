//! The Android manifest's versionCode follows its versionName (`scripts/android/build-apk.sh` derives both from Cargo.toml
//! and rewrites them at a version bump, so the build can't abort on drift): MAJOR*1_000_000 + MINOR*10_000 + PATCH*100 + build.

#[test]
fn the_manifest_code_is_derived_from_its_name() {
    let manifest = std::fs::read_to_string(format!("{}/android/AndroidManifest.xml", std::env::var("CARGO_MANIFEST_DIR").unwrap())).unwrap();
    let attr = |key: &str| manifest.split(&format!("android:{key}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or_default().to_string();
    let (name, code) = (attr("versionName"), attr("versionCode").parse::<u64>().expect("versionCode"));
    let (version, build) = name.split_once("-android.").expect("versionName is <version>-android.<build>");
    let parts: Vec<u64> = version.split('.').map(|n| n.parse().expect("number")).collect();
    let build: u64 = build.parse().expect("build number");
    assert!(parts.len() == 3 && (1..100).contains(&build), "{name}");
    assert_eq!(code, parts[0] * 1_000_000 + parts[1] * 10_000 + parts[2] * 100 + build, "versionCode of {name}; run scripts/android/build-apk.sh or fix by hand");
}

/// The app's Android identity is `xyz.pocketui.app` (pocketui.xyz) and can't change after the first public APK: Android
/// treats another name as another app, and saves don't follow. The Java classes and the native functions they call are
/// looked up by this name, so a file left on the old one is an app that stops at its first start.
#[test]
fn the_package_name_is_the_same_in_the_manifest_the_java_classes_and_the_native_names() {
    const PACKAGE: &str = "xyz.pocketui.app";
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest = std::fs::read_to_string(format!("{root}/android/AndroidManifest.xml")).unwrap();
    assert!(manifest.contains(&format!("package=\"{PACKAGE}\"")), "AndroidManifest.xml names another package");
    let folder = format!("{root}/android/java/{}", PACKAGE.replace('.', "/"));
    let classes: Vec<_> = std::fs::read_dir(&folder).unwrap_or_else(|e| panic!("{folder}: {e}")).flatten().map(|e| e.path()).collect();
    assert!(classes.len() >= 2, "the Java classes live in {folder}");
    for class in &classes {
        let text = std::fs::read_to_string(class).unwrap();
        assert!(text.starts_with(&format!("package {PACKAGE};")), "{} declares another package", class.display());
    }
    let mut others = Vec::new();
    fn java_files(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() { java_files(&path, found) } else { found.push(path) }
        }
    }
    java_files(std::path::Path::new(&format!("{root}/android/java")), &mut others);
    assert_eq!(others.len(), classes.len(), "Java files outside {folder}: {others:?}");
    let mangled = format!("Java_{}_", PACKAGE.replace('.', "_"));
    let mut natives = 0;
    for file in ["lib.rs", "player.rs"] {
        let text = std::fs::read_to_string(format!("{root}/android/src/{file}")).unwrap();
        for name in text.split("extern \"system\" fn ").skip(1).map(|rest| rest.split('(').next().unwrap_or_default()) {
            assert!(name.starts_with(&mangled), "{file}: {name} is not a function of {PACKAGE}");
            natives += 1;
        }
    }
    assert!(natives >= 10, "the native functions of both Activities were found ({natives})");
    let script = std::fs::read_to_string(format!("{root}/scripts/android/build-apk.sh")).unwrap();
    assert!(script.contains(&format!("android/java/{}/*.java", PACKAGE.replace('.', "/"))), "build-apk.sh compiles another folder");
}

/// pocket-ui can be the home screen of an Android handheld: Android offers it at the Home button and asks the player.
/// Only the launcher Activity; the player stays private.
#[test]
fn the_launcher_is_offered_as_home_screen_and_the_player_is_not() {
    let manifest = std::fs::read_to_string(format!("{}/android/AndroidManifest.xml", std::env::var("CARGO_MANIFEST_DIR").unwrap())).unwrap();
    let activity = |name: &str| {
        let start = manifest.find(&format!("android:name=\"{name}\"")).unwrap_or_else(|| panic!("{name}"));
        let rest = &manifest[start..];
        rest[..rest.find("</activity>").or_else(|| rest.find("/>")).unwrap()].to_string()
    };
    let launcher = activity(".PocketActivity");
    let home = launcher.split("<intent-filter>").find(|f| f.contains("category.HOME")).expect("PocketActivity has a HOME filter");
    assert!(home.contains("action.MAIN") && home.contains("category.DEFAULT"), "Android lists a home only with MAIN and DEFAULT");
    assert!(launcher.contains("category.LAUNCHER"), "the app icon stays");
    assert!(launcher.contains("singleTask"), "one launcher instance, also as home");
    assert!(!activity(".PlayerActivity").contains("category.HOME"));
}

/// `build-apk.sh` once made a new signing key when the release key was missing (a build on another machine, or after a data
/// loss): an APK that Android treats as another app, with no update over the one players have. Now it stops with a clear
/// message unless a new key is asked for on purpose. A QA or test package with its own key is unaffected. The test uses a
/// dummy key folder (empty files, never a real key) and `ANDROID_BUILD_KEY_CHECK_ONLY=1`, which stops right after the check.
#[test]
fn build_apk_stops_without_a_signing_key_unless_a_new_one_is_asked_for() {
    let root = env!("CARGO_MANIFEST_DIR");
    let manifest_before = std::fs::read_to_string(format!("{root}/android/AndroidManifest.xml")).unwrap();
    let run = |dir: &std::path::Path, allow: bool| {
        let mut cmd = std::process::Command::new("bash");
        cmd.arg(format!("{root}/scripts/android/build-apk.sh")).env("ANDROID_SIGNING_DIR", dir).env("ANDROID_BUILD_KEY_CHECK_ONLY", "1").env_remove("ANDROID_ALLOW_NEW_KEY");
        if allow {
            cmd.env("ANDROID_ALLOW_NEW_KEY", "1");
        }
        cmd.output().unwrap()
    };
    let none = std::env::temp_dir().join(format!("pocket-no-key-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&none);
    std::fs::create_dir_all(&none).unwrap();
    // No key: stopped, with words that say why and what to do; nothing was created.
    let out = run(&none, false);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a missing key must stop the build:\n{err}");
    assert!(err.contains("no signing key") && err.contains("ANDROID_SIGNING_DIR") && err.contains("ANDROID_ALLOW_NEW_KEY=1"), "the message says what to do:\n{err}");
    assert_eq!(std::fs::read_dir(&none).unwrap().count(), 0, "nothing is made in the key folder");
    // A key folder that does not even exist stops as well.
    assert!(!run(&none.join("missing"), false).status.success());
    // A QA package's own key (dummy files): carries on.
    std::fs::write(none.join("release.jks"), b"").unwrap();
    std::fs::write(none.join("store-password"), b"").unwrap();
    let out = run(&none, false);
    assert!(out.status.success(), "a folder with its own key is not stopped:\n{}", String::from_utf8_lossy(&out.stderr));
    std::fs::remove_dir_all(&none).unwrap();
    // On purpose: a new throwaway key is allowed (the check passes; the check-only run still creates nothing).
    let fresh = std::env::temp_dir().join(format!("pocket-new-key-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).unwrap();
    assert!(run(&fresh, true).status.success(), "ANDROID_ALLOW_NEW_KEY=1 lets a throwaway key be made");
    std::fs::remove_dir_all(&fresh).unwrap();
    // The check ran before the script touched anything: the manifest is as it was.
    assert_eq!(std::fs::read_to_string(format!("{root}/android/AndroidManifest.xml")).unwrap(), manifest_before, "the key check comes before the version rewrite");
}

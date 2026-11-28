//! Every XML file aapt reads for the APK (the manifest and android/res) is well-formed. A comment with "--" in it
//! (26.11.2026: `pocket-splash --android-icon` in drawable/splash_window.xml) passed every test and only failed when the
//! APK was packed; the gate never runs aapt.

fn xml_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            xml_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "xml") {
            out.push(path);
        }
    }
}

#[test]
fn every_android_resource_and_the_manifest_are_well_formed_xml() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("android");
    let mut files = vec![root.join("AndroidManifest.xml")];
    xml_files(&root.join("res"), &mut files);
    assert!(files.len() > 3, "{files:?}");
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        if let Err(e) = roxmltree::Document::parse(&text) {
            panic!("{} is not well-formed XML: {e}", file.display());
        }
    }
}

#[test]
fn the_start_shows_the_boot_screens_picture_not_the_app_icon() {
    let v31 = include_str!("../android/res/values-v31/styles.xml");
    let pocket = &v31[v31.find("name=\"PocketTheme\"").unwrap()..];
    let pocket = &pocket[..pocket.find("</style>").unwrap()];
    assert!(pocket.contains("windowSplashScreenAnimatedIcon\">@drawable/splash_mark"), "{pocket}");
    assert!(pocket.contains("windowSplashScreenBackground\">#000000"), "{pocket}");
    let before = include_str!("../android/res/values/styles.xml");
    assert!(before.contains("@drawable/splash_window"), "before Android 12 the window's background shows it");
}

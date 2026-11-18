//! Real-device legacy files are kept outside the repository. Run explicitly with POCKETUI_LEGACY_EARLIER.
#[test]
#[ignore = "requires a private legacy state fixture; never ship player states in the repository"]
fn a_real_legacy_state_survives_import_byte_for_byte() {
    use pocket_ui::{slots, timeline};
    let source = std::path::PathBuf::from(std::env::var_os("POCKETUI_LEGACY_EARLIER").expect("fixture file"));
    let root = tempfile::Builder::new().prefix("pocket-legacy-import-").tempdir().unwrap();
    let base = root.path().join("Game.with.dots");
    let old = base.with_extension("earlier0");
    std::fs::copy(&source, &old).unwrap();
    let before = std::fs::read(&old).unwrap();
    let modified = std::fs::metadata(&old).unwrap().modified().unwrap();
    let preview = slots::preview_path(&source);
    if preview.is_file() { std::fs::copy(&preview, slots::preview_path(&old)).unwrap(); }
    timeline::migrate_android(&base).unwrap();
    let entries = timeline::entries(&base);
    assert_eq!(entries.len(), 1);
    assert_eq!(std::fs::read(&entries[0].path).unwrap(), before);
    assert_eq!(pocket_ui::state_file::read(&entries[0].path).unwrap(), pocket_ui::state_file::read(&source).unwrap(), "the ordinary state reader must load the imported bytes too");
    assert_eq!(entries[0].at, modified);
    assert!(!old.exists());
    timeline::migrate_android(&base).unwrap();
    assert_eq!(timeline::entries(&base), entries);
    if preview.is_file() { assert_eq!(std::fs::read(slots::preview_path(&entries[0].path)).unwrap(), std::fs::read(preview).unwrap()); }
}

#[test]
fn an_orphan_destination_preview_cannot_consume_the_legacy_state_or_picture() {
    use pocket_ui::{slots, timeline};
    let root = tempfile::Builder::new().prefix("pocket-legacy-orphan-").tempdir().unwrap();
    let base = root.path().join("Game");
    let old = base.with_extension("earlier0");
    let destination = timeline::path(&base, timeline::Name { kind: timeline::Kind::Every, n: 1 });
    let old_preview = slots::preview_path(&old);
    let orphan = slots::preview_path(&destination);
    std::fs::write(&old, b"legacy state").unwrap();
    std::fs::write(&old_preview, b"legacy picture").unwrap();
    std::fs::write(&orphan, b"unrelated picture").unwrap();
    timeline::migrate_android(&base).unwrap();
    assert!(old.is_file(), "a preview collision must leave the legacy moment alone");
    assert_eq!(std::fs::read(old).unwrap(), b"legacy state");
    assert_eq!(std::fs::read(old_preview).unwrap(), b"legacy picture");
    assert_eq!(std::fs::read(orphan).unwrap(), b"unrelated picture");
    assert!(!destination.exists());
}

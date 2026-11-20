//! A locally built core must carry its matching source and notices through an update.
use std::process::Command;

#[test]
fn dsperate_packages_reject_missing_sources_notices_and_changed_binaries() {
    let output=Command::new("python3").args(["-I","tests/dsperate_package.py"])
        .current_dir(env!("CARGO_MANIFEST_DIR")).output().unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
}

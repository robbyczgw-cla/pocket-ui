//! Fake programs must be published without holding writable descriptors in the test process.

use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn publish(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let contents = contents.as_ref();
    // The offline curl is shared by all shots of one fixture. Leave it alone after its first publication.
    if std::fs::read(path).is_ok_and(|old| old == contents)
        && std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o7777 == 0o755)
    {
        return Ok(());
    }
    let staging = tempfile::Builder::new().prefix("fixture-executable-").tempdir_in(path.parent().unwrap_or(Path::new(".")))?;
    // A child owns every writable descriptor. Its cat closes the new inode before mv makes it executable by
    // other workers; a previous worker can continue executing the old inode after the atomic replacement.
    let mut writer = Command::new("sh")
        .args(["-c", r#"cat > "$1" && chmod 755 "$1" && mv -f "$1" "$2""#, "sh"])
        .arg(staging.path().join("program"))
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()?;
    let result = writer.stdin.take().unwrap().write_all(contents);
    let status = writer.wait()?;
    result?;
    if !status.success() {
        return Err(io::Error::other(format!("publishing {}: {status}", path.display())));
    }
    Ok(())
}

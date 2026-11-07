/// The crate's folder as cargo says it when the test RUNS, not when it was compiled: a test binary reused from a
/// shared build folder (the gate's, another copy's) would otherwise point into a folder that is gone. Used as
/// `{ROOT}` in `format!` and as `Path::new(&ROOT)`.
struct Root;

impl Root {
    fn dir() -> String {
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string())
    }
}

impl std::fmt::Display for Root {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&Root::dir())
    }
}

impl AsRef<std::ffi::OsStr> for Root {
    fn as_ref(&self) -> &std::ffi::OsStr {
        // Leaked once per process: cargo's value doesn't change while the test runs.
        static DIR: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        std::ffi::OsStr::new(DIR.get_or_init(Root::dir))
    }
}

impl AsRef<std::path::Path> for Root {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(AsRef::<std::ffi::OsStr>::as_ref(self))
    }
}

static ROOT: Root = Root;

#[path = "../../src/test_clock.rs"]
mod test_clock;

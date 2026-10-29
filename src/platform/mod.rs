//! OS adapters. Keep target selection here instead of in shared UI and matching code.
#[cfg(target_os = "linux")]
#[path = "linux_http.rs"]
pub(crate) mod http;

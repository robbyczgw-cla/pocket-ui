//! One background job at a time; the Activity transports bytes, the shared store checks and installs. The job is the one
//! the handheld view runs too (`pocket_ui::android_core_run`).
pub use pocket_ui::android_core_run::Run;

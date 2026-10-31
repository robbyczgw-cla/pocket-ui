//! ROCKNIX system queries, only compiled on Linux.
pub fn is_speaker() -> bool {
    crate::audio_route::current(&crate::audio_route::Real) == crate::audio_route::Output::Speaker
}
pub fn sink_volume() -> Option<(u32, bool)> { crate::sleep::sink_volume() }

/// The desktop Wikipedia request, kept identical to the original worker's curl invocation.
pub fn wiki_json(url: &str) -> Result<Vec<u8>, String> {
    let ua = crate::wiki_text::USER_AGENT;
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "10", "-H", &format!("User-Agent: {ua}"), "-H", "Accept: application/json", url])
        .output().map_err(|_| "no curl".to_string())?;
    if !out.status.success() {
        return Err(if out.status.code() == Some(22) { "no article".into() } else { "offline".into() });
    }
    Ok(out.stdout)
}

/// Console labels at the host-system boundary.
pub fn console_buttons(system:&str)->&'static [(u8,&'static str)] {crate::hostsys::find(system).map_or(&[],|h|h.buttons)}

/// The screenshot action belongs to the host input model; shared box painting stays platform-neutral.
pub fn screenshot_hint() -> &'static str { "No pictures yet. HOME + A in the game takes one." }

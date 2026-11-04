//! Exercise the actual old/new pause fragments from the shipped core patch.
#[test]
fn pause_drains_more_than_one_frame_before_waiting() {
    let patch = include_str!("../android/patches/ppsspp-pause-drain.patch");
    let fragment = |prefix: char| patch.lines().filter(|line| line.starts_with(prefix) && !line.starts_with(&prefix.to_string().repeat(3)))
        .map(|line| &line[1..]).collect::<Vec<_>>().join("\n");
    let old = fragment('-');
    let fixed = fragment('+');
    let source = format!(r#"
#include <functional>
#include <stdexcept>
#include <cassert>
enum class EmuThreadState {{ RUNNING, PAUSE_REQUESTED, PAUSED }};
EmuThreadState emuThreadState;
int frames, polls;
void sleep_ms(int, const char *) {{ if (++polls > 10) throw std::runtime_error("deadlock"); }}
struct Context {{
    bool ThreadFrame(bool) {{
        if (frames == 2) return false;
        if (++frames == 2) emuThreadState = EmuThreadState::PAUSED;
        return true;
    }}
    // GraphicsContext::ThreadFrameUntilCondition at the pinned PPSSPP revision,
    // minus logging. Drain the queue even after the pause is acknowledged.
    void ThreadFrameUntilCondition(std::function<bool()> stopped) {{
        bool exitOnEmpty = false;
        while (true) {{
            if (!exitOnEmpty && stopped()) exitOnEmpty = true;
            bool retval = ThreadFrame(false);
            if (!retval && exitOnEmpty) break;
            else sleep_ms(5, "exit poll");
        }}
    }}
}} context, *ctx = &context;
void pause_old() {{ {old} }}
void pause_fixed() {{ {fixed} }}
int main() {{
    emuThreadState = EmuThreadState::PAUSE_REQUESTED;
    bool stuck = false;
    try {{ pause_old(); }} catch (const std::runtime_error &) {{ stuck = true; }}
    assert(stuck && frames == 1);
    frames = polls = 0;
    emuThreadState = EmuThreadState::PAUSE_REQUESTED;
    pause_fixed();
    assert(frames == 2 && emuThreadState == EmuThreadState::PAUSED);
}}
"#);
    let dir = tempfile::tempdir().unwrap();
    let cpp = dir.path().join("pause.cpp");
    let exe = dir.path().join("pause");
    std::fs::write(&cpp, source).unwrap();
    let built = std::process::Command::new("c++").args(["-std=c++11", "-o"]).arg(&exe).arg(&cpp).output().unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    assert!(std::process::Command::new(exe).status().unwrap().success());
}

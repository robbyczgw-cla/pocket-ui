# Still menus

The UI event loop blocks in `ControlFlow::Wait` or `WaitUntil` for the earliest pending change. A still menu needs no
refresh-rate timer. A visible clock minute, changed battery or Wi-Fi status, worker result that changes the picture,
input, surface recovery, or an animation in progress may request a frame.

- `marquee.rs`: a long selected name rests, travels, rests at its end, and returns once. The two still rests have
  deadlines; the final position has none. Reselecting or changing layout restarts it; clock frames do not.
  Every primary paint clears the previous deadline, including a new-version card that covers a scrolling title.
  Covered Shelf/Cabinet artwork and animations also stop driving frames while those cards are open, even when the
  card appeared before an artwork read finished and the player uses the details page instead of the box warmer.
- `picture_cycle.rs`: Split visits its available pictures and returns to box art once, then has no timer.
- `main.rs`: toasts animate on entry/exit and wait between them; expiry explicitly removes the final picture.
  Notices are static. The Battery page refreshes when its deadline is due. Hidden Shelf/Cabinet animations do not
  drive the loop. Raw controller diagnostics and unused stick drift request no menu frame; the manual still scrolls.
- `app_sync.rs`: a still sync confirmation has one expiry deadline.
- Menu sound stays warm during browsing, with a two-second activity lease. After quiet it closes, so there is no
  silent audio stream keeping PipeWire running. The next sound reopens on the sound thread; input handling never waits.
- `app_second_screen.rs`: animation frames are paced from that window's last successful presentation. Each window
  tracks its own decoded-image dependencies, so an off-screen image does not repaint the shelf. Selection debounce,
  the box-to-info turn, bounded initial image retries and surface-recovery deadlines end when their work is done.

Clock minutes and status changes already have independent timers / worker notifications. Battery safety, controller
hotplug, fan safety and network/file watchers still sleep on their own timers or kernel events. The optional idle
slideshow deliberately changes pictures: its existing fade / Ken Burns pacing remains, with no shelf-rate loop
underneath it. Keyboards have no blinking cursor timer. Cozy motion speeds, easing and launch timing are unchanged.

## Repeating a measurement

Use a permitted test device and its device lock. Keep it muted, rumble disabled and backlight dark; keep the compositor
output active so frames can still be presented. Copy mutable data before running and restore it afterwards. Use a
separate `POCKETUI_DATA` copy for both binaries, with the same library, settings and selection. No game needs to run.

Enable `POCKETUI_TRACE=<tmpfs-file>`. Let startup and selection animations settle before leaving the menu untouched
for 60 seconds, below the normal screen-off deadline. `scripts/baseline-device.sh idle 60 <label> <process-name>` reads
all surviving threads' CPU ticks and voluntary/involuntary context-switch deltas. CPU is percent of one core;
voluntary context switches per second are the wakeup proxy, not hardware interrupt counts. Set `UILOG` to this run's
log so an old sleep entry cannot mislabel the measurement. Reject a run crossing screen-off or a process restart.

Count `frame` records inside the same monotonic measurement interval; these are emitted after successful primary
presentations. `second-frame` records count lower-window presentations. `event` records identify worker wakeups by
`UserEvent` discriminant without dumping image payloads. Keep raw logs and binary hashes with the private report.
A minute-long battery percentage delta is too coarse to extrapolate into a reliable drain-per-hour claim.

Logic regressions: `cargo test --profile gate --lib idle_shelf_requests_no_redraw` checks that a long title stays
settled through further clock/battery frames and that Split's preview stops cycling. No screenshot is compared.

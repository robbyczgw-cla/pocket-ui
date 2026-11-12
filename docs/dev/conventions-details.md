# Conventions, details and regressions

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you touch Wi-Fi activation, text fit / long titles or small-panel layout measuring.*

## Conventions, details

SM8550 experimental device profiles (`device.rs`: Pocket 6/TOP-DPAD, Nova and the Odin 2 family, [DEVICES.md](../DEVICES.md)); the Anbernic H700 handhelds (`device.rs`: one `Model::H700` with a row per model in `H700_BOARDS`, [DEVICES.md](../DEVICES.md#anbernic-h700-handhelds-2026-10-27-installable-2026-10-29-experimental); `hostsys::route_on` keeps N64/PSP/Dreamcast/DS on ROCKNIX there; `os_compat` only logs the H700 image's missing time-zone helper and `quantum`; `has_fan`, `has_touch`, `has_sticks`, `has_wifi`; the fan page, the LEDs row and the Wi-Fi page read them; `system_ctl.rs` reports `Radio::Absent` instead of mocking a radio)
reuse the existing layouts. `sleep_settings` normalises their buttons/timers and `sleep` keeps every path at
screen off, with an optional dark-time power-off and no RTC alarm. `power::set_fan`, `fan_curve::Engine` and
`hand_back`, plus launcher recovery, leave SM8550 PWM and thermal trips to ROCKNIX. CPU policies are discovered.

### Wi-Fi activation collision regressions

`wifi_activation::tests` advances a fake clock through boot/resume, joining an activation (success, failure and
50-second timeout), and switching SSIDs after disconnect. `wifi::imp::activation_tests` uses `testutil::TestDir` and
`write_executable` for nmcli: numeric states with translated descriptions, SSID lookup by UUID, observing an
existing activation through to success, and disconnect-before-up ordering. A controlled wait changes the fake
state without a sleep. `setup::friendly::maps_nmcli_errors_to_plain_language` checks every error class and its
German translation. `tests/user_journeys.rs::wifi_timeout_explains_the_next_step_without_nmcli_diagnostics` renders
the ordinary failed-connect handler (`setup:wifi-timeout`), which is also in the German/layout matrices.
`activation_tests::nm_with_iwd` is an nmcli stand-in for NetworkManager with iwd: profiles as files, iwd's mirrored
`<SSID>.psk`, and iwd's endless retry of a refused key (the device stays in `config`); the fresh-card incident of
2026-10-27 runs through it (wrong then right password, wrong twice, an old profile overwritten, a double forget).
`tests/user_journeys.rs::first_wifi_wrong_password_then_the_right_one_then_forget` does the same first start with the
real binary and stand-ins for `nmcli`, `ip`, `iw` and `curl` on PATH (`wifi_programs`).
`net_state::tests::watchdog_queued_during_a_user_activation_is_discarded` and
`a_radio_switch_stops_the_watchdogs_scan_before_it_can_connect` use channel/gate barriers to verify the shared
lane and cancellation when the player takes over.

The review regressions also cover `a_typed_password_replaces_the_old_activation_and_is_tried_once`,
`a_join_failed_for_missing_secrets_reopens_the_password_prompt`, and
`a_join_can_finish_after_networkmanagers_dhcp_timeout`. The joined error carries `GENERAL.REASON` through to the
password prompt. `radio_off_cancels_a_running_watchdog_activation_within_one_second` gates a real fake nmcli
process on a Unix socket, switches the fake radio off and condition-waits for cancellation; it checks `-w 30`
and the shared lane without sleeps. Network-not-found and device-not-ready errors have separate wording.

Odin 2 Mini/Portal use Xbox menu defaults (saved choices win); safe mode follows the device labels.
`pad::linux` also reads InputPlumber keyboard HOME/BACK aliases; paddle support is not claimed with the plain ds5 target.

Long-title regression: `cargo test --profile gate --test text_size long_game_titles` checks the
selected search row, switcher and game card with long Pokémon and Super Mario metadata, plus Burnout
Revenge, at Normal/Large, English/German, dark/light and 1240×1080/1280×960. Set
`POCKETUI_TEXT_FIT_OUTPUT` to retain the rendered PNGs. `POCKETUI_TEXT_FIT=1 scripts/size-sweep.sh`
uses the same long-title fixture for a wider sweep. Marquee unit tests compare the first frame and
selection resets pixel for pixel with one ellipsized text draw. The switcher reduces picture height on
960-line screens to keep the title and metadata clear of the hints; its neighbouring cards intentionally
peek through the screen edge, while selected-card overflow and all text intersections remain checked.
The title audit also covers the existing `Text::fit` paths in the pause header, Curator lists,
settings/storage/history rows and device web pages; the game card keeps its two-line wrapped header.

### Measuring small-panel layouts

`POCKETUI_SNAPSHOT_SIZE=640x480` writes a physical 640×480 PNG after painting the supported logical canvas.
`POCKETUI_SNAPSHOT_NATIVE=1` bypasses scaling for snapshots only, to compare raw-layout findings; never set it for
a scaled-layout sweep. `POCKETUI_FRAME_BENCH=1` repeats the settled final snapshot page 105 times (four warmups),
printing median logical paint, median paint plus live reduction, and p95 frame thread CPU time in microseconds.
Use `home`, `settings` or `main`; branches that return an earlier snapshot do not run this benchmark.
For large game lists, `--snapshot bench-shelf:<look>:<system> ignored.png` measures the first frame after
120 forward and 120 backward moves (Cabinet Right/Left, other looks Down/Up), after two warming passes.
`scripts/perf-library.py OUT 5000` creates a disposable generated library and covers; set `POCKETUI_ES_SYSTEMS`
and `POCKETUI_DATA` to its printed paths. Use `scripts/latency.sh -- --nav-system gbc` for live cold-cover scrolling;
the snapshot bench intentionally warms images first. See [LATENCY.md](../LATENCY.md). It excludes
startup, PNG compression and compositor presentation. See [H700-SCALED-LAYOUT.md](../H700-SCALED-LAYOUT.md).
`tests/vdev_profiles.rs` evaluates only the H700 profile-selection fragment of `docker/vdev/entrypoint.sh` under
`sh` and checks its screen, model, compatible, SoC, lid and absent touch against Rust device profiles; no Docker boot.

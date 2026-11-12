# Brick 0.4: integration evidence

This records checks on one original TrimUI Brick (A133P, 1024×768). Smart Pro and Brick Pro remain experimental;
these measurements do not validate their hardware. No firmware, installed app, game or save was replaced.

## Power-button sleep

The eleven commits from `feat/brick-sleep-default` were integrated with current main. The power-menu dispatcher retains
main's board-specific shutdown handling and delegates suspend to `brick-suspend`. The kernel transition and wake cleanup,
the power-key decoder, `app_sleep_power` and `power_menu` retain the physically tested `9ea38d32` path; the intentional
policy change is the default `power_button=Sleep`. The integration additionally retains `BRICK_INPUTD_SUSPEND`, pauses inputd
before service shutdown (the Pro handshake), and lets host fixtures replace global sync. Success, refusal and interruption
all prove input is released before the network returns. Subsequent main changes in `sleep.rs` add host-owned sleep observation and the safe wake-only FIFO
verb; the Brick suspend/resume ordering is unchanged. A recorded explicit Screen off choice still wins.

The new default's regression fails with the earlier ScreenOff constant and passes with Sleep. Old forced
`short_press=screen_off` files migrate to the new default; an explicit `power_button=screen_off` remains screen-only.

On 2026-11-10 the integrated UI and core ran from RAM with private game/config/save paths. Two RTC-bounded cycles slept
14.654 and 14.026 seconds and woke with `pm_wakeup_irq=344` (RTC). Each confirmed its save before suspend, returned to the
pause menu with the core still paused, restored the Wi-Fi default route, and added **zero** xruns, write errors, recoveries,
underruns or dropped frames. This is RTC evidence, not a new physical key press. The earlier physical power-key wake remains
`pm_wakeup_irq=379` ([the original evidence](TRIMUI-BRICK-SLEEP.md#waking-by-the-power-button-checked-0811)).

The firmware logged one `mmc2: error -22 whilst initialising SDIO card`. `mmc2` is the Wi-Fi SDIO device; the default route
returned. The general crash checker correctly marks that run as failed because it rejects MMC errors; the cycle evidence
and this known firmware recovery are reported separately. No filesystem/SD-card CRC error, kernel Oops or restart occurred.

After preserving the Pro input-marker override and its earlier polling pause, another RTC cycle slept 15.028 seconds,
woke with IRQ 344, returned paused, restored the default route and added zero audio errors. It again logged the SDIO
initialisation error and additionally a kernel **WARNING** in `net/core/skbuff.c:4383`, `skb_try_coalesce`, with an
`xradio_mac` receive stack. This is an unresolved firmware/network warning, not a green crash-free run. The original UI,
boot ID and pstore remained intact and restoration passed. No further device cycles were run after that finding.

## Configurable F1/F2 and lights

Settings → Sound & feel → Lights & buttons exposes F1 button and F2 button. Left/Right saves a global shortcut.
The original defaults remain Quick save / Quick load. Tests cover absence of a choice, existing choices and reload.
The navigation explorer reaches each row, changes it and restarts the UI with the saved choices.

The device probe uses `Decoder::new` on the real `TRIMUI Player1` descriptor and stock identity, then supplies local synthetic
edges to that decoder. It never injects events into the device. A missing old choice file produces Save / Load;
the UI-saved Load / Screenshot choices produce exactly one corresponding action for codes 317 / 318. The aliases 59 / 60
also pass save/load/screenshot checks, including repeat suppression. This complements the previously hand-tested keys.

The pad borrows Xbox vendor/product identifiers and advertises unused stick axes. Those capabilities cannot distinguish a
Brick from Brick Pro. `a52a4e4d` already verifies the stock identity before turning thumb codes into F keys; Pro and unknown
identities retain L3/R3. The earlier Xbox-ID regression and this Pro boundary stay covered by the captured input corpus.

Default lights are Off. Colour mode goes dark with the idle screen and restores its saved output after wake. An explicitly
chosen Battery & charging mode or individual battery zone is retained. The firmware's `frame_hex` read includes NUL padding;
the test ignores padding and still rejects every nonzero colour digit.

## Volume: real codec readbacks, silent

The card helpers were run against the real codec with private remembered levels. The DAC stayed zero, both output switches
were off and the speaker amp was muted throughout. This measures the control curve, **not acoustic SPL, distortion or
frequency response**. The vendor TLV has reversed direction: raw zero is the existing maximum, each raw tick attenuates by
1.16 dB. DAC 160 and the existing maximum are unchanged in normal playback.

| Volume | Codec raw | dB relative to maximum | Increase from previous step |
|---:|---:|---:|---:|
| 5% | 31 | -35.96 | — |
| 10% | 30 | -34.80 | 1.16 |
| 15% | 28 | -32.48 | 2.32 |
| 20% | 27 | -31.32 | 1.16 |
| 25% | 26 | -30.16 | 1.16 |
| 30% | 25 | -29.00 | 1.16 |
| 35% | 24 | -27.84 | 1.16 |
| 40% | 22 | -25.52 | 2.32 |
| 45% | 21 | -24.36 | 1.16 |
| 50% | 20 | -23.20 | 1.16 |
| 55% | 18 | -20.88 | 2.32 |
| 60% | 16 | -18.56 | 2.32 |
| 65% | 14 | -16.24 | 2.32 |
| 70% | 12 | -13.92 | 2.32 |
| 75% | 10 | -11.60 | 2.32 |
| 80% | 8 | -9.28 | 2.32 |
| 85% | 6 | -6.96 | 2.32 |
| 90% | 4 | -4.64 | 2.32 |
| 95% | 2 | -2.32 | 2.32 |
| 100% | 0 | 0.00 | 2.32 |

Every step increases gain. There is no discontinuity larger than two hardware ticks (2.32 dB). The lower half intentionally
uses gentler steps, so it is not a single uniform slope. Curve 3 is retained; these numbers do not justify a gain increase.
`python3 scripts/audio-bench/brick-volume.py READBACKS.json --csv levels.csv` regenerates the table from measured readbacks.
The 0% mute path remains covered by `scripts/test-brick-volume.py` (DAC zero and both output switches disabled).

## Session restoration

Each run holds the device lock and has a bounded guard. A device-side input watcher restores on a real press even if Wi-Fi
is unavailable. The host observer also cancels the run. The persistent development-access markers remain.
Afterwards the original UI runs, config and protected-file hashes match, CPU policy matches, mixer controls match, amp mute
matches, and session guards have ended. Helpers, scratch ROM/config/saves and RAM backups are removed; evidence stays on
the host. The volume session's full ALSA state file was byte-identical before and after.

## Network follow-up and probe-status correction

The existing helper already stops the stock WPA service and takes `wlan0` down before the kernel suspend write. After
resume it raises `wlan0`, starts the stock service and renews DHCP in the background. The original Brick's
`/etc/init.d/wpa_supplicant` uses the same down/up operations. NextUI's
[suspend script](https://github.com/LoveRetro/NextUI/blob/main/skeleton/SYSTEM/tg5040/bin/suspend) calls its
[Wi-Fi wrapper](https://github.com/LoveRetro/NextUI/blob/main/skeleton/SYSTEM/tg5040/etc/wifi/wifi_init.sh), which additionally
blocks/unblocks rfkill. Pocket UI deliberately avoids that additional reset after the earlier firmware-reset evidence.
Since the requested down/up path already exists, no speculative WLAN change or extra device cycles were made.
The three recorded cycles restored the route, with zero new audio errors; the final cycle still had seven SDIO RTO lines,
one reinitialisation and the warning above. Route presence was checked after recovery, not timed from the kernel wake;
these captures do not establish a precise route-recovery latency.

The last journey's trace identifies its scratch UI as PID 8044. It left a stale public `/tmp/pocket-ui.status` when the
scratch UI exited: the journey had not isolated this output, and the UI writer did not yet honor `POCKETUI_STATUS`.
The owner's UI remained PID 15502. The status correction directs the probe's writer and every journey status/PID guard
to `<session>/ui.status`, even when the launching environment supplies an owner's path. Host regressions first reproduce
both overwriting the owner's status and reading its PID; they now protect isolation. The device's stale status was left
untouched, for the owner's UI to replace at its next state change. The rule is in [DEVICE-TESTS.md](DEVICE-TESTS.md).

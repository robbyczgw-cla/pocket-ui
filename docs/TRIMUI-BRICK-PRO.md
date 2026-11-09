# TrimUI Brick Pro (experimental)

The Brick Pro uses the same TrimUI card and update channel as the Brick and Smart Pro. `Board::BrickPro` combines the
Brick's 1024 × 768 framebuffer path with the existing Smart Pro stick/analog handling. No project member owns this device;
all support is based on public sources and host tests. **The Smart Pro stick assumptions are also unproven on hardware.**
About shows **TrimUI Brick Pro (experimental)**. No new diagnostic mode or separate beta card is needed.

## Evidence and limits

| Difference | Evidence | Profile |
|---|---|---|
| Screen / chip | [Specification](https://trimui.net/products/trimui-brick-pro): 3.95 inches, 1024 × 768, 324 ppi, A133P / GE8300; [NextUI platform.h](https://github.com/LoveRetro/NextUI/blob/5291344b3aceae5fa197437a70824afb41e2d338/workspace/tg5040/platform/platform.h) uses the Brick framebuffer size | Brick screen, design type sizes: Body 28 ≈ 2.2 mm instead of the 400-ppi Brick's larger small type |
| Identity | [NextUI launch.sh](https://github.com/LoveRetro/NextUI/blob/5291344b3aceae5fa197437a70824afb41e2d338/skeleton/SYSTEM/tg5040/paks/MinUI.pak/launch.sh) checks `Trimui Brick Pro` inside stock MainUI; [PR 836](https://github.com/LoveRetro/NextUI/pull/836) identifies `/dev/i2c-3` as exclusive to Brick Pro in this family | Check Pro identity / stick bus **before** the shared 1024-wide screen |
| Stick clicks / F keys | [Brick Pro Fn-key project](https://github.com/nezyuske/trimui-brick-pro-fnkey-mod/blob/b3a17e65a3fe11c1348cc66e3e4c372234fbf46c/README.md) measured stock `TRIMUI Player1`: 317/318 L3/R3, 59/60 KEY_F1/KEY_F2. NextUI maps SDL indices 9/10 to L3/R3 and 11/12 to F1/F2 | 317/318 never become save/load with sticks; 59/60 are the separate configurable F keys |
| Analog axes | NextUI platform.h: X/Y and RX/RY, up negative, roughly ±30k | Reuse the decoder's capability ranges, existing menu navigation and core input. Test actual centering, ranges and axis order with an owner |
| Side switch / gpio243 | NextUI initializes and watches gpio243 on all three models; specification calls the Pro's switch a configurable FN lock. Neither proves our Brick mute polarity/semantics | No slider watcher or initial read (`BRICK_NO_SLIDER=1`); never treat an absent/unverified pin as muted |
| Light zones | [NextUI platform.c](https://github.com/LoveRetro/NextUI/blob/5291344b3aceae5fa197437a70824afb41e2d338/workspace/tg5040/platform/platform.c): f1, f2, m, lr (sticks), rear (shoulders), with distinct brightness controls | Keep LEDs off; no Brick light-zone editor or 23-LED raw frame. The page exposes only F1/F2 assignments |
| Standby | PR 836: stock inputd polls i2c-3; `/tmp/system_suspend` parks its threads; it reports 1030 interrupts/s without it and zero with it | Create marker, wait 200 ms, enter kernel, remove marker before network restart. Cleanup on refusal and handled signals; failure to create marker refuses sleep |
| Boot hang/reset | [Issue 833](https://github.com/LoveRetro/NextUI/issues/833) reports intermittent boot hangs, resets and lost NextUI settings. Later comments report recurrence despite PR 837 | Unresolved. We use kernel poweroff, no direct PMIC I²C sequence; set printk 3 before inputd starts. This is not proof of a boot fix |

[Official firmware v1.1.1-20260717](https://github.com/trimui/firmware_brickpro/releases/tag/v1.1.1-20260717) fixes LCD offset and
stick sensitivity. Use that firmware or later for testing. The existing glibc loader check still applies.
[Spruce's measured console fix](https://github.com/spruceUI/spruceOS/commit/f9a5821a9e3ff695b261a73f12bcb49d53c7bbe1)
links serial-console starvation to i2c-3 timeouts/panics, including during boot; lowering console output mitigated its tests.

## Detection and fallback

Rust `board_at` and card `board()` prefer the stock MainUI name or `/dev/i2c-3`. They also accept an explicit Pro name or the
TG4040 identifier ([NextUI PR 837](https://github.com/LoveRetro/NextUI/pull/837)) in model/device-tree files. **No captured Pro
`/etc/model` or device-tree dump is available.** `trimui,brick-pro` / `trimui,brickpro` are conservative explicit-name aliases,
not claimed stock compatible strings. Generic A133P / sun50iw10p1 never identifies a Pro. Fixtures for these files are synthetic.
Otherwise retain the Brick / Smart Pro screen and MainUI fallbacks. Input is stricter: only an exact stock `Trimui Brick`
identity, without Pro evidence, permits 317/318 to become F1/F2. A stale `POCKETUI_DEVICE=brick` override cannot defeat Pro
evidence, and an unknown identity keeps L3/R3 (so old F-key aliases may be unavailable until identity is confirmed). **Capabilities cannot distinguish these pads:** the repository's real Brick
capture (`tests/fixtures/input/trimui-brick.json`, 2026-11-07) advertises signed X/Y/RX/RY axes and KEY_F1/F2 even though that
Brick has no sticks. The captured bitmaps and expected Brick F-key behaviour remain unchanged. Fixture decoding now supplies
its known physical profile explicitly, while production checks stock identity once on the reader thread. No extra event-time I/O.


## Feedback and host checks

Settings → System → About → Report a problem adds input-device names and the last 60 lines of `logs/boot.log`, as on the
Smart Pro. The log records the selected board, framebuffer size and chip/model evidence. If the screen stays black, send the
card's `pocket-ui/logs` folder instead. Unknowns: first visible framebuffer/GPU output, input ranges and calibration, switch
semantics, LEDs, motor feel, battery/mixer behaviour, real suspend/resume and the separate intermittent boot failure.

- `brick_profile` fixtures cover MainUI, framebuffer, model/device-tree, i2c-3 and plain Brick / Smart Pro fallbacks.
- `pad::decode`: raw 317/318 events through merged input and configured save/load hotkeys must produce L3/R3 and no actions;
  separate KEY_F1/F2 still work; sticks remain analog. Also run with the legacy `POCKETUI_DEVICE=brick` override; the real Brick input corpus must retain F1/F2.
- `scripts/test-brickpro.py`: both dash and BusyBox ash; identity before framebuffer, pause marker at kernel entry, cleanup on
  success, refused suspend, marker-creation failure and termination; input restored before network services. Included in `scripts/test-brick-card.sh`.
- `brick_settings_walk`: experimental name, both stick readouts and the separate F keys, with no unverified light or slider rows.

Host tests cannot establish hardware safety or game performance. A community tester must validate the actual device before
support can be described as tested.

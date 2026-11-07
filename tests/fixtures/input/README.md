# Captured Linux input corpus

These descriptors are device evidence, not generic stand-ins. `src/input_corpus_tests.rs` loads each file and
runs every advertised key and axis through the production evdev decoder, controller merging/game input,
both menu layouts and hotkeys. The ordinary library gate runs the public devices. The private full gate
also runs `cargo test --features brick --lib input_corpus_tests`, so the built-in Brick is covered.

| Fixture | Source | What is covered |
|---|---|---|
| `trimui-brick.json` | Live read-only EVIOCGID/EVIOCGBIT/EVIOCGABS, 2026-11-07 | TRIMUI Player1, volume/power/jack companions; Xbox 360 identity collision, MENU, F1/F2 (317/318 and advertised keyboard aliases), axes and hats |
| `rg35xx-plus.json` | Live read-only sysfs + EVIOCGABS, 2026-11-07 | H700 Gamepad, power and volume; crossed X/Y, MENU, shoulders, D-pad and reported stick-click codes |
| `retroid-mini-native.json` | Same, opening the existing InputPlumber device fd read-only | Retroid Pocket Gamepad and volume/power; Direct X/Y positions from the independent source-to-virtual probe, HAT2X/HAT2Y analog triggers; unused Z/RZ channels stay unused |
| `odin-native.json` | Same, 2026-11-07 (device reachable) | AYN Odin2 Gamepad and volume/power; Direct X/Y positions confirmed by an injected source-to-virtual check, Z/RZ analog triggers and sticks |
| `retroid-mini-inputplumber.json`, `odin-inputplumber.json` | Live virtual devices, same read-only capture | The actual DualSense-shaped target and keyboard that applications see on ROCKNIX; MENU, BACK, volume, triggers and sticks |
| `8bitdo-lite2-d.json` | Real Lite 2 D-mode evtest descriptor from the 2026-11-06 hand-test | Nintendo face order, Z/RZ right stick, BRAKE/GAS triggers, hats |
| `8bitdo-lite2-s.json` | Real Lite 2 S-mode `Pro Controller`, same hand-test | Exact bus/VID/PID/version and capabilities with the matching bundled SDL record |

Native face positions follow the independent Mini V2 probe in `docs/COMPARE-ROCKNIX.md` and the 2026-11-07 Odin source-to-virtual event check: BTN_NORTH/WEST pass through unchanged. The latter was injected, not a physical hand-test. The virtual Odin advertised trigger keys but emitted only trigger axes in that check; alias tests cover either valid source and both release orders.

The Lite 2 expected positions come from the matching SDL record (S) and Nintendo HID convention (D).
The descriptors are real; this is **not** a new physical-label hand-test of either mode. All advertised,
unused firmware codes are listed explicitly with a null role. Stick-click capabilities do not imply the
handheld has physical sticks. Unique IDs, MAC addresses and host paths are deliberately absent.

The separate eleven SN30 Pro/Pro 2/Xbox/Switch/DualSense mode cases already in `pad_linux_decode.rs` remain
model-derived fixtures. They still run in the default gate; they are not relabelled as captured hardware.
No additional real Bluetooth descriptors for those controllers were available in the earlier test notes.

`scripts/input-corpus-capture.py` reads sysfs and EVIOCGABS without grabbing, reading gameplay events,
changing settings or creating device nodes. It can reopen the built-in pad through InputPlumber's existing
fd when its ordinary event node is hidden. `scripts/input-corpus-capture.c` uses EVIOCG* on explicitly named
nodes for devices without Python. Run either only while the device is handed over for a short read-only
capture. Retain the original capture separately, strip identifying fields, and review expected physical
positions against measured controls or the firmware's input profile before adding a fixture.

Changing a decoder must keep this entire corpus green. Do not alter a captured ID/capability list to make
a mapping pass, or change an expected physical role to follow a new heuristic. A supported built-in pad's
measured layout must take precedence over a generic controller record that happens to share its USB ID.

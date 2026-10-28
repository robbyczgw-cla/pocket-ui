# A version for the Retroid Pocket Nova? (feasibility study, October 2026)

> **Status (2026-10-25):** a design note. Nothing here has run on a Nova; nobody on the project has one. The
> hardware facts come from ROCKNIX's source tree (`next` at `651150f`, 2026-10-24, the `~/.cache/rocknix-distribution`
> clone `rocknix-diff.sh` uses) and from the pages under [Sources](#sources); pocket-ui facts from the code at
> `9ee8e0c`. The only code that came with this note is `POCKETUI_SNAPSHOT_SIZE` (snapshots at another screen size,
> [DEVELOPING.md](DEVELOPING.md#environment-variables)), so the layouts could be looked at in the Nova's 1280x960.

The question: could pocket-ui have a version for the Retroid Pocket Nova, a 4:3 handheld with a lot of attention,
without the device in hand? Short answer: technically yes, and less work than expected, because the Nova runs the same
ROCKNIX userland and most of pocket-ui already reads the hardware instead of assuming it. But "it starts" and "it
is as good as on the Mini V2" are far apart, and the second needs a Nova and somebody who tests on it. The
recommendation at the end is **not now**: keep the code size-agnostic at almost no cost, and come back once the Mini
V2 version is public and stable and ROCKNIX's SM8550 sleep works. If a second device is wanted before that, the first
Retroid Pocket Mini (same SoC and image as the Mini V2, the Nova's 1280x960 screen) is the cheaper step.

## 1. The Nova and its ROCKNIX state

| | Retroid Pocket Mini V2 (today) | Retroid Pocket Nova | Source |
|---|---|---|---|
| SoC | Snapdragon 865 (SM8250), Adreno 650 | QCS8550 (Snapdragon 8 Gen 2 without the modem), Adreno 740; cores 3 + 4 + 1 (cpufreq `policy0`, `policy3`, `policy7`) | press; ROCKNIX `quirks/platforms/SM8550/010-governors` |
| Screen | 3.92" AMOLED, panel 1080x1240 portrait, turned to 1240x1080 by sway | 4.5", 4:3, **1280x960 landscape-native** (no rotation), 60 and 120 Hz modes, 91 x 68 mm; AMOLED per Engadget | ROCKNIX `0105-drm-panel-Add-Retroid-Pocket-Nova-panel.patch` (`il97680a,rpnova`), `qcs8550-retroidpocket-rpnova.dts` (`/delete-property/ rotation`); press |
| Touch | ft5x06, portrait 1080x1240 | FocalTech ft5426 (edt-ft5x06), **1280x960 landscape**, not inverted | Nova DT (`touchscreen-size-x = <1280>`, `touchscreen-size-y = <960>`) |
| Pad | MCU `retroid-pocket-gamepad`, InputPlumber → virtual DualSense + keyboard | MCU on a UART, `rsinput` driver (named "AYN Odin2 Gamepad", from the shared AYN tree), InputPlumber's "AYN Layout" → virtual **DualSense** (`ds5`) + keyboard; BACK → F1 as on the Mini; HOME is `BTN_MODE`; back paddles `BTN_Z`/`BTN_C` (from the RP6 tree) | `inputplumber/devices/02-ayn-controller.yaml`, `capability_maps/ayn_mcu.yaml`, `0031_input--Add-driver-for-RSInput-Gamepad.patch` |
| Sticks | analog, left one rests at about +0.12 | analog, axes inverted in the DT (`invert-x`, `invert-y`), trigger dead zones 100 | Nova DT `&gamepad` |
| Fan | pwm-fan, cooling device of the `cluster1` zone with six `active` trips; ROCKNIX's `fancontrol` **moves those trips** | pwm-fan (`cooling-levels = <0 30 45 60 70 90 120 150>`, tachometer), mapped as a **passive** cooling device into 13 CPU/GPU zones; ROCKNIX's SM8550 `fancontrol` is a **shell loop that writes `pwm1` every 3 s** from the average of the CPU/GPU zones; the device page says "managed by the kernel" | `qcs8550-ayn-common.dtsi`, `quirks/platforms/SM8550/bin/fancontrol`, `020-fan_control` |
| Stick LEDs | 2 x 4 RGB, `multi-ledl{1..4}`/`multi-ledr{1..4}`, `multi_intensity` blue-green-red, platform scripts | the same sysfs names and channel order (HTR3212 driver), but the scripts are **device** quirks (`quirks/devices/Retroid Pocket Nova/bin/ledcontrol`, `analog_sticks_ledcontrol`, `battery_led_status`) instead of platform ones | RP6 DT, Nova quirk folder (since 2026-09-16) |
| Battery | `/sys/class/power_supply/battery`, 3 850 mAh design | 5 000 mAh; `qcom_battmgr` renamed to `battery` by ROCKNIX's patch, so the same path; which files it has (`current_now` sign, `charge_now`) is unknown | `0503-ROCKNIX-battery-name.patch`; press |
| Vibration | PMI8998 haptics (`pmi8998_haptics`) | PM8550B HV haptics, forwarded as force feedback through the `rsinput` pad | `1000-add-qcom-haptics-driver.patch`, `1003-rsinput-add-ff.patch` |
| Sleep | real suspend (`suspendmode mem`), measured | **off**: the SM8550 platform quirk runs `suspendmode off` ("Sleep is currently broken, so we'll disable it") and logind ignores the power key; the device page links only the fake-suspend guide | `quirks/platforms/SM8550/030-suspend_mode` (unchanged since 2025-07) |
| GPU stack | Freedreno GL, Turnip | Freedreno/Turnip, but **OpenGL is forced through Zink** (`MESA_LOADER_DRIVER_OVERRIDE=zink`) on SM8550 since 2026-05 | `quirks/platforms/SM8550/095-force_zink` |
| Audio | PipeWire; `/usr/sbin/quantum` sets the graph quantum at boot and wake | PipeWire (WCD9385 + two AW88166 amps); **no `/usr/sbin/quantum`** (it is in the SM8250 tree only) | `devices/SM8250/filesystem/usr/sbin/quantum` |
| Function keys | HOME and START as modifiers | the same (`070-modifiers`: `BTN_MODE`, `BTN_START`) | both platforms' `070-modifiers` |
| UI service | `sway.service essway.service` | the same | both platforms' `090-ui_service` |
| Image | `ROCKNIX-SM8250.aarch64-<v>.img.gz` (1.4 GB) | `ROCKNIX-SM8550.aarch64-<v>.img.gz` (1.5 GB), one image for every SM8550 device; the Nova is in every release since 20260801 | GitHub release 20261001 |
| Install | `fastboot boot` / OpenABL dual boot | from the SD card; if there is no fastboot menu, `rocknix_abl` is flashed from rooted Android first (`backup_abl.sh`, `flash_abl.sh`) | ROCKNIX device page |
| Cores | ROCKNIX 20261001's list | the same list plus `ares` and `gopher64` standalones; every core pocket-core hosts (`hostsys::routed_cores`) is on it | `documentation/PER_DEVICE_DOCUMENTATION/SM{8250,8550}/SUPPORTED_EMULATORS_AND_CORES.md` |

How mature: the Nova was added on 2026-07-26 (`3c3ac4f`) as a small overlay on the Retroid Pocket 6 tree, which itself
sits on the AYN Odin 2 tree; since then five commits touch it (stick axes, dead zones, the quirk folder, the LED
control). The SM8550 platform as a whole moves about as fast as SM8250 (64 against 55 commits under `devices/` since
July). The device page has no known-issues section. Paul Becker's write-up (September 2026) is positive but calls the
support early: the pad first shows up as a DualSense Edge with PlayStation button icons, the on-screen keyboard path
was broken under Wayland, and shader chains run into a texture-binding limit. Sleep, fan and battery aren't covered
there. (On the pad: every release since 20260801 asks InputPlumber for a plain DualSense, `ds5`; an Edge target was
in `next` for two days in July, `d63fb9d` and its revert `2f94c33`. Whether the author ran a build from between or
the plain target also reports as an Edge is for a device to answer; pocket-ui identifies the Pocket by bus either
way.)

One more fact that matters for the plan: the **first Retroid Pocket Mini** ("Mini v1") is also SM8250 and has
the Nova's screen size. ROCKNIX's `sm8250-retroidpocket-rpmini.dts` drives a `ch13726a,rpmini` panel of 960x1280,
turned by 90° to **1280x960**, touch 960x1280 (the Mini V2's is the same chip family at 1080x1240, turned by 270°).
It runs the Mini V2's own ROCKNIX image, quirks, fan, sleep and quantum. The RP5 and Flip 2, the other SM8250
Retroids, are 1920x1080.

## 2. What in pocket-ui assumes the Mini V2

Searched for the panel size (`1240`, `1080`), rotation, touch, the fan, battery, LED and input paths, sleep, audio,
the launcher, the ROCKNIX contract, cores, the CPU/GPU profiles and the per-console scaling. The good news first:
most of the code already **reads** the hardware. The UI paints into the window's size (`App::paint` gets the canvas
of `window.inner_size()`), pocket-core takes the view size and the refresh rate from the window and the monitor,
`dest_rect` computes the integer scale from the view, `power.rs`/`perf_fan.rs` list the cpufreq policies and
devfreq devices they find, `fan_zone` looks for whichever thermal zone has the pwm-fan, touch reads the axis
ranges and rotation from the device and `fbcon/rotate`, the pad identifies the Pocket by bus, not by name. What is
left:

Effort: **S** = hours, **M** = a day or two, **L** = several days (agent time, plus device time where it says so).

| Assumption | Where | What the Nova needs | Effort |
|---|---|---|---|
| Layouts measured for 1240x1080 (fixed font sizes, row heights, card sizes, `SCENE_BOTTOM = 992` in the Cabinet) | `main.rs` paint code, `app_cabinet.rs`, `perf_fan_page.rs`, `sync_page.rs`, the other `app_*` pages | 120 px less height (−11 %), 40 px more width. Most pages adapt; three break (Cabinet, Performance & fan, Sync) and the Split look drops facts: see [the snapshots](#layouts-at-1280x960) | M |
| Previews painted at the Mini V2's size | `app_look.rs` (`FRAME_W/H`), `features.rs` (`SCREEN`, `PREVIEW = 620x540`), `feature_previews.rs` | take the screen size; the half-size preview becomes 640x480 | S |
| Fallback sizes when no window exists | `main.rs` (two `map_or((1240, 1080), …)`), `pocket_core.rs` (three `(1240, 1080)` fallbacks, `frame_tools`), `pixel_standalone::Screen::default`, `pixel_path` ("1240×1080"), `hello_pad` | from a device profile instead of a constant | S |
| Snapshot size fixed | `main.rs` `fn snapshot` | **done** in this commit: `POCKETUI_SNAPSHOT_SIZE` | — |
| Per-console starting values "for this screen" | `options::console_default` (its comments name the Mini's numbers: ScummVM "stops at 3x, 960 of the 1240 px", the DS "720x1080, 2.8x"), so disc systems, N64, PSP and ScummVM start at Aspect | per device: on the Nova 320x240 content (PS1, N64 menus, ScummVM's 320x200 nearly) reaches 4x, so Integer becomes the better start for some of them; DS stays Aspect | S |
| Integer scale per console | `video::dest_rect` (computed), the frame tests at GB 7x / GBA 5x (`bezel.rs`, `ambient.rs`), `docs/FRAMES.md` | the code adapts by itself, the results change ([table below](#integer-scaling-on-a-43-panel)): NES, PC Engine, PS1/N64 at 320x240 and 640x480 content fill the Nova's screen exactly at 4x/2x, Game Boy drops from 7x to 6x. Frames get no room where the picture fills the screen; tests and FRAMES.md speak of the Mini's numbers | M |
| 60 Hz display | `pocket_core.rs` `sync_audio_rate`: within 5 % of the core's rate the core runs once per refresh, otherwise it is "scheduled by time" | the Nova's panel has 60 and 120 Hz modes. At 120 Hz every 60 fps core falls back to time scheduling (no longer one frame per refresh, the smoothest path). Either set the 60 Hz mode while a game runs, or run the core every second refresh at 120 Hz (also lowers latency); measure | M + device |
| Rotation and pre-rotation | `touch.rs` (`Rotation::resolve`), `video.rs` rotation, the launcher's `prerotate_*` | nothing: the Nova's panel and touch are landscape, `resolve` gives `Deg0` and ROCKNIX's `prerotate_env` has no turn to make | — |
| Touch fallback ranges | `touch.rs` `unwrap_or((0, 1079))`/`(0, 1239)` when the device reports none | the profile's panel size | S |
| Fan: Smooth fan moves the zone's six `active` trips and writes `pwm1`; the launcher hands back trips | `fan_curve.rs`, `power.rs` (`fan_zone`, `active_trips`, `Fan::Quiet/Cool` via trips), `deploy/run-pocket-ui.sh` `fan_handback`, contract rows `trips` | a different mechanism. The Nova's pwm-fan is a *passive* cooling device in 13 zones (no `active` trips to move) and ROCKNIX drives it from a userspace loop. Smooth fan would have to stop (or replace) ROCKNIX's `fancontrol` service and own `pwm1` directly, with the hand-back being "restart fancontrol"; the Quiet/Cool profiles map to `cooling.profile`. The tuning (`START_PWM = 51`, `MIN_RUN_PWM`, guard temperatures) is the Mini's | L + device |
| GPU cap script | `system_ctl.rs` `/usr/lib/autostart/quirks/platforms/SM8250/bin/gpu_overclock`, contract row | SM8550 has its own `gpu_overclock` but doesn't set `DEVICE_GPU_OVERCLOCK`; whether the Mini's freeze (GPU at its top OPP) exists on the Adreno 740 is unknown; probably skip the cap | S |
| Stick LEDs | `leds.rs` (script paths `usr/bin/ledcontrol`, `analog_sticks_ledcontrol`, probe `multi-ledl1`), contract rows on `quirks/platforms/SM8250/bin/*` | the sysfs names and channel order are the same and `/usr/bin/ledcontrol` hands over to the quirk by `${QUIRK_DEVICE}`/`${HW_DEVICE}`; only the contract rows need the device folder. Likely works as is | S |
| Battery | `battery.rs` (`/sys/class/power_supply/battery`, notes on the Mini's files), `battery_estimate.rs` | same path; check which files `qcom_battmgr` offers and the sign of `current_now` (the code already takes the state from `status`). The per-system draw numbers start empty | S + device |
| Vibration | `rumble.rs` prefers `pmi8998_haptics`, falls back to any force-feedback device | the fallback finds InputPlumber's DualSense or the `rsinput` pad, which forwards to the HV haptics; check strength and that the menu tick isn't too long | S + device |
| Input names and keys | `pad.rs` (`KEY_F1` → Back, `BTN_MODE` → Mode, built-in by bus), `fn_keys.rs` (HOME/START modifiers, `input_sense`), `button_layout.rs`, `controls.rs` ("Mini V2 button" wording) | the same InputPlumber model and modifiers. The Nova has two back paddles (`BTN_Z`/`BTN_C` → InputPlumber `LeftPaddle2`/`RightPaddle2`) that pocket-ui doesn't map; the face-button labels are the Nintendo layout on both (to check on the device). Docs and comments say "Mini V2" in many places | S |
| Sleep | `sleep.rs` (deep sleep = `systemctl suspend`, wake by `suspend_stats`; the power-key inhibitor), `powerkey.rs` | ROCKNIX has suspend switched off on SM8550. Deep sleep must not call a suspend that is known to be broken: a profile flag "light sleep only" (screen off, game paused, as today's first stage) plus power off with resume after a while. The light sleep needs the backlight path (`/sys/class/backlight`, PWM backlight in the DT) | M + device |
| PipeWire quantum | `audio.rs` (`force_quantum`, `restore_quantum`), `app_fast_switch.rs`, launcher `QUANTUM=/usr/sbin/quantum`, contract row `exec quantum` | `/usr/sbin/quantum` doesn't exist on SM8550. pocket-core sets the quantum through `pw-metadata` anyway, and `restore_quantum` already falls back to `pw-metadata … clock.force-quantum 0` without the script; the launcher's clean-up after a crash only runs the script, so it would leave a lowered quantum behind. The "ROCKNIX resets it to 1024 at wake" assumption and the latency numbers in `audio.rs` are the Mini's | S + device |
| OpenGL through Zink | the launcher's environment comes from `/etc/profile`, which on SM8550 sets `MESA_LOADER_DRIVER_OVERRIDE=zink` | pocket-ui and pocket-core would paint through Zink on Turnip instead of Freedreno GL. Measure start, first frame and frame time both ways; perhaps unset it for pocket-ui only | M + device |
| Display colour (gamma LUT) | `display.rs`, `gamma.rs` (the Mini's `msm` DPU and `ch13726a`) | the SM8550 DPU should offer a gamma LUT too; untested | S + device |
| Update channel | `update.rs` (refuses an image whose `HW_DEVICE` differs; the ABL update path is the Mini's) | a separate release per platform (`SM8550` images), signed and listed separately; the ABL note doesn't apply | M |
| Install path | `docs/INSTALL-DUAL-BOOT.md`, `docs/SD-IMAGE.md`, `scripts/make-image.sh`, `scripts/flash-sd.sh` | a Nova install guide (ROCKNIX's ABL flashing from rooted Android, then the SD card), a Nova SD image built on the SM8550 image | M |
| ROCKNIX contract | `scripts/rocknix-contract.txt` (rows on `quirks/platforms/SM8250/...`, the Mini's `fancontrol` override, `quantum`, the `trips` rows), `docs/ROCKNIX-CONTRACT.md` | rows per platform (a `HW_DEVICE` placeholder in paths, or a second data file for SM8550), and `--ref`/`--root` runs against the SM8550 tree and image | M |
| Tools list | `tools.rs` (ROCKNIX 20261001's tools on the SM8250 image) | compare with the SM8550 image | S |
| es_systems and cores | `hostsys.rs`, `emu_choice.rs` (ROCKNIX 20261001's es_systems.cfg for the Mini) | SM8550's list is a superset; the standalone emulators (ARMSX2, Dolphin, Azahar) have SM8550 builds in ROCKNIX, but `BIG-EMULATORS.md`'s settings and hotkey chords are the Mini's | S |
| Virtual device | `docker/vdev/sway-config` (`1240x1080@60Hz`), `vdev-pad.py` (portrait 1080x1240 touch, `transform 90`), `scripts/vdev.sh` (looks for `ROCKNIX-SM8250…img`) | a size and image knob: see [section 3](#3-what-can-be-checked-without-a-nova) | M |
| Wording | the manual (`docs/user`), `STATUS.md`, comments: "the Mini V2" throughout; the manual's controls tables | a device name from the profile in user texts that name the device; the manual's page height is the Mini's | M |

## Integer scaling on a 4:3 panel

What `dest_rect` gives at Native (the largest whole multiple of the picture height whose width, in the core's aspect,
still fits), in pixels and as a share of the screen:

| Console | Picture | Mini V2 1240x1080 | Nova 1280x960 |
|---|---|---|---|
| Game Boy / GBC / Game Gear | 160x144 | 7x = 1120x1008 (84 %) | 6x = 960x864 (68 %) |
| Game Boy Advance | 240x160 | 5x = 1200x800 (72 %) | 5x = 1200x800 (78 %) |
| NES | 256x240 (4:3) | 3x = 960x720 (52 %) | **4x = 1280x960 (100 %)** |
| SNES | 256x224 (4:3) | 4x = 1195x896 (80 %) | 4x = 1195x896 (87 %) |
| Mega Drive, Saturn, Neo Geo | 320/304x224 (4:3) | 4x = 1195x896 (80 %) | 4x = 1195x896 (87 %) |
| PC Engine | 256x240 (4:3) | 3x = 960x720 (52 %) | **4x = 1280x960 (100 %)** |
| PS1, N64 at 320x240 | 320x240 | 3x = 960x720 (52 %) | **4x = 1280x960 (100 %)** |
| Dreamcast, PS2 at 640x480 | 640x480 | 1x = 640x480 (23 %) | **2x = 1280x960 (100 %)** |
| DOS at 320x200 | 320x200 | 3x = 960x600 (43 %) | 4x = 1280x800 (83 %) |
| Atari Lynx | 160x102 | 7x = 1120x714 (60 %) | 8x = 1280x816 (85 %) |
| Neo Geo Pocket | 160x152 | 7x = 1120x1064 (89 %) | 6x = 960x912 (71 %) |
| WonderSwan | 224x144 | 5x = 1120x720 (60 %) | 5x = 1120x720 (66 %) |
| PICO-8 | 128x128 | 8x = 1024x1024 (78 %) | 7x = 896x896 (65 %) |
| DS, stacked | 256x384 | 2x = 512x768 (29 %) | 2x = 512x768 (32 %) |
| PSP | 480x272 | 2x = 960x544 (39 %) | 2x = 960x544 (42 %) |

So the Nova is the better screen for the home consoles of the 4:3 era (whole-number scaling that fills the screen,
the thing a 4:3 panel is bought for) and the worse one for the square-ish handhelds (Game Boy, Neo Geo Pocket,
PICO-8). Consequences for pocket-ui: per-console starting values would differ per device (`options::console_default`
starts PS1 and the other disc systems at Aspect because Integer stops at 3x on the Mini; on the Nova Integer fills the
screen for 320x240 games); screen frames have no room where the picture fills the screen, so Auto must pick "no
frame" there instead of drawing nothing by accident; and the frame and ambient tests, written around GB 7x and GBA
5x, need a second set of numbers.

## Layouts at 1280x960

Every state of `tests/i18n-states.txt` (406, English, dark, Curator's made-up library) was rendered at 1240x1080 and
at 1280x960 with `POCKETUI_SNAPSHOT_SIZE` (2026-10-25). All 812 rendered; none failed. (Safe mode's five rescue
pages still painted at 1240x1080 in that run; they take the size too since.) Strings cut short or drawn past the
right edge (the painter's own records, `POCKETUI_I18N_MISSES`): no new ones at 1280x960 (the one difference was a
rescue page showing the run's own data path). The width is not the problem; the height is, and the painter has no
record for running off the bottom, so about 45 of the pages were looked at by eye (Home in three looks, the shelves, the
Cabinet, the game card, Settings and several of its pages, quick settings, the pause menu and its slots, search,
the manual, first start, Bluetooth, Tailscale, the web companion's pairing, Sync, Battery, LEDs, Night light, the
console page, Runs on, the Tour, the Starter pack). The other 360 were rendered and not looked at.

What they show:

- **Most pages adapt.** Home, Settings, quick settings, the pause menu, search, the Tour cards, pairing pages with
  QR codes: the same layout with less empty space, or one or two list rows fewer. The game card and the manual
  scroll a little earlier.
- **Some pages drop information without saying so.** The Split look's facts beside the shelf keep three of six rows
  (Released, Sold, Your games; Ended, Inside and Runs on go), the list beside it one system fewer. Not broken, but on a
  4:3 screen the page would want a different split.
- **Three pages break.** The Cabinet's shelves run under the button hints (`SCENE_BOTTOM = 992` is below the 960 px
  screen; the A hint is covered); Settings → Playing → Performance & fan puts its live readout under the hints; Sync's
  status block (device ID, web interface, password) collides with the description line and the hints.

So the layouts are closer to size-independent than the constants suggest, and the fixes are local: a few pages that
place things from the top with Mini V2 numbers. Phase 2 below is still a full sweep with eyes on every page, dark and
light, plus a bottom-edge record in the painter so a test can find the next one.

## 3. What can be checked without a Nova

| Check | How | Needs | What it proves |
|---|---|---|---|
| Layouts at 1280x960 | `POCKETUI_SNAPSHOT_SIZE=1280x960 pocket-ui --snapshot <state> out.png`, every state of `tests/i18n-states.txt`, dark and light ([above](#layouts-at-1280x960)) | nothing more (done here in dark, about 45 pages looked at) | what the pages look like; not touch targets, speed or the panel's colours |
| A size test in `cargo test` | render a handful of key states at 1280x960 and fail when text runs past the bottom or the hint bar overlaps content (the i18n test already records strings cut short or drawn past the right edge, `POCKETUI_I18N_MISSES`; a bottom-edge record would be new) | a small painter change | that a 4:3 layout doesn't regress once it exists |
| Unit tests per device profile | the fake-sysfs tests of `power.rs`, `perf_fan.rs`, `fan_curve.rs`, `leds.rs`, `battery.rs`, `quick_profiles.rs` already take a root; add an SM8550 fake tree (policies 0/3/7, the 13 passive zones, the userspace `fancontrol`, battmgr's battery files) next to the SM8250 one | the profile abstraction (phase 1) | that each device's logic does what we think on what we think the device has |
| ROCKNIX contract for SM8550 from source | `scripts/rocknix-contract.sh --ref origin/next` with SM8550 rows | the contract split per platform | which promises the SM8550 tree keeps today; free and quick once the rows exist |
| ROCKNIX contract for SM8550 from the image | `unsquashfs` the SYSTEM of `ROCKNIX-SM8550.aarch64-20261001.img.gz` (a 1.5 GB download; SM8250's unpacks to 4.6 GB), `rocknix-contract.sh --root` | the download (not done here: agents don't download multi-GB images) | what the release really ships: es_systems.cfg, cores, glibc, systemd, the quirk scripts |
| A virtual Nova | `vdev.sh` with `ROCKNIX_IMG` pointing at the SM8550 image, and two new knobs: the sway output (`1280x960@60Hz`, or `@120Hz` to try the pacing) and the touch panel (landscape 1280x960, no turn) in `vdev-pad.py` | the image above on the MacBook Air (the base image is about 2.4 GB in Docker), half a day of vdev changes | that pocket-ui and pocket-core start and play on SM8550's userland (its glibc, sway, PipeWire; the pad is the vdev's uinput one, not InputPlumber); llvmpipe, so nothing about the GPU, Zink, the fan, sleep or the panel |
| The 4:3 screen on real hardware | a Retroid Pocket Mini v1 (SM8250, 1280x960, the Mini V2's image and quirks) | a Mini v1 (bought, or somebody who has one) | touch on a 4:3 panel turned the other way (90°), real frame times and latency at 1280x960, the 4:3 layouts in the hand; nothing about SM8550 |
| Input mapping | feed the Nova's evdev codes (`rsinput` + InputPlumber's `ayn_mcu` map) to `pad.rs` in a test | nothing | that HOME, BACK, the paddles and the sticks land where they should |

What can't be checked without the device: the fan and its noise, temperatures, battery drain and the estimate,
sleep (and whether light sleep alone is acceptable), 120 Hz pacing and latency, Zink against Freedreno GL, the
panel's colours and brightness curve, touch accuracy, vibration strength, the install path, real game speed for
PS2/GameCube/3DS. Those are most of what makes pocket-ui feel finished on the Mini V2, and most of the docs'
Status notes exist because of exactly such checks.

## 4. A plan, if it is done

| Phase | What | Effort | Done when |
|---|---|---|---|
| 0. Hygiene (now) | No new `1240`/`1080` constants; new pages paint from the canvas size; the snapshot size knob stays. Costs nothing | — | every new page also looks right at 1280x960 |
| 1. Device profile | `src/device.rs`: one `Device` read once at start from `/sys/firmware/devicetree/base/model` and `HW_DEVICE` in os-release (overridable for tests and the virtual device), with screen size and refresh, panel rotation, fan kind (`KernelTrips` / `UserLoop` / none), sleep kind (`Deep` / `LightOnly`), quantum tool, LED script folder, GPU cap, extra buttons. The Mini V2 profile is today's behaviour bit for bit (the existing tests prove it). The fallbacks and previews use it. Contract rows per platform | M–L (2–3 days) | `cargo test` green with both profiles; the Mini V2 unchanged on the device |
| 2. Layouts for 4:3 | Snapshot sweep of every state at 1280x960, dark and light; fix what doesn't fit (Cabinet, Performance & fan, Sync, the Split look's facts, whatever the full sweep finds); per-device console defaults and frame rules; the previews; a size test in `cargo test`. If a Mini v1 is at hand, this phase can be finished on real hardware without SM8550 | M–L (3–5 days) | the sweep shows no clipped or crowded page |
| 3. Experimental build in a virtual Nova | vdev on the SM8550 image (needs the 1.5 GB download on the Air), 1280x960 sway output, landscape touch; run the test plan's virtual parts; `rocknix-contract.sh --root` on the SM8550 SYSTEM; an SM8550 release build and update channel marked experimental | M (1–2 days) | pocket-ui plays every routed core in the virtual Nova |
| 4. A tester with a Nova | Somebody with a Nova and patience, or buying one (USD 229): fan mechanism and tuning, sleep (light only, power-off resume), 120 Hz pacing and latency (`scripts/latency.sh`), Zink vs GL, battery files and drain, LEDs, vibration, touch, the install guide, big emulators. Each finding is one fix and one more device round | L (1–2 weeks elapsed, mostly device time) | the Nova's STATUS rows say "device" for the core features |

Total to an experimental Nova build that one tester has used for a week: roughly two to three weeks of agent work plus
the tester's time, if nothing in ROCKNIX's SM8550 support gets in the way.

### Risks

- **Two devices to maintain.** Every ROCKNIX bump would need the contract, the diff and a device check on both
  platforms; every feature with hardware behind it (fan, sleep, LEDs, battery, latency) needs two device runs; the
  STATUS ledger doubles. The project is one maintainer with agents and one physical device, and the analyses of
  2026-10-24 already name maintenance against a moving ROCKNIX as the highest risk.
- **ROCKNIX's SM8550 support is young.** Sleep has been disabled as broken on the platform since 2025; OpenGL goes
  through Zink by default; InputPlumber's pad identity changed twice in a week in July; the Nova itself is an overlay
  of five commits. pocket-ui's selling points (instant sleep and wake, a quiet fan, low latency) sit exactly on the
  parts that are least mature there.
- **Support burden.** The Nova is the device with the hype, so a Nova version would draw more users and reports than
  the Mini V2, about a device nobody on the project can reproduce problems on. A version that is "mostly fine" on the
  Nova would be judged against the Mini V2 version, and against Retroid's own Android launcher, which the Nova ships
  with.
- **The claims.** pocket-ui's docs and manual say what was measured on the device. A Nova version can't say that
  until somebody measures, and shipping without it undercuts the thing that makes the project credible.
- **Focus.** Phases 1-4 are weeks taken from the Mini V2's own open list: the public SD image, the update server,
  overnight sleep drain, the device checks STATUS.md still lists.

## Recommendation

**Not now; keep the door open cheaply.**

The case for: the Nova runs the same ROCKNIX userland, the same InputPlumber pad model, the same LED and battery
interfaces, and pocket-ui reads most of its hardware already; a 4:3 1280x960 screen is the best fit there is for
whole-number scaling of NES, PC Engine, PS1 and N64; the layouts mostly take 1280x960 already (three pages to fix);
and the interest is real. A first build that starts and plays in a virtual Nova is one to two weeks of work away.

The case against is stronger for now. The independent analyses of 2026-10-24 said "do not chase more devices yet …
one perfectly fitted device is the moat; port after public stability on the Mini V2, if ever", and this study
doesn't contradict that: the parts that differ (fan, sleep, 120 Hz pacing, Zink, the install path) are the parts
that make pocket-ui better than EmulationStation, and none of them can be finished without a Nova in hand. The Mini
V2 version isn't public yet either. A half-fitted Nova version would cost the one thing the project has that
others don't.

What to do instead:

1. Phase 0 from today: size-agnostic new code, the snapshot knob, an occasional 1280x960 sweep.
2. Revisit when all three hold: the Mini V2 version is public (SD image, update server) and has been stable for a
   few weeks; ROCKNIX enables suspend on SM8550 (watch `quirks/platforms/SM8550/030-suspend_mode` in
   `rocknix-diff.sh`); and there is a Nova on the desk or a committed tester with one.
3. Then do phases 1-4 in order.
4. Should the question ever become "a second device" rather than "the Nova": the **Retroid Pocket Mini v1** is the
   cheap one. Same SoC, same ROCKNIX image and quirks (fan, sleep, quantum, contract: nothing of the SM8550 list
   above), and the Nova's 1280x960 screen. Phases 1 and 2 done for the Mini v1 are most of the 4:3 work a Nova
   version needs, with only the SM8550 platform left for later. (The RP5 and Flip 2 share the SoC too, but at
   1920x1080 they are a different layout question.)

## Sources

- ROCKNIX, Retroid Pocket Nova device page: <https://rocknix.org/devices/retroid/retroid-pocket-nova/> (read
  2026-10-25; no date on the page).
- ROCKNIX source, `github.com/ROCKNIX/distribution` `next` at `651150fc88` (2026-10-24):
  `projects/ROCKNIX/devices/SM8550/linux/dts/qcom/qcs8550-retroidpocket-rpnova.dts`, `…-rp6.dts`,
  `qcs8550-ayn-common.dtsi`; `patches/linux/0105-drm-panel-Add-Retroid-Pocket-Nova-panel.patch`,
  `0503-ROCKNIX-battery-name.patch`, `0031_input--Add-driver-for-RSInput-Gamepad.patch`, `1000-…haptics…`,
  `1003-rsinput-add-ff.patch`; `filesystem/usr/share/inputplumber/`; `packages/hardware/quirks/platforms/SM8550/`
  and `…/SM8250/`; `packages/hardware/quirks/devices/Retroid Pocket Nova/`;
  `documentation/PER_DEVICE_DOCUMENTATION/SM8550/SUPPORTED_EMULATORS_AND_CORES.md`; Nova commits `3c3ac4f`,
  `320d8e4`, `32a1d85`, `246e62c`, `6ec9104`; InputPlumber `2f94c33` ("revert to ds5"); Zink `53b958c`.
- The Mini v1: `projects/ROCKNIX/devices/SM8250/linux/dts/qcom/sm8250-retroidpocket-rpmini.dts`,
  `patches/linux/0058_DDIC-CH13726A-panel.patch` (same commit).
- ROCKNIX release 20261001 assets (sizes): <https://github.com/ROCKNIX/distribution/releases/tag/20261001>.
- Paul Becker, "Retroid Pocket Nova on ROCKNIX" (2026-09-16): <https://www.becker.kiwi/en/blog/2026/09/16/>.
- Engadget, "Retroid's Pocket Nova is a very capable 4:3 handheld that costs $229" (2026-06-25):
  <https://engadget.com/2201878/retroids-pocket-nova-is-a-very-capable-43-handheld-that-costs-dollar229>.
- Time Extension, specs and launch prices (2026-06):
  <https://www.timeextension.com/news/2026/06/here-are-the-specs-and-launch-prices-for-the-retroid-pocket-nova-retroids-new-43-handheld>.
- The independent analyses of 2026-10-24 (outside this repository, in the project's review folder).

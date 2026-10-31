# Android launcher and in-app player — 30 October 2026

## Current boundary

`android/` is a normal consumer of the root `pocket-ui` library. `src/lib.rs` selects the Linux or Android module
composition. The Android launcher uses shared media/spine/box painters, the interface kit, text/i18n, search,
Curator facets and metadata. Its navigation is Android's own controller, not the entire ROCKNIX App transplanted.
Shelf, Cabinet, Cards, Minimal and Split are available. Favourites and appearance/language/text/sound settings are
app-private; Linux radio, sleep, fan and system-management pages are absent. Search has the shared gamepad keyboard.
The game box supports front/back and scrolling. The full Linux This-game orchestration and every Curator workflow
are not yet feature-parity claims.

`PlayerActivity` is in the same APK, in a private `:player` process with its own `pocket_core_android` library. It
does not link the winit launcher; its dependency graph is audited independently for GPL-compatible cores. Android owns an EGL/GLES 3 `GLSurfaceView`; its GL thread owns the existing
`libretro::Core`, core options, SRAM/RTC writer and save/resume logic. The pinned small bundle contains mGBA for GB/GBC/GBA, Nestopia for NES, bsnes for SNES, Gearsystem for SMS/GG, ClownMDEmu for Mega Drive cartridges, Beetle WonderSwan, Stella for Atari 2600, optional Gambatte for GB/GBC, Beetle PCE Fast, Beetle NGP and Handy. The internal test bundle also includes PPSSPP, melonDS DS and Mupen64Plus-Next (see `android/CORE-LICENCES.md` for dependency licence choices and the public-release review).
Other systems currently use external intents. The package list is `android/cores.lock.json`; it is also the launcher's internal routing table. No BIOS or game ROM is included. See
[core package evidence](../android/CORE-LICENCES.md) for licences, immutable source and corresponding-source archives.
The software core's native framebuffer is a small GLES texture; touch controls are a cached overlay. A bounded
catch-up loop advances core/audio ticks independently of missed presentation vsyncs. This avoids stretching audio
when a high-DPI software emulator draws below the core's frame rate. PPSSPP and Mupen64Plus-Next negotiate an isolated GLES context; their quick-start evidence is described below and does not establish general 3D compatibility.

## Audio

`audio.rs` is the same resampling, smoothed rate control, bounded queue and writer on both platforms.
`platform/linux_audio.rs` retains the ALSA/Pulse sink; `platform/android_audio.rs` provides AAudio. Android volume and
audio focus remain Android's. AAudio's burst and actual performance mode select the queue policy; an AudioTrack
fallback that refuses buffer resizing remains usable, with a queue large enough for its reported burst. Linux's
minimum extra queue is zero, preserving its previous targets. Runtime AV/geometry changes update frame pacing, input sample rate and declared display aspect (including oversampled NES output). Suspend closes the player's stream; Continue reopens it.
No PipeWire operation controls Android audio. Real output quality, Bluetooth route changes and physical device latency
require separate evidence; neither a successful build nor emulator playback is an audio release sign-off.

A diagnostic launch (`--ez diagnosticAudio true` on the launcher Activity) records bounded post-resampling stereo
PCM and AAudio xrun changes under private `files/audio-diagnostics/`. It is off normally. Tests must distinguish
this client-side signal from captured physical output. The emulator runs with `-no-audio`; no real devices are used.

## Storage and lifecycle

The real `ACTION_OPEN_DOCUMENT_TREE` picker persists read permission. Scans run on workers across the persisted roots, each with
visited-ID protection, 50,000-entry and 32-level limits. Identity is `authority:documentId`, not the path, basename,
scan order or a generated fake filesystem URI. `document_library.rs` recognises system aliases in ancestors under
one picked root, including nested game folders. M3U/CUE/GDI references suppress their tracks/discs and missing direct
dependencies remain advice. Provider IDs remain as stable as the provider's own contract; cloud paging/cancellation,
renames that change an ID, and fully staged large disc trees are not solved by this identity rule.

The launch intent carries the current appearance/language/text settings so it cannot race asynchronous preference persistence.
Classic cartridges are copied from the selected document stream on a worker into private `files/content/<identity>/`.
The core receives that genuine local file, not a `content://` string dressed as a path. The original basename is retained;
saves and states are isolated by provider identity. Player files live in private `files/saves/`, `states/`,
`core-options/` and `screenshots/`. Quit and lifecycle suspend write an auto state; next launch calls shared `Core::resume`.
Manual slots are separate. Failed saves remain visible rather than silently quitting. A kill before a write finishes
cannot promise recovery. The Earlier ring is still a first implementation, not full Linux timeline parity.

Shared cover matching downloads art via Android HTTPS on a worker, with stable private cache keys. Offline game facts
come from the shared embedded data; Wikipedia summaries use the shared parser/cache and Android transport. Artwork and
Wikipedia text are fetched at runtime, with the box's existing attribution. Large-library incremental indexing remains open.

## Phones, touch and gamepads

The launcher has its own touch-first phone layout (`android/src/phone.rs`; the first start in `onboarding.rs`). It paints
at the screen's own pixels and sizes everything in dp (`Ui::dp`, from Android density): Body 16 sp, titles 22-30 sp,
targets at least 48 dp. A bottom tab bar (icons over labels in portrait, beside them in landscape; Games, Favourites,
Search, Curator, Settings; labels shrink before they are cut) replaces the handheld's top tabs. Portrait Shelf/Cards/
Cabinet are rows of fronts on boards (Cabinet: compartments) that scroll with momentum; Minimal/Split are 64 dp rows
with the system stripe. A strip above the tabs shows the selected game with Info and Play. Landscape keeps the
handheld shelf as tall as the page (spines and the selected front; Cards with smaller fronts) beside a panel with the
picture or the Cabinet's TV, the name, Info and Play. Tap selects (the case lifts), tap again or Play launches, a
450 ms hold opens the card; drags scroll lists or step the landscape shelf, a fling continues and decays. The game card
uses the shared box/info painters on a smaller canvas that is enlarged (`Ui::zoom`), with Play, Favourite, Turn over
and Back as pills (a column at the right in landscape). Sheet pages are one-line 56 dp rows (name, value right);
facet values have tick boxes and Curator's Show games is a sticky primary button. Search is a pill field with a
phone-sized keyboard (52 dp keys) that hides the tabs while open; Android Back closes it. Empty pages always show a
vignette, a heading, what to do and (where it helps) a button, also before any folder is chosen.
Controller hints (key caps above the tabs) and the selection's band/frame appear after a key press or when a
controller connects, and go away at the next touch (`Ui::touch_mode`). The shared game card painters still show
handheld wording in places (for example "Press Y" for the rating).
`scaled_layout::mobile_size` remains the player's canvas and the game card's base size. Text size honours Android font scaling and the
app's setting. A shared Java inset policy keeps the player's controls away from display cutouts and gesture edges and uses Android 12+'s
reported rounded-corner radii; the launcher (`SafeInsets.read(…, false)`) only keeps the cutout, the home-gesture bar and the
corners' ends clear and uses the full width.
The official X300 resolution is [2640×1216](https://www.vivo.com/in/products/param/x300); emulator density 480 dpi is a
**test assumption**, not a measured Vivo setting or a claim of OEM compatibility.

Launcher regions supply virtual accessibility nodes and the tests' touch bounds. Their names keep the full translated tab/menu/game text even when the painter abbreviates a caption on a narrow display.
`scripts/android/phone-design-smoke.py` captures every main screen (all looks, card, info page, Favourites, Search,
Curator, a facet, Settings, controller hints) per language and orientation, and with `--empty` every tab of a fresh
install that skipped the first start. Player touch supports simultaneous
pointers, a translucent D-pad, per-system face buttons, shoulder buttons where applicable, Select/Start and Pause.
The pause menu has two looks, chosen by the input mode. With touch (`touch_visible`) it is a flat sheet of large pills: the game title and a
status line (Saved to slot N, errors), a thumbnail of the paused picture on portrait phones, a 128 px Continue button, one slot picker (‹ › zones 120 px
wide beside "Slot N" and the save-medium row), Save | Load, then Earlier, Options, Screenshot and Quit. Every target is at least 106 logical px
(48 dp) high with 20 px gaps; portrait is one column bottom-aligned for the thumb, landscape two columns so nothing scrolls. A tap activates
at once (no select-then-tap), a tap that only reveals the touch menu after controller input is swallowed. The Options page shows one row per core
option, label over value with ‹ › zones, a Back pill, and scrolls by dragging (two columns in landscape). With a controller it stays the
handheld list with its hint line and the L/R slot shortcuts. Accessibility nodes carry the full labels ("Save · Slot 1", "Previous slot", "Back"...). Android gamepad keys feed RetroPad;
Back or controller Menu opens the pause menu. API 33+ uses the system Back callback; the window uses the default
hidden-bar behaviour because transient-bar mode suppresses edge Back until bars are revealed on Android 15.
The launcher wakes its event loop directly for Back; the player's GL queue wakes even while paused. OS Home is owned by Android and suspends/saves; it is not intercepted.
Left stick and hat navigation feed digital controls; both sticks also feed libretro analog values, with the device's dead zone. Analog/menu transitions are separate from physical key state. Hardware mapping and repeat still need device verification.

## Emulator recipes

`android/java/dev/pocket/spike/EmulatorIntents.java` owns explicit components, checks their presence, forwards read grants
with ClipData, and starts on the Java UI thread. Missing apps show an error; an Activity opening does not prove a core
or game loaded. Default variant order is explicit; editable user profiles and core overrides are still needed.

| Profile | Package / Activity | Action and ROM contract | Validation |
|---|---|---|---|
| RetroArch | `com.retroarch.aarch64`, `com.retroarch`, `com.retroarch.ra32` / `com.retroarch.browser.retroactivity.RetroActivityFuture` | MAIN; `ROM=saf://<escaped tree>/<relative name>`, `LIBRETRO=<installed app dataDir>/cores/<core>_libretro_android.so`; prefix read grant for the selected tree | Emulator test described in status report; package/core variant must be checked separately |
| NetherSX2 / AetherSX2 | `xyz.aethersx2.android/.EmulationActivity` | MAIN; `bootPath=<document URI>` | Recipe only, not installed/tested tonight |
| Dolphin official | `org.dolphinemu.dolphinemu/.ui.main.MainActivity` | MAIN; `AutoStartFile=<document URI>` | Recipe only |
| PPSSPP / Gold | `org.ppsspp.ppsspp` or `org.ppsspp.ppssppgold` / `org.ppsspp.ppsspp.PpssppActivity` | VIEW; document URI as data, octet-stream | Recipe only |
| DuckStation | `com.github.stenzek.duckstation/.EmulationActivity` | MAIN; `bootPath=<document URI>` | Recipe only; legacy path-only builds excluded |
| Azahar | `org.azahar_emu.azahar/org.citra.citra_emu.activities.EmulationActivity` | VIEW; document URI as data | Recipe only |
| Lime3DS / migrated Azahar | `io.github.lime3ds.android` / `org.citra.citra_emu.activities.EmulationActivity` or `io.github.lime3ds.android.activities.EmulationActivity` | VIEW; document URI as data | Recipe only, probes both components |
| Eden | `dev.eden.eden_emulator/org.yuzu.yuzu_emu.activities.EmulationActivity` | `android.nfc.action.TECH_DISCOVERED`; document URI as data | Recipe only, used only if present |
| Citron | `org.citron.citron_emu/org.citron.citron_emu.activities.EmulationActivity` | `android.nfc.action.TECH_DISCOVERED`; document URI as data | Recipe only, used only if present |

Recipes were inspected against the launcher's published
[PS2](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/SonyPlayStation2.json),
[Wii](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/NintendoWii.json),
[PSP](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/SonyPSP.json),
[PS1](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/SonyPlayStation.json),
[3DS](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/Nintendo3DS.json) and
[Switch](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/NintendoSwitch.json) definitions on 29 October 2026.
These are external launcher recipes, not an emulator support guarantee. RetroArch's actual SAF serialization was
checked in its [1.22.2 VFS source](https://github.com/libretro/RetroArch/blob/v1.22.2/libretro-common/vfs/vfs_implementation_saf.c),
and its extras in [platform_unix.c](https://github.com/libretro/RetroArch/blob/v1.22.2/frontend/drivers/platform_unix.c).


RetroArch receives its own default config path, installed data/APK locations and external-files directory, matching
its browser's startup extras. Pocket does not overwrite SRAM/state directories or copy emulator-private config.
Run RetroArch once and configure its own core/BIOS/save directories normally. Custom per-core config selection is not
inferred from another app's private preferences. RetroArch 1.22.2 explicitly exits on a different ROM/core delivered
to an existing Activity; automatic retry of that receiver-specific exit is not yet implemented. Same-game return
and Pocket's selected game are separate from this limitation. Standalone recipes remain version-sensitive and are
not called tested merely because a package resolved. Vita3K's installed-title-ID route remains to implement; VPKs
must not be represented as directly playable cartridges. Winlator is outside this milestone.

## Building and evidence

The Android code is on main (since 2026-10-30; no long-lived branch). The signed APK from main, in one command, on a Mac
with Android SDK 35, NDK 27.2, JDK 17 and `cargo-ndk` (the MacBook Air has all of them):

    scripts/android/build-apk.sh        # -> android/build/pocket-android-mvp.apk (+ .apk.sha256)

The merge gate compiles both Android crates on every merge (`scripts/check-platforms.sh`, [DEVELOPING.md](DEVELOPING.md#merge-gate)),
so main stays buildable for Android. See [android/README.md](../android/README.md). Builds use scripts on our machines only, never GitHub Actions.
The dedicated release keystore is outside every checkout, in `~/.local/share/pocket-ui/android-signing/release.jks`;
its password is a separate private file. Neither is committed. The first debug-signed preview must be uninstalled
once before installing this identity. Further APKs with this key update normally. Current Android version is
`0.1.7-android.1`, versionCode 10701 (the build script derives both from Cargo.toml); the source version is 0.1.8.

`player-smoke.py` uses real SAF, a hash-checked starter homebrew, live UI node bounds, in-app pause/slots/options,
screenshot, Quit/auto-resume and an original GB audio diagnostic. It does not install a mock emulator receiver.
`smoke-test.sh` installs the release APK and runs the own-player and classic-core checks without `run-as`.
`ANDROID_SMOKE_RETROARCH=1` adds `retroarch-smoke.py` against the separately prepared fixture; its test-only
`externalPlayer` launch extra exercises the fallback even for a bundled classic system. A subsequent ordinary launch
clears that override. No fixture setup script is for a real device.
Phone and handheld evidence must list the actual emulator resolution, density, insets and orientation. Screenshots
and status are exported by the coordinating session, not committed. Source builds, UI tests, Linux gates and audio
quality are reported separately; no implication that this milestone already completes the full requested core set.

The small-core lock additionally includes ClownMDEmu for Mega Drive cartridges (six-button touch layout), Beetle WonderSwan and Stella. Settings can choose Gambatte for GB/GBC; its save and core-option namespaces are separate from mGBA to prevent incompatible state loads. Sega CD is intentionally external until SAF disc/BIOS staging exists. See `android/CORE-LICENCES.md` for immutable source and licence evidence.

`build-apk.sh` checks the Android version name/code against the root package version before compiling; the dedicated signing identity stays outside the source tree.

The player initially hides touch controls when Android reports a gamepad. Gamepad key/stick input keeps them hidden and uses the full safe viewport; a touch reveals them again. Pause/back remains available in either input mode.

### Fast core milestone (2026-10-29)

The internal APK bundles fourteen cores in about 19 MB. New-core verification is one launch plus our
pause menu per system through `scripts/android/core-smoke.py`; source bundles and full audio evidence
are reserved for public-release preparation (`ANDROID_RELEASE_SOURCES=1`). PSP 2048, DS Wolveslayer
and N64 Hello World reached a visible frame and our pause menu in the Air emulator. N64's fixture is
a diagnostic, not a 3D game compatibility claim. Heavy downloads, Flycast, a public release audit and
real-device audio/performance validation remain open. No real device was accessed.

Android hardware rendering lives entirely in `src/platform/android_hw_render.rs`: libretro negotiation,
a separate GLES3 context, host framebuffer/depth/stencil, native-size frame capture and context teardown.
`CoreSession` keeps all core calls on that context, including state operations, while Java's Activity
context is restored before composition and lifecycle return. The core context tracks replaced window
surfaces across rotation. Direct shared-texture composition can remove the current readback later.

### Main refresh (2026-10-30)

The Android branch is rebased onto 0.1.5 main. Both Activities still use the same fourteen pinned cores.
The launcher uses the current shared `media`, `game_box`, `game_info`, `case_stickers` and portable
`cabinet_tv_art` painters: real cases, printed discs, GBA/N64 cartridges, favourite stickers and the TV.
Shelf/Cabinet keep native case proportions and use extra landscape width for more spines and a side TV;
portrait places the TV below the selected game. Tap selects, a second tap or A plays, X opens the case/info,
and swipes move selection. The lightweight Android navigation controller remains separate from Linux's
App and its services; this is shared rendering, not the complete Linux Home orchestration.

The info page and the box's back read the player's actual eight save slots, keyed by provider document
identity (including the separate Gambatte namespace). The pause menu uses `save_media::paint_row`, with
the medium's colour, occupied-slot marks and the selected-slot ring. Current ElevenLabs menu/case/shutter
samples play through AAudio. The first visible in-app game frame has the shared 300 ms CRT-on effect
and its sound; input dismisses it, rotation and resume do not replay it.

Remaining parity work: Linux's multi-shelf Cabinet navigation/launch choreography, editable case stickers
beyond favourites, Connections/Rediscover, the This-game/Extras pages, time-of-day lighting and idle
slideshow are not yet Android features. The TV uses a downloaded game screenshot, falling back to cover or console art; the
resume-picture carousel is still to connect. Sharing the remaining Home controller
requires replacing its Linux services with Android library/session adapters, rather than compiling nmcli,
standalone guest control or hardware settings into the APK. Estimated remaining UI parity: 5–8 agent-hours;
real-hardware audio/core compatibility is a separate validation pass.


### First start and library policy (0.1.5-android.2)

First installation opens seven native pocket-ui steps: Welcome, Language, ROM folders, Look,
Covers and pictures, Controls, Ready. Each step offers Continue and, before Ready, Skip; Back
returns to the preceding step. Progress and choices are saved in `launcher.json` after each action,
so force-stop or returning from the Android folder picker resumes the same step. Settings → First
start reopens the guide. Skipping does not require a ROM folder or an Internet connection.

Language is English/German. Look offers cached visual previews of Shelf, Minimal, Split, Cards and Cabinet, Cozy/Quick
animations and dark/light appearance. Controls explain touch and controller navigation; the app
asks Android whether a nonvirtual controller is connected. This uses the Android navigation
controller and shared Pocket painters; it does not provide every Linux Home/settings feature.

The real `ACTION_OPEN_DOCUMENT_TREE` picker takes and persists read permission. Each new selection
adds a root without revoking older grants; cancellation retains the library. A worker scans all saved
roots, retaining readable roots if another provider fails. Provider document identities remain the
save/selection keys; additional roots receive stable virtual path prefixes to isolate matching names.
Empty selections show a recoverable message and allow another selection or Skip. Settings can add
more folders later. Full disc/BIOS staging and removing individual roots remain follow-up work.

Covers and pictures are an explicit saved choice, also editable in Settings. With download enabled,
the browsing worker requests the selected game's cover, screenshot and known English Wikipedia
summary through the existing shared cover/game-picture/wiki sources. It rechecks Android network
capabilities before download: Wi-Fi, Internet capability and not metered are required. Cached media
remains usable offline; this is browse-time loading, not a full-library background downloader or
translation of Wikipedia articles. Choosing Not now or skipping the choice leaves downloads off.

`scripts/android/onboarding-smoke.py` targets only emulator-5582 after verifying `ro.kernel.qemu=1`.
It clears the app's data, preserves ROM fixtures, checks resume at steps 2/3, empty and populated SAF
roots, language/navigation and completion persistence, and captures all seven steps in English
portrait and German landscape. `--all-combinations` adds the other language/orientation pairs.
It derives every tap from a fresh UIAutomator tree. Run this destructive fresh-install check only
after player/core checks; it removes the emulator app's private states and preferences.

The older launcher/player smoke scripts assume setup has been completed (or skipped) first. The dedicated onboarding smoke clears data and must therefore run afterwards.


### Android player picture policy
`android/src/picture.rs` owns density-independent placement and persisted picture settings in
`picture/systems/<system>.json` and `picture/games/<stable-key>.json`. Missing files default to Fit,
no frame, Sharp; game settings override system defaults. The four Picture FileOption rows precede
core options and carry the Picture category for grouped menus. Integer sizing quantizes physical
source scanlines, preserving the core display aspect; landscape touch targets overlay a full-height
picture, portrait targets occupy the lower control region. Native framebuffer uploads remain unchanged.
The GLES game pass supports nearest/linear filtering and CRT/scanline shading, independently of the
menu texture. The small Console/CRT TV surrounds are Android procedural frames; imported Linux frame
packs and its full collection are not yet exposed here.

## 0.1.5-android.6: controller everywhere, pause sheet, touch pad

- Launcher gamepad: winit 0.30.13's Android backend dropped joystick motion (a pad's hat D-pad and stick).
  `vendor/winit-0.30.13-android` (Android platform only, `[patch.crates-io]` in `android/Cargo.toml`) turns hat and
  left-stick motion (`SOURCE_JOYSTICK`) into arrow-key presses with edges. Tested with a virtual USB pad through
  Android's `uinput` tool on the emulator (hat, stick, A/B/L/R/Start, Mode for pause).
- Player pause: one sheet for touch and controller (`touch_pause`), a focus ring and a hint line for the controller;
  the handheld list is gone. Landscape is two columns that always fit (buttons shrink to 48 dp before anything
  scrolls). Options: sections Picture (the player's own picture settings first), Sound, Controls, Speed, sorted by
  key/name words (`option_section`); model/BIOS/preset/restart options in "Advanced (this emulator)", one row that
  opens it; values On/Off; "(Restart)" becomes "after a restart" in the name line; Up/Down follow the page order.
- Touch pad (`touch_controls`): drawn with real transparency (the overlay's top byte is alpha; the shader falls back to
  "non-black is opaque" for pixels without one, such as text and the picture's frame). An 8-way D-pad from the
  finger's angle (`Player::bits_at`), face buttons per system, shoulders, Select/Pause/Start pills, at least 56 dp;
  landscape clusters at the sides with Pause under L, portrait clusters low under the picture. A new press calls
  `PlayerActivity.haptic()` (VIRTUAL_KEY feedback, follows the system setting); pressed controls light up.


## 0.1.6-android.8: the rest of the Cozy extras (2026-10-30/31, from main)

Memory card in the pause menu, printed discs, game connections, Rediscover, light by time of day, decorations and the idle
slideshow now work on a phone; every Cozy extra has its row (list above, tested by `scripts/android/extras-smoke.py`). The
launcher keeps two new things in `launcher.json` for them: `played` (when each game was last started) and `play_secs` (time
played per game). Shared code moved without a behaviour change on the handheld: `civil_date` out of `play_history`,
`room_props` out of `app_room_extras`, `slideshow` compiled for Android with the picture collection kept on Linux; the
shared info page and the box's back note where their linked facts are painted (`link_rects`, `link_rect`) for taps.

## 0.1.6-android.7: main refresh (2026-10-30, main 628c349)

Main merged into the Android line (`feat/android-main-refresh`). Shared painters now bring the real cases, memory-card and
cartridge media, stickers, the Cabinet TV, light/dark sets, the new logo and the case sounds. New shared modules in
`android_modules.rs`: `font_style`, `room_deco` (the painters call it), `button_prompts`, `cozy_extras`, `playful`.
(`room_light`, `room_props`, `slideshow` (timing and painting), `connections` and `civil_date` followed in 0.1.6-android.8.)
Settings (flat sheet, `ui.rs` `SETTINGS`): Look, Appearance, **Colours**, **Font**, Text size, **Cozy extras** (own sheet),
Language, Animations, Playful cases, Old-TV switch-on, **Motion**, UI sounds, Case sounds, Slideshow before screen off,
**Button symbols**, Game Boy core, ROM folder, Covers and pictures, First start. The cozy extras you hear or that move, and
the slideshow, are rows of their own as on Linux (Sound & feel, Sleep & power; `cozy_extras::Extra::home`); the sheet holds
the eight of Linux's Cozy extras page in its order (`Extra::PAGE_ROWS`). These look settings are kept in `launcher.json` (`Preferences`, a wrong value costs that value only), handed to
the shared painters' process-wide switches by `Preferences::apply_look` (every paint, at start, and in the player from
the launch's `playerSettings`).
Cozy extras on a phone (as rows of Settings or of the sheet, above): Case sounds, Old-TV switch-on (player), TV on the cabinet (landscape Cabinet panel), Stickers on
cases (the Favourite sticker is the only one a phone has), Memory card in the pause menu (off: the slot picker's slim row;
on: the game's card or cartridge with its eight slots in the picker, which grows to hold it; `player.rs` `touch_pause`;
`scripts/android/extras-smoke.py`), Printed discs (the disc or UMD in an open box, `ui.rs` `paint_box`; off: a plain disc in
the system's colour), Game connections (a fact on the card that other games share, Developer/Publisher/Series/Genre from the
embedded offline list, gets an arrow and is a tap target; its list is `Screen::Connection`, B returns to the card; controller:
Up/Down pick a fact, A opens; `ui.rs` `fill_links`/`open_link`, hit rectangles from `InfoView::link_rects` and
`BoxView::link_rect`), Rediscover (a Curator row: games started a year ago today or not for a year, from the start times
the launcher keeps in `launcher.json` `played`; a phone has no older history, so it says "None yet" for the first year),
Light by time of day (the Shelf and Cabinet looks' boards, compartments and cases take `room_light::at(minute_now())`: cool
morning, warm evening, dim night; half as strong on the Light page; read every 20 s, so it follows the next paint, `Ui::room_sync`),
Decorations (one more shelf at the end of the portrait list, and after the last case in landscape, with the plant at its stage
and a figurine per system played ten hours: `room_deco::earned` over the play time the launcher keeps per game in
`launcher.json` `play_secs`, counted from the launch to the launcher coming back, at most a day a launch; no completed or
"100 %" status exists on a phone, so no trophy or framed "100 %"; shared `room_props.rs`), Idle slideshow (off until switched on, as everywhere: after a quiet minute on Games or Favourites the
phone shows the games' covers and pictures and the player's save pictures and screenshots, `android/src/show.rs` over
`slideshow.rs`'s timing and painting at half the screen's size; a touch, key or Back only ends it; it ends by itself after
ten minutes; while it can run the window keeps the screen on, `PocketActivity.keepScreenOn`, and lets go when it has run
or found nothing to show, so Android's screen-off time applies again; `scripts/android/extras-smoke.py` with `SLOW=1`),
Playful animations. Every Cozy extra has a row on a phone. The Linux rows for LEDs, fan, sleep & power, Tailscale,
web companion, Bluetooth, battery and Store have no Android counterpart and no row. Appearance on Android stays Dark/Light.

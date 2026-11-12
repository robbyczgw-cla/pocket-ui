# Android and platform adapters

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you work on the Android app or the platform adapters.*

## Android feasibility branch (2026-10-28)

`android/` is an independent `cdylib` package using winit/android-activity and softbuffer. It includes selected existing
source modules by path, so it never compiles or launches the ROCKNIX app/services. `src/text_size.rs` contains the
platform-independent TextSize value (re-exported by settings); Android supplies its locale over JNI, not system.cfg.
Build, emulator setup, scope and controls: [android/README.md](../../android/README.md),
[user guide](../user/android-spike.md).
This is a feasibility experiment, not a supported release target.

## Platform adapters

`src/platform/mod.rs` selects OS adapters at I/O boundaries. `platform/linux_http.rs` owns the existing curl
transport for shared covers, pictures and ROM identification, including compressed-listing fallback. Network work
stays on the existing workers; this move changes neither curl arguments nor cache/persistence behaviour.

## Android launcher and in-app player

`src/lib.rs` selects `linux_modules.rs` or `android_modules.rs`. The Android entry depends on the normal shared
library; `platform/` supplies Android SAF/JNI, HTTPS/system queries and AAudio. `library_names.rs` and
`look_settings.rs` hold shared pure rules/values, re-exported by Linux. `audio.rs` keeps the shared queue/rate control; `platform/` selects the output sink. Android uses an AAudio data callback with the bounded atomic stereo handoff in
`platform/android_pcm.rs`; its host tests cover missing tails, restart fades, stereo order and concurrent counter wrap. The callback follows the [AAudio real-time restrictions](https://developer.android.com/ndk/guides/audio/aaudio/aaudio). The EGL Android player uses shared `libretro::Core` and save logic. The complete composition, supported subset,
intent table and build/emulator scripts are documented in [ANDROID-MVP.md](../ANDROID-MVP.md) and
[the Android build guide](../../android/README.md). The older feasibility section above records the original spike.
N64, PSP and DS remain experimental: `android/src/play_cards.rs` supplies the translated status
for phone shelves, game cards and Controls; `src/android_02_tests.rs` covers all phone looks and Continue.

`platform/android_audio_timing.rs` checks presentation estimates before scheduled UI sounds use
them. `AudioRoute.java` confirms the stream's local output by device ID; contradictory timestamps
on local Fast routes fall back to Written−Read, while raw values remain in diagnostics. The
microphone/frame comparison and regression commands are in the Android MVP guide.

Android portable rendering: `src/cabinet_tv_art.rs` holds the shared TV body/glass painter (Linux worker reexports it); `game_card_model` and `play_time` also provide the portable problem label and relative-time text used by box/info pages. Android target composition selects these painters without Linux services. See [ANDROID-MVP.md](../ANDROID-MVP.md#main-refresh-2026-10-30).

The private Brick build keeps its extractor and card installer in `src/untar/brick.rs` and
`src/update/brick.rs`, compiled only with `--features brick`. Standard embedded assets remain
in their original locations; Brick translations and manual variants are separate. See
[BRICK-CFG.md](../BRICK-CFG.md) for the reference comparison and outstanding verification.

### Android 0.1.14 refresh modules

`src/state_policy.rs` is the portable raw-state size exception list, used by
`hostsys::core_states_vary` and `android/src/player.rs`, through `Core::apply_state_policy`. PPSSPP is not in the
shorter-as-is list (it ignores the length it is given); it is in the padded list (`core_pads_short_states`), because its
`retro_serialize_size` is 128 MiB until its renderer is up (Android resumes before that) and later an 8 MiB-rounded
measure, while the state written at quit is 40 MiB.
`android/src/phone.rs` paints Easy and the Games Just Play action; `ui.rs` refreshes
Curator user history before the shared choice policy and launches through `Command::Launch`.
`onboarding.rs` uses separate action IDs for Animations/Appearance so adding a Look
cannot collide with those controls. See [ANDROID-MVP.md](../ANDROID-MVP.md#android-refresh-for-0114-branch-build)
for supported behavior and the remaining picture/player differences.

The pause-menu shot reader (`preview.rs`) accepts older one-image shots and an optional 4×1 game rectangle before the picture (the initial picture-first layout is also accepted), ignores invalid crop bounds, retries incomplete files, and bounds decoding to eight nonempty images, 4096² pixels each and 128 MiB per file. Both preview and shot reads use one open file handle so an atomic rename cannot mix requests.

The controls card remembers each console in `<data>/controls-card-seen` using `fsutil::write_atomic` before showing it; a failed write skips the interruption. Existing auto-resume states (including taken/retry states) consume the first-start mark without showing a card. Its twelve-second deadline participates in the event loop wake-up schedule. The card consumes the dismissing press before HOME/START shortcut handling. `user_journeys` exercises dismissal, an idle timeout and existing resume states with a stand-in game. Overlay taps take priority over the search and game card underneath; The Tour remains behind the card.

Android chooses the shared heavy audio reserve with `audio::needs_jitter_reserve` for N64, PSP and DS,
on initial open and after audio focus returns. The callback/endpoint policy stays separate.

DS stylus events pass through `platform/android_pointer.rs`: a complete tap between emulation
ticks reaches one tick, while pause, input-mode changes and cancellation discard pending touches.

Android periodic Earlier captures keep `retro_serialize` on the core/EGL thread and log its
cost separately. `platform/android_earlier.rs` moves the owned state and an `Arc` of the immutable
frame to one worker: pixel copying, PNG encoding and durable state/preview writes run there.
At most one capture is active and one newer capture waits; a newer waiting capture replaces the old
one. Quit finishes an active write and discards pending Earlier work (the newer Continue state is
saved separately). A worker failure preserves the previous state file and is logged; there is no synchronous
fallback in play. The Linux player already uses `timeline::Writer` and the serialize cost guard,
which defers expensive captures to a quiet pause.

## Optional systems

`android/src/core_downloads.rs` connects first-start selections and Extras to the shared store installer.
`CoreDownloads.java` selects an unmetered Wi-Fi network and retains partial archives; `ResumableDownload.java`
provides size-bounded HTTPS with Range continuation. Host coverage is `ResumableDownloadTest.java`,
`android_cores::tests` and the real Android controller in `android_02_tests`.
The package builder and local catalog instructions are in [STORE.md](../STORE.md#android-core-packages).

## Direct documents

`GameDirect.java` checks the current SAF document's seek/reopen capability on the content worker. The player opens its
own descriptor (Activity intents carry only URI/name), and `platform/android_content.rs::DirectGame` owns the native
file and an alias with the original suffix until after `CoreSession` drops. Its small large-document CRC cache preserves
old `GameCopy` fingerprints; no state migration is needed. `frozen_refresh` makes its own short-lived launcher alias when
there is no persistent copy. Game check uses the same probe for free-space advice. Disc sets/parent-dependent CHD and unsupported providers
keep `GameCopy`; the concrete copy reason is logged. Host tests cover suffixes, reopening/seeking, cleanup and old moments;
`GameDirectTest.java` checks routing for all pinned cores, and `core-info.c` prints their actual loader flags.

# Android launcher and in-app player — 30 October 2026

## Current boundary

`android/` is a normal consumer of the root `pocket-ui` library. `src/lib.rs` selects the Linux or Android module
composition. The Android launcher uses shared media/spine/box painters, the interface kit, text/i18n, search,
Curator facets and metadata. Its navigation is Android's own controller, not the entire ROCKNIX App transplanted.
Shelf, Easy, Split and Cabinet are available (Minimal and Cards went with 0.4; `Theme::Split` reads their old values), Shelf and Cabinet with [several shelves](#shelves-on-the-games-page). Favourites and appearance/language/text/sound settings are
app-private; Linux radio, sleep, fan and system-management pages are absent. Search has the shared gamepad keyboard.
The game box supports front/back and scrolling. The full Linux This-game orchestration and every Curator workflow
are not yet feature-parity claims.

`PlayerActivity` is in the same APK, in a private `:player` process with its own `pocket_core_android` library. It
does not link the winit launcher; its dependency graph is audited independently for GPL-compatible cores. Android owns an EGL/GLES 3 `GLSurfaceView`; its GL thread owns the existing
`libretro::Core`, core options, SRAM/RTC writer and save/resume logic. The pinned small bundle contains mGBA for GB/GBC/GBA, Nestopia for NES, bsnes for SNES, Gearsystem for SMS/GG, ClownMDEmu for Mega Drive cartridges, Beetle WonderSwan, Stella for Atari 2600, optional Gambatte for GB/GBC, Beetle PCE Fast, Beetle NGP, Handy and PCSX ReARMed for PlayStation ([PlayStation](#playstation)). The internal test bundle also includes PPSSPP, melonDS DS and Mupen64Plus-Next (see `android/CORE-LICENCES.md` for dependency licence choices and the public-release review).
Other systems currently use external intents. The package list is `android/cores.lock.json`; it is also the launcher's internal routing table. No BIOS or game ROM is included. See
[core package evidence](../android/CORE-LICENCES.md) for licences, immutable source and corresponding-source archives.
The software core's native framebuffer is a small GLES texture; touch controls are a cached overlay. A bounded
catch-up loop advances core/audio ticks independently of missed presentation vsyncs. This avoids stretching audio
when a high-DPI software emulator draws below the core's frame rate. PPSSPP and Mupen64Plus-Next negotiate an isolated GLES context; their quick-start evidence is described below and does not establish general 3D compatibility.

## Audio

`audio.rs` is the same resampling, smoothed rate control, bounded queue and writer on both platforms.
N64 and Dreamcast request no extra cold-start reserve. N64's former 352 ms queue remained above 380 ms after 20 seconds of continuous music, so normal latency takes priority over startup supply gaps. The Saturn candidate alone uses a measured 132 ms target. After 15 seconds of delivered PCM, intervals at least 50 ms long below -60 dBFS can return the excess with 5 ms guards and a 5 ms crossfade; partial release keeps ordinary pitch correction disabled until all extra latency has returned. Live diagnostics expose removed peak, splice step and full-return playing-output frame. A one-cent fallback preserves continuous music but cannot justify a reserve that remains after 20 seconds. Bomberman and Sega Rally startup probes reached normal latency within that bound; complete gameplay and post-Continue sound qualification still govern release. The shared normal heavy-core target remains 48 ms, and core startup/load gaps remain documented limits. The N64 regression drives the production mixer with 20 seconds of continuous tone and requires normal latency without PCM drops.
`platform/linux_audio.rs` retains the ALSA/Pulse sink; `platform/android_audio.rs` provides AAudio with a data callback.
It requests Exclusive LowLatency, then Shared if opening fails. A bounded stereo atomic handoff (768 frames on the fast test endpoint; at least two actual endpoint bursts)
(`platform/android_pcm.rs`) connects the existing mixer worker to 256-frame callbacks. The Android
speaker queue keeps at least 32 ms before a new core delivery: the Odin starved at startup with 20 ms,
and two callback blocks sometimes starved even with PCM waiting in the mixer; three callback blocks bridge that worker jitter.
AAudio itself requests three hardware bursts after two bursts reported a startup xrun on the test handheld. The callback takes no locks,
allocates nothing and never waits; only the worker waits for handoff space. An unavailable output is reopened
after an error callback or 100 ms without handoff progress. Partial starvation plays the available prefix, fades
its end and fills the missing tail with silence; a full callback of fresh PCM resumes with a short fade.
Android volume and audio focus remain Android's. AAudio's burst and actual performance mode select the queue policy; an AudioTrack
fallback that refuses buffer resizing remains usable, with a queue large enough for its reported burst. Linux's
minimum extra queue is zero, preserving its previous targets. Runtime AV/geometry changes update frame pacing, input sample rate and declared display aspect (including oversampled NES output). Suspend closes the player's stream; Continue reopens it.
No PipeWire operation controls Android audio. Real output quality, Bluetooth route changes and physical device latency
require separate evidence; neither a successful build nor emulator playback is an audio release sign-off.

A diagnostic launch (`--ez diagnosticAudio true` on the launcher Activity) records bounded post-resampling stereo
PCM and AAudio xrun changes under `audio-diagnostics/` in the app's own folder on shared storage (private files when
Android has none; [Reading a real handheld over adb](#reading-a-real-handheld-over-adb)). It is off normally. Tests must distinguish
this client-side signal from captured physical output. Capture defaults to 30 seconds; the optional
`--ei diagnosticAudioSeconds 240` is bounded to 1–240 seconds and lets a test retain startup plus
180 seconds of game PCM. This larger buffer exists only during diagnostic launches and changes
their memory footprint. The emulator runs with `-no-audio`; no real devices are used.

## Storage and lifecycle

The real `ACTION_OPEN_DOCUMENT_TREE` picker persists read permission. Scans run on workers across the persisted roots, each with
visited-ID protection, 50,000-entry and 32-level limits. Identity is `authority:documentId`, not the path, basename,
scan order or a generated fake filesystem URI. `document_library.rs` recognises system aliases in ancestors under
one picked root, including nested game folders. Each document also carries the selected folder's provider display name
as `root_name`: when the root itself is `psp`, `ps1`, `Genesis` or another known alias, it supplies the system context.
A nearer named system folder takes precedence. Relative disc/artwork paths and `authority:documentId` identities stay
unchanged; opaque provider ids are not mistaken for display names. PSP CHD is included in the same supported-type list
used by Game check, matching the pinned PPSSPP core. M3U/CUE/GDI references suppress their tracks/discs and missing direct
dependencies remain advice. Provider IDs remain as stable as the provider's own contract; cloud paging/cancellation,
renames that change an ID, and staged disc trees of several files are not solved by this identity rule.

The launch intent carries the current appearance/language/text settings so it cannot race asynchronous preference persistence.
A worker first checks direct document access (`GameDirect.java`; [big games](#big-games)). Seekable single-file games use a
read-only `ParcelFileDescriptor` opened inside `:player`, with an original-name symlink to `/proc/self/fd/N` kept alive until
after the core unloads (`platform/android_content.rs`). Providers that deny reopening/seeking, unchecked cores and disc sets
retain the private `files/content/<identity>/` stream-copy fallback (`GameCopy.java`). The original basename is retained;
saves and states are isolated by provider identity. Player files live in private `files/saves/`, `states/`,
`core-options/` and `screenshots/`. Quit and lifecycle suspend write an auto state; next launch calls shared `Core::resume`.
Manual slots are separate. Failed saves remain visible rather than silently quitting. A kill before a write finishes
cannot promise recovery. The Earlier ring is still a first implementation, not full Linux timeline parity.

Shared cover matching downloads art via Android HTTPS on a worker, with stable private cache keys. Offline game facts
come from the shared embedded data; Wikipedia summaries use the shared parser/cache and Android transport. Artwork and
Wikipedia text are fetched at runtime, with the box's existing attribution. Large-library incremental indexing remains open.

## Package name

The app is `xyz.pocketui.app` (pocketui.xyz) since 2026-11-09, before any public APK: Android treats another name as
another app, so it can't change later. Manifest, the Java classes (`android/java/xyz/pocketui/app/`), the native function
names in `android/src/lib.rs` and `player.rs` (`Java_xyz_pocketui_app_…`) and the smoke scripts carry it;
`tests/android_version.rs` fails when one of them is left behind (the app would stop at its first start). The internal
QA builds up to 0.3.1-android.1 were `dev.pocket.spike`: a different app to Android, which neither updates to this one
nor shares its saves. Remove it by hand on a test device. Log tag (`PocketMvp`) and APK file name are unchanged.

Azahar (3ds profile): `EmulatorIntents.create` adds `FLAG_ACTIVITY_CLEAR_TASK` for it only. Odin 2 Mini, 11.11.2026, Azahar 2126.2 with
its setup done first (its MainActivity in the task): Close Game → OK landed on Azahar's game list, a second Back stayed there,
play time was not counted; with the flag it returned to pocket-ui's shelf twice and counted 54 s and 51 s. Test:
`document_check::a_3ds_game_starts_azahar_in_a_clean_task_…`.

## Help

Settings → Help (`Setting::Help`, `Screen::Help`, `android/src/help_page.rs`; the row before Report a problem). One page of
eight topics, each a row; A shows its answer above the rows (the way Licences shows a core), Back or B leaves. The topics
and answers are the constants `help_page::TOPICS`: games folder, more systems, pause menu, save and keep playing, the copy of
your saves, Plays with, leaving another emulator app (ARMSX2 Close, Dolphin Exit Emulation, Azahar Close Game and OK, hakuX
Exit) and Report a problem. Every text has a German and a Spanish line in `assets/i18n`. Tests (`src/android_02_tests.rs`):
`android_help_is_reached_with_the_d_pad_alone_…` (D-pad reaches every row, every topic answers, B goes back onto Help's row),
`android_help_is_in_german_and_spanish_…` (no English left, every Settings row named exists) and
`android_help_fits_retroid_and_odin_screens_whole_…` (Retroid and Odin formats, two text sizes, three languages, nothing cut).
When an answer names a button or menu of another app, check it against the app's own menu first; the apps are not ours.

## Report a problem

Settings → Report a problem (`Setting::Report`, `Screen::Report`, `android/src/report_page.rs`; the last row, there is no
Help & tools group on a phone). Linux zips the logs onto the SD card (`src/report.rs`); Android hands one text to the
share sheet or the clipboard. Three rows (Share, Copy, Back) under the words of `Step::Rest`/`Busy`/`Said`.

- **Who writes what.** `src/report_phone.rs` (compiled and tested on Linux, listed in both module lists) puts the text
  together from `Facts`: `PocketActivity.deviceReport` (`key=value` lines: time, app version and build, channel, model,
  Android, ABI, screen, density, refresh, text size, controllers by USB id only, battery saver, free space; never the
  phone's name or a pad's name), the host's own facts (look, language, input, games per system with `player_core`, chosen
  and installed optional cores) and `PocketActivity.recentLog` (logcat `-d -t 6000 -v threadtime`, kept by pid and tag:
  the app and player's `PocketMvp` lines, warnings and errors of the two processes, crash traces naming the package).
  The log is split by process id (`main=<pid> player=<pid>` on its first line) into pocket-ui, player and earlier runs;
  the sound section is the player's last `player audio:` line (`android/src/player.rs`).
- **Privacy.** `src/report_redact.rs` is the Linux report's `Redactor` moved out unchanged (`report` re-exports it) with
  `clean_phone` added: `content://` addresses lose everything after the provider, `/storage/emulated/N/…`,
  `/storage/<card>/…` and `/sdcard/…` lose their folders, game names become `[game <system> <code>]`. Tests in
  `report_phone.rs` (private-log fixture) and `report_redact.rs` guard it; the page's words are German/Spanish in `de.tsv`/`es.tsv`.
- **Size.** At most `MOST_BYTES` (60,000, GitHub's issue limit is 65,536; an Intent's limit is about 1 MB). The logs
  give way: 30 % pocket-ui, 50 % player, 20 % earlier runs, newest lines first, each line cut at 400 bytes.
- **Flow.** `Command::ReportShare` / `ReportCopy` → `App::report_start` gathers the app's facts and spawns a worker
  (logcat is read there, never on the event thread) → `ReportSupport.share`/`copy` on the UI thread (`ACTION_SEND`
  `text/plain` in `Intent.createChooser`, or `ClipboardManager`) → the worker's sentence comes back through a channel
  read in `about_to_wait`; the page says it, and a toast does when the page was left meanwhile.

## Saves out and back

Settings → Save backup (`Setting::Saves`, `Screen::Saves`, `android/src/saves_page.rs`). A backup is one zip:
`saves/<key>/…` and `states/<key>/…` as they lie under the app's files, and `pocket-ui-saves.json` (format 1: each
key's console, file name, title and core namespace, each file's time of writing in ms).

- **Who decides what.** `src/save_transfer.rs` (compiled and tested on Linux): `export_plan` lists every file of every
  game that has any; `read_backup` judges a picked zip's manifest and names; `Backup::plan(replace)` says which entries
  go where, which folders go first and which move in. `SaveBackup.java` only moves bytes by that plan
  (`java.util.zip`, streaming, so a PSP game's states need no memory and a cloud provider's pipe works).
- **Flow.** `Command::SavesExport` → `exportSaves(plan, name)` → `ACTION_CREATE_DOCUMENT` → worker → status.
  `Command::SavesImport` → `importSaves` → `ACTION_OPEN_DOCUMENT` → worker lists the zip → `savesStatus` says
  `listed` → `read_backup`; with games that have saves here the page asks (`Step::Ask`), else `importApply(plan)` at
  once. The host reads `savesStatus` at its 150 ms look only while something is on its way (`SavesRun::waiting`).
- **Never half.** An export that fails or is cancelled deletes its document. An import unpacks everything into
  `files/import.part` first; only when every listed file is there do the phone's sets of the replaced games go and the
  backup's folders move in, a game at a time. `import.part` is removed at the end and before the next import.
- **Finding a game again.** By its key (same folder chosen again). Otherwise by console, file name and core namespace
  when exactly one game of the library fits and its key is free; otherwise the files keep their key and wait.
- **What comes back, when both have saves.** Nothing without the player's answer: Keep adds only the games without
  saves here; Use the backup's removes those games' `saves/<key>` and `states/<key>` whole before the backup's move in,
  so an old in-game save never sits beside a newer slot.
- **Times.** Files get their old time of writing back: `frozen` doesn't resume a moment that is older than the in-game
  save, so their order has to survive.
- **Names from a zip are data.** Only `saves/<16 hex>/…` and `states/<16 hex>/…` with plain parts are taken (Rust);
  Java checks again that a target stays inside `import.part` and that a moved folder is `saves|states/<16 hex>`.
- **Not in a backup:** `launcher.json` (settings, favourites, stickers, play time, when each game was last started),
  core options, picture settings, cheats' switches, screenshots, the copies of games. So after an import on a fresh
  install the Continue shelf is empty until a game is started (`frozen_refresh` goes by `played` and the staged copy):
  the start itself resumes and the slots load, seen on an Odin 2 Mini through uninstall, install and import. Carrying
  `played` in the manifest would bring the shelf back at once; not done yet.
- **Tests.** `save_transfer::tests` (survives uninstall, time order, keep/replace, another phone, the other Game Boy
  core, wicked names, wrong files); `android_02_tests::android_save_backup_*` (D-pad reach and B, every state in words,
  nothing cut in EN/DE/ES: `Ui::cut` lists what a sheet page had to shorten). The zip mover itself is Java: checked on an
  emulator or a handheld, not in the gate.

## Saves copied into the games folder

A copy of each game's `saves/<key>` and `states/<key>` goes into `pocket-ui saves/<title> [<key>]/` inside the chosen SAF
tree after the game ends; it survives uninstalling (files made through SAF are not app data), and the first scan after a
new install brings the copies back. The brief's alternatives: SAF write grant on the chosen tree (done) against
`Documents/` via MediaStore (not done: an app that was removed does not get its MediaStore files back on Android 11+
without a user grant, so a restore would need SAF anyway).

- **Who decides.** `src/save_copy.rs` (compiled and tested on Linux, a `Store` trait stands for the folder). `SaveCopy.java`
  is that store over `DocumentsContract` (list, read list, put, get; a folder per game; `README.txt` and `.nomedia` in
  `pocket-ui saves`). `android/src/copy_run.rs` is the worker (`AndroidStore` calls `PocketActivity.copy*`), `lib.rs`
  `copy_tick`/`copy_done` start and judge it. `PocketActivity.scanLibrary` skips a folder named `pocket-ui saves`.
- **When.** Never while playing. The player, after Save & quit or any game end (`nativeFrame` result 2, after the core is
  gone), leaves a note `files/copy-pending/<key>` (`save_copy::mark_pending`). The launcher copies the noted games in the
  background (`copy_tick`, every 3 s, only with a shelf), and at each start copies the games whose files are not what was
  copied last (`save_copy::stale`: file times and sizes against `files/copy-state.json`, no folder access). A note is
  removed only when the run had no trouble. A trouble stops further tries until a newer note or a new folder choice.
- **Not slow.** Save & quit does the same as before plus one empty file; the copy is on a worker in the launcher process
  (`copy_done` logs `save copy: … in N s`). Files whose time and size match the last copy are not hashed again.
- **Permission.** `chooseFolder` asks for read and write; older installs have a read-only grant. `copyTargets` gives each
  folder's state (`ok`, `read-only`, `lost`, `gone`); copying uses the first `ok`, restoring any readable one.
  Without one: a toast once per game end (never silent), `Settings → Save backup → Saved copies` says Off / Folder
  missing and A leads to `Command::Folder`.
- **Format.** Per game `copy.json` (format 1: key, console, file, title, core, and per file time in ms, size, SHA-256) and
  the files below `saves/` and `states/`, without the key. Files first, list last. `copy-state.json` in the app's files is
  the *base*: what the folder held when both last agreed, plus the games that conflict.
- **Never older over newer.** A file is overwritten only if it differs and is later in time than the one it replaces, and
  only while the folder is as this phone left it. Otherwise nothing is written and `copy-state.json` records the conflict;
  logcat has `save copy: conflict for … nothing overwritten`. Restore brings back only games without saves here; identical
  files are in sync, differing ones are a conflict. A restore checks every file against the list's size and hash and moves a
  game's folders in only when all are right (`copy-restore.part`); times come back as they were.
- **Direct documents.** Games read through a document descriptor (no copy of the ROM, CHD, etc.) copy the same: the copy
  only knows the game's key (and its console and file name for finding it on another phone).
- **Tests.** `save_copy::tests` (copy, nothing rewritten, restore after reinstall with times, found by console and file,
  conflicts, older never replaces newer, another phone's copy, no access / folder gone with a sentence and no false
  "done", a cut-short copy, a file that fails its hash, names that lead outside, direct documents, the start's look),
  `android_02_tests::android_saved_copies_row_*` and the words test (EN/DE/ES, nothing cut). The SAF store is Java:
  checked on the Odin 2 Mini with a QA package (report `ANDROID-SAVE-COPY.md`).

## Which way up

Both Activities ask for `fullSensor` in the manifest, and while Android reports a gamepad they ask for `sensorLandscape`
instead (`PocketActivity.orientFor` at each look at the controls, `PlayerActivity.orientFor` at its start and at each
controller change). Found on the AYN Odin 2 Mini (Android 13, 2026-11-09): its panel is an upright 1080×1920 one, its
own pad is "external" to Android like any plugged-in one, and with auto-rotate off `fullSensor` still followed the
accelerometer, so the app stood upright when the handheld lay on its side. A phone without a pad turns freely as before;
a pad that connects turns the page the long way round. Java only: `android-test-kit`'s self-test on a handheld is the
check (its pictures must be wider than high while a pad is present).

## Reading a real handheld over adb

For a test on hardware nothing needs root or a debuggable APK:

- The game stream logs how it opened (`AAudio opened: rate= burst= buffer= capacity= mode= sharing= device= xruns=`,
  tag `PocketMvp`), and the player's 10-second line carries AAudio's numbers for the game's stream:
  `player audio: frames= underruns= queue_ms= dropped_frames= input_rate= target_ms= out_ms= aaudio_xruns= burst= buffer= callback_underruns= handoff_ms= reported_out_ms= timestamp_fallback=` (`android_audio::game_line`;
  `out_ms` is the validated scheduling lead, including the handoff; `reported_out_ms` retains the raw presentation estimate).
  `callback_underruns` counts gaps at the worker/callback handoff separately from the main queue and AAudio xruns;
  `handoff_ms` is its current fill. Do not treat these timestamp estimates as measured speaker latency.
- A diagnostic launch (`--ez diagnosticAudio true`) writes its capture to the app's folder on shared storage
  (`Android/data/xyz.pocketui.app/files/audio-diagnostics/`, `diagnosticsPath`), which `adb pull` reads without root.

## Big games

### Direct documents

`GameDirect.probe` attempts an actual `/proc/self/fd/N` reopen, a last-byte read and a rewind on a worker.
The result is a capability check for this provider and file, never inferred from Android version or URI shape.
The old Android 15 emulator's Downloads/FUSE provider returned `EACCES`; that is a provider-specific fallback reason,
not a reason to copy every file on every Android device. Cloud streams/pipes can also require a copy.

The launcher sends the URI and original basename, not a raw fd number or a descriptor in an Activity intent.
The player opens its own descriptor using the package's persisted SAF grant, duplicates it for native ownership,
and creates a process-local symlink with the real suffix. Launcher and player never reuse each other's fd paths.
The core drops before the native descriptor/alias; pause and background keep it available, while process recreation
reopens the document. A revoked grant fails the start, rather than playing an inaccessible old copy.
The core path still comes from `corePath(core)`, so a downloaded library and its verified system directory work
independently of the ROM descriptor. Download consent and missing-system checks happen before any game access.

All 15 pinned loaders, bundled or installed from optional packages, get a direct attempt for single files: mGBA, Gambatte, Nestopia, bsnes, Gearsystem,
ClownMDEmu, Beetle WonderSwan, Stella, Beetle PCE Fast, Beetle NGP, Handy, PPSSPP, melonDS DS and Mupen64Plus-Next,
plus PCSX ReARMed (15 cores: mGBA covers three systems). The real suffix remains visible to both `Core::load`'s ZIP
logic and loaders which select formats by path. `scripts/android/test/core-info.c` reads each binary's actual
`need_fullpath` and extensions without initialising or running it. The size cap below is a product policy; several
small-console cores also set `need_fullpath` and read their own paths.

For a direct document over 64 MiB, the launcher's content worker first reads CRC-32 once while the insertion animation
runs, without writing the game. The player then reuses that checksum. A tiny per-game
`direct-content/<key>/source.json` caches that CRC with file size, nanosecond modification and change times,
device/inode identity and samples at 16 positions. Changed metadata/samples or unavailable times/identity force
a fresh checksum, including when a patching tool restores the original modification time. The session's tiny `.stage`
uses the existing copy-CRC format so old moments retain the same fingerprint. No state conversion or mark rewrite
is needed. After a successful direct core load and resume check the old game's disposable `content/<key>` copy
is removed. States, SRAM, RTC and their original names stay in their existing directories.

CUE/BIN, M3U/GDI and other sibling-dependent sheets keep the complete-set copy path below. Standalone CHD v3/v4/v5
can use a direct path: the small header is checked for parent flags, MD5 and SHA-1 according to pinned libchdr.
Parent-dependent CHD and unknown/unreadable header versions retain the copy fallback. Their relative names and disc switching must remain coherent; rewriting indices alone has not
been qualified for these pinned loaders. This is an explicit fallback, logged as such. Future/unchecked cores also
keep a copy until their path handling has been checked. Game check asks the same capability probe before claiming
that a large game needs copy space.

The pinned source declarations (`android/cores.lock.json` and the corresponding-source archives) give:

| Core | `need_fullpath` | Direct single-file path |
|---|---|---|
| mGBA, Gambatte, Nestopia, Gearsystem, Beetle WonderSwan, Stella, Handy, melonDS DS, Mupen64Plus-Next | false | Original suffix retained; the frontend reads data or ZIP content as before |
| bsnes, ClownMDEmu, Beetle PCE Fast, Beetle NGP, PPSSPP, PCSX ReARMed | true | Descriptor stays open through all core reads/seeks; sibling-dependent formats retain the copy |

### Copy fallback

- **Limits now.** PSP (PPSSPP) and PlayStation (PCSX ReARMed), both `need_fullpath` (`document_library::plays_from_its_file`,
  `GameCopy.PLAYED_FROM_FILE`): none but room on the phone: the game's size plus 256 MiB (`KEEP_FREE`). Other systems retain the
  512 MiB product cap, the biggest DS card (`LARGEST_IN_MEMORY`), regardless of whether their loader reads data or opens a path. Flycast isn't in the APK.
- **Several files.** A disc sheet's files (a cue's tracks, a playlist's discs and theirs) are `Game::parts`, found by the scan
  (`document_library::parts_of`, three sheets deep as `frozen::fingerprint` reads) and copied by `GameCopy.prepareSet`
  beside the sheet under the names the sheet writes, each once. One bar, one room and one `.stage` for the whole set: its first
  line is the whole's `size time crc` (what `frozen::mix_file` reads), then `size time name` per file. A later start compares each
  file as for one. `Game::size` is the set's total for room checks; `Game::document_size` is the selected entry's own size for the Continue check against `Mark::rom_bytes`. A mark keeps its existing sheet size and full-set fingerprint; no saved moment or mark needs conversion. A name that leaves the sheet's
  folder (`../`) is refused with a sentence; files of an older copy under other names are removed after the renames.
- **Up to 64 MiB**, when a copy is required (`COPIED_EVERY_START`, `frozen::WHOLE_UP_TO`), it is refreshed at every start without a window.
- **Over 64 MiB** the first start shows a window with a bar, the size, the time left and Cancel (Back too), the screen stays on. `.stage` beside
  the copy holds `size time crc`. A later start doesn't copy when the document has the same size and time and the same bytes at 16 places.
  The document is opened at every start, so a lost grant or a missing file says the usual sentence and the copy isn't played behind its back.
- **Space.** Before a copy the room is asked of `StorageManager` (which may free other apps' caches) and reserved (`allocateBytes`). If it can't
  be enough even without the older big copies, nothing is removed and the sentence has both numbers. Otherwise the oldest big copies go first, this
  game's own last. At most 5 stay (`frozen::MOST`). A game whose copy went stands closed on the shelf; its next start copies and finds its moment.
- **Kill mid-copy.** Only a `.part` is written; it is renamed at the end, so a part is never played, and the next start of any game removes it.
  Leaving the app during a copy stops it as Cancel does (`cancelLaunch`, the launch's turn): the next start copies from the beginning.
- **Frozen games.** The whole file's crc in `.stage` goes into `frozen::fingerprint`, so a patch of the same size between the samples is another
  game. A patch that keeps both size and time and misses the 16 places isn't seen by the copy check: the old copy goes on being played, and the
  moment with it. Time 0 in `.stage` (no time from the provider, or the file changed during the copy): the crc still counts, the next start copies again.
- **PPSSPP's moments.** Until its renderer is up the core can report 128 MiB while its real saved state is smaller.
  The pinned adapter ignores the supplied length, so the minimum-size guard remains: a short state is preserved under Load
  and the player pauses safely instead of passing an undersized buffer. Direct documents do not change this Continue limit.

Test: `scripts/android/frozen-smoke.py ... big`, needing root (Google APIs image) and little disk: `Big Test` is PSP2048 (homebrew) in a generated ISO,
padded with holes to 1843 MiB; Later Test (300 MiB) takes the faults that copy again. `BIG_REAL_BYTES=1` adds 600 MiB of real bytes for the speed. The launch
extra `slowCopy` (test only, ms a block) brakes the copy so that Cancel can be pressed; a moving bar keeps UIAutomator from reading the window, so it goes by the log
and a fixed place. Times: [MEASURED.md](MEASURED.md#big-psp-games-on-android-emulator).

## Shelves on the Games page

`src/phone_shelves.rs` (Linux runs its tests in the gate) orders the page with the shared `Cabinet::build`: Recent games (the last 12 started,
newest first), Favourites (grouped by case), then one shelf per system by name; empty ones are left out. A game can stand on several shelves, so
`Ui::visible` holds it more than once and a position says which shelf is meant; after the favourites or the recent games change, the selection stays
on its game and on its shelf while it is there (`Ui::reshelve`). Shelf and Cabinet have them; with one shelf (one system, nothing played) the page
is the old rows of fronts. Split stays a flat list, as do Favourites, Search and Curator's results.

Portrait (`paint_rows`): the shelves one under the other, each named with its number of games and as high as its tallest case. A swipe goes along the
shelf it started on or up the page, whichever way it set off (`drag_along`), and a little of the next case shows at the right. Landscape: one shelf at a time, as tall as the
page, its name as the title. Two buttons in the header or a swipe up or down change the shelf, one a swipe; a swipe along stays on the shelf. Controller: Up/Down
change the shelf (portrait: the case under the selected one; landscape: where that shelf was left), Left/Right go along it and on to the next one.

## Android apps

The drawer under the shelf (`android/src/apps_page.rs`, `Screen::Apps`; Java `AppDrawer.java`). **Apps** on the Games
page's top row (beside Just Play, the only button there in Easy and before any games) or R2 there opens it; B, Android
Back and the Games tab close it. The selection on the Games page is kept (`positions`, by game key after the shelf changed).

- **Which apps.** `launcherApps` lists `queryIntentActivities(MAIN, LAUNCHER)`: every app with a start icon but
  pocket-ui, each once, by name. Visibility on Android 11+ comes from one `<queries><intent>` for MAIN/LAUNCHER in the
  manifest. QUERY_ALL_PACKAGES would show more (apps without an icon) and is a Play policy permission whose permitted
  uses (device search, antivirus, file managers, browsers) do not name home screens; the intent filter is the narrowest
  way that shows exactly what a home screen shows, and it needs no Play declaration. It is what Android's own guide
  offers for "interact with apps that have a launcher activity".
- **When.** Read on a thread at each start and each return (`resumed`), because apps come and go while pocket-ui is in
  the background. Icons are rendered once per app version to `cache/app-icons/<package>-<lastUpdateTime>.argb` (96 px,
  raw ARGB, stale ones removed) and painted at 30 dp, four fifths grey (`apps_page::mute`): no wall of bright icons.
- **Starting.** `getLaunchIntentForPackage` with NEW_TASK | RESET_TASK_IF_NEEDED, as any home screen. An app that went
  in between gives a toast and a new read.
- **Android games.** Y on a card, or a held finger, marks it (`Preferences::android_games`, package → name in
  `launcher.json`). Android's `CATEGORY_GAME` is only a hint ("Looks like a game"), because many games don't set it and some
  apps do. Marked apps are `Game`s with `system_id` `android`, key `android-app:<package>`, appended at the end of the
  library (`put_android_games`): Shelf/Cabinet give them their own shelf "Android" by system name, in a digital case
  (`media::is_digital`) with the title card of a game without a cover; Split, Favourites, stickers, play time and search take
  them as any game. Curator's index, connections, Just Play and covers know only the chosen folders' games. Play starts
  the app (no insert scene); the time until the launcher is back counts as played. Before Android's first answer the
  shelf comes from the saved names, so the selection survives a restart; an uninstalled app leaves the shelf and keeps its
  mark for a reinstall. Without any chosen folder, marked apps alone make the Games page.
- **Tests.** `android_02_tests::drawer` (in the gate): listing and suggestion, the mark and its shelf, start from it and
  after a restart, D-pad and touch reach on the Retroid screens, every empty state, icons, and text fit in EN/DE/ES.
  Java is checked on a handheld (see [Building and evidence](#building-and-evidence)).

## Case stickers

Completed, 100 %, Playing now and Game of the year (with its year) are kept per game in `launcher.json` `stickers`; the star is the game being a favourite, on a phone as
before, so Favourite on the sheet and on the card are one thing. The card's Stickers button or SELECT opens the sheet (`Screen::Stickers`; tick boxes, a Year row with
arrows; a sticker that is on takes the new year at once; the year starts at the game's release year or this one, `case_stickers::default_year`). B comes back to the card. The shared painters show
them on the case's front, its spine and the box, as on Linux. Off in Cozy extras, or in Split, the sheet says why they don't show. Completed and 100 % earn the trophy and the framed "100 %" (`room_deco::earned`). A phone has no Curator stickers of its own yet.

Test: `scripts/android/phone-design-smoke.py --parity` clears the app and goes through the first start with the real folder picker, in each orientation.

## Phones, touch and gamepads

The launcher has its own touch-first phone layout (`android/src/phone.rs`; the first start in `onboarding.rs`). It paints
at the screen's own pixels and sizes everything in dp (`Ui::dp`, from Android density): Body 16 sp, titles 22-30 sp,
targets at least 48 dp. A bottom tab bar (icons over labels in portrait, beside them in landscape; Games, Favourites,
Search, Settings, and Curator before Settings with Settings → Curator tab, `Preferences::curator_tab`, `Ui::has_tab`;
labels shrink before they are cut) replaces the handheld's top tabs. Portrait Shelf/
Cabinet are rows of fronts on boards (Cabinet: compartments) that scroll with momentum; Split is 64 dp rows
with the system stripe. A strip above the tabs shows the selected game with Info and Play. Landscape keeps the
handheld shelf as tall as the page (spines and the selected front) beside a panel with the
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

## Plays with (per-system player choice)

Decision of 11.11.2026: our own player stays the default for every system, including N64, PSP, DS, Saturn and
Dreamcast ("Experimental" for N64, PSP, Saturn and Dreamcast, `play_cards::experimental`; DS left the list on 12.11.2026). Settings → **Plays with** (`android/src/plays_with_page.rs`,
`Screen::PlaysWith`, a sub-page like Save backup) lists the library's own-player systems that have at least one installed
app in `EmulatorIntents.apps(profile)`, and A steps own player → app → ... → own player. `Preferences::plays_with` maps the
system id to the app's package (absent = ours). The host asks the phone (`PocketActivity.installedApps`, a JSON of
profile → apps with the phone's names) at start, after a scan and on every resume (`App::read_apps`); `Ui::plays_in_app`
answers only for an app that is installed, so a removed app means our player, and the choice returns with the app.
`lib.rs::player_core` is None for such a game: it takes the external path (`launchGame(..., preferred)`, the same
play-time, shelf, start picture and way back; `EmulatorIntents.create` tries the chosen package first), it has no
Continue row, and the case's Controls page and the "Experimental" word (about our player) are not shown for it.
Tests: `plays_with_*`, `the_choice_is_saved_*`, `experimental_belongs_to_our_player_*` in `src/android_02_tests.rs`.
**Hook for later:** `Ui::plays_suggestion(system)` answers nothing today. With testers' numbers a weak device could
suggest the app there (heavy systems only, never automatic); nothing is chosen for anyone yet.

## Emulator recipes

`android/java/xyz/pocketui/app/EmulatorIntents.java` owns explicit components, checks their presence, forwards read grants
with ClipData, and starts on the Java UI thread. Missing apps show an error; an Activity opening does not prove a core
or game loaded. Default variant order is explicit; editable user profiles and core overrides are still needed.

| Profile | Package / Activity | Action and ROM contract | Validation |
|---|---|---|---|
| RetroArch | `com.retroarch.aarch64`, `com.retroarch`, `com.retroarch.ra32` / `com.retroarch.browser.retroactivity.RetroActivityFuture` | MAIN; `ROM=saf://<escaped tree>/<relative name>`, `LIBRETRO=<installed app dataDir>/cores/<core>_libretro_android.so`; prefix read grant for the selected tree | Emulator test described in status report; package/core variant must be checked separately |
| ARMSX2 (first for PS2) | `com.armsx2/.Main` | VIEW; document URI as data. 2.9.3 boots it at once (`handleExternalLaunchIntent`), Close returns to pocket-ui (`ui.exitToLauncherExternal`, on by default) | Odin 2 Mini, 10.11.2026, ARMSX2 2.9.3 (GitHub release): straight into the game (a PS2 disc's intro), its menu's Close returned to pocket-ui |
| NetherSX2 / AetherSX2 | `xyz.aethersx2.android/.EmulationActivity` | MAIN; `bootPath=<document URI>` | Recipe only (not installed: ARMSX2 is tried first) |
| Dolphin official | `org.dolphinemu.dolphinemu/.ui.main.MainActivity` | MAIN; the ClipData's URI (the read grant), `AutoStartFile=<document URI>` for old builds. 2603a's `StartupHandler` boots ClipData before data before extras | Odin 2 Mini, 10.11.2026, Dolphin 2603a (F-Droid): straight into the game, Exit Emulation returned to pocket-ui |
| PPSSPP / Gold | `org.ppsspp.ppsspp` or `org.ppsspp.ppssppgold` / `org.ppsspp.ppsspp.PpssppActivity` | VIEW; document URI as data, octet-stream | Recipe only |
| DuckStation | `com.github.stenzek.duckstation/.EmulationActivity` | MAIN; `bootPath=<document URI>` | Recipe only; legacy path-only builds excluded |
| Azahar | `org.azahar_emu.azahar/org.citra.citra_emu.activities.EmulationActivity` | VIEW; document URI as data, octet-stream (its intent filter) | Odin 2 Mini, 10.11.2026, Azahar 2126.2 vanilla (GitHub release): a `.3dsx` homebrew straight into the game; the way back through its menu not yet checked |
| Lime3DS / migrated Azahar | `io.github.lime3ds.android` / `org.citra.citra_emu.activities.EmulationActivity` or `io.github.lime3ds.android.activities.EmulationActivity` | VIEW; document URI as data | Recipe only, probes both components |
| hakuX (Xbox) | `com.rfandango.haku_x/.LauncherActivity` | VIEW; document URI as data. 1.3.0 starts the core at once when a games folder is chosen in hakuX, else shows its setup; the game alone in its task, so closing it is back here | Odin 2 Mini, 10.11.2026, hakuX 1.3.0 (GitHub release): straight into the game, its menu's Exit returned to pocket-ui |

Recipes were inspected against the launcher's published
[PS2](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/SonyPlayStation2.json),
[Wii](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/NintendoWii.json),
[PSP](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/SonyPSP.json),
[PS1](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/SonyPlayStation.json),
[3DS](https://github.com/TapiocaFox/Daijishou/blob/main/platforms/Nintendo3DS.json) definitions on 29 October 2026.
A game's start sets no Continue row: Continue holds only the in-app player's open moments. The time away counts as played
when pocket-ui is back (`Ui::game_returned`). Without the app, Play shows `document_check::app_missing` (which app, where
to get it, the one step in it) in a dialog and changes nothing; `<queries>` must list every package here, or Android hides
an installed app (`every_emulator_app_a_start_hands_to_is_one_android_lets_pocket_ui_see`).
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

The merge gate compiles both Android crates on every merge (`scripts/check-platforms.sh`, [dev/merge-gate.md](dev/merge-gate.md#merge-gate)),
so main stays buildable for Android. See [android/README.md](../android/README.md). Builds use scripts on our machines only, never GitHub Actions.
The dedicated release keystore is outside every checkout, in `~/.local/share/pocket-ui/android-signing/release.jks`;
its password is a separate private file. Neither is committed. `build-apk.sh` stops with a message when the key is missing
(`ANDROID_ALLOW_NEW_KEY=1` makes a throwaway key on purpose; QA packages use their own `ANDROID_SIGNING_DIR`). The first debug-signed preview must be uninstalled
once before installing this identity. Further APKs with this key update normally. Current Android version is
`0.1.8-android.3`, versionCode 10803 (the build script derives both from Cargo.toml); the source version is 0.2.0.

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

### Public preview release

A public Android preview is a GitHub pre-release with fixed asset names, so that Obtainium (and IzzyOnDroid, which reads the same
release) can follow it. Nothing here publishes anything: the release itself needs the owner's go.

1. Export the public tree (`scripts/export-public.sh`) and copy it to the Apple Silicon build host. The release APK is built from that
   tree, never from the private one: the private tree's artifacts carry private strings.
2. `scripts/android/release-apk.sh --version X.Y.Z --out DIR [--build N]` there. It refuses to run without the release key (a missing key
   would be created by `build-apk.sh`, and an APK signed by another key is another app to Android), builds all cores with
   `ANDROID_RELEASE_SOURCES=1`, and writes `pocket-ui-android.apk`, `.apk.sha256`, `pocket-ui-android-core-sources.tar` (the exact
   corresponding-source archive of every core, with `manifest.json`) and `pocket-ui-android-licences.txt`. The asset names do not change from
   release to release. versionName is `X.Y.Z-android.N`, versionCode `X*1000000 + Y*10000 + Z*100 + N`; a second build of the same version
   needs `--build N+1`. The manifest in the private tree is bumped to what was released in the commit after the release.
3. `scripts/android/release-check.sh DIR/pocket-ui-android.apk --sources <tree>/android/build/cores/corresponding-source --previous-code <last published versionCode>`
   from the private tree on the same host. It checks the signature (the signer's certificate must be the release key, fingerprint in
   `android/release-cert.sha256`), package and version, minSdk 26 and arm64-v8a only, the size (30 MB, the IzzyOnDroid line), the 16 KB
   page alignment of every library, that `assets/cores.json` matches the packed libraries and names a source archive for each core,
   the store listing, and the export's leak scan over the APK's contents. Its parts that need no Android SDK are tested by
   `scripts/android/test-release-check.py` (part of the shell lane of the gate).
4. 16 KB pages: Android 15 devices may use 16 KB memory pages and Google Play requires aligned libraries. `build-cores.py` links every core
   with `-Wl,-z,max-page-size=16384` and asserts it on the finished library; `build-apk.sh` does the same for our two libraries and aligns
   stored libraries with `zipalign -P 16` (ours are deflated: `extractNativeLibs` is true, so the ELF alignment is what counts).
   Existing Make-based caches are upgraded by removing a final library whose LOAD alignment is too small, then relinking it with the
   current flags; object files are retained. CMake/Ninja tracks changed linker commands itself. A shared `android/build/cores` symlink
   is resolved before configuring CMake, so exported trees can reuse the same cache without changing its recorded source directory.
5. The store listing is `fastlane/metadata/android/<locale>/` (title, short and full description, `changelogs/<versionCode>.txt`, and for
   en-US real screenshots taken on the AYN Odin 2 Mini). Only real screenshots, no generated pictures and no commercial cover art. A new
   versionCode needs its own changelog file; the release check fails without it.

### Experimental N64 and PSP (DS qualified on 12.11.2026)

These bundled systems (N64, PSP, Saturn, Dreamcast) remain experimental until real games on an Android handheld pass full-speed play,
zero audio underruns, input, pause, save/load, Continue and exit. DS needed both screens and stylus mapping on top and
passed on 12.11.2026 (three games, 180 s each, 0/0/0 underruns, Continue with sound in a new process: Mario Kart DS,
New Super Mario Bros., Contra 4; the coordinator's ANDROID-BIG-SYSTEMS.md holds the matrix), so it carries no
"Experimental" word any more.
`play_cards::experimental` owns this Android-only qualification status. The phone's system line (including Continue),
list rows, game card and Controls page expose it, translated at paint time. Host regression
`unverified_android_systems_are_labelled_before_play_and_continue` covers all six looks in both orientations.
The original Homebrew smoke below is evidence of a start only, not release qualification.
Both player audio opens choose `audio::needs_jitter_reserve(system)`: N64/PSP/DS use the existing 48 ms
heavy-core reserve for frame/render jitter; other consoles retain their previous target. The callback handoff
and endpoint stay unchanged. `audio::android_large_core_keeps_sound_through_a_late_video_frame` covers
missing PCM during a delayed frame and preservation of its catch-up deliveries.

### Odin commercial-game checks (2026-11-10)

N64 and PSP retain the experimental status (DS was qualified on 12.11.2026, see above). In the separate, disposable QA package on an Odin 2 Mini:

- Super Mario 64 and Mario Kart 64: 180 s each of courtyard/race input, full 60 core calls/s, zero new
  gameplay underruns after selecting the existing 48 ms reserve. Early startup counters still ended at 94/49;
  those are unresolved and are not counted away as a clean start. Manual save, load, Continue and exit passed.
- Mario Kart DS: 360 s of race input after the Earlier worker fix, 59.825 core calls/s (nominal 59.826),
  zero app/callback/mixer/AAudio underruns and zero late presentations. A real 19,498,039-byte capture at 300 s
  cost 13.455 ms on the core thread (size query, buffer and serialize); the worker took 99.586 ms. Both screens
  remain visible; held and short stylus map taps were checked. Save/load/Continue/exit passed.
- New Super Mario Bros.: 180 s in World 1-1, 59.826 core calls/s, zero underruns, save/load/Continue/exit passed.
- PSP remains experimental: Jeanne d’Arc CHD now reaches its decoded video and first rendered 3D/dialog scene;
  180 s measured 59.943 core calls/s with 16 new app underruns (50 total). Manual save/load/exit passed, Continue
  remained refused by the existing state safety guard. Bomberman's system dialog is now readable; an attract scene
  is not counted as verified player input. Before these fixes it had unlabelled dialogs and a black picture.
  That test APK bundled the pinned PPSSPP system assets (the current optional package carries them instead). Before loading a PSP core, the Activity
  installs missing files into private `files/bios/PPSSPP` through temporary files and rename, preserving
  existing files and completing interrupted installations. Older diagnostic runs that manually seeded
  assets remain separate from qualification of this corrected stock APK.
  The pinned Android core now enables FFmpeg, matching the enabled video decoder in the Linux core;
  the static Android ARM64 libraries and their sources come from PPSSPP’s pinned FFmpeg submodule.

The runs stayed muted and therefore do not establish heard sound quality. Direct Activity launches exclude
SAF/import qualification. These are short scene checks, not playthroughs or a general compatibility claim.

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

A game's start is the handhelds' (`src/insert_scene.rs`, shared with `app_cabinet`; `android/src/insert.rs`): Cozy + Playful plays the console's rig, every other system the plain animation, from where the case stood (no case on the page, as in Split or Search: no animation). The console stands as on the handhelds (`insert_scene::phone_stage`): as large against the screen as on the handhelds' 1240 x 1080 (the Odin 2 Mini's 1920 x 1080 gets the handhelds' own 2 screen pixels per art pixel), on the handhelds' floor low on the screen, under the case, so the medium comes down into it. `PlayerActivity` is held back until a player's start (`insert::PLAYER_START`, 250 ms) before the fade has run (1.6 s), its process warmed meanwhile (`PlayerWarm`), and comes up on black; a tap or any button skips; Quick, Motion Reduced/Off and frozen games don't hold (`cabinet::starts_cozy`). `scripts/android/insert-smoke.py`, `insert-sync.py` (sound against picture, up to AAudio's presentation time and the frame handed to the screen).

The way back after Save & quit is the handhelds' too (`src/eject.rs`, `android/src/eject.rs`): the player writes `last-picture.json` beside the saves (the game's key, the auto state's preview PNG, the game's rectangle in its window, the time); the launcher reads it once when it is back (older than 10 s: ignored), and when the page shows the game's case (Cozy, Motion Full, Shelf or Cabinet look, the game just played selected) the picture switches off where it stood (an old TV's line and dot with Old-TV switch-on, a handheld's display going dark), black lifts off the page and the start's scene runs backwards to the case's place; the game stands open from the first frame back (`Ui::back_from`) and comes a little out of its case at the end (`Ui::ajar`). The page under it is the live shelf: any button or touch ends it and does what it does there (a held button's repeats do neither). `log`: `quit: saved=… at`, `back: resumed at`, `back: first frame at … eject=` (the quit's time, monotonic ms). `src/android_eject_tests.rs`.

Remaining parity work: the This-game page (its rows are Linux's `options::GameSettings`, `emu_menu` and `standalone` files; on a phone the pause menu has the picture and the options) and Linux's screen frames. The TV uses a downloaded game screenshot, falling back to cover or console art; the
resume-picture carousel is still to connect. Sharing the remaining Home controller
requires replacing its Linux services with Android library/session adapters, rather than compiling nmcli,
standalone guest control or hardware settings into the APK. Real-hardware audio/core compatibility is a separate validation pass.


### First start and library policy (0.1.5-android.2)

First installation opens eight native pocket-ui steps: Welcome, Language, ROM folders, Look,
Covers and pictures, More systems?, Controls, Ready. Each step offers Continue and, before Ready, Skip; Back
returns to the preceding step. Progress and choices are saved in `launcher.json` after each action,
so force-stop or returning from the Android folder picker resumes the same step. Settings → First
start reopens the guide. Skipping does not require a ROM folder or an Internet connection.

Language is English/German/Spanish. Look offers cached visual previews of Shelf, Easy, Split and Cabinet, Cozy/Quick
animations and dark/light appearance. Controls explain touch and controller navigation; the app
asks Android whether a nonvirtual controller is connected. This uses the Android navigation
controller and shared Pocket painters; it does not provide every Linux Home/settings feature.

The real `ACTION_OPEN_DOCUMENT_TREE` picker takes and persists read permission. Each new selection
adds a root without revoking older grants; cancellation retains the library. A worker scans all saved
roots, retaining readable roots if another provider fails. Provider document identities remain the
save/selection keys; additional roots receive stable virtual path prefixes to isolate matching names.
Empty selections explain that the player can choose the parent of the system folders or a system folder itself,
and allow another selection or Skip. Settings can add
more folders later. Full disc/BIOS staging and removing individual roots remain follow-up work.

Covers and pictures are an explicit saved choice, also editable in Settings. With download enabled,
the browsing worker requests the selected game's cover, screenshot and known English Wikipedia
summary through the existing shared cover/game-picture/wiki sources. It rechecks Android network
capabilities before download: Wi-Fi, Internet capability and not metered are required. Cached media
remains usable offline; this is browse-time loading, not a full-library background downloader or
translation of Wikipedia articles. Choosing Not now or skipping the choice leaves downloads off.

`scripts/android/onboarding-smoke.py` targets only emulator-5582 after verifying `ro.kernel.qemu=1`.
It clears the app's data, preserves ROM fixtures, checks resume at steps 2/3, empty and populated SAF
roots, language/navigation and completion persistence, and captures all eight steps in English
portrait and German landscape. `--all-combinations` adds the other language/orientation pairs.
It derives every tap from a fresh UIAutomator tree. Run this destructive fresh-install check only
after player/core checks; it removes the emulator app's private states and preferences.

The older launcher/player smoke scripts assume setup has been completed (or skipped) first. The dedicated onboarding smoke clears data and must therefore run afterwards.


### Android player picture policy
`android/src/player_video.rs` retains the picture delivered by the initial timing/geometry run.
A newer picture after loading a saved moment takes precedence. NULL duplicate frames and runs
without a video callback keep the last picture, including that initial delivery. The shared
libretro conversion handles RGB565, 0RGB1555, XRGB8888 and padded source rows before
`player_graphics.rs` uploads native-size pixels and composites the cached touch overlay.

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


## Frozen games (0.1.8)

The rules are Linux's (`src/frozen.rs`); `src/frozen_phone.rs` is where they meet Android's files, and its tests run on Linux in
the gate. Settings has the row (`Setting::Extra(FrozenGames)`, on by default). Differences from Linux, all from the files:

- Identity: a game's states are `states/<game_key>/<stem>` and the core plays the direct document alias or the copy fallback
  (`content/<game_key>/<name>`). The mark lives beside the state (`<stem>.pstate.auto.z.mark`, written by the player before the state, from
  this session's file), not in a recents list; "recent" is `launcher.json` `played`.
- Which stand open: `App::frozen_refresh` on a worker, at each return from a game and on a changed setting or library. Only games
  `player_core` would start (bundled cores; never `launchGame`'s external apps, nor with the test's `externalPlayer`). The shelf
  can compare the document's size (`Document::size`) and the copy; a same-size change of the document is found at the start.
- The start: `launchInternal` checks direct access or prepares the copy fallback, then the player's `frozen_phone::at_start` judges that content
  before the core sees a state. This holds with the switch off too. Cartridges up to 64 MiB are fingerprinted whole on a phone
  (Linux samples them), so a patch in the middle counts as another file. The launcher hands `frozen_shown` in the settings JSON.
  A frozen start has no old-TV switch-on (`cabinet::starts_cozy`).
- A moment that isn't loaded (another file or core, refused, older than the in-game save) goes to the first free slot, or to
  `.pstate.auto.z.kept<n>` when all eight are taken: the next Quit would remove it where it lies (`slots::auto_written`). The pause
  menu's slot picker goes on after slot 8 through the kept moments (`frozen_phone::kept`), which Load loads and Save refuses, and a
  start moves them into a slot that has come free. A note over the game's first 8 seconds says so (`Player::note`, also a semantic node).
- `Player::automatic` writes the battery save before the state, as pocket-core's exit does.
- Home while a start is on its way, a copy too (`App::suspended`), moves `launchTurn` on (`PocketActivity.cancelLaunch`): that worker
  stops at its next block, starts nothing and says nothing, and each worker has its own `.part`, so the same game can start again
  at once. The shelf comes back after 20 s only when no copy runs any more (`launchOnItsWay`): a slow copy keeps its words on screen.
- A document that is gone or unreadable: `launchInternal` deletes its partial copy and says so plainly; nothing else is touched.
- A kill while the game is on screen leaves the taken state: the shelf still shows it open and the next start goes back there with
  a note (`Resume::ResumedAgain`). `onPause` (Home, app switch) writes a new state first, so only a kill without it does this.
- A state from before the marks has no mark and goes by its state as the normal start always did.

Test: `scripts/android/frozen-smoke.py` (phases flow, five, safety, strip, measure, lost; emulator only). Times: [MEASURED.md](MEASURED.md#frozen-games-on-android-emulator).
## Game check (parity with Linux's Settings → Games → Game check)

Settings → Game check (`android/src/check_page.rs` over the shared `game_check_page`) runs `src/document_check.rs` on a worker.
`game_check.rs` keeps the shared logic; its seam is `Files` (size of a file, a zip's refusal, how a place is shown) and `Words` (the
sentences that name where games are copied to): `Disk` on Linux, a provider's listing on Android, which gets it from
`PocketActivity.checkListing` (the scan of every chosen folder, or per folder why not: `lost` when the persisted grant is gone or a
`SecurityException` comes, `gone` for any other failure, `big` past 50,000 entries). A zip's directory is read from a detached
descriptor (`openDocument`, `archive::refusal_in`), a cloud provider's pipe that can't seek is not judged. `document_check.rs`
compiles on Linux too, so `cargo test` runs it on real listings with typical faults.

What is true on Android, and so what it says:

- **BIOS**: no check, only tips. The in-app player's system directory is the private `files/bios`. PlayStation's BIOS files
  among the chosen folders fill it ([PlayStation](#playstation)); none of the cartridge cores needs one. An emulator app's BIOS folder
  can't be read. Tips for what can't start without one in its emulator app (`needs_own_files`: PS2, Sega CD), for
  PlayStation in this app's player when no BIOS is among the folders, and for other files in a `bios` folder, which nothing reads.
  `scan` doesn't list files in such a folder as games (`syscard3.pce` looks like one), nor a PlayStation BIOS anywhere.
- **What "can't play" means**: `Host::bundled` is the APK's core list, otherwise the launch profile's app must be there
  (`EmulatorIntents.available`, the same components `create` starts; `<queries>` in the manifest makes them visible). A game an
  installed app takes is no fault; a missing app is one line per system with its number of games. Nothing can be said about a RetroArch
  core from outside: a tip. `Group::NotHere`: over `document_check::MAX_IN_MEMORY` on any system but PSP (`GameCopy.LARGEST_IN_MEMORY`; change them together), a big game without its copy when the phone hasn't the room for a first start (`Host::room_for_copy`),
  PC Engine CD (one file is staged, and no way to its BIOS), PlayStation Vita (no launch recipe).
- **File types**: `document_library::takes` per system, because a folder's system wins on the shelf: `.gba` in `snes` is listed and bsnes
  can't open it. A cartridge type in a folder that names no system is fine, a disc or zip there isn't on the shelf: one line per folder.
- **Sheets**: a cue in a playlist isn't on the shelf, so `sheet_files` looks at every sheet, not only the games'. Sheets are read as UTF-8,
  else Latin-1, so an old cue's umlauts no longer read as a missing track (`sheetText`).
- **After the check**, the shelf is brought up to date when the folders changed (no folder that can't be read), as X on Linux reads again.
- **Adding games**: a folder chosen from Settings with new games offers the check (`App` when `scan_picked`), the first start's end says
  where it is (a toast), and a folder chosen on the page itself looks again at once.

`scripts/android/game-check-smoke.py` drives it on the emulator from a fresh install through the real picker, per language and orientation,
with a real lost grant (`urigrants.xml` removed with root, zygote restarted) and 5000 files (`--phase big`). A phone has no hiding yet, so
`Fix::Hide` is not offered (`Words::twin_hides`).

The 0.4 cleanup removes the idle slideshow and its screen-on override on Linux and Android, plus the pause-menu memory-card switch. The info page and case back keep their memory card. Old optional keys are ignored.

## 0.1.6-android.8: the rest of the Cozy extras (2026-10-30/31, from main)

Printed discs, game connections, Rediscover, light by time of day and decorations work on a phone; every Cozy extra has its row (list above, tested by `scripts/android/extras-smoke.py`). The
launcher keeps two new things in `launcher.json` for them: `played` (when each game was last started) and `play_secs` (time
played per game). Shared code moved without a behaviour change on the handheld: `civil_date` out of `play_history`,
`room_props` out of `app_room_extras`; the
shared info page and the box's back note where their linked facts are painted (`link_rects`, `link_rect`) for taps.

## 0.1.6-android.7: main refresh (2026-10-30, main 628c349)

Main merged into the Android line (`feat/android-main-refresh`). Shared painters now bring the real cases, memory-card and
cartridge media, stickers, the Cabinet TV, light/dark sets, the new logo and the case sounds. New shared modules in
`android_modules.rs`: `font_style`, `room_deco` (the painters call it), `button_prompts`, `cozy_extras`, `playful`.
(`room_light`, `room_props`, `connections` and `civil_date` followed in 0.1.6-android.8.)
Settings (flat sheet, `ui.rs` `SETTINGS`): Look, Appearance, **Colours**, **Font**, Text size, **Cozy extras** (own sheet),
Language, Animations, Playful cases, Old-TV switch-on, **Motion**, UI sounds, Case sounds,
**Button symbols**, Frozen games, Game Boy core, **Plays with** (0.4), ROM folder, Game check, Save backup (0.3.2), Covers and pictures, Updates, **Licences**, First start, **Help**, Report a problem. The cozy extras you hear or that move, and
the slideshow, are rows of their own as on Linux (Sound & feel, Sleep & power; `cozy_extras::Extra::home`); the sheet holds
the eight of Linux's Cozy extras page in its order (`Extra::PAGE_ROWS`). These look settings are kept in `launcher.json` (`Preferences`, a wrong value costs that value only), handed to
the shared painters' process-wide switches by `Preferences::apply_look` (every paint, at start, and in the player from
the launch's `playerSettings`).
Cozy extras on a phone (as rows of Settings or of the sheet, above): Case sounds, Old-TV switch-on (player), TV on the cabinet (landscape Cabinet panel), Stickers on
cases ([Case stickers](#case-stickers)), Printed discs (the disc or UMD in an open box, `ui.rs` `paint_box`; off: a plain disc in
the system's colour), Game connections (a fact on the card that other games share, Developer/Publisher/Series/Genre from the
embedded offline list, gets an arrow and is a tap target; its list is `Screen::Connection`, B returns to the card; controller:
Up/Down pick a fact, A opens; `ui.rs` `fill_links`/`open_link`, hit rectangles from `InfoView::link_rects` and
`BoxView::link_rect`), Rediscover (a Curator row: games started a year ago today or not for a year, from the start times
the launcher keeps in `launcher.json` `played`; a phone has no older history, so it says "None yet" for the first year),
Light by time of day (the Shelf and Cabinet looks' boards, compartments and cases take `room_light::at(minute_now())`: cool
morning, warm evening, dim night; half as strong on the Light page; read every 20 s, so it follows the next paint, `Ui::room_sync`),
Decorations (one more shelf at the end of the portrait list, and after the last case in landscape, with the plant at its stage
and a figurine per system played ten hours: `room_deco::earned` over the play time the launcher keeps per game in
`launcher.json` `play_secs`, counted from the launch to the launcher coming back, at most a day a launch; the trophy and the
framed "100 %" from the Completed and 100 % stickers; shared `room_props.rs`),
Playful animations. Every Cozy extra has a row on a phone. The Linux rows for LEDs, fan, sleep & power, Tailscale,
web companion, Bluetooth, battery and Store have no Android counterpart and no row. Appearance on Android stays Dark/Light.

## Android refresh for 0.1.14 (branch build)

The shared library is based on main 0.1.14. Settings and the first-start Look step now
include Easy: one large case, its name and system, previous/next buttons, and direct
Play or Continue. Controller arrows change games; X or tapping the name opens Info.
The regular Games page adds Just Play; Quick settings offers it too. It uses the
shared Curator policy with current stars, completion/playing stickers and session
history, then the ordinary launch path. Automatic picks are limited to bundled
cores and nonempty games without known missing disc files; Android still checks the
SAF document at launch. External emulator apps remain explicit choices.

The pause sheet calls its existing save-and-exit action **Save & quit**, for touch,
controller and accessibility. Failed saves leave the sheet open. It still saves the
battery data and automatic moment; this adds no quit-without-saving action.

`src/state_policy.rs` shares the raw-state size exceptions between both players:
mGBA and Nestopia only. The pinned PPSSPP reader ignores the supplied size, so its
old Android exception is removed. Shorter PSP states may consequently fail to resume;
they go through the existing kept-moment recovery instead of bypassing the minimum.
This is not a repair of the pinned PPSSPP parser or writer. Flycast is not bundled:
Dreamcast uses an external emulator and has no internal APK state-size exception.
Main's shared save retry is inherited; no new state format or identity is introduced.

Android retains Sharp, Smooth, CRT and Scanlines, its own GLES shader and procedural
Console/CRT frames. The newer Linux beam/LCD/glow/colour effects and full frame packs
are not ported by this refresh. The upcoming Look controls require a separate refresh.
No device measurements or full Linux feature parity are implied by an APK build.

The Games shelf now calls its first row Continue and includes only games whose open
moment was verified by the Android worker. Recently played external games without
an internal moment remain on their system shelf. Easy keeps its single-game order.

Automatic states from 0.2.1 use the shared `state_file` codec (`.pstate.auto.z`); raw older Auto states remain readable.
The Android player uses it for both automatic writes and all state reads. Manual slots/undo/Earlier stay raw. Moving a
compressed moment into a manual slot decodes it first. Downgrading starts from the in-game save and cannot overwrite the
compressed moment; Linux uses the identical format (HOST.md, compressed automatic states).

## Android 0.2

The [Android preview guide](../android/README.md#android-02-preview) describes the new
Controls, Zoom, Hold to toggle, font scaling, resume reminder and Android lifecycle behavior.
`android/src/play_cards.rs` holds the phone's control the continuous frozen-frame
crop and shared card presentation; both native libraries use it. The launcher passes old
history facts before recording this launch. The player shows the reminder only after
`Core::resume` actually succeeds and a restored frame exists; it runs no further ticks or
audio while the card is being read. Android has no editable per-game notes yet.

`Preferences.haptics` defaults false; `remind_where: Option<bool>` defaults to enabled for
older preferences. Per-game toggle selections live under private `android-controls/<key>.json`;
core descriptor snapshots use `android-buttons/<key>.json`, loaded once on explicit Controls open.
They use the game/core identity, so one game's action names cannot describe another game.
These settings change no SAF permission, game-file location or manual slot format.
`type_scale::set_system_scale` applies Android's configured factor separately from the
shared three text sizes, on the drawing thread. The player receives it in `nativeFrame`,
so a font change does not require JNI work from Rust on every game tick. `fontScale` is a
handled configuration change in both Activities. New visible strings include DE/ES.

The player observes InputManager device events, releases digital/analog/touch/latching
inputs and pauses on a controller change/removal. An unrelated keyboard/mouse change
leaves the game alone. GLSurfaceView's pause path queues the save on its GL thread before
parking the surface; duplicate audio-focus/background notifications share one suspend
save per foreground period. The queued-event-before-pause ordering is visible in
[Android 15's GLThread](https://github.com/aosp-mirror/platform_frameworks_base/blob/android-15.0.0_r1/opengl/java/android/opengl/GLSurfaceView.java).
Resume keeps the pause sheet open. See the Android framework's
[GLSurfaceView lifecycle](https://developer.android.com/reference/android/opengl/GLSurfaceView)
and [edge-to-edge guidance](https://developer.android.com/develop/ui/views/layout/edge-to-edge).
SAF remains the only access to external game files; app-private copies, saves and settings
remain in the sandbox. The existing external-emulator intents retain their own lifecycle.


## PlayStation

PCSX ReARMed (`pcsx_rearmed` in `cores.lock.json`, GPL-2.0-or-later, the core `hostsys` routes PS1 to on ROCKNIX and Brick) is
built with the NDK's clang through its `Makefile.libretro`: ARM64 dynarec (`ari64`), NEON GPU, CHD, no physical CD-ROM, and no
`-lpthread` (bionic has it in libc). About 2 MB. It opens the game's file itself, so its discs follow [big games](#big-games),
several files included.

- **Files**: `.cue`/`.bin`, `.chd`, `.pbp` (in a PlayStation folder; elsewhere `.pbp` is PSP's), `.iso`, `.m3u`.
- **BIOS**: `document_library::ps1_bios` takes the PlayStation BIOS files among the chosen folders by name
  (`is_ps1_bios_name`: `scph…` and `psxonpsp660`, any letters) and size (512 KiB), anywhere in them; the launch passes them to
  `PocketActivity.copyBios`, which writes them in lower case to `files/bios`, the core's system folder, before the copy. The
  scan's list is kept on `ui::Library::bios`, the Game check's listing refreshes it. Without one the core uses its HLE BIOS; the
  first such start in a run of the app says `document_library::PS1_BIOS_MISSING` as a toast (`bios_hint`), and the Game check
  has the same as a tip. No BIOS is downloaded or bundled.
- **Several discs**: an `.m3u` starts with disc 1. Options has a **Disc** row ("Disc 2 of 2", in the Controls section) for a
  game the core reports more than one disc for (`Core::disc`); a change is `Core::set_disc` (tray open, disc, tray closed).
  It is the last of `display_options`, so the rows above keep their indices.
- **Evidence**: `document_library` tests (sets, playlists, capitals, BIOS by name and size, the missing-BIOS sentence in every
  language), `document_check` (bundled PS1, the set's room, the tip), `frozen` (the set's stage). Real-device numbers:
  [MEASURED.md](MEASURED.md).

## Android 0.3 internal refresh

See [Android 0.3 QA behavior](../android/README.md#android-03-internal-qa). `Preferences.retro_colours` is passed to
both native processes. Phone title bands and result rows use `title_block`; `layouts::system_ids` and
`layouts::listed_picture` serve the SAF scan as well as the card conventions. `document_art::Index` resolves artwork
and exact-stem cheat sidecars from bounded documents; `gamelist::parse` supplies tolerant XML parsing without its
filesystem scanner. `PocketActivity.readPicture` decodes only a granted document on the existing media worker.
`CheatMenu::from_text`/`try_press` provide per-identity persisted choices without a command FIFO. The player calls
the shared core cheat API directly. `android/src/moments.rs` checks the undo/rollback transaction; Android's
`Core::block_saves` also protects the destructor after a failed rollback. The save guard is Android-only; the shared bounded-code handling also protects Nestopia on Linux.

The internal APK version can be overridden by `ANDROID_QA_VERSION` (strict numeric major.minor.patch); normal builds
still derive it from Cargo. Android Updates uses a separate signed APK channel, sharing Linux’s manifest verification and page actions; Android owns installation through PackageInstaller sessions.

Android phone rows, selected bands and box headers use explicit literal name scopes (`draw_name`/`title_block::name_layout_in`), including their accessibility labels. The pause header is `pause_head` (shared `src/pause_head.rs`): the whole name in up to three literal lines (`title_block::name_layout`) and the status line wrapped under it; the type goes a step smaller, name first, only while the buttons would lose their 48 dp height, and the rows start under the head. Options remains interface text. DE/ES framebuffer regressions compare real name regions against literal reference painting in both orientations at 100%/200%; `android_pause_header_shows_the_whole_name_and_status_and_leaves_the_rows_their_room` lays the header out at 360x800 dp portrait and landscape, 100 %/200 %, Barlow/Retro, EN/DE/ES, and fails on a cut name or status, a line wider than the sheet, or a head that takes the buttons' room.

Phone box-header title baselines and the system-line gap use `type_scale::px`, so Android font scaling cannot make their ink overlap. The regression inspects actual painted title/system ink in portrait and landscape.

### Comparing AAudio routes without making sound

`scripts/android/aaudio-latency.c` is a standalone, silent Android API 28+ diagnostic, outside the APK.
Build it with the NDK clang for the device ABI, linking `-laaudio -ldl`. It requests the player's
48 kHz stereo I16 output, 256-frame callback and three-burst buffer, always writes zeroes and never
changes volume or app data. Arguments are `USAGE CONTENT PERFORMANCE SHARING [SESSION]` using the
AAudio enum values. For example, `1 2 12 0` requests Media/Music/LowLatency/Exclusive; `14 4 12 0`
requests Game/Sonification, and a final `0` allocates a session. Each run takes about twelve seconds.
An allocated session makes Android's legacy AAudio request FAST without RAW; it can also prevent
MMAP, so it is a diagnostic comparison rather than a general recommendation. Check the actual
performance mode, sharing mode, device, session and MMAP result in its header, and collect
`dumpsys media.audio_flinger` during the run to identify the active track, flags and effects.

Save stdout as CSV and run `python3 scripts/android/analyze-aaudio-latency.py CSV [CSV ...]`.
The analyser skips the first two seconds, extrapolates each monotonic presentation timestamp to
query time and compares against AAudio's actual written counter, including callback silence.
It reports the written/read gap separately: this is client-buffer occupancy, not physical output
latency. The counter sampling interval can span a callback; its maximum frame uncertainty is
reported too. A stable timestamp delay establishes the driver's reported backlog, not whether
the speaker really has that delay. Use a physical loopback or an explicit listening test for that.

`scripts/android/aaudio-acoustic-ab.c` is the separate physical A/B probe. It records one continuous
48 kHz mono microphone stream while alternating FAST|RAW and an allocated-session FAST stream,
six short known noise clicks per block, four blocks (twelve clicks per path, about 29 seconds).
The output peak is 16384/32768 (about -6 dBFS); only an explicitly authorised operator may raise
media volume to the lowest nonzero step, and must restore and verify zero immediately afterwards.
Build with NDK clang for API 28+ and `-laaudio -lm`; the sole argument is a new output PCM path.
Stdout contains input-read delivery times and click-generation times on CLOCK_MONOTONIC, plus
per-block output timestamp/xrun counters. Matched filtering the known click sequence against the
microphone PCM gives generation-to-input-delivery delay. That includes unknown input latency;
only the difference between the two paths with the same continuous input isolates their relative
output delay. Do not report either absolute input-delivery number as speaker latency.

`python3 scripts/android/analyze-aaudio-acoustic.py LOG.csv MICROPHONE.s16le` (requires numpy)
finds each emitted signature with a normalised matched filter. It accepts a peak only above six
times the surrounding correlation RMS, outside the peak's +/-20 ms region, and requires at least
ten accepted clicks per path. Otherwise it reports `insufficient-signal` and no A/B verdict.
Synthetic checks exercise equal paths, a real 500 ms offset and an undetectable signal:
`python3 scripts/android/test-aaudio-acoustic.py`. Timestamp extrapolation/startup checks live in
`python3 scripts/android/test-aaudio-latency.py`.

On the Odin 2 Mini (Android 13, 2026-11-10), the physical A/B probe recognised all twelve
clicks per route: callback generation to microphone-read delivery was 37.2 ms median
(standard deviation 1.7 ms) for FAST|RAW, versus 54.5 ms (2.2 ms) for FAST without RAW.
The four alternating blocks all had zero AAudio xruns. The RAW driver's concurrent estimate
was 529–531 ms; its AudioFlinger track still reported 525 ms while the clicks arrived at the
microphone in under 41 ms. This establishes an erroneous RAW presentation estimate on that
firmware. The microphone input latency is included in the physical numbers, so they are not
absolute speaker latency. Allocating a session solely to lower the reported value would make
this measured path about 17 ms slower; the product keeps its existing stream policy.

### Scheduled sounds and inconsistent presentation timestamps

`platform/android_audio_timing.rs` chooses the lead for scheduled UI sounds. On a confirmed
local LowLatency output (speaker or analogue wired headset, matched by the stream's device ID,
burst at most 10 ms), a timestamp is rejected if its transport delay exceeds
`max(written_read_us, buffer_us) + max(100 ms, 4 * (buffer_us + burst_us))`.
The fallback is Written−Read plus the software handoff, with the existing buffer floor.
The handoff is added once, after validating the transport estimate. A real counter-confirmed
backlog and valid timestamps survive; Bluetooth, USB, unknown outputs and slower routes retain
their timestamps. `AudioRoute.java` identifies the actual device at stream open, outside the callback;
AAudio disconnects the legacy stream when routing changes, and the existing writer reopens it.
Neither stream flags/session, burst, buffer nor the core's pacing is changed.

`out_ms` is this validated estimate, including the handoff, rather than a physical speaker
measurement. `reported_out_ms` preserves the raw timestamp estimate and `timestamp_fallback`
identifies rejection. The first rejection per stream is logged with the raw and counter values;
diagnostic JSON retains the raw presentation trace and the rejection count.

For an independent animation check, build `scripts/android/aaudio-cue-microphone.c` with NDK
clang API 28+ and `-laaudio`. Run it with a new PCM path and duration in seconds (at most 60).
It only records the built-in/default microphone; it emits no sound and never changes volume.
Save stdout as `microphone.csv`, recorded PCM as `microphone.s16le`, and the diagnostic
launcher's monotonic logcat as `app-log.txt` in one folder. Use the isolated QA package and
`diagnosticAudio=true, insertRehearsal=true` to repeat real SNES insertion animations without
launching a game. Any audible run requires the explicitly authorised low-volume daytime slot;
restore and verify media volume zero immediately afterwards.

`python3 scripts/android/analyze-android-cues.py FOLDER` matches the CartIn waveform's physical
microphone peak against the first `insert-frame` presentation at elapsed 1200 ms. It requires
three sequences with correlation magnitude at least 0.05 and peak/null-RMS ratio at least six;
otherwise it reports no aggregate verdict. The result includes unknown microphone delivery
latency and buffer-present-to-display delay, and is not calibrated glass-to-speaker latency.
The same input setup before/after reveals a gross scheduling advance independently of AAudio's
output timestamp. Run `python3 scripts/android/test-android-cues.py` for synthetic early/on-time,
noise-only and missing-animation checks. Rust regression coverage includes a false 529 ms
timestamp, coarse read counters, real backlogs, large handoffs and unknown/Bluetooth routes.

Odin microphone/frame check (2026-11-10), three real SNES CartIn rehearsals per build:
before validation, microphone delivery minus the 1200 ms frame's presentation was
-494.9 / -306.3 / -482.9 ms (median -482.9 ms, population SD 86.2 ms).
After validation: +18.9 / +21.0 / +22.3 ms (median +21.0 ms, SD 1.4 ms).
Both builds kept the same FAST|RAW stream, 144-frame burst and 1152-frame UI buffer;
all six recorded streams had zero AAudio xruns. The corrected run's AudioFlinger track
still reported 540 ms, showing why that firmware estimate must not schedule the cue.
These numbers include microphone delivery latency and are not calibrated absolute AV offsets.
DS touch retains a down/up pair arriving between core ticks for one tick, so quick taps can reach
the lower screen. A held stylus follows motion; pause, load and cancellation clear pending input.

Android periodic Earlier captures keep `retro_serialize` on the core/EGL thread and log its
cost separately. `platform/android_earlier.rs` moves the owned state and an `Arc` of the immutable
frame to one worker: pixel copying, PNG encoding and durable state/preview writes run there.
At most one capture is active and one newer capture waits; a newer waiting capture replaces the old
one. Quit finishes an active write and discards pending Earlier work (the newer Continue state is
saved separately). A worker failure preserves the previous state file and is logged; there is no synchronous
fallback in play. The Linux player already uses `timeline::Writer` and the serialize cost guard,
which defers expensive captures to a quiet pause.

## Optional core downloads

The APK bundles the small cores through PS1; Saturn is also bundled; PSP, DS, N64 and Dreamcast are optional store packages.
First start has eight steps: **More systems?** follows artwork consent, starts with no selected systems and shows
archive sizes. Downloads start after Continue, on unmetered Wi-Fi; Extras offers them afterwards and a missing
core routes Play there before any start animation, game copy, recents or player process is started.
`android/src/core_downloads.rs` runs one worker; Android's `CoreDownloads`/`ResumableDownload` transport supports
bounded HTTPS, same-policy redirects, Wi-Fi loss, Range continuation and cancellation. The signed catalog and
SHA-256 use `signed_data.rs`, the same functions as Android APK updates. `android_cores.rs` applies the store's
freshness guard, confines each package to `files/android-cores/<core>`, validates its library/source references and
PSP assets, then calls the existing `Store::install_bytes`. The installed core path wins over the bundled path.
Catalog addresses remain unset for the local test milestone. See [package setup](STORE.md#android-core-packages).

## Signed updates in the Android Settings page

**Licences and the download channel (11.11.).** Settings → Licences (`Setting::Licences`, `Screen::Licences`,
`android/src/licences_page.rs`) reads `assets/licences/CORES.tsv`, which the build writes from the core manifest
(`scripts/android/core_licences.py list`): every core including the optional downloads, with licence, upstream commit and
patches. The same script checks before packing (`check`) that each core has licence texts, a full pinned commit, its
source archive and its patches, and that each download package carries its licences and `licences/PATCH--*`;
`build-apk.sh` stops otherwise, `apk_audit.py` repeats the checks on the finished APK. `SOURCE-OFFER.txt`
(`android/SOURCE-OFFER.txt.in`) is the written offer in the APK and, with the patches, in the release assets.
Tests: `scripts/android/test-core-licences.py`, `test-release-check.py`, `android_02_tests`. The system download channel is
`android/core-channel.url` (copied to `assets/core-channel.url`; `ANDROID_CORE_CHANNEL_URL` for one build): empty until it is
public, and then `Ui::core_channel_open` is false, the extra systems read "Coming soon" (first start, Extras, the press on
a game) and `cores_poll` never starts a download. An address ending in `/catalog.json` switches the offers on, without
code changes. Not to be confused with the intent extra `coreCatalogUrl`, which overrides it in tests.

The flat Settings → Updates row opens `android/src/updates_page.rs`: installed/offered versions,
Check again, Install pocket-ui / Install now / Later, automatic checks and download progress.
`src/update_page.rs` supplies the actions and offer ordering to Linux and Android;
`src/update/channel.rs` and `src/signed_data.rs` supply the same signed manifest/trust anchor.
`android/src/updates.rs` reads the signed channel on a worker and polls the Java install worker.
`update-hint.json` keeps the automatic-check choice; its idle/backoff schedule reuses `update_hint`.

`UpdateInstaller.java` binds network connections to Wi-Fi, bounds every transfer, checks signed
size/SHA-256 and APK identity/version/current signers, and writes a PackageInstaller session. It
requests unknown-app permission only when missing. An explicit non-exported receiver handles the
system confirmation and result. Android closes the app when replacing it; the confirmation tells the
user to open it again. The first opening consumes a version-matched cache marker and says “Updated
to …”. No background launch, overlay/alarm permission, file provider or broad storage access is added. Cancellation abandons an uncommitted session and removes update cache only. Games and
save folders are never touched. A committed install uses Android’s own confirmation/cancellation.

The prepared public URL is `/updates/android/update.json`; no deployment is part of this change.
Local QA HTTP is restricted to a debuggable `xyz.pocketui.qa.*` identity with an explicit launch extra.
Release preparation and offline checks are in [UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#android-apk-channel).

### Saturn and Dreamcast internal player

The 17-core bundle adds pinned Flycast (GLES3/AArch64) and YabaSanshiro (GLES3.1/AArch64).
Both remain experimental, including on Continue and Controls. They use the existing native player,
shared disc insertion, pause/menu, slots, private SRAM and retained moments. File-based disc cores
are staged through `GameCopy`, with CUE/GDI/M3U dependencies; Dreamcast CDI is recognised too.

`document_library::player_bios` discovers supported BIOS among selected SAF roots, normalises case
and validates sizes. `BiosFiles.java` checks actual bytes and atomically installs private copies.
Saturn requires a 512 KiB `saturn_bios.bin`, `sega_101.bin` or `mpr-17933.bin`; a missing or unreadable
one yields a clear start error. Dreamcast uses `dc/dc_boot.bin` (2 MiB, `dc_bios.bin` alias) and
`dc/dc_flash.bin` (128 KiB), and explains its fallback when no boot BIOS is available.
The shared heavy-core audio reserve applies on first open and audio-focus recovery.
The shared core loader connects all four Flycast Maple ports before the first frame, including
the controller that carries its VMU. Android library prefixes are removed when selecting curated
core options. Its `VMU A1` default covers the pinned Flycast core's `reicast_*` option names and
legacy `flycast_*` aliases; Linux keeps its existing option profile.
Android enables its frame-rate detection (an explicit user choice still wins for unlocked titles)
and disables Yaba's busy-wait auto-frameskip to avoid a second clock before presentation.
Source timing also updates the audio queue's cadence reserve on start, AV changes and focus recovery.
The GLES host negotiates the core's required ES minor version and creates an isolated EGL context
with that requirement; unsupported drivers fail with a version-specific message.

The application disables native heap pointer tagging for Flycast's fast-memory fault handling.
This is app-wide, including other cores: allocator top-byte consistency checks are relinquished.
It is a compatibility workaround, not a fix for the core's pointer assumptions or future MTE
support. The release check verifies the compiled manifest; hardware checks compare representative
other cores and process memory. Details and the Android platform reference are in
[the Android guide](../android/README.md#saturn-and-dreamcast).
`player audio` diagnostics separately count emulation ticks and new core video frames; compare
those rates with SurfaceFlinger before attributing a visible frame-rate limit.

For game-start QA, distinguish the attract movie from the actual title: both can
show “Press Start”. Sega Rally Championship and Virtua Tennis 2 first return from
attract to their title; another Start enters the menu. Widely spaced attempts can
keep reopening the title after it has timed out, even when the core reads every
press. `scripts/android/title_entry.py --phase attract X Y OUTPUT` prints a bounded
two-press probe with 180 ms holds, a three-second settling interval and before/after
images. Use `--phase title` for an already visible title, and `--pause-after` to stop
Pocket's emulation clock while inspecting the resulting menu. Coordinates must
come from the visible Start node in an existing guarded QA session; the helper
neither opens a device session nor identifies game scenes. Run its regression with
`python3 -m unittest discover -s scripts/android -p test_title_entry.py`.
An input callback or a changed title image is not a gameplay qualification: inspect
the actual race/match and demonstrate movement before starting its timed window.

## RetroAchievements (feat/android-ra)

The same code as pocket-core on Linux: `src/achievements/` (session, offline copy, hashing, snapshot), `src/achievements_page.rs`
(the sign-in page's state), rcheevos (`vendor/rcheevos`) with `src/rc_shim.c`, built into both native libraries
(`build.rs` no longer skips Android). What is Android's own:

- **Network:** `achievements::http::AndroidHttp` asks the Activity (`raRequest`, `raDownload` in `PocketActivity` and
  `PlayerActivity`, both over `RaHttp.java`): HTTPS only (also across redirects), 8 s connect and 25 s read timeouts,
  worker threads only, the POST body (the password at sign-in, else the sign-in key) never logged or written to a file,
  and never followed to a second host. The User-Agent names the client, the platform ("Android"), the core and rcheevos.
- **Account:** `<data>/retroachievements.json` (0600), the token only, as on Linux. Signed in or out in the launcher
  (`android/src/achievements_ui.rs`, Settings → RetroAchievements, the app's keyboard; the login runs on a worker
  `ra-login`); the `:player` process reads the same file. Without it `player_achievements::Achievements::start` returns
  `None`: no thread, no request, no pause-menu row.
- **Player** (`android/src/player_achievements.rs`, hooks in `player.rs`): the session starts after the core and its
  resume; `frame()` after each emulated frame, `idle()` while paused, `reset()` after every state load (also Earlier).
  Hashing, sign-in and loading the set run on the session's threads, so the start is not slower. The unlock cards and
  notes are `achievements::cards` (tested on Linux) painted with `unlock_card` (shared with pocket-core's HUD). The
  pause menu's **Achievements** row is the last one (index 9, the others keep their numbers) and opens `Menu::Achievements`,
  the list as lines (`achievements_page::list_lines`), read from `<data>/achievements/running.json` once a second.
  Softcore only, as on Linux (hardcore would forbid states).
- **Recognising the game:** `achievements::hash_rom` on the path the core was given. A game opened without a copy is the
  alias `/proc/self/fd/N` of the document (`android_content::DirectGame`) under its original name; `hash_tests.rs` proves
  it hashes like the file itself, also in a zip, as an ISO, a CUE sheet and an M3U playlist. A CHD is not identified: rcheevos
  has no CHD reader of its own and pocket-ui none (libchdr is built into the cores, not reachable); the pause menu says so.
- **The sticker:** when the session sees every achievement unlocked it leaves `<data>/achievements/mastered/<key>`
  (`achievements::mastery`, `<key>` is the player's 16-hex game identity). The launcher (`mastery_tick`, every 3 s once
  the shelf is there) sets the golden "100 %" sticker on that game, moves the mark to `mastered-done/` (so a sticker taken
  off stays off) and `room_sync` counts a game with the "100 %" sticker as completed for the trophy.
- **Tests:** `achievements::hash_tests`, `achievements::cards`, `unlock_card`, the session's mastery test and the offline
  tests (shared with Linux), and in `android_02_tests.rs` the D-pad reach, the sign-in flow (the password never in a label),
  the fit on every Retroid format in English, German and Spanish, and the sticker and trophy.

## Controllers: A/B, triggers, players

`pad::android` (src/pad.rs) turns Android keys into pocket-ui's buttons (launcher) and libretro bits (player). Android's
`BUTTON_A/B/X/Y` are the letters printed on the pad: an Xbox pad prints A at the bottom, a Nintendo pad on the right.
`Arrangement` says which, and `retro_bit_on` keeps each button on its place (libretro B bottom, A right, Y left, X top).
Menus use the keys as Android sends them (A confirms, B goes back) on every pad.

- **Odin 2 Mini.** Its "Controller style" (`Settings.System flip_button_layout`: 1 Xbox, 0 Odin, 2 pad off; read from
  OdinSettings.apk, `ControllerStylePreference`) does not change a key layout: the input service replaces the whole virtual
  pad, 2020:0112 "Xbox Wireless Controller" (Xbox style) by 2020:0111 "Odin Controller" (Odin style) and sends other evdev
  codes for the same buttons (measured 11.11.2026: the same scan codes give the same keys in both). So the style is read from
  the device, and the game must keep player 1 over the switch (`is_odin_pad`: one controller in both). Xbox style reads
  as Xbox places, Odin style as the pad's own Nintendo lettering. That mapping follows from the settings app and the
  names; pressing the real buttons in both styles was not possible without a person.
- **Other pads.** Read by make: vendor 057e (Switch Pro, Joy-Con, 8BitDo in Switch mode) is Nintendo-lettered (A on the
  right), everything else Xbox-lettered. A press's scan code can not tell instead: a simulated 057e pad on the Odin delivered
  its right-hand button as `BUTTON_A` with the bottom button's scan code. Retroid pads: not checked (no device); they are read
  as Xbox until someone with one reports its vendor/product and `Settings.System flip_button_layout`.
- **Triggers.** The Odin's "L2/R2 mode" (`Settings.Secure trigger_input_mode`: 0 analog, 1 digital, 2 both, 2 unless changed) is analog (axes `LTRIGGER/RTRIGGER` or `BRAKE/GAS`), digital (`BUTTON_L2/R2`
  keys) or both; all three reach games and menus. An axis counts as pressed past 3/4 and let go under 1/2.
- **Players.** `PlayerActivity` sends the roster of gamepads (`InputDevice` id, descriptor, name, vendor, product) to
  `android_pads::Pads` whenever `InputManager` reports a change, and the device id with every key and motion event.
  `Pads` seats them with `players::Seats` (the Pocket's own pad first, then in order of connection). Player 1 keeps the
  player's own input path; players 2 to 4 go to `Core::set_player`. A core that lists no second port hears every pad as
  player 1. A pad that leaves pauses the game (as before) and returns to its player; a pad is known by make, model and name, the
  Odin's own pad (both styles) as one. The roster goes out 250 ms after the last change (a new pad is reported added, then
  changed, a few ms apart).


# PortMaster on the standalone TrimUI card

The card reuses the existing Ports shelf, store, artwork and pause menu. Stock firmware stays in place.
Brick and Smart Pro are in PortMaster's upstream device list. Brick Pro uses the A133P runtime with its own
controls profile and still needs hardware verification. Smart Pro S is outside this card's supported hardware.

Settings → Games → Pictures & extras → PortMaster games first offers **Get PortMaster (~300 MB, one time)**.
Only A accepts the optional download. B cancels and returns to Extras; retry keeps the completed download checkpoint. Downloads pause when a game starts. Offline,
progress, retry and runtime update use the existing store step. Ordinary app updates retain the runtime;
only a changed pinned manifest offers an update. Allow at least 700 MiB free for staging and rollback.
At 2 MiB/s, writing about 277 MiB alone takes at least 139 seconds (roughly 2½ minutes);
card fsyncs, fonts, retained engines and the download add time. The installer runs in a tracked child,
reports `unpacking|done|total` after each durable block and the UI polls from its worker every 100 ms.
The page says **Preparing PortMaster…** with changing **done / total MB**; input and B cancellation stay
on the UI thread. Each component has its own real byte total, so a new component starts a new count.

| Data | Card location |
|---|---|
| Runtime generation | `pocket-ui/portmaster/current/{PortMaster,runtime}` |
| Previous generation / resumable cache | `pocket-ui/portmaster/previous`, `pocket-ui/portmaster/cache` |
| Launchers, game files, bundled pictures | `Roms/ports`, visible at `/storage/roms/ports` |
| Home/XDG saves and settings | `pocket-ui/storage/home` |

Each engine chooses its saves: some use their port folder, and some offer no persistent saves at all. Celeste
Classic's state is in RAM only. The pause
menu makes no autosave promise. There is no general PortMaster savestate format.
Pictures use the port's own cover, then its screenshot, then the PortMaster cache. A missing picture leaves a
labelled plain box. Bootstrap does not fetch bulk image packs. Ports and engines retain their own licences;
PortMaster itself is MIT. Game-file requirements stay visible in the existing two catalogue lists.

The pinned manifest gives URL, size and SHA-256. `update::payload` reuses the updater's checked/resumable
transport, RAM network phase and paced SD checkpoint. The helper stages and validates a complete generation,
then renames it into place and keeps the previous generation. Both runtime unpackers now pace card output
at 2 MiB/s, with at most 256 KiB per write followed by fsync and Linux idle I/O. The Rust ZIP extractor
uses `update::brick_io::Pacer`; the TrimUI Python policy uses the same constants and sleeps before each
block, flushes and calls `os.fsync`. Link materialization, Unicode font preparation, pinned shared-library
copies and preserved engine-library copies use that policy too. Validation and decompression never earn
credit for a later burst. Failed downloads keep their cache for retry.
Python, PortMaster and engine runtimes stay outside `app/`; the card and OTA include only small launch
helpers, Bash and licence notices. Bash source is always in the separate source ZIP, including beta builds.

`deploy/brick/portmaster/device-env.sh` declares model, glibc 2.33, aarch64, screen and physical sticks.
The Brick has zero sticks, Pro models two. No desktop OpenGL/Vulkan/armhf capability is advertised.
Firmware `trimui_inputd` supplies `TRIMUI Player1`; its measured SDL map overrides its misleading Xbox ID.
gptokeyb/gptokeyb2 provide keyboard/mouse controls, and MENU stays with pocket-ui.

Firmware SDL2 uses the direct `mali` framebuffer driver although the GPU is PowerVR. `portmaster-run`
registers a native window with `pocket-fb`: hidden UI updates do not cover the game, focus displays the pause
menu, and Continue gives the screen back. Native screenshots read the visible framebuffer page. Pause and
screen-off stop the private port session, including mappers in another process group. Quit targets that session.

Stock lacks Bash and PID namespaces. `portmaster-exec` builds a temporary chroot with bind mounts below the
run directory, supplying `/bin/bash` and legacy paths without firmware writes. Its pidof/pkill/killall wrappers
are limited to this session. An owner watcher outside the paused session compares the UI PID and start time; if it dies, stopped guests are terminated before the supervisor resumes to clean up. It reaps helpers and detaches its own mounts; a failed unmount never triggers
recursive deletion through a live bind. Symlinks are confined to RAM, not the FAT card.

Build `scripts/brick-portmaster-parts.sh OUT [WORK]` on the ARM build host and pass `BRICK_PORTMASTER_PARTS=OUT`
to `brick-make-card.sh` (default `$BRICK_PARTS/portmaster`). This builds pinned GNU Bash and our launcher;
firmware libraries are used from the device. Check `scripts/test-brick-portmaster-{runtime,launch,lifecycle}.py`, unit
tests for ports/setup/control/update::payload, `brick_fb`, and `portmaster_runtime_offer` in `nav_explorer`.
The visual/navigation matrix covers dark/light, Barlow/Retro, Large DE/ES and B/cancel/retry/reopen. `space` reports needed/available KiB; `remove-runtime` preserves configs, engine runtimes, games and saves for reinstall. Device claims go
in STATUS only after the actual play/pause/quit/restore journey.

Bootstrap prepares the upstream source tree's version from its pinned Git SHA and safely extracts its Unicode
fonts with the bundled Python before activation and completes the source tree's resource initialization; the first game does not copy fonts. Stock tar does not need XZ support. The private root also
provides `/tmp/shm` for stock's `/dev/shm` symlink. Pause screenshots on the card request a bounded PNG directly over the bridge IPC, without starting grim's shell chain; ROCKNIX keeps its existing grim capture path.

Bootstrap uses strict RAM/network separation. Arbitrary port downloads currently use card curl paced to
512 KiB/s and upstream MD5 verification. This reduces pressure on the stock SDIO/Wi-Fi driver; it does not
repair that kernel. Cheats, PortMaster and cover transfers share `optional_data::lock`, including cancellation checkpoints.

Native framebuffer capture uses aligned 32-/16-bit volatile pixel reads, retaining byte reads for unaligned maps or rows. The stock Brick follow-up captured the real paused game within the existing menu budget; see [STATUS.md](STATUS.md).

Runtime ZIP and TrimUI port ZIP/tar extraction preflight paths, case aliases, forbidden FAT/exFAT names,
file/expanded sizes, compression ratios and metadata before writing. Internal file and directory links are
materialized as ordinary files, including forward references and directory aliases; external, missing and
cyclic targets are refused. Source timestamps are not restored. Mode restoration is best effort; scripts run
through private Bash. Native ELF programs still require an executable mount. FAT32's per-file limit is
4 GiB minus one byte; exFAT/ext4 use the general streaming size budget and available-space check. Existing
ROCKNIX extraction remains unchanged, including its native links and modes on ext4. exFAT itself has no
Unix symlinks or per-file modes, on either platform.

`scripts/test-brick-portmaster-archive.py` exercises the portable policy. The explicit, privileged
`scripts/test-brick-portmaster-filesystems.py` matrix takes the pinned archives and the actual production
runtime unpacker, creates loop volumes and cleans up its mounts. FAT32 and ext4 use kernel drivers; exFAT
was verified with its real on-disk format through the FUSE driver. All three produce identical runtime and
Celeste Classic payloads. This host test does not execute ARM game binaries.

The card's supported installation format remains FAT32. The tested Brick kernel lists exFAT and its stock
launcher waits for the mounted card without selecting a filesystem. A cold boot of this standalone card
from exFAT has not been tested; extraction support does not establish that boot path. TrimUI's Smart Pro
[v1.0.3 release notes](https://github.com/trimui/firmware_smartpro/releases/tag/v1.0.3) distinguish Linux
exFAT access from FAT32-only firmware updates in U-Boot.

Host regression checks in `scripts/test-brick-portmaster-archive.py` measure pre-write sleep and per-block
fsyncs (including aliases and tiny files), progress updates, idle-I/O restoration and failure before rename.
The Brick-feature Rust archive test measures elapsed extraction time across files; the setup test checks
that unpacking events update the visible byte counter. These are host checks; no Brick device test was run
for the paced-extraction change.

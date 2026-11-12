# TrimUI Brick card: Tailscale, Remote access, updates over Wi-Fi

The Brick's own card ([TRIMUI-BRICK-STANDALONE.md](TRIMUI-BRICK-STANDALONE.md)) with three things: Tailscale for the network (an
optional page, as on ROCKNIX), SSH for logs and tests, and updates of the card's program folder over Wi-Fi. A card from the public
zip updates from the public channel: a signed list on pocketui.xyz, the tarball on the GitHub release, no Tailscale and no account
([UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#the-brick-card)). The maintainer's development cards ask a private channel inside a tailnet instead
(below, "Updates over Wi-Fi (private channel)"); this file names no tailnet. Updates over Wi-Fi, the way back, Tailscale and SSH have
run on a real Brick since the first development cards (2026-11-03); what is still open is at the end.

## The card's pocket-ui folder

    pocket-ui/start.sh          the fixed part (deploy/brick/boot.sh): picks the program folder, takes a failed update back
    pocket-ui/app/              the program: start.sh (deploy/brick/start.sh), bin/, lib/, cores/, msg/, hwroot/, licences/,
                                sources/, RELEASE (the version), update-url (the channel). An update replaces it as a whole.
    pocket-ui/app.previous/     the version before the last update (the way back)
    pocket-ui/storage/          the player's: settings, Tailscale's login (.cache/tailscale/), the Remote access password and
                                SSH host key (.config/pocket-ui/remote/), update-pending, rolled-back and known-good (pocket-ui/)
    pocket-ui/logs/             boot.log
    pocket-ui/authorized_keys   SSH keys for root, one per line (made empty at the first start, never in the card zip)

The entries `.tmp_update/updater` and `trimui/app/MainUI` call `pocket-ui/start.sh` as before. Copying a newer card zip over a card
keeps `storage/`, `logs/` and `authorized_keys` (the zip has none of them).

## Tailscale

The official static arm64 build, `tailscale` and `tailscaled` 1.104.5, unchanged (BSD-3-Clause; version and sha256 pinned in
`scripts/brick-remote-parts.sh`, which also fetches the licence and Tailscale's list of its Go dependencies). Settings › Connections ›
Tailscale is the page the ROCKNIX devices have (`tailscale.rs`, `app_tailscale.rs`); on this card its `Tools` point at the card:

| ROCKNIX | Brick card |
|---|---|
| `/usr/sbin/tailscale` | `POCKETUI_TAILSCALE`: `hwroot/usr/bin/tailscale`, the card's CLI with the card daemon's socket in `/tmp/pocket-ui` |
| `systemctl start/stop tailscaled` | the `systemctl` stand-in → `hwroot/usr/bin/brick-tailscale start/stop` |
| `tailscale.up=1` in system.cfg, ROCKNIX starts it at boot | the same line in the card's `storage/.config/system/configs/system.cfg`; `app/start.sh` starts it at boot |
| `tailscale up --reset --ssh=false` | the same, plus `--accept-dns=false --netfilter-mode=off` (`tailscale::up_args_for`): the firmware's resolv.conf and firewall are in its flash, which the card never changes |
| key file of an auth-key login in `/run` | `/tmp/pocket-ui` (`XDG_RUNTIME_DIR`; the firmware has no `/run` we rely on) |

**Signing in on the device**, the same as on ROCKNIX: Settings › Help & tools › For tinkerers › Tailscale, switch on, *Sign in*; the page shows the
sign-in address as a QR code and as text, the phone opens it, the owner approves, the page says *Connected*. The device's suggested
name is `pocket-brick`. An auth key (`tskey-auth-…`) can be typed instead.

`brick-tailscale` (BusyBox sh):

- **State on the card.** `tailscaled` keeps its state in a RAM folder (it sets file modes, which a FAT card refuses: `chmod` gives
  EPERM there) and a watcher copies `tailscaled.state` and `tailscaled.log.conf` to `storage/.cache/tailscale/` within 5 s of a change
  (a temporary name, then a rename). `start` puts the card's copy back first. So the login survives restarts and card updates; a
  power cut within 5 s of signing in can cost that one sign-in.
- **Tunnel or userspace.** With `/dev/net/tun` (a character device) it starts `--tun=tailscale0`; if that does not come up it starts
  `--tun=userspace-networking` by itself; without the device it starts userspace at once. `pocket-ui/tailscale-mode` on the card
  (`tun` or `userspace`) forces one. Incoming connections to the Brick's tailnet address reach its local services in both modes
  (in userspace mode tailscaled forwards them to 127.0.0.1), so SSH over the tailnet works either way.
- **A local proxy** on `127.0.0.1:1055` (`--outbound-http-proxy-listen`) reaches the tailnet by its MagicDNS names in both modes.
  pocket-ui's updater uses it for addresses ending in `.ts.net` (`POCKETUI_TS_PROXY`, `update::proxy_args`); everything else goes
  directly as before.
- Its log is `/tmp/pocket-ui/tailscaled.log` (RAM, cut back at 2 MB).

## Remote access (SSH)

**Decision (owner, 2026-11-03):** no Tailscale SSH (`--ssh=false` stays, as on ROCKNIX); plain SSH with dropbear, listening only while
Settings › Help & tools › For tinkerers › Remote access is on, with a password the device makes and shows, and keys from the card. Root logs in over
Tailscale and the local network alike. The firmware has no dropbear of its own that we know of (NextUI's SSH pak brings one; we use
nothing of NextUI), so the card ships it:

- **dropbear 2026.94** (MIT), built by `scripts/brick-remote-parts.sh` with zig, static against musl, with
  `deploy/brick/dropbear/pocket-ui.patch` and `localoptions.h`. The patch (four small hunks, active only through its environment):
  `DROPBEAR_PASSWORD_FILE` makes the password the one in that file (compared in constant time, at least 8 characters; the account's
  own password, the firmware's, is never asked; no file means no password login); with it the firmware's root shell is accepted
  when it runs even if `/etc/shells` does not list it; a key folder given with `-D` is checked itself, not every folder above it
  (`/tmp` is writable by everyone); `DROPBEAR_HOME` puts the session's home in RAM, so a shell's history never lands in the flash.
  No X11 or agent forwarding, no sftp-server: `scp -O` (the old protocol) and `ssh brick cat FILE` work.
- `remote.rs` on this card: *switch on* makes the password once (`secret::load_or_create`, `storage/.config/pocket-ui/remote/password`,
  the same kind as on ROCKNIX: three words and three digits), shows it, and runs `systemctl start sshd` (the stand-in → `brick-ssh`);
  *switch off* stops the listener and ends open sessions. Off after a restart, as on ROCKNIX; a card with
  `storage/.config/pocket-ui/dev-ssh` starts it at every boot. No `setrootpass`: root's password in the firmware stays as it is.
- `brick-ssh` copies `pocket-ui/authorized_keys` (comments dropped) into `/tmp/pocket-ui/ssh/` at every start, keeps the host key
  on the card (`remote/dropbear_ed25519_host_key`; the same key across restarts and updates) and logs to `/tmp/pocket-ui/dropbear.log`.
- An SSH shell's `PATH` starts with `/tmp/pocket-ui/bin`: `tailscale` (the card's), `curl`, `scp`, `dropbearkey`.

    ssh root@pocket-brick              # over the tailnet (MagicDNS) or ssh root@<its Wi-Fi address>
    scp -O root@pocket-brick:/mnt/SDCARD/pocket-ui/logs/boot.log .

`secret::write_private` takes an EPERM from `chmod` on the card's FAT as "the card decides" (the file is made 0600 anyway); before,
the password could not be kept on this card at all.

## Updates over Wi-Fi (private channel)

The channel is `https://<the build machine's tailnet name>/brick-updates/<channel>/update.json` with its `.sig`, served by
`tailscale serve` from `~/work/drop/export/web/brick-updates/` on the build machine, so only devices in the owner's tailnet reach it.
The card gets the address at build time (`BRICK_UPDATE_URL` → `app/update-url` → `POCKETUI_UPDATE_URL`); settings.json `update_url`
still overrides it. Signing and checks are those of [UPDATE-CHANNEL.md](UPDATE-CHANNEL.md): the store key signs the list, the
device checks the signature against `store::PUBLIC_KEY`, https only, size and sha256 of the tarball against the list, a version
must be newer than the running one.

**Installing** (`update::install_brick`, Settings › System › Updates, *Install*):

1. Space first: the conservative app-size allowance plus 32 MB must be free on the card, else "The SD card is too full for
   this update: it needs N MB more. Nothing was changed." Changed components require their unpacked size as well.
2. Download into a private `/tmp/pocket-ui-update-<card-hash>/` folder on the Brick's RAM tmpfs, through Tailscale's proxy
   when needed. A partial app download resumes during this boot. All compressed downloads together are capped at 320 MiB,
   with 128 MiB of available memory reserved for the system. Read the component inventory directly from the verified app
   archive, hash installed assets, then download and verify **every** needed component into RAM. No update directory,
   extraction, cleanup or bulk card write begins until all download children have exited. A low-memory refusal changes
   nothing on the card; there is no fallback to downloading onto it. Rebooting discards a partial RAM download.
3. Unpack as it is read (`untar::unpack_file_throttled`, bounded decompression and the usual refusals for links, devices,
   `..` and data behind the end) into `pocket-ui/app.new`. Card writes run on the update worker with Linux I/O class idle,
   at most 2 MiB/s and a `fsync` after every chunk of at most 256 KiB. There is no saved credit that permits a later burst.
   Phone/web transfers, local save backup/restore, starter ROM output, settings backups/restore and
   conflict copies share this write budget on Brick. Other device families retain
   their previous behavior. A full card removes the incomplete folder and retains the app download in RAM for another try.
4. Check the required startup files, `RELEASE`, `sh -n start.sh`, and exact version output of both programs. Extract and
   verify changed components from their completed RAM archives. Matching immutable cores, engine data and helper binaries
   move by rename with the recovery manifest below, avoiding the roughly 600 MB inherited copy on a normal update.
5. `.ready` in `app.new`, `storage/pocket-ui/update-pending` = the version, `sync`, then `app` → `app.previous` and `app.new` →
   `app` (one older `app.previous` goes first). pocket-ui exits 75; `app/start.sh` undoes its binds and ends with 75; `boot.sh`
   starts the new `app/start.sh`. Tailscale and an open SSH session keep running through this.

**The way back** (`deploy/brick/boot.sh`, `deploy/brick/start.sh`): the new pocket-ui removes `update-pending` once its first picture
is on screen (`update::watch_health`, as on ROCKNIX; one that hangs is ended after 20 s with 124). Until then any end is a failed
start: `app/start.sh` ends with 76 at once (a missing part, a pocket-fb that does not come up, a crash; no failure picture, no ten
minutes), `boot.sh` moves `app` to `app.failed` and `app.previous` back to `app`, writes `rolled-back` (pocket-ui says it once) and
starts again. A start script that is not even valid sh ends the same way. A power cut between the two renames is finished at the
next start when `app.new` is complete (`.ready`), else `app.previous` comes back. An `update-pending` that names another version than
`app/RELEASE` is stale and dropped. A version that confirmed itself and then keeps ending (three crashes, or three quick ends)
before it ever ran 120 s on this card goes back the same way: `app/start.sh` writes `update-pending` for it again and ends with 76,
so even a card whose `boot.sh` is older than this rule takes `app.previous` back. A version that did run 120 s is the card's known
good one (`storage/pocket-ui/known-good`, as ROCKNIX's `known-good` link) and keeps its place: its crashes end on the failure
picture as before. A restart pocket-ui asks for (exit 0) is no crash. Roms, Saves, covers (`Roms/<system>/.media`), `storage/`, `logs/` and `authorized_keys` are
never touched. Not covered: a new `start.sh` that hangs before pocket-ui starts (it is checked with `sh -n` and comes from a build
that ran its tests).

**Unchanged assets and power loss**: component updates can reuse verified unchanged files by renaming them on the same
card instead of copying the large core/BIOS/helper payload. Before its first such rename, the updater must durably install the
current `boot.sh` as the fixed `pocket-ui/start.sh`, then atomically publish `app/.asset-moves`. This manifest contains one
relative file path per line, including a final newline. Only `cores/**`, `bios-data/**`, and
`bin/{brick-egl,curl,dropbearmulti,syncthing,tailscale,tailscaled}` are allowed; empty paths, empty/`.`/`..` components, carriage
returns, backslashes, NUL bytes, links and non-regular files are refused. The updater renames those files from `app` into
`app.new`, durably finishes every move, and only then publishes `.ready`, `update-pending` and the two program-folder renames.
The previous version retains its manifest for a later rollback; the next update discards that older previous version before
starting its own transaction.

On a power cut before the first program-folder rename, `boot.sh` checks the **whole** manifest and both trees before changing
anything, revokes `app.new/.ready`, and restores missing old files from `app.new`. A file already present in the old version is
kept. Every restored rename is synced; the manifest is removed only after restoration is durable. If `app` is absent, a ready
`app.new` still completes the normal swap. An incomplete `app.new` instead returns its reusable assets to `app.previous`
before that previous version is selected. If any listed asset is missing from both trees or the manifest is malformed, boot
stops, logs `asset recovery blocked`, and leaves the program folders available for recovery.

A failed new version follows the same validation before returning its reused assets. A synced
`app.previous/.restore-assets` phase marker records the failed release **before** restoration starts; it survives moving the
failed version to `app.failed` and `app.previous` back to `app`. On restart that marker takes priority over any staged update,
including the boundary where `app` is absent and only `app.failed` remains. Once the returned version and `rolled-back` are
durable, `update-pending`, the asset manifest and finally the phase marker are cleared. This also supports the older
copy-based updates that have no asset manifest. `BRICK_BOOT_TESTS_ONLY=1 bash scripts/test-brick-card.sh` exercises every asset
move and swap/rollback boundary under dash and BusyBox ash; the regular card test also retains the complete legacy update flow.
These tests model persisted filesystem states; physical power-loss durability still needs the real-card gate.

**Publishing** a card build (on the build machine, never elsewhere):

    POCKETUI_VERSION=0.1.14-brick.6 cargo zigbuild --release --features brick --target aarch64-unknown-linux-gnu.2.33 \
      --bin pocket-ui --bin pocket-core --bin pocket-fb          # the version must count up
    scripts/brick-remote-parts.sh PARTS                          # once per version change of Tailscale, curl, dropbear
    BRICK_PARTS=PARTS BRICK_UPDATE_URL=https://<tailnet name>/brick-updates/beta/update.json \
      scripts/brick-make-card.sh OUT BINS LIBS CORES 0.1.14-brick.6
    scripts/brick-publish.sh OUT/card --channel beta --notes "what changed"

`brick-publish.sh` refuses a folder without the parts the card needs, programs that are not aarch64 or not built as `RELEASE`, a
broken `start.sh`, a version not newer than the channel's, and a key pocket-ui does not trust; it packs reproducibly, makes and
checks the signed list aside, puts the tarball in place first and the list last (by rename), keeps the last three tarballs (a Brick
in the middle of a download finishes it), and reads the list back over the tailnet (`--full-check`: the tarball too). The key is
never printed. The first card with this updater goes onto the Brick by hand (the zip); every later one can come over Wi-Fi.

**Earlier numbered builds:** `<release>-brick.<n>`, for example `0.1.14-brick.6`, remains a readable and comparable
version. About shows the exact identifier with the device name; Updates shows the exact identifier. `update::newer`
compares number groups, so older date builds and numbered builds still update in the same order. The old private channel
folder and `--channel beta` remain compatibility identifiers for existing cards. Current public cards use the public channel.

## curl

pocket-ui fetches covers and updates with `curl`; the firmware's curl and CA certificates are unknown, so the card ships stunnel's
static curl 8.23.0 (curl licence; OpenSSL, zlib-ng, brotli, zstd, c-ares, libidn2, libunistring, libpsl, libssh2, nghttp2/3, ngtcp2;
all licences from its `-dev` package in `licences/curl/`) with its own CA bundle. `app/start.sh` puts it first on `PATH`.

## Licences (`app/licences/`, `app/sources/network/`)

| Part | Licence | On the card |
|---|---|---|
| Tailscale 1.104.5 (tailscale, tailscaled) | BSD-3-Clause; its Go dependencies (BSD, MIT, Apache-2.0, MPL-2.0…) listed in `DEPENDENCIES.md` | licence and dependency list; source: the v1.104.5 tag (SOURCES.txt) |
| curl 8.23.0 (static) | curl (MIT-like) and its libraries' licences; libidn2 2.3.8 and libunistring 1.5 LGPL-3.0+/GPL-2.0+, linked in statically | every licence; the two LGPL libraries' source and the scripts that built this curl (`static-curl-8.23.0-1.tar.gz`), so it can be rebuilt with changed versions of them; where every other source is (SOURCES.txt) |
| dropbear 2026.94 + our patch | MIT (and libtomcrypt/libtommath, public domain, in its LICENSE) | licence, the release tarball, `pocket-ui.patch`, `dropbear-localoptions.h` |

Nothing of NextUI or MinUI is used or copied.

## Tests

| What | Where | Proves |
|---|---|---|
| Updates on the card | `src/update.rs` `brick_card_tests` | a good update switches folders, keeps `app.previous`, writes `update-pending`, leaves Roms/covers/saves/settings/Tailscale login/keys byte for byte; a wrong signature, a changed list, a wrong sha256, **network loss mid-download** (a server that dies after 40 %: the old version keeps running, the part is kept and the next try resumes with a range), **a full card** (before the download, and while unpacking), **bad builds** (programs that do not run or are another version, a broken start script, a missing part, a version that does not match) each leave `app` as it was; the tailnet proxy is used for `.ts.net` only |
| Streaming unpack | `src/untar.rs` | modes, several gzip members, links/escapes/cut-short/not-an-archive/over-limit refused with nothing left behind, ENOSPC told apart |
| Start scripts | `scripts/test-brick-card.sh` (dash and BusyBox ash) | install → exit 75 → the new folder starts and confirms; a crashing update, one missing a part, one with a broken start script, one that crashes right after its first picture go back to the previous version by themselves (no failure picture), a version that ran 120 s stays; a power cut mid-switch is finished or undone; a stale `update-pending` is dropped; the zip's layout |
| Remote access | `scripts/test-brick-card.sh` (a host build of the same dropbear and patch) | no password file: keys only; the device's password logs in, another does not; home in RAM; host key kept; a key from `authorized_keys`; switching off ends open sessions |
| Tailscale | `scripts/test-brick-card.sh` (a stand-in tailscaled) | userspace without a tunnel device, tunnel with one, fallback when the tunnel fails, `tailscale-mode` forces; the login reaches the card and comes back after a restart; the CLI uses the card's socket |
| Publishing | `scripts/test-brick-card.sh` (a throwaway key, `file://`) | list, signature and tarball; same bytes twice; refusals; three tarballs kept; the key never printed |
| On a real aarch64 CPU | `scripts/brick-remote-smoke.sh PARTS` (arm64 Debian with BusyBox on an Apple-silicon Mac) | Tailscale and curl run; https with curl's own CA bundle; tailscaled in userspace mode answers through the card's socket; `tailscale up` prints the sign-in address; the state reaches the card; the local proxy carries https; dropbear lets root in with the device's password and a shell missing from `/etc/shells`, not with the firmware's root password; `scp -O`; switching off ends it |

## Still open on the device

Seen on the real Brick (development cards 1 to 7, 2026-11-03 to 11-05): Tailscale up and the sign-in, SSH over the tailnet and the local
network, updates fetched over the tailnet, unpacked on the card's FAT, the restart into the new version. Not yet seen: an update from
the public channel (GitHub's redirect to its download host, through the Brick's own curl and clock) on a card from the public zip,
a Wi-Fi loss in the middle of that download, and the way back after a real failed start (host tests only).

Public small updates: `brick-publish.sh --public` prepares a complete app tar plus immutable
per-file core/data/helper archives. The device reuses only copies with the signed inventory's
exact size and SHA-256, downloading changed components from the website with a GitHub fallback.
Development-channel publishing stays full for old clients. Rebuild the initial public card with this updater;
the publisher checks for its support marker. See [UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#small-standalone-card-updates-public-brick-channel).


## Bulk-write audit: optional assets and local copies

The stock 4.9 sunxi-mmc kernel has crashed during heavy card writes with Wi-Fi reception. Pacing reduces
that exposure; host checks cannot establish kernel stability. No device access was used for this audit.

| Path on the standalone Brick | Write policy |
|---|---|
| App/component update and boot asset seed (`update::brick`, `brick_seed`) | Existing RAM downloads, paced extraction/copies, idle I/O; unchanged assets use same-card renames |
| PortMaster Python ZIP (`archive::unpack_python_runtime`) | Shared Rust Pacer, 2 MiB/s, 256 KiB maximum block, sync_all per block, idle I/O |
| TrimUI PortMaster ZIP/tar, aliases, fonts, pinned libraries and retained config/engine libraries (`portmaster-archive.py`, `portmaster-runtime.sh`) | Equivalent Python pre-write sleep, flush + os.fsync per 256 KiB or smaller block; idle I/O restored on exit |
| Cheat database (`update::cheats`, `cheat_download`) | Existing RAM/checkpoint + shared Pacer; packed cheats are looked up in memory, not expanded as one large card tree |
| Box art batches (`covers::download_from`, `game_pictures`) | Each response finishes in RAM before the shared paced atomic write; batch workers retain the optional-data lock |
| Identification DAT tables (`rom_id`, `disc_serial`) | Download/parse in memory; large cached lookup tables use paced atomic output on their worker |
| Starter pack and generic store downloads (`starter`, `store`) | RAM network phase and paced checkpoint; extracted ROM, raw package and tar output also paced with idle I/O |
| Save backup/restore and phone/web save import (`backup_saves`, `web::files`) | Existing shared Pacer; import/export archive assembly is in RAM |
| Settings/library backup and restore (`backup`) | Explicit worker-only paced atomic bulk writes; ordinary UI settings writes retain their latency |
| Save-conflict preservation and cross-filesystem conflict moves (`sync_conflicts`) | Shared Pacer and idle I/O; same-filesystem moves remain renames |
| Syncthing save sync (`brick-syncthing`, `sync::folder_json`) | 512 KiB/s receive cap including LAN, one syncing folder, one concurrent folder writer and 256 KiB pending requests; inherited idle I/O, native fsync enabled |

The Syncthing cap limits network input, not every physical card write: its native file-copy and index database
writes do not use the Rust mutex or a per-256-KiB fsync schedule. Its internal block handling and filesystem
writeback can still burst, so this path needs separate device validation. Other external writers (games,
SSH/SCP and file sharing) likewise cannot acquire an in-process Rust budget. Patched/ZIP ROM working copies live in `/tmp` (RAM), not on the card. Small metadata, progress,
configuration and marker files are not bulk payloads. Same-card renames do not duplicate payload bytes.

PortMaster preparation now takes at least roughly 2½ minutes for about 277 MiB at 2 MiB/s, plus download,
fsync and extra font/engine copies. Both unpackers publish durable-byte progress to the temporary progress
file; the worker polls it every 100 ms and the page shows **Preparing PortMaster…**, changing MB counts
and B cancellation. See [TRIMUI-PORTMASTER.md](TRIMUI-PORTMASTER.md).

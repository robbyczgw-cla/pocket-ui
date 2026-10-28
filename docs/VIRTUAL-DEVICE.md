# Virtual Mini V2

An aarch64 Linux container on the Mac in which pocket-ui and pocket-core run like on the device, so
several sessions can develop and test at the same time without waiting for the one physical Mini V2.
The device stays the reference for speed, battery, thermals and the final check before a release.

```bash
scripts/vdev.sh deploy          # build for aarch64, start the container if needed, start pocket-ui
scripts/vdev.sh send down a     # drive the UI through its command FIFO
scripts/vdev.sh shot name       # screenshots/name.png
```

Same shape as `scripts/dev.sh`, no SSH, no lock.

## What is inside

The container is the **real ROCKNIX 20261001 userland**, not an imitation: the SYSTEM squashfs of the
SM8250 image is unpacked into a Docker image on your machine. So these are the device's own files:
glibc 2.41, sway 1.11, grim, PipeWire 1.2 + WirePlumber + pipewire-pulse (started with the arguments of the
systemd units), libpulse, wayland libs, python, curl, `/usr/lib/libretro/*.so` and `es_systems.cfg`.
The binaries from `cargo zigbuild --target aarch64-unknown-linux-gnu.2.38` run unchanged.

On top of it:

| Device | Virtual device |
|---|---|
| sway on the DSI panel, `wayland-1`, `sway-ipc.0.sock` | the same sway binary, headless backend, pixman renderer, one 1240x1080 output |
| Adreno 650, Mesa Freedreno | Debian's Mesa with **llvmpipe** in `/opt/mesa` (see below) |
| PipeWire on the codec | the same PipeWire with a null sink "Virtual speakers" |
| InputPlumber's virtual pad | a uinput pad (`docker/vdev/vdev-pad.py`) |
| `deploy/run-pocket-ui.sh` under systemd | the same script, started by `vdev.sh deploy` |
| `/storage/roms/<system>` on the SD card | symlinks to the files of a read-only host folder |

`/storage/pocket-ui/{releases/dev,current,pocket-ui,pocket-core}` has the layout `dev.sh deploy` creates
(docs/UPDATES.md), `/storage/.config/pocket-ui` and `/storage/.config/emulationstation/es_systems.cfg` are
where pocket-ui expects them, `/var/log/pocket-ui.log` is the log.

**Why not ROCKNIX's Mesa?** The Mini V2 has a real GPU, so ROCKNIX's Mesa is built for Freedreno and Zink
and has no llvmpipe (`GALLIUM_DRIVER=llvmpipe` fails to create a screen). Without a GPU the GLES 3 path of
pocket-core falls back to softpipe: 2.5 frames per second at 1240x1080. Debian trixie's Mesa 25.0 (same glibc, 2.41) brings llvmpipe; only its EGL/GLES/LLVM libraries
are copied to `/opt/mesa` and found through `LD_LIBRARY_PATH`. Everything else, sway and PipeWire included,
runs on ROCKNIX's libraries (`entrypoint.sh` starts the services with that variable removed).

## First time

Needs Docker with an aarch64 engine (colima or Docker Desktop on Apple silicon; `vdev.sh` starts colima if
Docker isn't running), `cargo-zigbuild`, and the ROCKNIX image. The image is looked for in
`downloads/rocknix-*/` of the repo, of the main checkout and next to it, or set `ROCKNIX_IMG=<.img>`.

```bash
scripts/vdev.sh up
```

builds what is missing. Once per machine, one to two minutes (67 s on the second run, the first also pulls Debian):
1. `docker/Dockerfile.extract` is a tool image (7zip, squashfs-tools). `docker/extract-rocknix.sh` runs in it,
   reads the `.img` read-only, unpacks GPT → FAT → SYSTEM squashfs, drops what a virtual device never needs
   (wine, firmware, standalone emulators, libretro cores over 20 MB such as MAME and FBNeo) and streams the
   rootfs into `docker import` as `pocket-vdev-base:<ROCKNIX version>` (2.4 GB). Nothing is written into
   the repo or anywhere outside Docker; the image file is not modified and the base image is never
   pushed or exported.
2. `docker/Dockerfile` adds Mesa llvmpipe and the boot scripts in `docker/vdev/` (15 s, rebuilt
   automatically when those files change; the tag is a hash of them).

`scripts/vdev.sh image --rebuild-base` redoes step 1 for a newer ROCKNIX image, `image --prune` removes old
`pocket-vdev:<hash>` images.

## Commands

```
vdev.sh up [--device D] | down     start / remove the container; D: miniv2 (default), rp5, flip2, mini1 (Other devices, below)
vdev.sh lid close|open             the Flip 2's lid (a container started with --device flip2); `send lid close` does the same
vdev.sh deploy [--no-build]        zigbuild, copy, (re)start pocket-ui; creates the container if needed
vdev.sh send up down a home        pocket-ui's command FIFO (up down left right a b x y l1 r1 l2 r2 select start home quit)
vdev.sh pad press A 150            virtual pad: press, down/up, seq, stick LX LY [RX RY] (see scripts/vpad.py); also MODE
vdev.sh shot NAME                  grim of the whole screen -> screenshots/NAME.png
vdev.sh log [-f] [boot|sway|pipewire|pulse|pad]   pocket-ui's log by default (pocket-core logs into it)
vdev.sh sh [COMMAND]               shell, or one command, inside
vdev.sh run-core gambatte f.gbc --bench 10   pocket-core directly, with CPU time next to the frame stats
vdev.sh stop                       stop pocket-ui (the container stays)
vdev.sh ls                         all virtual devices on this Docker host
```

`send` is for the UI (menus, `home` for the pause menu); a running game reads the **pad** (`pad press A`),
as on the device. The pad also reaches pocket-ui, like the real pad does. `run-core` takes a core name
(`gambatte`, `mgba`, `nestopia`, `snes9x` or a path), and a ROM path inside the container or a file
on the Mac (it is copied to `/storage/roms/<system>/`; unknown extension: `VDEV_SYSTEM=<system>`).

### ROMs

`/mnt/roms` is a read-only mount of `$VDEV_ROMS` (default `~/.cache/pocket-vdev/roms`). Its folders are matched
to ES system names without regard to case, with the usual short names (`GBC` → `gbc`, `SFC` → `snes`,
`FC` → `nes`, `MD` → `megadrive`), and the files are linked into `/storage/roms/<system>/`. Saves and save
states are written into the container, not to your folder. They go away with `vdev.sh down`.

The default folder holds one homebrew or test ROM per system (uCity, Nova the Squirrel, MegaMan & Bass,
Contra III). colima mounts your home folder only: for ROMs elsewhere (`/Volumes/...`) either copy them or
start colima with `colima start --mount /Volumes/<disk>` once.

### A second controller

`scripts/vpad-bt.py` adds a pad that looks like a Bluetooth controller (bus 0x05, its own `phys`), for couch co-op
([HOST.md](HOST.md#players-couch-co-op-srcplayersrs)): copy it into the container and start it there, drive it through
its FIFO, end it with `quit` (the controller "switches off"):

```bash
docker cp scripts/vpad-bt.py vdev-dev:/tmp/ && docker exec -d vdev-dev python3 /tmp/vpad-bt.py
docker exec vdev-dev sh -c 'echo "press MODE 120" > /tmp/vpad2.cmd'   # HOME on the second controller
docker exec vdev-dev cat /tmp/pocket-core.players.json                  # who is which player in the game
```

### Several at once

`VDEV_NAME=<name>` selects the container `vdev-<name>` (default `dev`), everything else follows:

```bash
VDEV_NAME=menu-work scripts/vdev.sh deploy
VDEV_NAME=menu-work scripts/vdev.sh send b
```

Each instance has its own sway, PipeWire, pad, `/storage` and FIFOs. Instances see only their own input
device. They share the VM's CPUs (`colima start --cpu N` changes that, `VDEV_CPUS=<n>` limits one).
Other knobs: `VDEV_ONBOARDING=1` starts with the first-start wizard (default: already set up),
`VDEV_LIVE=1` mounts `docker/vdev` over the image's copy while you edit the boot scripts, `VDEV_DOCKER_ARGS="..."` adds
`docker run` arguments (the Wi-Fi bench starts its device with `--network none`, [WIFI-BENCH.md](WIFI-BENCH.md)).

**Online without Wi-Fi.** pocket-ui counts as online when `wlan0` has an address. `POCKETUI_WIFI_IFACE=eth0` in
`/storage/pocket-ui/dev.env` makes it look at the container's network instead, so the automatic cover download
(`src/auto_covers.rs`), the Extras and the share address can be tried; the Wi-Fi list itself stays empty.

**Trying the first start's time zone step.** The mock device starts in America/New_York, which the step shows as
"already set". A fresh ROCKNIX is on UTC, and the container has no Wi-Fi chip (`iw reg get` fails there). To see the
guess from the Wi-Fi region, put `POCKETUI_MOCK_ZONE=UTC` into `/storage/pocket-ui/dev.env` (only `POCKETUI_*` lines
are read, at each start of `run-pocket-ui.sh`) and replace `/usr/sbin/iw` in the container with a script that
prints a saved `iw reg get` (copy the device's, or one with `country US:` for the list of a multi-zone country),
then restart pocket-ui with `vdev.sh stop` and `vdev.sh deploy --no-build`. Both are inside the container only.
On the pad the button labelled X on screen is `y` for `vdev.sh send` (the FIFO names buttons the Xbox way).

### Other devices

`vdev.sh up --device rp5|flip2|mini1` starts a virtual Retroid Pocket 5, Flip 2 or first Mini instead of the Mini
V2 (`--device miniv2`, the default; `VDEV_DEVICE=` does the same; [DEVICES.md](DEVICES.md)). Use a name of your own
so nobody else's container changes, and `down` it at the end:

```bash
VDEV_NAME=rp5 VDEV_ONBOARDING=1 scripts/vdev.sh up --device rp5
VDEV_NAME=rp5 scripts/vdev.sh deploy
VDEV_NAME=flip2 scripts/vdev.sh up --device flip2 && VDEV_NAME=flip2 scripts/vdev.sh lid close
```

What changes with the device (`docker/vdev/entrypoint.sh`):

| | miniv2 | rp5 | flip2 | mini1 |
|---|---|---|---|---|
| sway's output | 1240x1080 | 1920x1080 | 1920x1080 | 1280x960 |
| Touch panel (`vdev-pad.py`, `VDEV_PANEL`) | 1080x1240 | 1080x1920 | 1080x1920 | 960x1280 |
| Device tree `model` / first `compatible` | Retroid Pocket Mini V2 / `retroidpocket,rpminiv2` | Retroid Pocket 5 / `retroidpocket,rp5` | Retroid Pocket Flip2 / `retroidpocket,rpflip2` | Retroid Pocket Mini / `retroidpocket,rpmini` |
| Lid | — | — | `gpio-keys-lid` with `SW_LID` (`vdev.sh lid close\|open`) | — |

The device tree's two files are written to a tmpfs mounted over the container's `/sys/firmware` (the container is
privileged); where an engine doesn't allow the mount, `POCKETUI_DEVICE=<device>` goes into `dev.env` instead. The
boot log says which (`vdev.sh log boot`: `device: rp5 (Retroid Pocket 5, 1920x1080; device tree in /sys/firmware)`).
Touch positions for `pad touch` are on the device's screen. The touch rotation is the virtual device's own (no
`fbcon`), the same turn for every device; the real panels' turns are in `touch::tests` ([DEVICES.md](DEVICES.md#touch)).

The lid acts only where pocket-ui handles the power key, and the container has no logind: put `POCKETUI_POWER_KEY=1`
(handle it without the inhibitor) and `POCKETUI_SUSPEND_CMD=sleep 5` (a blocking command counts as a suspend) into
`/storage/pocket-ui/dev.env` and restart pocket-ui to try closing and opening it. There is no backlight, so "screen
off" only shows in the log (`sleep: lid closed`, `sleep: awake (the lid)`).

## Measured (2026-10-21, M1 Max, colima 6 vCPU / 8 GB, host busy with other builds)

- `vdev.sh up` on an existing image: 1.0-1.4 s. `deploy --no-build`: 0.6 s. A second instance: the same.
- pocket-ui first frame 72-78 ms after start (device: 150-300 ms).
- Idle container: 0 % CPU, about 45 MB.
- Screenshots through grim at the full 1240x1080.

pocket-core on llvmpipe, `run-core --bench 10` (same ROMs as the device table in docs/HOST.md):

| | llvmpipe in the container | Mini V2 (docs/HOST.md) |
|---|---|---|
| GBC uCity | 56-60 fps, CPU 88-96 % (one core's worth, spread over threads), 0-120 underruns | 60.00 fps, 12 % |
| GBA MegaMan & Bass | 56 fps, CPU 70-81 % | 60.00 fps, 22 % |
| NES Nova | 56 fps, CPU 82 % | 60.00 fps, 27 % |
| SNES Contra III | 56 fps, CPU 93 % | 60.00 fps, 24 % |

The emulation itself costs the same as on the device (core 0.2-0.6 ms per frame); the CPU is the software
rasterizer drawing 1240x1080 pixels per frame.

## What is different from the device

Do not take performance numbers from here.
- **Speed.** llvmpipe draws on the CPU. It holds 60 fps only when the VM has CPU to spare; under host load
  it ran 55-56 fps (frame 17.9 ms) in this session, with 80-120 audio underruns in 10 s, and 60.00 fps
  with 0 underruns when the host was quiet. Compare frame timing and underruns only against runs made
  the same way. CPU percentages are llvmpipe's, not the device's. GPU frequency and power do not exist.
- **Audio.** A null sink on a real-time clock: buffer fill and underruns behave, the latency figures
  (`audio latency: server N ms`) do not mean anything.
- **Kernel and hardware.** The kernel is the VM's (Linux 6.8), not ROCKNIX's. No battery (About says
  "unknown"), no charger, no fan, no cpufreq or thermal sensors, no backlight,
  no power key, no suspend, no Bluetooth, no Wi-Fi (the pad has the volume keys, `VOLUP`/`VOLDOWN`; the quick settings
  panel mocks what is missing, see `docs/SYSTEM-CONTROLS.md`; volume is real): Settings → Connections → Wi-Fi says "No networks found", Extras
  ask to connect first, the About page says "Not connected". pocket-ui shows these pages without errors
  (Wi-Fi, Extras and About were checked); only the real device shows real values.
- **HTTPS.** ROCKNIX's `/etc/ssl/cacert.pem` points at `/run/rocknix/cacert.pem`, which the device writes at boot
  and the container does not have, so curl fails with error 77 (cover art, the game card's Wikipedia text, updates).
  `vdev.sh sh 'mkdir -p /run/rocknix && cp /etc/ssl/cacert.pem.system /run/rocknix/cacert.pem'` fixes it for that container.
- **Services.** No systemd: `systemctl` calls in `run-pocket-ui.sh` (`essway`) and the `smbd` check fail and are
  ignored; the Samba switch in Add games cannot work. There is no EmulationStation to hand the screen back to.
- **Syncthing and Moonlight.** `extract-rocknix.sh` drops `usr/bin/syncthing`; `moonlight` is in the image but
  finds no PC. To try Settings → Connections → Sync (as done for `docs/user/sync.md`): copy the device's `/usr/bin/syncthing` into
  the container (`docker cp`), put a small `systemctl` stand-in in front of the real one (`start`, `restart`, `stop`,
  `is-active`, `daemon-reload` for `syncthing`, running the `ExecStart` of the drop-in), create
  `/storage/.config/system/configs/system.cfg`, and give the container a `wlan0` address (`ip link add wlan0 type
  dummy; ip addr add 192.0.2.23/24 dev wlan0`) so the page has an address to show. A second `syncthing` started
  with `--home /tmp/peer-home` and paired over `127.0.0.1` stands in for a computer. Nothing of this is part of the
  image.
- **Tailscale.** `extract-rocknix.sh` drops `usr/sbin/tailscale{,d}` and there is no systemd, so Settings → Connections → Tailscale says "Not installed".
  `VDEV_NAME=<name> scripts/vdev-fake-tailscale.sh install` puts stand-ins for `tailscale` and `systemctl` into that container
  (nothing leaves it, no tailnet is contacted): `up` prints a sign-in address and waits for `... approve` ("approved on the phone"),
  `... hang` makes every call hang to see that the menu stays responsive, `... state` shows what was called and what
  `tailscale.up` says in `system.cfg`, which pocket-ui writes for real (`src/system_cfg.rs`, under ROCKNIX's lock). How the page works: docs/user/remote.md.
- **Bluetooth.** No radio and no systemd: the switch is the quick settings' mock, and Settings → Connections → Bluetooth uses
  `MockBluetooth` (a paired Xbox controller; Search finds more controllers, a speaker and nameless noise; each step
  takes 1.2 s). A second uinput pad created while pocket-ui runs (as a paired controller would appear) is picked up
  within a second: `pad: connected ...` in the log.
- **Force feedback.** The test pad accepts `FF_RUMBLE` and sine `FF_PERIODIC` like the vibration motor and logs what it receives to `/tmp/vpad-ff.log` (`upload`, `play`, `stop`, `erase`); `pocket-core --rumble-test` shows it end to end (docs/HOST.md, Rumble). It can't tell whether a real motor would move.
- **Cheats.** `extract-rocknix.sh` keeps the libretro cheat database (`usr/share/libretro-database/cht`, only the folders of the consoles pocket-core hosts, about 70 MB) since `image --rebuild-base`; an older base image has none: `docker cp <extracted cht folder> vdev-<name>:/usr/share/libretro-database/cht` or `POCKET_CHEATS_DB=<folder>` in pocket-core's environment.
- **Input.** The uinput pad has buttons and the two sticks (`vdev.sh pad stick 0 1` pushes the left one down, `stick 0 0` centres it), but no triggers as axes. A second uinput device, "generic ft5x06 (8d)", is the touch screen: multitouch, direct, portrait 1080x1240 like the panel. `vdev.sh pad touch 620 540` taps, `pad swipe 600 800 600 300 400` swipes, `pad fdown X Y` / `fmove X Y` / `fup` hold a finger; positions are on the 1240x1080 screen and are turned into panel coordinates like sway's `transform 90` (`VDEV_TOUCH_ROT=0|90|180|270` for the container). The container is
  privileged for `/dev/uinput`; without it (an engine that doesn't allow it) the container still starts and
  only `send` works.
- **Emulators.** Systems pocket-core hosts (gb, gbc, gba, nes, snes) work completely. Games launched through
  `/usr/bin/runemu.sh` (RetroArch and the standalone emulators; ROCKNIX's `runemu.sh` expects the device's
  services and config) were not tried and most standalone emulators are not in the image; libretro cores of
  20 MB or more are left out. To try one (Flycast for NAOMI, docs/HOST.md "More systems"), unpack it from the ROCKNIX
  image with the `pocket-vdev-extract` tool image (`unsquashfs ... usr/lib/libretro/flycast_libretro.so`) and `docker cp`
  it into the container. Flycast's memory also needs more than the container's 64 MB `/dev/shm` (grim then dies with a
  bus error): `vdev.sh sh 'mount -o remount,size=512M /dev/shm'`.
- **Boot.** The container's boot id is the VM's and stays the same across `vdev.sh deploy`, so Settings → Playing → On start
  ("Last game") sees every start as a restart and stays on Home. `vdev.sh sh rm /storage/.config/pocket-ui/started-boot`
  before a start makes it the first start of a boot (docs/DEVELOPING.md, Start time).
- **Display.** One headless output at the device's size (1240x1080 unless `--device`), no rotation, no DPMS, no panel colour.
- **Disk.** Images take 2.4 GB (shared by all instances) plus 180 MB of Mesa. `vdev.sh down` removes
  the instance and its saves.

## Files

- `scripts/vdev.sh`: the interface above
- `docker/Dockerfile.extract`, `docker/extract-rocknix.sh`: unpack the ROCKNIX image into `pocket-vdev-base`
- `docker/Dockerfile`, `docker/mesa-closure.sh`: llvmpipe Mesa and the boot scripts on top
- `docker/vdev/entrypoint.sh`: PID 1, brings up PipeWire, sway, the pad and `/storage`
- `docker/vdev/{sway-config,pipewire-null-sink.conf,roms-link.py,vdev-pad.py,bin/}`


### Experimental SM8550 profiles

`up --device rp6|nova|odin2mini|odin2|odin2portal` uses the same **SM8250 userland**, with an SM8550 device tree:
RP6 (`Retroid Pocket 6`, `retroidpocket,rp6`) is 1920×1080 with 1080×1920 touch;
Nova (`Retroid Pocket Nova`, `retroidpocket,rpnova`) is 1280×960 with landscape touch and no turn.
Both include `qcom,qcs8550` and `qcom,sm8550`. Services and hardware remain absent/mocked:
this checks profile selection, UI, input and hosted cores, never SM8550's actual services, GPU,
Zink, fan, battery, backlight or panel. No real suspend is permitted by these profiles.

`odin2mini` writes `AYN Odin 2 Mini`, `ayn,odin2mini` and both Qualcomm compatibles. It uses 1920×1080,
1080×1920 kernel-corrected touch with Deg270, and Xbox menu defaults. `odin2` uses the same geometry/turn
with Nintendo defaults; `odin2portal` uses Deg90 and Xbox defaults. The Mini’s kernel swap/inversion is represented
by the corrected virtual evdev axes; the real driver, InputPlumber, backlight and SM8550 services are not exercised.

The virtual profile records `POCKETUI_TOUCH_ROTATE` in its dev.env to match the simulated axes; there is no
device-specific fbcon in the VM. Its exact compatible list is written as board + SoC + platform, NUL-separated.

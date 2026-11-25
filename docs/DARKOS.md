# pocket-ui as a dArkOS port (test builds)

A test port, offered only as test builds on its own channel (`darkos-test`); on main since 2026-11-15 inside the `brick`
build, switched on only by `deploy/darkos/start.sh` (`POCKETUI_HOST=darkos`), so every other build is unchanged. pocket-ui runs as a port on dArkOS (christianhaitian/dArkOS, the Debian-based
successor of ArkOS for RK3566 and RK3326 handhelds), the way the TrimUI Brick build does
([TRIMUI-BRICK.md](TRIMUI-BRICK.md)): no flashing, nothing installed outside the port's folder, and leaving pocket-ui
brings EmulationStation back. Nothing here comes from dArkOS or EmulationStation; it was written from reading the images
(v08272026, RG353M and RG351MP) and dArkOS's sources (dArkOS, ogage, EmulationStation-fcamod branch 503noTTS, the kernels'
device trees), 2026-11-11.

## What dArkOS is

| | RK3566 | RK3326 |
|---|---|---|
| System | Debian 13.6 trixie, glibc 2.41, aarch64 userland (plus armhf for ports) | the same |
| Kernel | 5.10.226 (Rockchip BSP), `CONFIG_DRM_FBDEV_EMULATION=y`, `FBDEV_OVERALLOC=200` | 4.4.189 (Rockchip BSP) |
| GPU | Mali-G52 blob (GBM/KMS) | Mali-G31 blob (GBM) |
| Sound | ALSA only: `default` = plug → softvol `Master` → dmix `hw:0,0` 44.1 kHz, period 1024, buffer 4096 (`~ark/.asoundrc`; `/etc/asound.conf` links to it); no rate converter but alsa-lib's linear one | `~ark/.asoundrc` = plug → dmix 44.1 kHz (no softvol; the volume is the codec's `Playback`). EmulationStation's `perfmax` **deletes** it while a port runs and `perfnorm` copies `.asoundrcbak` back |
| Pad | `retrogame_joypad`: A 305, B 304, X 307, Y 308, L1/R1 310/311, L2/R2 312/313, Select 314, Start 315, F 316, L3/R3 317/318, D-pad 544–547, two sticks; Vol± on `gpio-keys`, Power on the PMIC | `GO-Super Gamepad`: the same face, shoulder and D-pad codes; Select/Start/L3/R3 are BTN_TRIGGER_HAPPY1–4 (704–707), Vol± in the pad |
| Ports | `/roms/ports/*.sh`, started by EmulationStation as `ark`: `sudo perfmax …; nice -n -19 %ROM%; sudo perfnorm`; `/roms` is exFAT (`umask=000`); `ark` has sudo without a password | the same |

The A/B codes are what dArkOS's own RetroArch autoconfig and `es_input.cfg` use (A = button 1 = 305), whatever the device
tree's labels say.

**The handhelds** (dArkOS's build targets; `~ark/.config/.DEVICE` holds the name; screens from the device trees):

| `.DEVICE` | Device | Chip | Screen | pocket-ui |
|---|---|---|---|---|
| RG353M | Anbernic RG353M | RK3566 | 640×480 | runs |
| RG353V | Anbernic RG353V/VS | RK3566 | 640×480 | runs |
| RK2023 | Powkiddy RK2023 | RK3566 | 640×480 | runs (its hotkey is R3) |
| RG503 | Anbernic RG503 | RK3566 | 960×544 | refused: no layout check |
| RGB30 | Powkiddy RGB30 | RK3566 | 720×720 | refused (the CubeXX's 720×720 is `not_yet` too) |
| RGB20PRO | Powkiddy RGB20 Pro | RK3566 | 1024×768 | refused: no layout check |
| MINILOONG | MiniLoong Pocket 1 | RK3566 | 720×960, turned | refused |
| RG351MP | Anbernic RG351MP | RK3326 | 640×480 | runs |
| G350 | BatleXP G350 | RK3326 | 640×480 | runs |
| A10MINI | A10 Mini | RK3326 | 640×480 | runs |
| RGB10 | Powkiddy RGB10 (ODROID-GO Advance) | RK3326 | 320×480, turned | refused |
| (device tree `model`, see below) | R36S, R36XX, R36H and their clones | RK3326 | 640×480 | runs as `darkos-r36s`; the 720×720 Plus, Max and Ultra are refused by the screen check |

There is no official R36S image: the R36S family runs community builds of dArkOS (southoz's dArkOSRE-R36, ArkOS4Clone). They
keep dArkOS's layout (EmulationStation, `ports`, ogage, the `GO-Super Gamepad` of the ODROID-GO3 trees) but set the device
tree's `model` per board and panel (`R36S-V21 2024-12-18 2551`, `HL-R36H-V20 2024-05-18`, `G80CA-MB V1.2-20250422 Panel 8`,
`Y3506_V04_20250529 2548`). start.sh takes `R36*`, `HL-R36H*`, `GR36S*`, `G80C*` and `Y3506*` as `R36S` (whatever `.DEVICE`
holds); any other 640×480 board starts as `darkos-generic`. The R36S's FN/Menu button (BTN_TRIGGER_HAPPY5, 708; dArkOSRE-R36's
RetroArch `input_menu_toggle_btn = "16"`) is HOME like R3. Not checked on a device.

## How it runs

`deploy/darkos/pocket-ui.sh` is the entry in Ports; it runs `pocket-ui/start.sh` as root (ark's sudo). start.sh:

1. checks the programs, `/dev/fb0` and the board: dArkOS's `.DEVICE` name, else the device tree's `model`, against the table
   above (`src/darkos.rs` has the same rows, a test holds the two together). A refused board or a screen that is not
   640×480 gets a message on the screen and EmulationStation comes back (see "When it cannot start" below). It sets `POCKETUI_HOST=darkos`,
   `POCKETUI_DEVICE=darkos-<board>`;
2. continues in a mount namespace of its own (`unshare -m`, private): a tmpfs view in `/tmp/pocket-ui-darkos/view` with the
   system's folders bound in (`/roms`, and the port's own folder at its own path when it sits on a second card, `/roms2`),
   our `storage/` as `/storage`, and the player's `<card>/<system>` bound **read only** onto `/storage/roms/<system>`. The mounts end with the namespace, also after a crash;
3. reads dArkOS's installed `es_systems.cfg` (including its user override), canonicalises the system names and writes our
   shelf list. Existing ARM64 libretro cores are offered through pocket-core, PSP and Dreamcast included (below);
   GameCube uses the host's own ES command. All other native systems and games pocket-core cannot open retain that host
   route. The selected card supplies games and a read-only BIOS view (two subfolders writable, below). Saves of pocket-core stay in `storage/saves`, states in
   `storage/roms/savestates`. Native launches re-enter the original host root and run as ark with dArkOS's own tools,
   configuration and save locations; pocket-fb's existing native display handover returns to the shelf afterwards.
4. picks the sound device (below), puts tty1 in graphics mode (KDSETMODE), starts `pocket-fb --take` on fb0 and pocket-ui in
   the view (`chroot`), `POCKET_CORE_SOFTWARE=1` (the game's picture by the CPU);
5. at the end puts the console back and lights a screen left dark; volume and brightness stay as the player set them.

**The screen.** EmulationStation-fcamod hides itself before it runs a port (`HideWindow`, true on Linux): `SDL_Quit` closes
its KMS device, then it waits in `system()` and opens it again when the port ends. Nobody stops EmulationStation.
When the last KMS client closes, the DRM driver's fbdev emulation puts `/dev/fb0` back on the panel; `pocket-fb --take`
sets fbdev's mode again (forced) and pans, in case the panel still shows the last KMS picture. The kernel refuses that
(EBUSY) while another program holds the display; `logs/pocket-fb.log` says which.

**Sound.** On dArkOS pocket-ui and pocket-core send 44.1 kHz (`audio::out_rate`), the rate of dArkOS's dmix, so ALSA converts
nothing: the game's sound goes through our own sinc resampler once (core rate → 44.1 kHz), the UI's 48 kHz sound files
through the same converter (`resample::convert_mono`). The device (`POCKET_CORE_ALSA_DEVICE`, logged in `start.log`):

- `/etc/asound.conf` readable and with a dmix (RK3566; RK3326 when started from a shell): `default`, shared by the UI's
  sounds and the game, the volume keys move its softvol `Master`;
- RK3326 from EmulationStation (perfmax deleted the file): dArkOS's own `~ark/.asoundrcbak` (the same dmix, the same key,
  the same 44.1 kHz) becomes our `HOME`'s `.asoundrc` while we run, so it is `default` again;
- neither: `plughw:0,0`, the card alone: only one sound at a time (no UI sound while a game plays).

The first stream that opens logs ALSA's chain (`snd_pcm_dump`): `audio out: ALSA "default" at 44100 Hz, rate converted by
ALSA: no; chain: Plug PCM > Soft volume PCM > Direct Stream Mixing PCM > Hardware PCM …`. A `YES (linear-interpolation)`
there is a fault.

**Power key ownership.** `/dev` is already recursively bound into the chroot, including `/dev/input/event*`.
On RG353M the RK817 PMIC registers `rk805 pwrkey` (KEY_POWER press/release), not a fixed event number. The UI's input
supervisor takes EVIOCGRAB only on a separate power-only node. pocket-core never grabs it. ogage/logind receive no
power events during ownership; all other devices remain shared. The fd is close-on-exec and the kernel drops the grab
when the UI exits, also on SIGKILL. No service is stopped and no host configuration is changed, so ogage's exact original
state is retained. A failed grab is logged and retried; unclaimed power events are not interpreted by pocket-ui.

A short press saves the game then requests the real host `systemctl suspend` through start.sh. Holding for 800 ms opens
Sleep, Restart, Power off and Back to EmulationStation. Back to EmulationStation returns directly without a confirmation, including from Settings and the quick settings. In a game
it runs the same save/quit worker as Restart pocket-ui, then exits with code 77. The PMIC emergency long hold is separate; its register settings
are untouched. The firmware's sleep hook can restart ogage after wake without breaking the exclusive grab.

**Keys that stay dArkOS's.** `ogage` keeps running; only the separate power node is exclusive:

| Keys | ogage (rg353v branch; rk2023 and rg351mp alike, their hotkey R3) | pocket-ui on dArkOS |
|---|---|---|
| Vol+ / Vol- | `volume.sh` (softvol `Master`; RK3326 `amixer Playback`) | shows the new level, sets nothing (`quick`'s observe mode, as with ROCKNIX's `input_sense`) |
| HOME + Vol, HOME + D-pad up/down | brightness (`brightnessctl`) | HOME + Vol shows the brightness; HOME + D-pad also moves the shelf |
| HOME + D-pad left/right | volume ±1 % | in a game the previous / next save slot: both happen (not solved in the prototype) |
| Power, short | no event while the UI owns the key | saves first, then host `systemctl suspend` |
| Power, held | no event while the UI owns the key | opens the power menu after 800 ms |
| HOME + Power | no power event while the UI owns the key | same short/held power behavior |
| HOME + L3 / R3 | speaker / Bluetooth toggle | nothing |

**RetroArch's HOME shortcuts.** A game in dArkOS's own command (Runs on → dArkOS) whose command starts dArkOS's RetroArch
(`/usr/local/bin/retroarch` or `retroarch32`, also as `%EMULATOR%`) gets pocket-ui's HOME shortcuts, as on ROCKNIX: HOME + R1
save, L1 load, R2 fast-forward, L2 held rewind, A screenshot, Y the FPS, X RetroArch's menu, START quit (twice: dArkOS's
`quit_press_twice`). Before the start pocket-ui writes them, with the game's Options → Shortcuts (Kids Mode, Easy), into
`/tmp/pocket-ui-darkos/retroarch-shortcuts.cfg` (`ra_hotkeys::prepare_darkos`), and host-launch.py puts
`--appendconfig=<that file>` behind the program in dArkOS's command template. dArkOS's `~ark/.config/retroarch/retroarch.cfg`
is never written. The hotkey button is pocket-ui's HOME, numbered from the pad's key bitmap the way RetroArch's udev driver
does: F (BTN_MODE, 10) on the RK3566 `retrogame_joypad`, FN (708, 16) on an R36S pad that has it, else R3 (707, 15). dArkOS's own
hotkey (Select on the RG353M) does nothing in these games then. HOME + A is RetroArch's screenshot into pocket-ui's Screenshots
(`storage/roms/screenshots`), since pocket-fb cannot read the screen dArkOS's RetroArch draws on; rewind is enabled while its
shortcut is on (dArkOS ships it off). Without the pad's autoconfig or HOME key there is no file and dArkOS's hotkeys stay.
Emulator scripts (`amiga.sh`, `coco.sh`, standalone emulators) are not changed. RetroArch keeps only the last `--appendconfig`, so dArkOS's
own files stay in front of ours in one `a|b` list: an `--appendconfig` in the ES command, and the wrapper's
`retroarch.cfg.ext` for an external pad (RK3566, more than 7 input nodes, a second joystick: the wrapper's own test).
Not checked on a device.

**Over the game.** pocket-fb runs here without `--gl`, so pocket-core's picture is the CPU copy (`soft_screen`) and no GPU draws
the overlay. The copy blends the same pieces in itself (`soft_hud`). The H700's quiet game display (docs/H700-VOLUME.md) does
not apply here, although the boards use the H700 profile's rows (`mako_osd::quiet_for`). Shown: the volume or brightness bar when ogage changes a level
(at the top centre, at the size pocket-ui's shelf shows it on 640x480), "Battery low", the save warnings, the first game's note
and Over the game's FPS line, with the same pills as everywhere else. Nothing is drawn or paid while none is up; a piece is
rasterised again only when its text changes. Pixel Path's strip and the achievement card are not drawn on the CPU copy.

pocket-ui's own sleep after a while without input saves the game and then suspends through the real `systemctl suspend`
(the hwroot stand-in lets `suspend`, `poweroff` and `reboot` through); dArkOS's sleep hook (`sleep.rk3566`) stores and puts
back the backlight and ALSA's state and restarts ogage after the wake. `killer_daemon` and `autosuspend` ship disabled.

The stand-ins in `deploy/darkos/hwroot/usr/bin` keep pocket-ui away from dArkOS's services: `systemctl` passes only poweroff,
reboot and suspend; `nmcli`, `wifictl`, Bluetooth, the clock and `swaymsg` are "not available"; `volume`/`pactl` use the
control ogage moves (`Master`, else `Playback`); `grim` is the Brick's (asks pocket-fb).

### Settings in the port

A row that does nothing is worse than none, so the port leaves out what dArkOS keeps for itself or the port can't do
(`darkos::LEAVES_OUT` and `NEEDS_GPU`, through `device::hides_in`: the Settings rows and the Tour's cards alike;
`tests/brick_settings_walk.rs` walks every category as the port shows it):

| Left out | Why |
|---|---|
| Bluetooth, Remote access (SSH), Time | dArkOS's own: the stand-ins answer "not available" (set them in dArkOS) |
| Tailscale, Sync (Syncthing) | not in the port, and `systemctl` starts no service |
| RomM | each system's games folder is shown read only, so a download has nowhere to go |
| Screen frames, a game's Picture rows, Pixel Path's live strip (Tour) | need the bridge's GPU path; pocket-fb runs without `--gl` here (`brick_profile::picture_rows_out`) |

Kept: Wi-Fi with a page of its own ("Set up Wi-Fi in dArkOS", connected or not and wlan0's address, `setup::DARKOS_WIFI`;
nothing is scanned or switched), Pocket Sync, the web companion, Updates (the port's own channel), Back to EmulationStation, and
everything of the look and the games. The texts that name a Settings row name one that is there: the offline notes lead to
Wi-Fi, the clock notices say "Set it in dArkOS", Test buttons says to pair in dArkOS, the web companion's notes leave Tailscale
out (`tests/brick_settings_walk.rs`, `the_darkos_port_points_only_at_rows_it_has`). The starter pack's page says it doesn't work in the port (`starter::NOT_IN_DARKOS`: the games folders are read
only), and RomM games never stand on the shelf there (`romm::shelf_games`).

**Wi-Fi.** The board rows say `wifi: false`: pocket-ui offers no Wi-Fi page, phone page or file sharing in the port
(`device::no_own_wifi`). dArkOS's own Wi-Fi still counts for the pages that need a network (`device::no_wifi_chip`): when the
kernel shows `wlan0` or another wireless interface (`darkos::wifi_here`, read once at the start), Pocket Sync, the web
companion, the box art and the rest speak of a network, not of "this handheld has no Wi-Fi". Whether it is online is wlan0's
address, as everywhere (`wifi::ip`).

**Haptics.** dArkOS's pad drivers own the motor: on the RK3566 boards `singleadcjoy.c` (kernel_5_10_226) gives `retrogame_joypad`
FF_RUMBLE and sets the PWM (`pwm5` on the RG353M) from the strong channel alone; the RK3326 `odroidgo3_joypad` has its rumble too
(`rumble_boost_*`). The menu tick there is a full-power pulse of 30 ms (`rumble::tick_default`): the Mini V2's 22 ms at 55 % did
not spin the motor up (an RG353M player, 21.11.2026). At the start `rumble::look_for_motor` looks once, on a thread, for a
force-feedback device; when there is none (a board whose tree has no motor, the R36S family) the Haptics row and its Tour card go
(`darkos::no_motor`). Never written by hand: the motor's PWM sysfs (the backlight is a PWM too).

In pocket-ui (`brick` feature only): `src/darkos.rs` (the boards as rows of the H700 profile, the RK3326 pad's Select, Start,
L3 and HOME, ogage, the leave dialog's text), one line each in `device.rs`, `pad.rs`, `rescue.rs`, `quick.rs`, `sleep.rs`
and `platform/linux_audio.rs`, and `audio::out_rate`. SELECT + START held and About's EmulationStation row ask "Back to
EmulationStation?" (Cancel, Back to EmulationStation).

## Build

    export REMAP_ROOT=$PWD
    . scripts/release-rustflags.sh
    cargo zigbuild --release --features brick --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core --bin pocket-fb
    sh deploy/darkos/make-port.sh OUT <folder with the three programs> <version>

`OUT/ports/` goes into `/roms/ports/` (then EmulationStation: Start → Game settings → Update gamelists, or a restart).
Remove pocket-ui by deleting `/roms/ports/pocket-ui.sh` and `/roms/ports/pocket-ui/`.

## When it cannot start

EmulationStation shows only "Please wait…" while a port runs, so a port that stops at once looks like a black screen and a
jump back to the list. start.sh therefore shows the reason itself: on the console (tty1), with `setfont` and `printf` like
dArkOS's own scripts and PortMaster, for 15 s: the reason, what to do, the last lines of `start.log`, `pocket-fb.log` and
`pocket-ui.log`, and "Send a photo of this screen." It needs no program of ours, so it also works when `bin/` cannot be run.
The launcher allows about 20 seconds for the Bridge's Wayland and IPC sockets. `start.log` reports which sockets
have appeared after five seconds and at the deadline. `pocket-fb.log` starts fresh on every attempt and timestamps the
framebuffer setup, display handover and socket setup. On dArkOS RK3326 boards the bridge reads fbdev geometry and writes frames at the displayed offset with `pwrite`. This
uses neither framebuffer mapping nor display-control ioctls. The log brackets the first frame write.
It appears when a check fails (missing or non-executable program, refused board, wrong screen size, a view that cannot be
built), when pocket-ui is not in its view, when pocket-ui ends with an error or within 4 s, and (from `pocket-ui.sh`) when
start.sh never ran. start.sh is run by `sh`, so it needs neither the executable bit nor a clean first line.

**PSP and Dreamcast (test-6).** As on ROCKNIX and the Brick they run on pocket-core, with the full pause menu and
Continue: Dreamcast, NAOMI and Atomiswave with the image's `flycast_libretro.so`, PSP with its `ppsspp_libretro.so` (both
64-bit, in `~ark/.config/retroarch/cores`; the 32-bit, rumble and "xtreme" variants of dArkOS's list are not used). The
core options are pocket-core's, the same as on ROCKNIX, with one exception: dArkOS's Flycast is a 2024 build (the older
`reicast_*` option names) that crashes saving a state while it renders on its own thread, so start.sh puts
`reicast_threaded_rendering = "disabled"` into the port's own `storage/.config/retroarch/retroarch-core-options.cfg`,
which pocket-core reads beneath the player's choices; what that costs on an RK3566 is not measured yet. Their GLES context is the Brick's (`brick_gles`): an off-screen
EGL surface on the system's `libEGL.so.1` (dArkOS: the Mali blob), each frame read back to the CPU picture. When a core
is missing the system keeps dArkOS's command (`dreamcast.sh`, `ppsspp.sh`), and a start that fails before the first
picture (no GL context, for example) retries once with that command (`fallback`); a game that already has saved
moments is not handed over by itself, the player is told and chooses. Runs on → dArkOS chooses it for a
game or the console. PSP minis and GameCube stay on dArkOS's emulators.

- BIOS as on ROCKNIX: the Dreamcast uses `<card>/bios/dc/dc_boot.bin` when it is there and starts without it (Flycast's
  own BIOS); NAOMI and Atomiswave need `naomi.zip`/`awbios.zip` there. `bios/dc` is writable in the view: one memory card per
  game, the file dArkOS's standalone Flycast uses there too (`vfs::share_flycast_vmus`). Cards dArkOS's RetroArch made
  next to the game are not read.
- ScummVM: the core reads and writes `scummvm.ini` in its system directory, which on ROCKNIX is the BIOS folder. Here that
  folder is read only and its `scummvm.ini` is dArkOS's RetroArch's, so start.sh gives the core pocket-ui's own
  `storage/.config/scummvm` instead (`POCKETUI_SCUMMVM_SYSTEM`); its ini starts as a copy of `<card>/bios/scummvm.ini`,
  which stays untouched. Engine data and themes are dArkOS's standalone ScummVM's (`/opt/scummvm/extra`,
  `/opt/scummvm/themes`). Saves go to pocket-ui's `storage/.local/share/scummvm/saves`, not to `<card>/scummvm`, where
  standalone ScummVM keeps its own (the games folder is read only in the view).
- PSP: before a game pocket-core copies the fonts, the atlas and the VFPU tables it lacks from dArkOS's standalone PPSSPP
  (`/opt/ppsspp/assets`, `POCKETUI_PPSSPP_ASSETS`) into `<card>/bios/PPSSPP` (writable), never replacing a file there.
  The memory stick (`PSP/SAVEDATA`) is dArkOS's standalone PPSSPP's `<card>/psp/ppsspp`, bound over `storage/saves/psp`,
  so a game saved in either continues in the other; when it is missing start.sh makes it the way `ppsspp.sh` does on its
  first start (a copy of `/opt/ppsspp/backupforromsfolder/ppsspp`).
  pocket-core passes the memory-stick root with a final `/.` to PPSSPP. This avoids its case-insensitive `PSP`
  basename shortcut: the image's 1.18.1 core creates `SAVEDATA` but tries opening `PSP/SAVEDATA` when that shortcut
  is active. Before this fix the `storage/saves/psp` mount accidentally selected it, so in-game saves could fail
  with ENOENT even though Save & quit and slots worked. The files now go to `<card>/psp/ppsspp/PSP/SAVEDATA`.
  Existing files are never deleted. If an older session produced saves in `<card>/psp/ppsspp/SAVEDATA`, back up
  that folder and copy the game's folder into `PSP/SAVEDATA` while the emulator is stopped; keep both copies if
  a destination already exists. A failed save created no progress to recover, but an existing pocket-ui moment
  can still be resumed and then saved from inside the game with the fixed player.

The second card (`/roms2`, "Switch to SD2 for Roms") is mounted by dArkOS with `umask=0000,iocharset=utf8,noatime,uid=1000,gid=1000`
(exFAT, FAT) or `defaults` (ext4): no `noexec`, and every file is executable on exFAT and FAT.

## What is tested (card layouts)

`deploy/darkos/container-test.sh` runs in a container made from the real dArkOS rootfs (RG353M image, aarch64, the kernel's
exFAT driver). The cards are mounted the way dArkOS mounts them (`umask=0000,iocharset=utf8,uid=1000,gid=1000`, which shows as
`fmask=0000,dmask=0000`), and the port is started as EmulationStation starts it: user `ark`, through `ports/pocket-ui.sh` and sudo.

| Port and games on | Card | Result |
|---|---|---|
| `/roms` (EASYROMS) | exFAT | welcome screen, GBA game starts (gpSP), Save & quit to the shelf, clean end |
| `/roms2` (second card) | exFAT | the same (this layout failed in test 1: the view had no `/roms2`) |
| `/roms2` | exFAT, `/roms` also exFAT, both mounted at once (the tester's RG353M) | the same |

`container-test.sh pad` (a virtual `retrogame_joypad` from `vpad.py`, evdev codes pressed on the welcome screen) checks the face buttons with the standard layout (no system file is read); `container-test.sh ui` checks Back to EmulationStation from Settings and from the quick settings (clean end, no
failure screen) and that Power off reaches `systemctl poweroff`; both leave `clean-exit` in pocket-ui's logs (no logger runs on
dArkOS), so the next boot's start shows no "The device shut down unexpectedly last time.", while a pocket-ui killed like a
power cut still does (the container keeps the host's boot id: the test gives the state file an earlier one). `container-test.sh power` uses a virtual `rk805 pwrkey`
to check short/held presses, a new auto state before the suspend request, no delivery to an existing/restarted host reader,
and restoration after Back to ES and a UI SIGKILL. It requires `vpad.py` and `power-observer.py` next to the test script.
**Run power tests under a `systemd-inhibit --what=handle-power-key --mode=block` in the VM outside the container**, with
`POWER_TEST_VM_INHIBITED=1` passed to the container test. uinput devices belong to the shared VM kernel: the checks that
intentionally deliver power to the fake host reader can otherwise shut down the VM through its own logind (dArkOS's
logind ignores power, but the VM's does not). The VM inhibitor ends with the test; no host configuration is changed.

`container-test.sh osd` starts the GBA game, presses Vol+ through the status FIFO and compares the screen file before, during and
2.5 s after: the level bar's rectangle at the top centre must change, and be back to the game's picture afterwards
(`osd-before/during/after.raw` in `/out`).

`container-test.sh systems` (test-6) plays PSP (`Psp2048.pbp`) and Dreamcast (`240pSuite.cdi`), both free homebrew, with
the image's own cores: first frames through the GLES context, 8 s running, the game's picture on the screen, slot 2 saved
and loaded, the pause menu, Save & quit to the shelf; then the same systems without their cores and with a GLES context that
cannot open, which must start dArkOS's (instrumented) commands instead. The container has no Mali GPU: for these runs its
`libEGL.so.1`, `libGLESv2.so.2`, `libgbm.so.1` and `libmali.so.1` (both cores link it) point at the image's own Mesa
llvmpipe, with `GLENV="EGL_PLATFORM=surfaceless "`, and `/dev/shm` is made RAM-sized as on dArkOS (Docker's 64 MB stops
PPSSPP with SIGBUS). `container-test.sh update56` installs the previous zip (`UPDATE56_ZIP`), plays and saves a game, and
updates through Settings from a signed list holding only the changed packages (`UPDATE56_DIR`), with the network off. A newer
version first shows its "new in" cards over the shelf; the test closes them with B, as a player would. Settings is reached
by category name (`open_category`: START, then R1 until the status says `settings:List System`), never by a fixed count of
presses, because the tree of categories changes between versions; on dArkOS System starts with Back to EmulationStation,
then Updates.

Failure screens (each shows the reason and "Send a photo of this screen."): program missing, pocket-ui ends with an error, pocket-ui
ends at once, screen bridge dies, `start.sh` missing, card mounted `noexec`. Not covered: FAT and ext4 cards, other boards,
the real console (the container checks the bytes written to tty1 and their layout for 64×24, not the panel).

## Not done in the prototype

Nothing has run on a real RK3566 or RK3326 yet. The GPU (Mali's GBM/KMS), Wi-Fi, Bluetooth, updates, HDMI and the screens
other than 640×480 are out. The virtual power test covers input ownership and saving, not real suspend/wake or PMIC timing. What was checked
where, and the tester's steps: `~/pocket-review/DARKOS-PROTO.md` (outside the repository).

## test-5: signed port updates

The port uses the existing Brick component updater and boot recovery, including the merged unchanged-file delta code.
`pocket-ui/start.sh` is the shared `deploy/brick/boot.sh`; the programs, port scripts and immutable data live in `app/`.
The player's `storage/` and `logs/` stay beside it, including saves from previous test builds. The ES entry remains
`ports/pocket-ui.sh`. A manual test-4 → test-5 installation replaces the program files but keeps `storage/`.

Settings → Updates asks the releases' channel `https://pocketui.xyz/updates/darkos/update.json`, or the list that
`pocket-ui/update-url` names (a beta zip carries the beta channel there, see "Channels and versions"; test-5 to test-7 had
no file and asked `updates/darkos-test/`). An old settings.json update URL cannot select the Brick or stable channel. The exact list must
be signed by the existing project key, name `target: darkos-port`, and contain no firmware offer. The app must carry
`DARKOS-PORT` with `1` plus a newline. Updates replace only this port's `app/`, using the Brick's app.new/app.previous
transaction, component hashes, unchanged-file moves, recovery markers and first-frame watchdog. No dArkOS updater,
firmware files, user ROMs, native emulator saves or system services are installed by this updater.

`deploy/darkos/prepare-update.sh APP OUT RELEASE_BASE_URL` prepares the existing component format and signs/verifies a
separate local channel using store-tool (`CHANNEL=darkos` or `darkos-test`, `APP_BASE_URL` for the app tarball). It performs no upload. Stable is untouched. `scripts/brick-update-pack.py --platform darkos` puts immutable port data in separate content-addressed packages; only missing/changed components
are fetched. The core app archive is downloaded when its signed release changes; this is component reuse, not a binary
diff protocol. `port-data/` extends the existing safe asset path list and is restored by the same boot recovery.

Tests use the original RG353M image's cores, free homebrew and instrumented native launch commands. A route in the
inventory is not proof of full game compatibility. Real DRM/Mali output, hardware audio, all games, GameCube/3D speed
and suspend/wake still require an RG353M. All virtual power presses must be inside an outer VM handle-power-key
inhibitor, even though dArkOS itself ignores that key in logind.

## Channels and versions (from 0.5)

Every release ships pocket-ui for dArkOS beside ROCKNIX and the TrimUI card, labelled beta: `pocket-ui-darkos-X.Y.Z.zip` and the
app tarball `pocket-ui-darkos-X.Y.Z.tar.gz` are assets of the normal `vX.Y.Z` release, built from the release's own public tree
(the tag is the source, no patch) with the release's `--features brick` programs. The unchanged component packages stay on the
pre-release `darkos-test-1`, where every package so far lives (content-addressed, the same name is the same bytes); a release
uploads only the new ones there.

| Channel | List | Who reads it |
|---|---|---|
| releases | `updates/darkos/update.json` | every card whose `pocket-ui/update-url` is missing (a release zip has none) |
| betas | `updates/darkos-test/update.json` | cards with a beta zip (its `update-url` names this list) and test-5 to test-7, which asked it by default |

A release puts the same signed list in both folders, so the beta cards are offered the release too; the installed release then asks
the releases' channel unless the card's `update-url` keeps it on betas (a beta zip leaves one; delete it to follow the releases).

Versions (`update::newer` compares only the number groups): a release is plain `X.Y.Z`; a beta between releases is the last public
release + `-darkos-test-N` (`0.4.2-darkos-test-7`), so it sorts above the release it is built on and below the next one. A beta named
after the coming release (`0.5.0-darkos-test-7`) would count as newer than `0.5.0`, and its cards would never be offered it.


Most R36S clones on dArkOS have no Wi-Fi: each release also has `pocket-ui-update-X.Y.Z-darkos.zip`, the releases' signed list,
the app tarball and all component packages (about 16 MB), copied onto EASYROMS as it is and installed from the card
([UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#the-update-file-no-wi-fi)).

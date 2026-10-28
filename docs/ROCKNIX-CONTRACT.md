# What pocket-ui relies on in ROCKNIX

pocket-ui replaces only ROCKNIX's EmulationStation service (essway). Everything else is ROCKNIX's: the settings file
and its lock, the shell functions, the game launcher `runemu.sh`, es_systems.cfg, the cores, the start scripts of the
standalone emulators, the services, sleep, the fan and the lights. Each of these is a promise pocket-ui depends on,
and a ROCKNIX release can break any of them without anything in pocket-ui changing. This page lists them all: what,
where our code relies on it, what we expect, and how `scripts/rocknix-contract.sh` checks it.

## The check

```sh
unsquashfs -d /tmp/rocknix-root SYSTEM               # SYSTEM: on the image's first (FAT) partition, or in an update tar
scripts/rocknix-contract.sh --root /tmp/rocknix-root # an unpacked or mounted root
scripts/rocknix-contract.sh --host root@<device>     # a running ROCKNIX, read-only, over one SSH session
scripts/rocknix-contract.sh --ref origin/next        # ROCKNIX's source tree (github.com/ROCKNIX/distribution)
scripts/rocknix-contract.sh -q ...                   # only what is not ok
```

One line per promise: `ok`, `CHANGED` (there, but not as pocket-ui expects) or `MISSING`, with the evidence (the line
found, the value, the sha256, the extensions that came or went); a CHANGED or MISSING line also names the code that
relies on it. The last line is `CONTRACT: ok` (exit 0) or `CONTRACT: <n> changed` (exit 1); exit 2 when the system
can't be read.

- `--root` reads the folder only; links inside it are followed inside it, not on the computer. A SYSTEM file unpacks
  in about 10 s to 4.6 GB (20261001). On a Mac without `unsquashfs`, unpack it on a Linux computer or in Docker.
- `--host` sends the checker and the data through `ssh <host> sh -s` (one connection, nothing written on the device)
  and adds the rows only a running system has (`live:`, the card under `/storage`). The checker is POSIX sh with the
  tools ROCKNIX has (bash, BusyBox `sed`, `awk`, `tr`, GNU `grep`); its self-test also runs with BusyBox's.
- `--ref` takes the files the data maps to ROCKNIX's source tree (`src` rows) from a clone in
  `~/.cache/rocknix-distribution` (`ROCKNIX_SRC`, the one `rocknix-diff.sh` uses) and checks those. What the build
  makes (es_systems.cfg, the cores, glibc, systemd, files of packages the data doesn't map) prints `n/a`; for those
  compare images, or use `rocknix-diff.sh` on the refs.

`scripts/rocknix-diff.sh` is the other half: it compares two ROCKNIX versions and lists everything that changed in the
parts pocket-ui touches. The contract says whether one version still keeps pocket-ui's promises; the diff shows what
moved ([UPDATES.md](UPDATES.md#a-rocknix-bump) has the order: contract, diff, then the fixes).

Results (2026-10-24): ROCKNIX 20261001 (`--root` on the unpacked SYSTEM of `ROCKNIX-SM8250.aarch64-20261001.img`):
`CONTRACT: ok`, every row ok but the three `live:` rows (n/a). ROCKNIX `next` at 651150f (`--ref origin/next`): `CONTRACT:
ok` for the 131 rows it can check; the known changes (the `flock` lock, start_es.sh's pre-rotation and
`scanout_reset_stale`) are versions pocket-ui already handles. inputplumber.service moved in the source tree
(`sources/rootfs/...`) but installs to the same place; the data maps both.

## The data

The rows are `scripts/rocknix-contract.txt`, one promise per line: `kind|label|where pocket-ui relies on it|path|argument`
(its head explains the kinds). Where a row comes from the code, `cargo test` keeps the two in step
(`os_compat::tests::the_rocknix_contract_data_matches_the_code`, and `fan_curve`'s for the trips): the sha256s of
start_es.sh and es_settings are `os_compat::KNOWN_UI_START`, every file of `os_compat::FILES` and `DLOPEN` has a row,
the cores are `hostsys::routed_cores`, every system name hostsys knows has an es_systems row, the lock and the folder
are `system_cfg`'s, setsettings.sh's keys include every key of the Runs on page, input_sense's every key `fn_keys`
sets, and the fan trips are `fan_curve::rocknix_trips`. `cargo test` also runs the script's self-test
(`scripts/test-rocknix-contract.sh`: a tiny made-up root with a row of every kind, ok and broken, about a second).

When pocket-ui starts relying on something new of ROCKNIX's, add a row (with a content check where it relies on more
than the file being there) and a line in the tables below. When a ROCKNIX release changes a row: fix the code that
relies on it first, then the row.

## Not checked here

- The initramfs (`KERNEL`): how `/storage/.update` is installed at boot (src/update.rs) is not in SYSTEM.
- The kernel's sysfs files (the fan's thermal zone and trips, the backlight, the LEDs, cpufreq): they are the device's,
  not the image's; src/power.rs, fan_curve.rs and leds.rs find them at run time and say when they can't.
- `get_setting`'s order of levels (game, system, `system.`, plain, `global.`): the rows check that the levels are
  there, not their order; `rocknix-diff.sh` shows any change of the function for a reading.
- What scripts do at run time (a game that starts, a suspend that resumes): docs/DEVICE-TESTS.md on the device.
- games.slice: the launcher stops it when it is active (ROCKNIX after 20261001 has it, 20261001 doesn't), so it is no
  promise either way.

## The promises

### The UI start the launcher repeats (deploy/run-pocket-ui.sh, "ROCKNIX's UI start")

| What | Where in our code | What we expect | Check |
|---|---|---|---|
| start_es.sh is a version the launcher repeats | deploy/run-pocket-ui.sh ROCKNIX's UI start; os_compat::KNOWN_UI_START | `/usr/bin/start_es.sh` has a sha256 the launcher knows (`463acec36231…`, `142a7f97832b…`) | `sha` |
| es_settings is a version the launcher repeats | deploy/run-pocket-ui.sh ROCKNIX's UI start; os_compat::KNOWN_UI_START | `/usr/bin/es_settings` has a sha256 the launcher knows (`d6498b7f5dc0…`) | `sha` |
| start_es.sh calls only ROCKNIX functions the launcher repeats | deploy/run-pocket-ui.sh prerotate_start, scanout_reset_stale | the functions of `/etc/profile.d/001-functions` that `/usr/bin/start_es.sh` calls are only `prerotate_env`, `scanout_reset_stale` | `calls` |
| es_settings calls only ROCKNIX functions the launcher repeats | deploy/run-pocket-ui.sh rocknix_ui_env, rocknix_ui_chores | the functions of `/etc/profile.d/001-functions` that `/usr/bin/es_settings` calls are only `get_setting`, `set_kill` | `calls` |
| start_es.sh loads es_settings | deploy/run-pocket-ui.sh repeats es_settings in its place | `/usr/bin/start_es.sh` has a line matching `es_settings` | `grep` |
| start_es.sh starts EmulationStation | deploy/run-pocket-ui.sh run_es: EmulationStation as the fallback | `/usr/bin/start_es.sh` has a line matching `^[[:space:]]*(exec[[:space:]]+)?emulationstation([[:space:]]\|$)` | `grep` |
| es_settings exports SDL_AUDIODRIVER | deploy/run-pocket-ui.sh rocknix_scan reads the value from this line | `/usr/bin/es_settings` has a line matching `^[[:space:]]*export[[:space:]]+SDL_AUDIODRIVER=[A-Za-z0-9_,-]+[[:space:]]*$` | `grep` |
| es_settings reads system.timezone and system.language | deploy/run-pocket-ui.sh rocknix_ui_env, rocknix_ui_chores (cfg_value) | `/usr/bin/es_settings` reads `system.timezone`, `system.language` | `keys` |
| es_settings compiles locales under /storage/.config/emulationstation/locale | deploy/run-pocket-ui.sh LOCALE_DIR | `/usr/bin/es_settings` has a line matching `/storage/\.config/emulationstation` | `grep` |
| es_settings writes /storage/.cache/timezone | deploy/run-pocket-ui.sh TZ_CACHE, rocknix_ui_chores | `/usr/bin/es_settings` has a line matching `TIMEZONE=.*/storage/\.cache/timezone` | `grep` |
| es_settings restarts tz-data.service | deploy/run-pocket-ui.sh rocknix_ui_chores | `/usr/bin/es_settings` has a line matching `systemctl restart tz-data\.service` | `grep` |
| es_settings imports 999-export into systemd | deploy/run-pocket-ui.sh rocknix_ui_chores | `/usr/bin/es_settings` has a line matching `/etc/profile\.d/999-export` | `grep` |
| es_settings sets the kill chord's target | deploy/run-pocket-ui.sh rocknix_ui_chores (set_kill set "emulationstation") | `/usr/bin/es_settings` has a line matching `set_kill set "emulationstation"` | `grep` |
| 999-export, the quirk variables | deploy/run-pocket-ui.sh rocknix_ui_chores | `/etc/profile.d/999-export` is there | `file` |
| /etc/profile, which loads 001-functions | deploy/run-pocket-ui.sh in_rocknix (ROCKNIX_PROFILE) | `/etc/profile` is there | `file` |
| essway.service runs start_es.sh | deploy/essway-pocket-ui.conf replaces ExecStart; scripts/autostart.sh | `/usr/lib/systemd/system/essway.service` has a line matching `^ExecStart=/usr/bin/start_es\.sh` | `grep` |
| essway.service restarts always | deploy/essway-pocket-ui.conf: EmulationStation ending starts the launcher again | `/usr/lib/systemd/system/essway.service` has a line matching `^Restart=always` | `grep` |
| essway.service comes after sway.service | deploy/run-pocket-ui.sh waits for sway (SWAY_WAIT) | `/usr/lib/systemd/system/essway.service` has a line matching `^After=.*sway\.service` | `grep` |
| sway's socket is where the launcher looks | deploy/run-pocket-ui.sh SWAYSOCK | `/etc/profile.d/050-sway.conf` has a line matching `SWAYSOCK="?\$\{?XDG_RUNTIME_DIR\}?/sway-ipc\.0\.sock` | `grep` |
| sway's Wayland display is wayland-1 | deploy/run-pocket-ui.sh WAYLAND_DISPLAY | `/etc/profile.d/050-sway.conf` has a line matching `WAYLAND_DISPLAY=wayland-1` | `grep` |
| XDG_RUNTIME_DIR is /var/run/0-runtime-dir | deploy/run-pocket-ui.sh XDG_RUNTIME_DIR | `/etc/profile.d/050-sway.conf` has a line matching `XDG_RUNTIME_DIR=/var/run/0-runtime-dir` | `grep` |
| systemd 254 or later (RestartSteps in the drop-in) | deploy/essway-pocket-ui.conf; scripts/autostart.sh | `libsystemd-shared-<n>.so` in `/usr/lib/systemd` with n ≥ 254 | `systemd` |
| systemd reads units and drop-ins from /storage/.config/system.d | deploy/essway-pocket-ui.conf (scripts/autostart.sh), sync.rs DROPIN_PATH | `/usr/lib/systemd/libsystemd-shared-*.so` contains the text `/storage/.config/system.d` | `text` |
| tz-data.service | deploy/run-pocket-ui.sh rocknix_ui_chores; system_ctl time zone | `/usr/lib/systemd/system/tz-data.service` is there | `file` |
| localedef and its sources | deploy/run-pocket-ui.sh rocknix_ui_chores | `/usr/bin/localedef` is there | `file` |
| /usr/share/i18n | deploy/run-pocket-ui.sh rocknix_ui_chores (I18NPATH) | `/usr/share/i18n` is there | `file` |

### system.cfg, its lock and ROCKNIX's functions (src/system_cfg.rs, emu_choice.rs, fn_keys.rs)

| What | Where in our code | What we expect | Check |
|---|---|---|---|
| system.cfg lives in /storage/.config/system/configs | system_cfg::PATH | `/etc/profile.d/001-functions` has a line matching `^J_DIR="/storage/\.config/system"` | `grep` |
| J_CONF is $J_DIR/configs/system.cfg | system_cfg::PATH | `/etc/profile.d/001-functions` has a line matching `^J_CONF="\$\{J_DIR\}/configs/system\.cfg"` | `grep` |
| the lock is /tmp/.system.cfg.lock | system_cfg::LOCK | `/etc/profile.d/001-functions` has a line matching `^J_CONF_LOCK="/tmp/\.system\.cfg\.lock"` | `grep` |
| wait_lock takes the lock in a style system_cfg knows | src/system_cfg.rs LockStyle::of_device | `wait_lock` in `/etc/profile.d/001-functions` takes `${J_CONF_LOCK}` with `flock`, or creates it with `noclobber` and `$$` | `lockstyle` |
| get_setting | emu_choice, fn_keys, port_perf, the launcher (in_rocknix) | `/etc/profile.d/001-functions` defines `get_setting()` | `fn` |
| set_setting | system_cfg::set imitates it; fn_keys, port_perf call it | `/etc/profile.d/001-functions` defines `set_setting()` | `fn` |
| del_setting | system_cfg::with_setting imitates it | `/etc/profile.d/001-functions` defines `del_setting()` | `fn` |
| wait_lock | system_cfg::LockStyle | `/etc/profile.d/001-functions` defines `wait_lock()` | `fn` |
| set_kill | deploy/run-pocket-ui.sh rocknix_ui_chores | `/etc/profile.d/001-functions` defines `set_kill()` | `fn` |
| set_setting starts from the shipped system.cfg | system_cfg SEED (SystemCfg::edit) | `set_setting()` in `/etc/profile.d/001-functions` has a line matching `cp .*/usr/config/system/configs/system\.cfg` | `body` |
| set_setting takes only keys starting with a letter or digit | system_cfg::valid_key | `set_setting()` in `/etc/profile.d/001-functions` has a line matching `\[\[:alnum:\]\]` | `body` |
| set_setting deletes the key first | system_cfg::with_setting | `set_setting()` in `/etc/profile.d/001-functions` has a line matching `del_setting "\$\{1\}"` | `body` |
| set_setting appends key=value | system_cfg::with_setting | `set_setting()` in `/etc/profile.d/001-functions` has a line matching `echo "\$\{1\}=\$\{2\}" >> "\$\{J_CONF\}"` | `body` |
| set_setting's value "default" only removes | system_cfg::with_setting | `set_setting()` in `/etc/profile.d/001-functions` has a line matching `"default"` | `body` |
| del_setting removes every key= line | system_cfg::with_setting | `del_setting()` in `/etc/profile.d/001-functions` has a line matching `sed -i "/\^\$\{1\}=/d"` | `body` |
| get_setting reads per-game keys system["file"].key | emu_choice::resolve, emu_settings::es_game_key | `get_setting()` in `/etc/profile.d/001-functions` has a line matching `\[\\"` | `body` |
| get_setting falls back to system.key | emu_choice RA_SETTINGS levels | `get_setting()` in `/etc/profile.d/001-functions` has a line matching `system\.['"]*\$\{1\}` | `body` |
| get_setting falls back to global.key | emu_choice RA_SETTINGS levels | `get_setting()` in `/etc/profile.d/001-functions` has a line matching `global\.['"]*\$\{1\}` | `body` |
| ROCKNIX's default system.cfg | system_cfg SEED; emu_choice::SHIPPED_CFG (Restore defaults) | `/usr/config/system/configs/system.cfg` is there | `file` |
| default system.language | i18n (system language), the launcher (LANG) | `/usr/config/system/configs/system.cfg` has `system.language=` | `cfg` |
| default system.timezone | system_ctl time zone, the launcher | `/usr/config/system/configs/system.cfg` has `system.timezone=` | `cfg` |
| default wifi.enabled | system_ctl Wi-Fi switch | `/usr/config/system/configs/system.cfg` has `wifi.enabled=` | `cfg` |
| default ssh.enabled | remote.rs | `/usr/config/system/configs/system.cfg` has `ssh.enabled=` | `cfg` |
| default samba.enabled | wifi.rs shares | `/usr/config/system/configs/system.cfg` has `samba.enabled=` | `cfg` |
| default updates.enabled | rescue.rs | `/usr/config/system/configs/system.cfg` has `updates.enabled=` | `cfg` |
| default cooling.profile | fan_curve Auto, power.rs | `/usr/config/system/configs/system.cfg` has `cooling.profile=` | `cfg` |
| default audio.volume | sleep.rs volume after wake | `/usr/config/system/configs/system.cfg` has `audio.volume=` | `cfg` |

### Games through runemu.sh (src/main.rs spawn_game, emu_choice.rs, standalone.rs)

| What | Where in our code | What we expect | Check |
|---|---|---|---|
| runemu.sh | main.rs spawn_game | `/usr/bin/runemu.sh` is an executable file | `exec` |
| runemu.sh takes the game as its first argument | main.rs spawn_game: runemu.sh <rom> -P<system> --core= --emulator= --controllers= | `/usr/bin/runemu.sh` has a line matching `ROMNAME="\$1"` | `grep` |
| runemu.sh reads -P<system> | main.rs spawn_game | `/usr/bin/runemu.sh` has a line matching `PLATFORM="\$\{ARGUMENTS##\*-P\}"` | `grep` |
| runemu.sh reads --core= | main.rs spawn_game | `/usr/bin/runemu.sh` has a line matching `CORE="\$\{ARGUMENTS##\*--core=\}"` | `grep` |
| runemu.sh reads --emulator= | main.rs spawn_game | `/usr/bin/runemu.sh` has a line matching `EMULATOR="\$\{ARGUMENTS##\*--emulator=\}"` | `grep` |
| runemu.sh reads --controllers= | main.rs spawn_game | `/usr/bin/runemu.sh` has a line matching `--controllers=` | `grep` |
| RetroArch loads cores from /tmp/cores | emu_choice::installed; store::core_path | `/usr/bin/runemu.sh` has a line matching `/tmp/cores/\$\{CORE\}_libretro\.so` | `grep` |
| standalone emulators start as start_<core>.sh | emu_choice::installed | `/usr/bin/runemu.sh` has a line matching `/usr/bin/start_\$\{CORE%-\*\}\.sh` | `grep` |
| Mednafen starts with start_mednafen.sh | emu_choice::installed | `/usr/bin/runemu.sh` has a line matching `start_mednafen\.sh` | `grep` |
| GameCube starts with start_dolphin_gc.sh | emu_choice::installed | `/usr/bin/runemu.sh` has a line matching `start_dolphin_gc\.sh` | `grep` |
| Wii starts with start_dolphin_wii.sh | emu_choice::installed | `/usr/bin/runemu.sh` has a line matching `start_dolphin_wii\.sh` | `grep` |
| runemu.sh runs setsettings.sh for RetroArch | emu_choice RA_SETTINGS (the Runs on page) | `/usr/bin/runemu.sh` has a line matching `/usr/bin/setsettings\.sh` | `grep` |
| runemu.sh switches cooling.profile per game | fan_curve (runemu games), system_cfg (the lock exists for this) | `/usr/bin/runemu.sh` has a line matching `set_setting cooling\.profile` | `grep` |
| runemu.sh reads cooling.profile and MangoHud | fan_curve; emu_settings MANGOHUD_KEY | `/usr/bin/runemu.sh` reads `cooling.profile`, `rocknix.mangohud.enabled` | `keys` |
| runemu.sh sets the kill chord back to EmulationStation | control.rs, deploy/run-pocket-ui.sh (same target) | `/usr/bin/runemu.sh` has a line matching `set_kill set "emulationstation"` | `grep` |
| setsettings.sh | emu_choice RA_SETTINGS | `/usr/bin/setsettings.sh` is an executable file | `exec` |
| setsettings.sh reads the Runs on page's keys | emu_choice::RA_SETTINGS | `/usr/bin/setsettings.sh` reads `ratio`, `integerscale`, `smooth`, `shaderset`, `rewind`, `runahead`, `autosave` | `keys` |
| setsettings.sh has NO_REWIND | emu_choice NO_REWIND | `/usr/bin/setsettings.sh` has a line matching `NO_REWIND=\(` | `grep` |
| setsettings.sh has NO_RUNAHEAD | emu_choice NO_RUNAHEAD | `/usr/bin/setsettings.sh` has a line matching `NO_RUNAHEAD=\(` | `grep` |
| start_armsx2.sh | standalone.rs, emu_settings (PS2) | `/usr/bin/start_armsx2.sh` is an executable file | `exec` |
| start_armsx2.sh reads the PS2 options | emu_settings PS2 table | `/usr/bin/start_armsx2.sh` reads `internal_resolution`, `graphics_backend`, `enable_widescreen_patches`, `aspect_ratio`, `show_fps` | `keys` |
| start_azahar.sh | standalone.rs, emu_settings (3DS) | `/usr/bin/start_azahar.sh` is an executable file | `exec` |
| start_azahar.sh reads the 3DS options | emu_settings 3DS table | `/usr/bin/start_azahar.sh` reads `resolution_scale`, `screen_layout`, `graphics_backend` | `keys` |
| start_dolphin_gc.sh | standalone.rs, emu_settings (GameCube) | `/usr/bin/start_dolphin_gc.sh` is an executable file | `exec` |
| start_dolphin_gc.sh reads the GameCube options | emu_settings DOLPHIN more() | `/usr/bin/start_dolphin_gc.sh` reads `gamecube_controller_profile`, `swap_shoulders`, `rumble`, `shader_mode`, `shader_precompile`, `vsync`, `clock_speed`, `sync_gpu_thread`, `skip_efb_cpu_access`, `store_efb_to_texture_only`, `store_xfb_to_texture_only`, `texture_cache_accuracy` | `keys` |
| start_dolphin_wii.sh | standalone.rs, emu_settings (Wii) | `/usr/bin/start_dolphin_wii.sh` is an executable file | `exec` |
| start_dolphin_wii.sh reads the Wii options | emu_settings WII more() | `/usr/bin/start_dolphin_wii.sh` reads `wii_controller_profile`, `shader_mode`, `vsync` | `keys` |
| start_mednafen.sh | emu_choice::installed | `/usr/bin/start_mednafen.sh` is an executable file | `exec` |
| start_scummvm.sh | hostsys (ScummVM) | `/usr/bin/start_scummvm.sh` is an executable file | `exec` |
| start_pico8.sh | hostsys, app_emu_choice (PICO-8) | `/usr/bin/start_pico8.sh` is an executable file | `exec` |
| start_portmaster.sh | ports.rs | `/usr/bin/start_portmaster.sh` is an executable file | `exec` |
| /storage/cores and /tmp/cores (the cores overlay) | store::CORES_DIR, emu_choice::installed | `/usr/lib/systemd/system/tmp-cores.mount` is there | `file` |
| /tmp/shaders | emu_choice::SHADER_DIR | `/usr/lib/systemd/system/tmp-shaders.mount` is there | `file` |
| RetroArch's cheat database | cheats.rs | `/usr/share/libretro-database/cht` is there | `file` |

### The library: es_systems.cfg (src/library.rs, hostsys.rs)

| What | Where in our code | What we expect | Check |
|---|---|---|---|
| es_systems.cfg | library::load, library::all_systems | `/usr/config/emulationstation/es_systems.cfg` is there | `file` |
| every game system's folder is under /storage/roms/ | library::all_systems (folder = path without /storage/); the others are Tools, Music and the like | every `<path>` in `/usr/config/emulationstation/es_systems.cfg` starts with `/storage/roms/ idtech music scummvm steam tools` | `sysroot` |
| es_systems lists emulators by name | library.rs, emu_choice (Runs on) | `/usr/config/emulationstation/es_systems.cfg` has a line matching `<emulator name="` | `grep` |
| es_systems marks the default core | library.rs default core | `/usr/config/emulationstation/es_systems.cfg` has a line matching `<core default="true">` | `grep` |

### Sleep, sound, fan, keys, lights (src/sleep.rs, audio.rs, power.rs, fan_curve.rs, fn_keys.rs, leds.rs)

| What | Where in our code | What we expect | Check |
|---|---|---|---|
| logind suspends on the power key | sleep.rs holds a handle-power-key inhibitor while pocket-ui handles the key | `/etc/systemd/logind.conf` has a line matching `^HandlePowerKey=suspend` | `grep` |
| ROCKNIX's sleep hook | sleep.rs (volume after wake) | `/usr/lib/systemd/system-sleep/sleep` is an executable file | `exec` |
| the wake hook restores audio.volume | sleep.rs writes audio.volume through volume before it suspends | `/usr/lib/systemd/system-sleep/sleep` reads `audio.volume` | `keys` |
| volume | sleep.rs, system_ctl (volume <n>) | `/usr/bin/volume` is an executable file | `exec` |
| volume takes a number | sleep.rs, system_ctl: volume <n> | `/usr/bin/volume` has a line matching `VOLUME=\$\{1\}` | `grep` |
| volume sets PipeWire's sink | sleep.rs | `/usr/bin/volume` has a line matching `pactl .*set-sink-volume @DEFAULT_SINK@` | `grep` |
| volume stores audio.volume | sleep.rs | `/usr/bin/volume` has a line matching `set_setting "?audio\.volume"?` | `grep` |
| quantum | audio.rs resets PipeWire's quantum; the launcher after a crash | `/usr/sbin/quantum` is an executable file | `exec` |
| fancontrol | power.rs, fan_curve.rs (ROCKNIX's profiles) | `/usr/bin/fancontrol` is an executable file | `exec` |
| fancontrol hands over to the platform's quirk | power.rs, fan_curve.rs (the SM8250 profiles below) | `/usr/bin/fancontrol` has a line matching `quirks/platforms/\$\{HW_DEVICE\}/bin/fancontrol` | `grep` |
| no fancontrol of the Mini V2's own | power.rs, fan_curve.rs (the SM8250 profiles below) | `/usr/lib/autostart/quirks/devices/Retroid Pocket Mini V2/bin/fancontrol` is not there | `absent` |
| the Mini V2's own quirks | docs/device-baseline (DEVICE_* variables) | `/usr/lib/autostart/quirks/devices/Retroid Pocket Mini V2` is there | `file` |
| fancontrol's quiet trips | fan_curve::rocknix_trips, power::Fan::Quiet | `TRIPS_QUIET="52000 62000 69000 75000 80000 85000"` in `/usr/lib/autostart/quirks/platforms/SM8250/bin/fancontrol` | `trips` |
| fancontrol's moderate trips | fan_curve::rocknix_trips | `TRIPS_MODERATE="45000 55000 62000 68000 73000 78000"` in `/usr/lib/autostart/quirks/platforms/SM8250/bin/fancontrol` | `trips` |
| fancontrol's aggressive trips | fan_curve::rocknix_trips, power::Fan::Cool | `TRIPS_AGGRESSIVE="40000 48000 55000 61000 66000 71000"` in `/usr/lib/autostart/quirks/platforms/SM8250/bin/fancontrol` | `trips` |
| fancontrol writes the zone's trip points | power.rs, fan_curve.rs (only the active trips) | `/usr/lib/autostart/quirks/platforms/SM8250/bin/fancontrol` has a line matching `trip_point_\$\{idx\}_temp` | `grep` |
| gpu_overclock | system_ctl GPU cap | `/usr/lib/autostart/quirks/platforms/SM8250/bin/gpu_overclock` is an executable file | `exec` |
| input_sense | fn_keys.rs | `/usr/bin/input_sense` is an executable file | `exec` |
| input_sense reads the function key settings | fn_keys::KEYS | `/usr/bin/input_sense` reads `key.function.b`, `key.function.b.up`, `key.function.b.down` | `keys` |
| input_sense runs the configured action | fn_keys::KEYS (true = do nothing) | `/usr/bin/input_sense` has a line matching `FN_B_ACTION_UP` | `grep` |
| input.service | fn_keys restarts it | `/usr/lib/systemd/system/input.service` is there | `file` |
| mako-notify | mako_osd.rs (the box it shows is hidden by a mako mode) | `/usr/bin/mako-notify` is an executable file | `exec` |
| makoctl | mako_osd.rs (mode -a/-r pocket-ui, reload) | `/usr/bin/makoctl` is an executable file | `exec` |
| input_sense shows its volume and brightness box with mako-notify | mako_osd.rs hides it while pocket-ui is in front | `/usr/bin/input_sense` has a line matching `/usr/bin/mako-notify .*-no-es` | `grep` |
| ledcontrol | leds.rs | `/usr/bin/ledcontrol` is an executable file | `exec` |
| ledcontrol hands over to the platform's quirk | leds.rs | `/usr/bin/ledcontrol` has a line matching `quirks/platforms/\$\{HW_DEVICE\}/bin/ledcontrol` | `grep` |
| no ledcontrol of the Mini V2's own | leds.rs | `/usr/lib/autostart/quirks/devices/Retroid Pocket Mini V2/bin/ledcontrol` is not there | `absent` |
| ledcontrol off | leds.rs | `/usr/lib/autostart/quirks/platforms/SM8250/bin/ledcontrol` has a line matching `^[[:space:]]*off\)` | `grep` |
| ledcontrol rgb | leds.rs | `/usr/lib/autostart/quirks/platforms/SM8250/bin/ledcontrol` has a line matching `^[[:space:]]*rgb\)` | `grep` |
| ledcontrol battery starts batteryledstatus.service | leds.rs | `/usr/lib/autostart/quirks/platforms/SM8250/bin/ledcontrol` has a line matching `systemctl restart batteryledstatus\.service` | `grep` |
| ledcontrol stores led.color | leds.rs | `/usr/lib/autostart/quirks/platforms/SM8250/bin/ledcontrol` has a line matching `set_setting led\.color` | `grep` |
| analog_sticks_ledcontrol | leds.rs | `/usr/bin/analog_sticks_ledcontrol` is an executable file | `exec` |
| the SM8250 analog_sticks_ledcontrol | leds.rs | `/usr/lib/autostart/quirks/platforms/SM8250/bin/analog_sticks_ledcontrol` is an executable file | `exec` |
| batteryledstatus.service | leds.rs | `/usr/lib/systemd/system/batteryledstatus.service` is there | `file` |

### Wi-Fi, Bluetooth, time, services (src/wifi.rs, bluetooth.rs, system_ctl.rs, remote.rs, sync.rs, tailscale.rs)

| What | Where in our code | What we expect | Check |
|---|---|---|---|
| wifictl | system_ctl Wi-Fi switch | `/usr/bin/wifictl` is an executable file | `exec` |
| wifictl enable | system_ctl: wifictl enable | `/usr/bin/wifictl` has a line matching `^[[:space:]]*"?enable"?\)` | `grep` |
| wifictl disable | system_ctl: wifictl disable | `/usr/bin/wifictl` has a line matching `^[[:space:]]*"?disable"?\)` | `grep` |
| wifictl reads wifi.ssid, wifi.key and wifi.country | wifi.rs writes them | `/usr/bin/wifictl` reads `wifi.ssid`, `wifi.key`, `wifi.country` | `keys` |
| rocknix-bluetooth | bluetooth.rs, system_ctl | `/usr/bin/rocknix-bluetooth` is an executable file | `exec` |
| rocknix-bluetooth enable | system_ctl | `/usr/bin/rocknix-bluetooth` has a line matching `^[[:space:]]*"enable"\)` | `grep` |
| rocknix-bluetooth disable | system_ctl | `/usr/bin/rocknix-bluetooth` has a line matching `^[[:space:]]*"disable"\)` | `grep` |
| rocknix-bluetooth save | bluetooth.rs (after pairing) | `/usr/bin/rocknix-bluetooth` has a line matching `^[[:space:]]*"save"\)` | `grep` |
| rocknix-bluetooth stores controllers.bluetooth.enabled | bluetooth.rs | `/usr/bin/rocknix-bluetooth` has a line matching `set_setting controllers\.bluetooth\.enabled` | `grep` |
| bluetooth.service | bluetooth.rs, system_ctl | `/usr/lib/systemd/system/bluetooth.service` is there | `file` |
| bluetooth-agent.service | bluetooth.rs | `/usr/lib/systemd/system/bluetooth-agent.service` is there | `file` |
| steamos-set-timezone | system_ctl time zone | `/usr/bin/steamos-polkit-helpers/steamos-set-timezone` is an executable file | `exec` |
| steamos-set-timezone stores system.timezone | system_ctl time zone | `/usr/bin/steamos-polkit-helpers/steamos-set-timezone` has a line matching `set_setting system\.timezone` | `grep` |
| /usr/share/zoneinfo | system_ctl time zones | `/usr/share/zoneinfo` is there | `file` |
| systemd-timesyncd.service | system_ctl (time sync) | `/usr/lib/systemd/system/systemd-timesyncd.service` is there | `file` |
| sshd.service starts only with /storage/.cache/services/sshd.conf | remote.rs SERVICE_FLAG | `/usr/lib/systemd/system/sshd.service` has a line matching `^ConditionPathExists=\\|?/storage/\.cache/services/sshd\.conf` | `grep` |
| smbd.service | wifi.rs shares | `/usr/lib/systemd/system/smbd.service` is there | `file` |
| nmbd.service | wifi.rs shares | `/usr/lib/systemd/system/nmbd.service` is there | `file` |
| syncthing.service | sync.rs | `/usr/lib/systemd/system/syncthing.service` is there | `file` |
| tailscaled.service | tailscale.rs | `/usr/lib/systemd/system/tailscaled.service` is there | `file` |
| tailscale | tailscale.rs | `/usr/sbin/tailscale` is there | `file` |
| tailscaled.defaults | tailscale.rs (copied when missing) | `/usr/config/tailscaled.defaults` is there | `file` |
| inputplumber.service | tools.rs INPUT_SERVICE | `/usr/lib/systemd/system/inputplumber.service` is there | `file` |
| ROCKNIX mirrors /usr/config/modules at every boot | store::ROCKNIX_MODULES (Store tools go elsewhere) | `/usr/lib/autostart/common/001-sync-modules` has a line matching `rsync .*--delete.*/usr/config/modules/? .*/storage/\.config/modules` | `grep` |
| /usr/config/modules | tools::ROCKNIX_DIRS | `/usr/config/modules` is there | `file` |
| foot | tools::TERMINAL | `/usr/bin/foot` is there | `file` |
| ScummVM's data | hostsys (ScummVM) | `/usr/share/scummvm` is there | `file` |

### The system itself (src/update.rs, os_compat.rs)

| What | Where in our code | What we expect | Check |
|---|---|---|---|
| os-release has OS_VERSION | update.rs (the version shown and compared) | `/etc/os-release` has a line matching `^OS_VERSION=` | `grep` |
| os-release has HW_DEVICE | update.rs (same device) | `/etc/os-release` has a line matching `^HW_DEVICE=` | `grep` |
| os-release has HW_ARCH | update.rs (same device) | `/etc/os-release` has a line matching `^HW_ARCH=` | `grep` |
| glibc 2.38 or newer | pocket-ui and pocket-core are built for glibc 2.38 (zigbuild) | `/usr/lib/libc.so.6` is glibc 2.38 or newer | `glibc` |
| sh | os_compat::FILES | `/bin/sh` is there | `file` |
| bash | os_compat::FILES | `/usr/bin/bash` is there | `file` |
| sway | os_compat::FILES | `/usr/bin/sway` is there | `file` |
| swaymsg | os_compat::FILES | `/usr/bin/swaymsg` is there | `file` |
| systemctl | os_compat::FILES | `/usr/bin/systemctl` is there | `file` |
| start_es.sh | os_compat::FILES | `/usr/bin/start_es.sh` is there | `file` |
| emulationstation | os_compat::FILES | `/usr/bin/emulationstation` is there | `file` |
| 001-functions | os_compat::FILES | `/etc/profile.d/001-functions` is there | `file` |
| nmcli | os_compat::FILES | `/usr/bin/nmcli` is there | `file` |
| iwd | os_compat::FILES | `/usr/lib/iwd`, `/usr/libexec/iwd` is there | `file` |
| pipewire | os_compat::FILES | `/usr/bin/pipewire` is there | `file` |
| pactl | os_compat::FILES | `/usr/bin/pactl` is there | `file` |
| pw-metadata | os_compat::FILES | `/usr/bin/pw-metadata` is there | `file` |
| bluetoothctl | os_compat::FILES | `/usr/bin/bluetoothctl` is there | `file` |
| curl | os_compat::FILES | `/usr/bin/curl` is there | `file` |
| tar | os_compat::FILES | `/usr/bin/tar` is there | `file` |
| unsquashfs | os_compat::FILES | `/usr/bin/unsquashfs` is there | `file` |
| timeout | os_compat::FILES | `/usr/bin/timeout` is there | `file` |
| grim | os_compat::FILES | `/usr/bin/grim` is there | `file` |
| moonlight | os_compat::FILES | `/usr/bin/moonlight` is there | `file` |
| libwayland-client.so.0 | os_compat::DLOPEN | `/usr/lib/libwayland-client.so.0` is there | `file` |
| libxkbcommon.so.0 | os_compat::DLOPEN | `/usr/lib/libxkbcommon.so.0` is there | `file` |
| libEGL.so.1 | os_compat::DLOPEN | `/usr/lib/libEGL.so.1` is there | `file` |
| libwayland-egl.so.1 | os_compat::DLOPEN | `/usr/lib/libwayland-egl.so.1` is there | `file` |
| libasound.so.2 or libpulse-simple.so.0 | os_compat::DLOPEN | `/usr/lib/libasound.so.2`, `/usr/lib/libpulse-simple.so.0` is there | `file` |

### libretro cores pocket-core plays with (hostsys::routed_cores)

Checked by `core` rows: `/usr/lib/libretro/<core>_libretro.so` is there, for every core pocket-core plays with (`hostsys::routed_cores`):

`gambatte`, `mgba`, `nestopia`, `snes9x`, `genesis_plus_gx`, `gearsystem`, `beetle_pce_fast`, `beetle_wswan`, `stella`, `pcsx_rearmed`, `mupen64plus_next`, `ppsspp`, `flycast`, `melondsds`, `yabasanshiro`, `dosbox_pure`, `scummvm`, `fake08`, `tic80`, `beetle_supergrafx`, `beetle_vb`.

### es_systems.cfg entries of the systems pocket-ui knows by name (hostsys), as in ROCKNIX 20261001

Checked by `sys` rows: the `<system>` of that name in `/usr/config/emulationstation/es_systems.cfg` has the same `<path>` and the same set of extensions as in ROCKNIX 20261001 (CHANGED lists the folder and the extensions that came or went). Where pocket-ui relies on it: `library.rs` reads the folder and the file types, `hostsys.rs` routes the system to pocket-core, and some code names folders directly (`sync.rs`, `ports.rs`, `hostsys::BIOS_DIR`).

| System | Folder | Extensions in 20261001 |
|---|---|---|
| `gb` | `/storage/roms/gb` | 4 |
| `gbc` | `/storage/roms/gbc` | 4 |
| `gba` | `/storage/roms/gba` | 3 |
| `nes` | `/storage/roms/nes` | 5 |
| `snes` | `/storage/roms/snes` | 6 |
| `megadrive` | `/storage/roms/megadrive` | 7 |
| `megadrive-japan` | `/storage/roms/megadrive-japan` | 7 |
| `genesis` | `/storage/roms/genesis` | 7 |
| `genh` | `/storage/roms/genh` | 7 |
| `megadriveh` | `/storage/roms/megadriveh` | 7 |
| `mastersystem` | `/storage/roms/mastersystem` | 4 |
| `gamegear` | `/storage/roms/gamegear` | 4 |
| `ggh` | `/storage/roms/gamegearh` | 4 |
| `sg-1000` | `/storage/roms/sg-1000` | 4 |
| `segacd` | `/storage/roms/segacd` | 4 |
| `megacd` | `/storage/roms/megacd` | 4 |
| `sega32x` | `/storage/roms/sega32x` | 6 |
| `pcengine` | `/storage/roms/pcengine` | 4 |
| `tg16` | `/storage/roms/tg16` | 4 |
| `pcenginecd` | `/storage/roms/pcenginecd` | 5 |
| `tg16cd` | `/storage/roms/tg16cd` | 5 |
| `ngp` | `/storage/roms/ngp` | 4 |
| `ngpc` | `/storage/roms/ngpc` | 4 |
| `wonderswan` | `/storage/roms/wonderswan` | 4 |
| `wonderswancolor` | `/storage/roms/wonderswancolor` | 4 |
| `atarilynx` | `/storage/roms/atarilynx` | 5 |
| `atari2600` | `/storage/roms/atari2600` | 4 |
| `atari7800` | `/storage/roms/atari7800` | 4 |
| `pokemini` | `/storage/roms/pokemini` | 3 |
| `psx` | `/storage/roms/psx` | 11 |
| `neogeo` | `/storage/roms/neogeo` | 3 |
| `cps1` | `/storage/roms/cps1` | 2 |
| `cps2` | `/storage/roms/cps2` | 2 |
| `cps3` | `/storage/roms/cps3` | 2 |
| `fbn` | `/storage/roms/fbneo` | 2 |
| `n64` | `/storage/roms/n64` | 5 |
| `psp` | `/storage/roms/psp` | 4 |
| `dreamcast` | `/storage/roms/dreamcast` | 5 |
| `nds` | `/storage/roms/nds` | 3 |
| `saturn` | `/storage/roms/saturn` | 4 |
| `pc` | `/storage/roms/pc` | 4 |
| `scummvm` | `/storage/.config/scummvm/games` | 3 |
| `pico-8` | `/storage/roms/pico-8` | 2 |
| `tic-80` | `/storage/roms/tic-80` | 1 |
| `famicom` | `/storage/roms/famicom` | 5 |
| `nesh` | `/storage/roms/nesh` | 5 |
| `sfc` | `/storage/roms/sfc` | 6 |
| `snesh` | `/storage/roms/snesh` | 6 |
| `gbh` | `/storage/roms/gbh` | 3 |
| `gbch` | `/storage/roms/gbch` | 4 |
| `gbah` | `/storage/roms/gbah` | 3 |
| `gbav` | `/storage/roms/gbav` | 3 |
| `fds` | `/storage/roms/fds` | 3 |
| `supergrafx` | `/storage/roms/sgfx` | 7 |
| `virtualboy` | `/storage/roms/virtualboy` | 3 |
| `naomi` | `/storage/roms/naomi` | 5 |
| `atomiswave` | `/storage/roms/atomiswave` | 5 |
| `ps2` | `/storage/roms/ps2` | 9 |
| `gamecube` | `/storage/roms/gamecube` | 8 |
| `wii` | `/storage/roms/wii` | 9 |
| `3ds` | `/storage/roms/3ds` | 12 |
| `ports` | `/storage/roms/ports` | 2 |
| `moonlight` | `/storage/roms/moonlight/` | 1 |

### Only on a running system (--host)

| What | Where in our code | What we expect | Check |
|---|---|---|---|
| system.cfg on the card | system_cfg::PATH | `/storage/.config/system/configs/system.cfg` is there (running system only) | `live:file` |
| the games folder | library.rs, hostsys::BIOS_DIR | `/storage/roms` is there (running system only) | `live:file` |
| /storage/.config/system.d, systemd's folder for drop-ins | scripts/autostart.sh, sync.rs DROPIN_PATH | `/storage/.config/system.d` is there (running system only) | `live:file` |


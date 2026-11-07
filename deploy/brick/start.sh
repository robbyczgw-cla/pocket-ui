#!/bin/sh
# pocket-ui on a TrimUI Brick, without NextUI: the card's start script (docs/TRIMUI-BRICK-STANDALONE.md). Two thin entries call
# pocket-ui/start.sh (deploy/brick/boot.sh: it picks the program folder and takes a failed update back), which runs this file as
# pocket-ui/app/start.sh:
#   .tmp_update/updater        for a Brick whose /usr/trimui/bin/runtrimui.sh is NextUI's (it runs that file when it exists)
#   trimui/app/MainUI          for a Brick on TrimUI's own firmware (runtrimui-original.sh runs it when /mnt/SDCARD/trimui exists;
#                              premainui.sh and preload.sh next to it are its two companions)
# The program is pocket-ui/app (P below, replaced as a whole by an update); what is the player's stays in pocket-ui (D): storage/
# (settings, Tailscale's login, the Remote access password), logs/, authorized_keys (docs/TRIMUI-BRICK-REMOTE.md).
# Argument 1 is which entry it was (written to the log). The two firmwares do different things before handing over; see step 3.
#
# What it does, in order:
#   1. puts a picture on the screen at once (never a black screen), and writes pocket-ui/logs/boot.log
#   2. checks that the card has everything; if not, it says so on the screen and stops there (shown for 10 minutes, then off)
#   3. does what the firmware's own start script does for the hardware (5 V rail, rumble off, the pad's input daemon)
#   4. makes /storage (where pocket-ui looks for games, saves, cores) out of this card's folders
#   5. sets up the stand-ins for the helpers pocket-ui expects (volume, brightness, battery, power, Wi-Fi switch)
#   6. runs pocket-fb (the small compositor) and pocket-ui; starts them again when they end, but when they end three times
#      within a minute it shows "pocket-ui does not start" instead of trying forever. pocket-ui's exit 75 (an update was installed)
#      ends this script with 75 for boot.sh; while the version is still unconfirmed (update-pending), any failure ends it at once
#      (exit 76), so boot.sh can put the previous version back instead of showing a failure for ten minutes. A version that
#      confirmed itself (its first picture) but has never run STABLE_SECS on this card and then crashes three times before that
#      goes back the same way (it writes update-pending again for boot.sh, which works the same on every card, also on one
#      whose boot.sh is older than this file). A version that did run that long is the card's known good one (KNOWN_GOOD) and
#      keeps its place: its crashes end on the picture as before.
# Plain sh (BusyBox ash). Everything outside the card is /tmp, /storage (one empty folder, below) and the firmware's own files.
# Tests: scripts/test-brick-card.sh (BRICK_* variables below move every path, so it runs on a computer).

CARD=${BRICK_CARD:-$(cd "$(dirname "$0")/../.." && pwd)}
ENTRY=${1:-direct}
D=$CARD/pocket-ui
P=$D/app
FB=${BRICK_FB:-/dev/fb0}
FBSYS=${BRICK_FBSYS:-/sys/class/graphics/fb0}
RUN=${BRICK_RUN:-/tmp/pocket-ui}
FW=${BRICK_FIRMWARE:-/usr/trimui}
STORAGE=${BRICK_STORAGE:-/storage}
BATTERY=${BRICK_BATTERY:-/sys/class/power_supply/axp2202-battery}
SHOW_FOR=${BRICK_SHOW_FOR:-600}      # seconds a failure picture stays before the Brick switches itself off
FAST_END=${BRICK_FAST_END:-20}       # a pocket-ui that ends within this many seconds counts as "did not start"
MAX_FAST=${BRICK_MAX_FAST:-3}
STABLE_SECS=${BRICK_STABLE_SECS:-120} # a run this long makes its version the card's known good one (ROCKNIX's launcher: known-good)
LOG=$D/logs/boot.log
STATE=$D/storage/.config/pocket-ui
PENDING=$D/storage/pocket-ui/update-pending
KNOWN_GOOD=$D/storage/pocket-ui/known-good

say() { echo "$(date '+%H:%M:%S' 2>/dev/null) $*" >>"$LOG" 2>/dev/null; }

# A picture of pocket-fb (it knows the screen's page and format); without pocket-fb a failure picture is the raw frame, else a grey
# screen. The "starting" picture is only ever pocket-fb's: there is nothing to say about a start that has no bridge.
show() { # name seconds
  name=$1; secs=${2:-0}
  if [ -x "$P/bin/pocket-fb" ] && [ -r "$P/msg/$name.png" ]; then
    LD_LIBRARY_PATH="$P/lib:$FW/lib" "$P/bin/pocket-fb" --picture "$P/msg/$name.png" --hold "$secs" --fb "$FB" ${BRICK_FB_MODE:+--mode "$BRICK_FB_MODE"} >>"$LOG" 2>&1 && return 0
  fi
  [ "$name" = starting ] && return 0
  [ -w "$FBSYS/pan" ] && echo 0,0 >"$FBSYS/pan" 2>/dev/null
  if [ -r "$P/msg/$name.raw" ]; then
    cat "$P/msg/$name.raw" >"$FB" 2>/dev/null
  else
    head -c "${BRICK_FB_BYTES:-3145728}" /dev/zero | tr '\000' '\200' >"$FB" 2>/dev/null
  fi
  [ "$secs" -gt 0 ] 2>/dev/null && sleep "$secs"
}

# A version that an update just installed and that has not shown its first picture yet (update-pending names it): boot.sh puts
# the previous one back. Nothing on screen waits for ten minutes for that.
pending() { [ -f "$PENDING" ] && [ "$(cat "$PENDING" 2>/dev/null)" = "$(cat "$P/RELEASE" 2>/dev/null)" ]; }

# The version in app/ has never run STABLE_SECS on this card, and there is one to go back to.
unproven() { [ -f "$D/app.previous/start.sh" ] && [ "$(cat "$KNOWN_GOOD" 2>/dev/null)" != "$(cat "$P/RELEASE" 2>/dev/null)" ]; }

# Stops after a failure picture: it stays up, then the Brick switches off so the battery is not emptied in a bag.
give_up() { # picture
  if pending; then
    say "giving up on $(cat "$P/RELEASE" 2>/dev/null), just installed: back to the previous version ($1)"
    undo_mounts
    exit 76
  fi
  say "giving up: $1"
  show "$1" "$SHOW_FOR"
  say "showed $1 for $SHOW_FOR s, powering off"
  sync
  [ -n "${BRICK_NO_POWEROFF:-}" ] || poweroff
  exit 1
}

# The binds of step 4, undone in reverse (an update restarts this script from the new program folder, whose cores are another folder).
undo_mounts() {
  [ -n "${BRICK_NO_MOUNT:-}" ] && return 0
  for s in gb gbc gba nes snes psx megadrive mastersystem gamegear ngp ngpc wonderswan wonderswancolor pcengine atarilynx atari2600 atari7800 fbn neogeo cps1 cps2 cps3 n64 psp dreamcast; do umount -l "$STORAGE/.config/pocket-ui/covers/$s" 2>/dev/null; done
  for m in "$STORAGE/cores" "$STORAGE/roms/savestates" "$STORAGE/roms" "$STORAGE"; do umount -l "$m" 2>/dev/null; done
}

mkdir -p "$D/logs" 2>/dev/null
[ -f "$LOG" ] && [ "$(wc -c <"$LOG" 2>/dev/null || echo 0)" -gt 200000 ] && mv -f "$LOG" "$LOG.old"
say "=== boot via $ENTRY, $(cat "$P/VERSION" 2>/dev/null) battery $(cat "$BATTERY/capacity" 2>/dev/null)% $(cat "$BATTERY/status" 2>/dev/null)"
show starting "$SHOW_FOR" &
STARTING=$!
# A card copied over one of the older layout (before app/, up to brick-7f95a8fd) keeps that layout's program files beside app/: never
# used, and pocket-ui/VERSION would name the old version. Only those go; storage/, logs/, authorized_keys and Roms stay.
if [ -f "$D/bin/pocket-ui" ] && [ -f "$P/bin/pocket-ui" ]; then
  for f in bin lib cores msg hwroot licences sources VERSION BUILD-INFO.txt asoundrc es_systems.cfg; do rm -rf "${D:?}/$f"; done
  say "removed the older card layout's program files beside app/"
fi

# --- 2. is everything there ---------------------------------------------------------------------------------------
missing=
for f in bin/pocket-ui bin/pocket-core bin/pocket-fb bin/brick-hw lib/libwayland-client.so.0 lib/libffi.so.8 msg/starting.png es_systems.cfg asoundrc; do
  [ -e "$P/$f" ] || missing="$missing $f"
done
for c in gambatte mgba nestopia snes9x pcsx_rearmed gpsp fceumm picodrive beetle_ngp beetle_wswan beetle_pce_fast handy stella prosystem fbneo mupen64plus_next ppsspp flycast gearsystem beetle_supergrafx pokemini beetle_vb fake08 tic80 melondsds dosbox_pure scummvm yabause; do [ -e "$P/cores/${c}_libretro.so" ] || missing="$missing cores/${c}_libretro.so"; done
if [ -n "$missing" ]; then
  say "missing:$missing"
  kill "$STARTING" 2>/dev/null; wait "$STARTING" 2>/dev/null
  give_up missing
fi

# --- 3. the hardware ----------------------------------------------------------------------------------------------
# TrimUI's own runtrimui-original.sh has done, before it runs the card's MainUI: the 5 V rail, rumble off, the DIP-switch pin, syslogd,
# bluetoothd, the LED effect, the USB mode, and it has STOPPED wpa_supplicant and udhcpc and put wlan0 down when its own setting says
# Wi-Fi is off (/mnt/UDISK/system.json "wifi":0). It has NOT started keymon, trimui_inputd, scened, trimui_btmanager or hardwareservice
# (it looks for them in the card's trimui/app, where we have none). NextUI's start script does none of that before the updater hook.
# So the same set-up runs here for both (all of it harmless twice), and what neither does for us is done below.
echo 107 >/sys/class/gpio/export 2>/dev/null; echo -n out >/sys/class/gpio/gpio107/direction 2>/dev/null
echo -n 1 >/sys/class/gpio/gpio107/value 2>/dev/null            # the 5 V rail (the pad's board)
echo 227 >/sys/class/gpio/export 2>/dev/null; echo -n out >/sys/class/gpio/gpio227/direction 2>/dev/null
echo -n 0 >/sys/class/gpio/gpio227/value 2>/dev/null            # rumble off
if [ -x "$FW/bin/trimui_inputd" ] && ! pidof trimui_inputd >/dev/null 2>&1; then
  mkdir -p /tmp/trimui_inputd
  ( cd "$FW/bin" && LD_LIBRARY_PATH="$FW/lib" exec ./trimui_inputd ) >>"$LOG" 2>&1 &   # the pad: "TRIMUI Player1" appears after it
  say "trimui_inputd started"
else
  say "trimui_inputd: $(pidof trimui_inputd 2>/dev/null || echo 'not found')"
fi
echo 0 >/sys/class/led_anim/effect_enable 2>/dev/null
# The driver's raw picture glows on past the effects (03.11.): black from the start, pocket-ui puts the choice on.
echo 0 >/sys/class/led_anim/anim_frames_enable 2>/dev/null
[ -e /sys/class/led_anim/frame_hex ] && printf '%s' '000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 ' >/sys/class/led_anim/frame_hex 2>/dev/null
# Wi-Fi: on, unless pocket-ui's own switch put it off (the stand-in wifictl keeps that in $STATE/wifi-off). Not waited for.
[ -e "$STATE/wifi-off" ] || [ ! -x "$P/hwroot/usr/bin/wifictl" ] || { BRICK_STATE=$STATE "$P/hwroot/usr/bin/wifictl" enable >>"$LOG" 2>&1 & }

# --- 4. /storage --------------------------------------------------------------------------------------------------
# pocket-ui looks for everything below /storage (ROCKNIX's layout). The card's folders are bound there. The folder
# /storage itself is made once in the Brick's own flash (an empty folder, and it stays empty); everything else is the card.
# authorized_keys: SSH keys for Remote access, empty until the player puts some in (made here, not in the card zip, so copying a new
# card over this one keeps them).
[ -e "$D/authorized_keys" ] || : >"$D/authorized_keys" 2>/dev/null
mkdir -p "$D/storage/roms" "$D/storage/.config/pocket-ui" "$D/storage/.config/emulationstation" "$D/storage/pocket-ui" "$D/storage/cores" "$CARD/Roms" "$CARD/Saves/savestates"
for s in gb gbc gba nes snes psx megadrive mastersystem gamegear ngp ngpc wonderswan wonderswancolor pcengine atarilynx atari2600 atari7800 fbn neogeo cps1 cps2 cps3 n64 psp dreamcast fds sg-1000 segacd sega32x pcenginecd supergrafx pokemini virtualboy pico-8 tic-80 nds pc scummvm saturn; do mkdir -p "$CARD/Roms/$s"; done
# Engine data is checked once per release; unchanged files are read, never recopied. The helper uses idle I/O, a
# 2 MiB/s write budget, chunk fsyncs and atomic replacement. It invalidates the marker before changing any file,
# so a failed update's rollback also repairs partially seeded new data from the old app's bundled files.
if [ -d "$P/bios-data" ] && [ "$(cat "$CARD/Roms/bios/.pocket-ui-data" 2>/dev/null)" != "$(cat "$P/RELEASE" 2>/dev/null)" ]; then
  if mkdir -p "$CARD/Roms/bios" &&
    LD_LIBRARY_PATH="$P/lib:$FW/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" nice -n 19 "$P/bin/pocket-ui" --brick-seed-data "$P/bios-data" "$CARD/Roms/bios" "$(cat "$P/RELEASE")" >>"$LOG" 2>&1; then
    say "BIOS-folder data of $(cat "$P/RELEASE" 2>/dev/null) checked in Roms/bios"
  else
    say "BIOS-folder data could not be prepared"
    kill "$STARTING" 2>/dev/null; wait "$STARTING" 2>/dev/null
    give_up missing
  fi
fi
if [ -z "${BRICK_NO_MOUNT:-}" ]; then
  [ -d "$STORAGE" ] || mkdir "$STORAGE" 2>>"$LOG"
  if grep -q " $STORAGE " /proc/mounts 2>/dev/null; then
    say "$STORAGE is already mounted"
  else
    mount --bind "$D/storage" "$STORAGE" >>"$LOG" 2>&1 || say "bind of $STORAGE failed"
    mount --bind "$CARD/Roms" "$STORAGE/roms" >>"$LOG" 2>&1 || say "bind of Roms failed"
  fi
  mkdir -p "$CARD/Saves/savestates" "$STORAGE/roms/savestates"
  mount --bind "$CARD/Saves/savestates" "$STORAGE/roms/savestates" >>"$LOG" 2>&1 || say "bind of Saves failed"
  mount --bind "$P/cores" "$STORAGE/cores" >>"$LOG" 2>&1 || say "bind of cores failed"
  # Covers lie next to the games: Roms/<system>/.media/<game file name without extension>.png is where pocket-ui keeps (and looks for) its box art
  # (`covers.rs`: <data>/covers/<system>/<stem>.png), so each system's .media is bound there. Pictures from another card's .media folders work as they are.
  for s in gb gbc gba nes snes psx megadrive mastersystem gamegear ngp ngpc wonderswan wonderswancolor pcengine atarilynx atari2600 atari7800 fbn neogeo cps1 cps2 cps3 n64 psp dreamcast fds sg-1000 segacd sega32x pcenginecd supergrafx pokemini virtualboy pico-8 tic-80 nds pc scummvm saturn; do
    mkdir -p "$CARD/Roms/$s/.media" "$STORAGE/.config/pocket-ui/covers/$s"
    mount --bind "$CARD/Roms/$s/.media" "$STORAGE/.config/pocket-ui/covers/$s" >>"$LOG" 2>&1 || say "bind of the $s covers failed"
  done
  # Never write on: without these binds /storage would be the Brick's own flash, and saves would end up there.
  for m in "$STORAGE" "$STORAGE/roms" "$STORAGE/roms/savestates"; do
    grep -q " $m " /proc/mounts 2>/dev/null || { say "$m is not a mount of the card: stopping"; give_up crashed; }
  done
fi
mkdir -p "$STORAGE/roms/bios" "$STORAGE/roms/savestates" "$STORAGE/pocket-ui/licences" 2>>"$LOG"
mkdir -p "$STORAGE/home" && cp -f "$P/asoundrc" "$STORAGE/home/.asoundrc" 2>>"$LOG"     # ALSA's "pipewire" device = the sound chip
[ -f "$STORAGE/pocket-ui/licences/README.txt" ] || cp -R "$P"/licences/. "$STORAGE/pocket-ui/licences/" 2>/dev/null   # Settings > About names this folder
cp -f "$P/es_systems.cfg" "$STORAGE/.config/emulationstation/es_systems.cfg" 2>>"$LOG"

# --- 5. stand-ins for the helpers ---------------------------------------------------------------------------------
# pocket-ui's hardware seam looks below a root folder (system_ctl RealHw::with_root; POCKETUI_HW_ROOT): sysfs files for the
# backlight and battery, and programs in usr/bin. The programs are ours (hwroot/usr/bin), not NextUI's.
HW=$RUN/hwroot
rm -rf "$HW"; mkdir -p "$HW/sys/class/backlight" "$HW/sys/class/power_supply" "$HW/usr/bin" "$HW/run/systemd/system" "$HW/storage" "$HW/etc" "$HW/usr/share"
# The clock: the card carries its own zone data (pocket-ui/zoneinfo, IANA, public domain), because the firmware's may hold
# few zones or none. The choice is the zone's NAME on the card (a FAT card takes no symlink); the links pocket-ui's hardware
# seam reads (usr/share/zoneinfo, etc/localtime) are rebuilt here, in /tmp, and TZ points every program we start at the
# zone file itself. No name yet: the firmware's own zone (its /etc/localtime link), else UTC. A zone the card has no file
# for falls back the same way, so a bad name can never leave the clock without a zone. The firmware's link goes through
# /tmp (/etc/localtime -> /tmp/localtime -> /usr/share/zoneinfo/Europe/Berlin): followed until the zone's name shows.
ZONEINFO=$P/zoneinfo
[ -d "$ZONEINFO" ] || ZONEINFO=${BRICK_FW_ZONEINFO:-/usr/share/zoneinfo}
fw_zone() {
  z=${BRICK_FW_LOCALTIME:-/etc/localtime}; n=0
  while [ -L "$z" ] && [ $n -lt 8 ]; do
    case "$z" in *zoneinfo/*) break ;; esac
    t=$(readlink "$z" 2>/dev/null) || break
    case "$t" in /*) z=$t ;; *) z=${z%/*}/$t ;; esac
    n=$((n + 1))
  done
  case "$z" in *zoneinfo/*) echo "${z##*zoneinfo/}" ;; *) echo UTC ;; esac
}
ZONE=$(head -n 1 "$STATE/timezone" 2>/dev/null)
case "$ZONE" in *[!A-Za-z0-9/_+-]* | "") ZONE=$(fw_zone) ;; esac
[ -f "$ZONEINFO/$ZONE" ] || { say "time zone $ZONE is not in $ZONEINFO, falling back"; ZONE=$(fw_zone); }
[ -f "$ZONEINFO/$ZONE" ] || ZONE=UTC
if [ -f "$ZONEINFO/$ZONE" ]; then
  ln -s "$ZONEINFO" "$HW/usr/share/zoneinfo"
  ln -s "../usr/share/zoneinfo/$ZONE" "$HW/etc/localtime"
  export TZ=$ZONEINFO/$ZONE
  say "time zone: $ZONE (zone data $ZONEINFO)"
else
  say "no zone data on the card or in the firmware: the clock page stays empty"
fi
rm -f "$STATE/ntp-synced" 2>/dev/null  # network time has not set this boot's clock yet (the timedatectl stand-in reads this)
ln -s "$BATTERY" "$HW/sys/class/power_supply/battery"
ln -s "${BRICK_RFKILL:-/sys/class/rfkill}" "$HW/sys/class/rfkill"
ln -s "$STORAGE/.config" "$HW/storage/.config"
for f in "$P"/hwroot/usr/bin/*; do ln -s "$f" "$HW/usr/bin/$(basename "$f")"; done
chmod +x "$P"/hwroot/usr/bin/* 2>/dev/null
SAVED_BRIGHT=$(cat "$STATE/brick-brightness" 2>/dev/null); SAVED_BRIGHT=${SAVED_BRIGHT:-70}
"$P/bin/brick-hw" watch "$HW/sys/class/backlight/panel" "$SAVED_BRIGHT" >>"$LOG" 2>&1 &
HWPID=$!
# Power, held for 6 s (what keymon's "6 seconds power off" was): the game saves and quits, then the Brick powers off. Shorter presses are
# pocket-ui's own: short = sleep, long = power menu (Settings > System > Sleep & power).
BRICK_POWER_CMD="$P/hwroot/usr/bin/brick-power-off" BRICK_POWER_LOG=$LOG "$P/bin/brick-hw" power-watch 6 >>"$LOG" 2>&1 &
POWERPID=$!
export BRICK_STATE=$STATE BRICK_HW=$P/bin/brick-hw BRICK_SPEAKER=${BRICK_SPEAKER:-/sys/class/speaker/mute}
export BRICK_SLIDER_CFG="$D/brick-slider" BRICK_SLIDER_MUTED=${BRICK_SLIDER_MUTED:-1}
[ -x "$P/hwroot/usr/bin/volume" ] && "$P/hwroot/usr/bin/volume" restore >>"$LOG" 2>&1
# Network time, one try in the background (Wi-Fi may need a few seconds; the clock page can ask again). Without a right
# clock the TLS of every download fails, so the try matters more than the display does. Never blocks the start.
{ sleep "${BRICK_NTP_DELAY:-5}"; "$P/hwroot/usr/bin/systemctl" restart systemd-timesyncd && say "network time set the clock" || say "network time did not answer (ntpd or no network)"; } >>"$LOG" 2>&1 &

# After the volume is restored, so a slider found in the muted position keeps the speaker off.
# The mute slider (gpio243): the position that means "muted" is BRICK_SLIDER_MUTED: 1, the slider down (read on the real Brick with its owner, 03.11.; 0 is up),
# and the sound chip's switches are `volume mute|unmute`. Settings > Lights & buttons > Mute slider writes "mute" or "nothing" to $D/brick-slider.
BRICK_SLIDER_CFG="$D/brick-slider" BRICK_SLIDER_CMD="$P/hwroot/usr/bin/volume" BRICK_SLIDER_MUTED=${BRICK_SLIDER_MUTED:-1} "$P/bin/brick-hw" slider-watch >>"$LOG" 2>&1 &
SLIDERPID=$!
# The network keeper: a lost default route on wlan0 gets its lease renewed (brick-net; seen on the real Brick after a Wi-Fi restart).
BRICK_NET_LOG="$LOG" "$P/hwroot/usr/bin/brick-net" watch >/dev/null 2>&1 &
NETPID=$!

# The network parts (docs/TRIMUI-BRICK-REMOTE.md). curl (with its own CA bundle) is the card's, first on PATH for pocket-ui (covers,
# updates); an SSH shell finds tailscale, curl and scp in $RUN/bin (dropbear's PATH starts there).
mkdir -p "$RUN/bin"
for t in curl; do [ -x "$P/bin/$t" ] && ln -sf "$P/bin/$t" "$HW/usr/bin/$t" && ln -sf "$P/bin/$t" "$RUN/bin/$t"; done
[ -x "$P/bin/dropbearmulti" ] && ln -sf "$P/bin/dropbearmulti" "$RUN/bin/scp" && ln -sf "$P/bin/dropbearmulti" "$RUN/bin/dropbearkey"
ln -sf "$P/hwroot/usr/bin/tailscale" "$RUN/bin/tailscale"
export BRICK_APP=$P BRICK_DATA=$D BRICK_RUN=$RUN
export POCKETUI_BRICK_BLUETOOTHCTL=$HW/usr/bin/brick-bluetoothctl
# Pairing data belongs to this card; the service uses the firmware BlueZ and its own agent.
[ ! -e "$STATE/bluetooth-on" ] || { "$HW/usr/bin/brick-bluetooth" start >>"$LOG" 2>&1 & }
# Tailscale comes up by itself when it was on (Settings > Tailscale writes tailscale.up=1, as on ROCKNIX), not waited for: it joins
# when Wi-Fi is there. Already running (pocket-ui restarted after an update) it stays as it is, and so does an SSH session.
if grep -q '^tailscale.up=1' "$D/storage/.config/system/configs/system.cfg" 2>/dev/null; then
  "$HW/usr/bin/brick-tailscale" start >>"$LOG" 2>&1 &
fi
# Save sync comes up by itself when it was on (Settings > Sync writes syncthing.enabled=1, as on ROCKNIX). It needs the password pocket-ui made
# when it was switched on; without one it does not start (brick-syncthing says so in its log).
if grep -q '^syncthing.enabled=1' "$D/storage/.config/system/configs/system.cfg" 2>/dev/null; then
  "$HW/usr/bin/brick-syncthing" start >>"$LOG" 2>&1 &
fi
# Remote access is off after a restart (as on ROCKNIX), unless this is a developer card (storage/.config/pocket-ui/dev-ssh).
[ -e "$STATE/dev-ssh" ] && { "$HW/usr/bin/brick-ssh" start >>"$LOG" 2>&1 & }

# --- 6. the bridge and pocket-ui ----------------------------------------------------------------------------------
WL=$RUN/wayland-0; IPC=$RUN/sway.sock
export LD_LIBRARY_PATH=$P/lib:$FW/lib
export PATH=$HW/usr/bin:$PATH
export WAYLAND_DISPLAY=$WL SWAYSOCK=$IPC XDG_RUNTIME_DIR=$RUN
export POCKETUI_DEVICE=brick POCKETUI_HW_ROOT=$HW POCKETUI_BACKLIGHT=$HW/sys/class/backlight/panel POCKETUI_BATTERY=$BATTERY
export POCKETUI_HOST_BIN=$P/bin/pocket-core POCKETUI_POWER_KEY=1
export POCKETUI_BRICK_STANDALONE=1 POCKET_CORE_SOFTWARE=1 HOME=$STORAGE/home BRICK_FB=$P/bin/pocket-fb POCKETUI_TAILSCALED_SOCKET=$RUN/tailscaled.sock   # BRICK_FB: the grim stand-in asks pocket-fb for the screen
# Settings > Tailscale drives the card's tailscaled through this CLI; Updates fetch the card's channel (app/update-url) through Tailscale's
# local proxy, and install into this pocket-ui folder.
export POCKETUI_TAILSCALE=$HW/usr/bin/tailscale POCKETUI_TS_PROXY=http://${BRICK_TS_PROXY:-127.0.0.1:1055} POCKETUI_BRICK_CARD=$D
[ -s "$P/update-url" ] && POCKETUI_UPDATE_URL=$(head -n 1 "$P/update-url") && export POCKETUI_UPDATE_URL
[ -n "${POCKETUI_LANG:-}" ] && export POCKETUI_LANG
mkdir -p "$RUN"

# The GPU draws transitions and windows through pocket-fb --gl (docs/TRIMUI-BRICK.md, "The GPU"); on this card nothing else holds the GPU,
# so it is on by default (POCKET_BRICK_GL=0 or a file pocket-ui/gl-off turns it off). brick-egl may crash or hang inside the driver, so
# it is asked once in a process of its own with a time limit; if that fails, or the bridge or pocket-ui ends at once with the GPU on,
# the CPU copy runs and boot.log says so.
FBGL=
# gl-off is the player's choice, so it lives next to storage/ (D), not in the program folder an update replaces.
if [ "${POCKET_BRICK_GL:-1}" = 0 ] || [ -e "$D/gl-off" ]; then say "GL: off by choice: CPU copy"
elif [ ! -x "$P/bin/brick-egl" ]; then say "GL: no bin/brick-egl: CPU copy"
else
  "$P/bin/brick-egl" --probe --window "${POCKET_GL_WINDOW:-null}" >>"$LOG" 2>&1 &
  gp=$!
  ( sleep "${BRICK_GL_PROBE_SECONDS:-10}"; kill -KILL "$gp" 2>/dev/null ) >/dev/null 2>&1 &
  gk=$!
  wait "$gp"; gc=$?
  kill "$gk" 2>/dev/null
  if [ "$gc" = 0 ]; then FBGL="--gl --gl-window ${POCKET_GL_WINDOW:-null}"; say "GL: the probe works, the screen goes through the GPU"
  else say "GL: the probe failed (exit $gc): CPU copy"; fi
fi

fast=0; window_start=$(date +%s); crashes=0
while :; do
  # The "starting" picture goes before the bridge takes the screen (the picture's end puts the screen back as it was).
  if [ -n "$STARTING" ]; then kill "$STARTING" 2>/dev/null; wait "$STARTING" 2>/dev/null; STARTING=; fi
  rm -f "$WL" "$IPC" "$WL.lock"
  "$P/bin/pocket-fb" --wayland "$WL" --ipc "$IPC" --fb "$FB" ${BRICK_FB_MODE:+--mode "$BRICK_FB_MODE"} $FBGL >>"$LOG" 2>&1 &
  FBPID=$!
  n=0; while [ ! -S "$WL" ] || [ ! -S "$IPC" ]; do
    n=$((n + 1)); [ $n -gt 50 ] && break; sleep 0.1
  done
  if [ ! -S "$WL" ] || [ ! -S "$IPC" ]; then
    say "pocket-fb did not come up (sockets missing)"
    kill "$FBPID" 2>/dev/null; wait "$FBPID" 2>/dev/null
    if [ -n "$FBGL" ]; then say "GL: the bridge stopped with it: CPU copy"; FBGL=; continue; fi
    rc=99; started=$(date +%s)
  else
    started=$(date +%s)
    say "pocket-ui starts"
    # STABLE_SECS into the run this version becomes the card's known good one (ended early, the timer goes with it).
    ( trap 'kill "$t" 2>/dev/null; exit 0' TERM; sleep "$STABLE_SECS" & t=$!; wait "$t" && cp "$P/RELEASE" "$KNOWN_GOOD.new" 2>/dev/null \
      && mv -f "$KNOWN_GOOD.new" "$KNOWN_GOOD" && say "$(cat "$P/RELEASE" 2>/dev/null) ran $STABLE_SECS s: the known good version on this card" ) &
    GOODPID=$!
    "$P/bin/pocket-ui" >>"$LOG" 2>&1
    rc=$?
    kill "$GOODPID" 2>/dev/null; wait "$GOODPID" 2>/dev/null
    say "pocket-ui ended with $rc"
    kill "$FBPID" 2>/dev/null; wait "$FBPID" 2>/dev/null
  fi
  # An update was installed (pocket-ui exits 75): boot.sh starts the new program folder. Tailscale and an SSH session keep running.
  if [ "$rc" = 75 ]; then
    kill "$HWPID" "$POWERPID" "$SLIDERPID" "$NETPID" 2>/dev/null
    undo_mounts
    say "pocket-ui installed an update: restarting from the program folder"
    exit 75
  fi
  now=$(date +%s)
  gl_dropped=
  if [ $((now - started)) -lt "$FAST_END" ]; then
    fast=$((fast + 1))
    [ -n "$FBGL" ] && { say "GL: pocket-ui ended at once with the GPU on: CPU copy from now on"; FBGL=; gl_dropped=1; }
  else fast=0; fi
  # A just-installed version that ends before it confirmed itself did not start: the way back, at once (give_up ends with 76).
  # Only the GPU's own way back (CPU copy) gets one more try first.
  if pending && [ -z "$gl_dropped" ]; then
    kill "$HWPID" "$POWERPID" "$SLIDERPID" "$NETPID" 2>/dev/null
    give_up crashed
  fi
  # A crash (not a restart pocket-ui asked for, exit 0) before STABLE_SECS; the GPU's way back above doesn't count.
  [ "$rc" != 0 ] && [ -z "$gl_dropped" ] && [ $((now - started)) -lt "$STABLE_SECS" ] && crashes=$((crashes + 1))
  # A version that confirmed itself but never ran long on this card and keeps ending (three crashes, or the three quick ends
  # that would show the failure picture): the previous one instead.
  if { [ "$crashes" -ge "$MAX_FAST" ] || [ "$fast" -ge "$MAX_FAST" ]; } && unproven; then
    kill "$HWPID" "$POWERPID" "$SLIDERPID" "$NETPID" 2>/dev/null
    say "$(cat "$P/RELEASE" 2>/dev/null) keeps ending and never ran $STABLE_SECS s on this card: back to the previous version"
    mkdir -p "$D/storage/pocket-ui" && cp "$P/RELEASE" "$PENDING" 2>/dev/null
    undo_mounts
    exit 76
  fi
  # Three quick endings in a row: the picture, not a fourth try. A long run in between starts the count again.
  if [ "$fast" -ge "$MAX_FAST" ]; then
    kill "$HWPID" "$POWERPID" "$SLIDERPID" "$NETPID" 2>/dev/null
    give_up crashed
  fi
  sleep 1
done

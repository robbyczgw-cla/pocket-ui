#!/bin/sh
# The device side of scripts/rocknix-compare.sh (docs/COMPARE-ROCKNIX.md). Runs on the Mini V2 as the unit
# pocket-compare (systemd-run), so that a dropped SSH connection (a deep sleep takes Wi-Fi down) does not end it.
#
#   sh rocknix-compare-device.sh            in the folder the files were unpacked to (COMPARE_WORK), with run.env
#
# run.env (written by rocknix-compare.sh): UIS="es pocket-ui", DRIVE_ARGS="--scenarios ... --reps 3", PLAN=plan.json
#
# Whatever happens (an error, `systemctl stop pocket-compare`, SIGHUP), the trap puts back:
#   - no game and no UI of the run left: essway stopped, retroarch, pocket-core and the showcase's wf-recorder ended
#     (pidof, never pkill -f)
#   - pocket-ui's data folder (/storage/.config/pocket-ui, without the big caches: covers, snaps, titles, wikipedia,
#     store, identify, logs), RetroArch's config (retroarch.cfg, the core options, config/, playlists/),
#     EmulationStation's settings files, system.cfg, the saves and states of the test games and their systems'
#     gamelist.xml, pocket-ui's dev.env
#   - the speaker's mute and volume, the backlight's brightness, no RTC alarm left
#   - pocket-ui as the UI again: the ES-only mark removed, essway restarted
# The copies stay in /storage/.cmpbak/<time> (the newest three are kept). Exit code in $COMPARE_WORK/rc.
# Never touches the boot path, the bootloader, the partitions; system.cfg only as EmulationStation itself writes it.
set -u
W=${COMPARE_WORK:-$(cd "$(dirname "$0")/.." && pwd)}
cd "$W" || exit 2
UIS="es pocket-ui"
DRIVE_ARGS=""
PLAN=$W/scripts/rocknix-compare-plan.json
[ -f "$W/run.env" ] && . "$W/run.env"
case $PLAN in /*) ;; *) PLAN=$W/$PLAN ;; esac
OUT=$W/out
# COMPARE_ROOT and COMPARE_PY are for scripts/test-rocknix-compare.sh (a made-up root, a stand-in for the driver).
P=${COMPARE_ROOT:-}
PY=${COMPARE_PY:-python3 $W/scripts/rocknix-compare.py}
DATA=$P/storage/.config/pocket-ui
RA=$P/storage/.config/retroarch
ESCFG=$P/storage/.config/emulationstation
SYSCFG=$P/storage/.config/system/configs/system.cfg
DEVENV=$P/storage/pocket-ui/dev.env
ES_ONLY=$P/tmp/pocket-ui.es-only
ALARM=$P/sys/class/rtc/rtc0/wakealarm
ROMS=$P/storage/roms
BACKLIGHT=$P/sys/class/backlight
STAMP=$(date +%Y%m%d-%H%M%S)
BAK=$P/storage/.cmpbak/$STAMP
BIG="covers snaps titles wikipedia store identify logs"
export XDG_RUNTIME_DIR=/var/run/0-runtime-dir WAYLAND_DISPLAY=wayland-1 SWAYSOCK=/var/run/0-runtime-dir/sway-ipc.0.sock
rm -f "$W/rc"
mkdir -p "$OUT"
say() { echo "device: $*"; }

# --- what this run measures on ---------------------------------------------------------------------------------
facts() {
  {
    echo "{"
    echo "\"rocknix\": \"$(sed -n 's/^OS_VERSION="\(.*\)"/\1/p' "$P/etc/os-release")\","
    echo "\"pocket_ui_md5\": \"$(md5sum "$P/storage/pocket-ui/pocket-ui" 2>/dev/null | cut -d' ' -f1)\","
    echo "\"es_theme\": \"$(sed -n 's/.*name="ThemeSet" value="\([^"]*\)".*/\1/p' $ESCFG/es_settings.cfg 2>/dev/null | head -1)\","
    echo "\"system_cfg\": \"$(grep -E '^(global\.(autosave|bezel|shaderset|runahead|ratio|smooth)|(gbc|gba|snes)\.(core|emulator|autosave|bezel)|system\.(autohotkeys|suspendmode|power\.)|audio\.volume)' $SYSCFG 2>/dev/null | tr '\n' ';' | sed 's/"/\\"/g')\","
    echo "\"governors\": \"$(cat /sys/devices/system/cpu/cpufreq/policy*/scaling_governor 2>/dev/null | tr '\n' ' ')\","
    echo "\"battery\": \"$(for f in status capacity; do cat /sys/class/power_supply/*/$f 2>/dev/null | head -1; done | tr '\n' ' ')\","
    echo "\"brightness\": \"$(cat "$BACKLIGHT"/*/brightness 2>/dev/null | head -1)\","
    echo "\"mem_available_kb\": \"$(sed -n 's/^MemAvailable: *\([0-9]*\).*/\1/p' /proc/meminfo)\","
    echo "\"sway\": \"$(sway --version 2>/dev/null)\","
    echo "\"kernel\": \"$(uname -r)\""
    echo "}"
  } >"$OUT/device.json"
}

# --- copies ------------------------------------------------------------------------------------------------------
GAMES=$(python3 -c "import json,sys; p=json.load(open(sys.argv[1])); print(' '.join(f'{s}:{g}' for s, g in p['games'].items()))" "$PLAN")
# The test games' systems: every save next to a ROM, every state (a wrong press may start another game of the
# list; the restore deletes what is new, so a deleted or a new file is covered as well as a changed one), gamelist.xml.
game_files() {
  for sg in $GAMES; do
    s=${sg%%:*}
    find "$ROMS/$s" -maxdepth 1 -type f 2>/dev/null | grep -i -E '\.(srm|rtc|sav)$|/gamelist\.xml$'
    find "$ROMS/savestates/$s" -maxdepth 1 -type f 2>/dev/null
  done
}
data_entries() { # what of pocket-ui's data folder is copied and put back
  ls -A "$DATA" 2>/dev/null | while read -r e; do
    case " $BIG " in *" $e "*) continue ;; esac
    case $e in backup-*) ;; *) echo "$e" ;; esac
  done
}

# Archives are made from name lists (tar -T, one name a line): game files and RetroArch's core options have spaces.
backup() {
  mkdir -p "$BAK" || return 1
  data_entries >"$BAK/data-entries"
  if [ -s "$BAK/data-entries" ]; then
    tar -C "$DATA" -czf "$BAK/pocket-ui-data.tgz" -T "$BAK/data-entries" || return 1
    tar -tzf "$BAK/pocket-ui-data.tgz" >/dev/null || return 1
  fi
  : >"$BAK/ra-entries"
  for e in retroarch.cfg retroarch-core-options.cfg config playlists; do [ -e "$RA/$e" ] && echo "$e" >>"$BAK/ra-entries"; done
  for e in "$RA"/*.opt "$RA"/*.cfg; do [ -f "$e" ] && basename "$e" >>"$BAK/ra-entries"; done
  sort -u "$BAK/ra-entries" -o "$BAK/ra-entries"
  if [ -s "$BAK/ra-entries" ]; then
    tar -C "$RA" -czf "$BAK/retroarch.tgz" -T "$BAK/ra-entries" || return 1
  fi
  game_files | sort -u >"$BAK/game-files"
  { ls "$ESCFG"/*.cfg "$ESCFG"/*.xml 2>/dev/null; cat "$BAK/game-files"
    [ -f "$SYSCFG" ] && echo "$SYSCFG"; [ -f "$DEVENV" ] && echo "$DEVENV"; } | sed 's#^/##' >"$BAK/files"
  if [ -s "$BAK/files" ]; then
    tar -C / -czf "$BAK/files.tgz" -T "$BAK/files" || return 1
    tar -tzf "$BAK/files.tgz" >/dev/null || return 1
  fi
  [ -f "$DEVENV" ] && echo yes >"$BAK/had-devenv"
  pactl get-sink-mute @DEFAULT_SINK@ 2>/dev/null | awk '{print $2}' >"$BAK/mute"
  pactl get-sink-volume @DEFAULT_SINK@ 2>/dev/null | sed -n 's#.* \([0-9]*\)%.*#\1#p' | head -1 >"$BAK/volume"
  cat "$BACKLIGHT"/*/brightness 2>/dev/null | head -1 >"$BAK/brightness"
  # A screen dark in pocket-ui's light sleep reads 0; its level is in the dark file (src/sleep.rs). Put back 0 to a
  # pocket-ui restarted awake would leave it black: the restore takes that level then.
  cat "$P/tmp/pocket-ui.brightness" >"$BAK/dark-level" 2>/dev/null
  # Only the newest three copies stay.
  ls -d "$P"/storage/.cmpbak/*/ 2>/dev/null | sort -r | tail -n +4 | while read -r old; do rm -rf "$old"; done
  say "copies in $BAK"
}

RESTORED=0
restore() {
  [ "$RESTORED" = 1 ] && return
  RESTORED=1
  trap '' INT TERM HUP
  say "restoring"
  systemctl stop essway 2>/dev/null
  for p in $(pidof wf-recorder) $(pidof retroarch) $(pidof pocket-core) $(pidof emulationstation) $(pidof pocket-ui); do kill "$p" 2>/dev/null; done
  i=0; while [ -n "$(pidof retroarch)$(pidof pocket-core)$(pidof emulationstation)$(pidof pocket-ui)" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i+1)); done
  for p in $(pidof retroarch) $(pidof pocket-core) $(pidof emulationstation) $(pidof pocket-ui); do kill -9 "$p" 2>/dev/null; done
  # The recorder and the driver (this script's children).
  for p in $(pidof python3); do
    case "$(tr '\0' ' ' </proc/$p/cmdline 2>/dev/null)" in *rocknix-compare.py*) kill "$p" 2>/dev/null ;; esac
  done
  echo 0 >"$ALARM" 2>/dev/null
  if [ -d "$BAK" ]; then
    if [ -f "$BAK/pocket-ui-data.tgz" ]; then
      # What the run added at the top of the data folder goes; what was copied comes back as it was.
      ls -A "$DATA" 2>/dev/null | while read -r e; do
        case " $BIG " in *" $e "*) continue ;; esac
        case $e in backup-*) continue ;; esac
        rm -rf "${DATA:?}/$e"
      done
      tar -xzf "$BAK/pocket-ui-data.tgz" -C "$DATA" || say "COULD NOT put back $DATA (copy: $BAK)"
    fi
    if [ -f "$BAK/retroarch.tgz" ]; then
      while read -r e; do rm -rf "${RA:?}/$e"; done <"$BAK/ra-entries"
      tar -xzf "$BAK/retroarch.tgz" -C "$RA" || say "COULD NOT put back $RA (copy: $BAK)"
    fi
    # Saves and states the run made for the test games that were not there before go.
    game_files | sort -u | while read -r f; do grep -qxF "$f" "$BAK/game-files" || rm -f "$f"; done
    [ -f "$BAK/files.tgz" ] && { tar -xzf "$BAK/files.tgz" -C / || say "COULD NOT put back the files of $BAK/files"; }
    [ -f "$BAK/had-devenv" ] || rm -f "$DEVENV"
    m=$(cat "$BAK/mute" 2>/dev/null); v=$(cat "$BAK/volume" 2>/dev/null)
    [ -n "$v" ] && pactl set-sink-volume @DEFAULT_SINK@ "$v%" 2>/dev/null
    case $m in yes) pactl set-sink-mute @DEFAULT_SINK@ 1 ;; *) pactl set-sink-mute @DEFAULT_SINK@ 0 ;; esac 2>/dev/null
    b=$(cat "$BAK/brightness" 2>/dev/null)
    [ "$b" = 0 ] && [ -s "$BAK/dark-level" ] && b=$(cat "$BAK/dark-level")
    [ -n "$b" ] && for f in "$BACKLIGHT"/*/brightness; do echo "$b" >"$f"; done 2>/dev/null
  fi
  sync
  rm -f "$ES_ONLY"
  systemctl reset-failed essway 2>/dev/null
  systemctl restart essway
  journalctl ${INVOCATION_ID:+_SYSTEMD_INVOCATION_ID=$INVOCATION_ID} -u pocket-compare -o cat --no-pager >"$OUT/run.log" 2>/dev/null
  (cd "$W" && tar -czf "$W/out.tgz" out 2>/dev/null)
  say "restored; pocket-ui is the UI again"
}

RC=0
finish() {
  rc=$?
  [ "$RC" = 0 ] && RC=$rc
  restore
  echo "$RC" >"$W/rc"
}
trap 'RC=3; exit 3' INT TERM HUP
trap finish EXIT

say "start $STAMP: UIs $UIS, $DRIVE_ARGS"
facts
backup || { say "the copies failed: nothing measured"; RC=2; exit 2; }
pactl set-sink-mute @DEFAULT_SINK@ 1 2>/dev/null

# Which source key gives which DualSense button: with no UI on the screen (each press would act).
systemctl stop essway
sleep 1
if $PY probe --out "$OUT/codes.json" >"$OUT/probe.json" 2>"$OUT/probe.log"; then
  CODES="--codes $OUT/codes.json"
elif grep -q '"codes": {}' "$OUT/probe.json" 2>/dev/null; then
  # Nothing came out of InputPlumber's DualSense: it grabs its source, so presses go into the DualSense itself.
  say "probe: no key passed through InputPlumber; writing into its DualSense instead (out/probe.json)"
  CODES="--inject target"
else
  say "probe incomplete (out/probe.json): the default codes for what it did not find"
  CODES=""
  [ -s "$OUT/codes.json" ] && CODES="--codes $OUT/codes.json"
fi

# The wake test's time asleep (drive's --asleep, 15 s by default), for pocket-ui's suspend command below.
ASLEEP=$(echo " $DRIVE_ARGS " | sed -n 's/.* --asleep \([0-9][0-9]*\) .*/\1/p')
ASLEEP=${ASLEEP:-15}
for ui in $UIS; do
  if [ "$ui" = pocket-ui ]; then
    # pocket-ui's own suspend sets its RTC alarm for the switch-off after sleeping (3 h): the run's suspend command
    # sets the alarm the wake test needs after it, as scripts/latency.sh --wake does (docs/SLEEP.md).
    { [ -f "$BAK/had-devenv" ] && grep -v -e '^POCKETUI_SUSPEND_CMD=' -e '^POCKETUI_SLEEP_MINUTE_SECS=' "$DEVENV"
      echo "POCKETUI_SUSPEND_CMD=\"echo 0 >/sys/class/rtc/rtc0/wakealarm; echo +$ASLEEP >/sys/class/rtc/rtc0/wakealarm; systemctl suspend\""
      # Light sleep (3 min by default) would stop the game in the 5-minute play windows; RetroArch has none.
      echo "POCKETUI_SLEEP_MINUTE_SECS=3600"
    } >"$DEVENV.new" && mv "$DEVENV.new" "$DEVENV"
  fi
  say "measuring $ui"
  # shellcheck disable=SC2086
  $PY drive --ui "$ui" --plan "$PLAN" $CODES --out "$OUT/$ui" $DRIVE_ARGS || { r=$?; say "$ui: drive ended with $r"; [ $r -gt "$RC" ] && RC=$r; }
done
exit "$RC"

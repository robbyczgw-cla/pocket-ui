#!/bin/sh
# Saturn check on the device (docs/HOST.md, "Sega Saturn"): runs deploy/syscheck-play.sh on a Saturn disc
# with the sound muted, and puts everything back afterwards.
#   saturn-check.sh ROM [STEP...]      (BENCH=N for a benchmark run, BIN= the pocket-core to use, CORE= another core)
# Without a Saturn BIOS on the device it can still run the core in HLE mode: HLE=1 creates an empty
# placeholder roms/bios/saturn_bios.bin for the run (yabasanshiro only checks that one exists before it
# looks at its "Force HLE BIOS" option, and never reads it then) and removes it again; the option itself
# comes from a test data dir (POCKETUI_DATA), so the user's pocket-ui settings are not touched.
ROM=$1; shift
HERE=$(cd "$(dirname "$0")" && pwd)
BIOS=/storage/roms/bios/saturn_bios.bin
TESTDATA=/storage/pocket-test/data
MUTED=$(wpctl get-volume @DEFAULT_AUDIO_SINK@ | grep -c MUTED)
PLACED=
restore() {
  pidof pocket-core >/dev/null && timeout 3 sh -c 'echo quit > /tmp/pocket-core.cmd'
  [ -n "$PLACED" ] && [ ! -s "$BIOS" ] && rm -f "$BIOS"
  [ "$MUTED" = 0 ] && wpctl set-mute @DEFAULT_AUDIO_SINK@ 0
}
trap restore EXIT INT TERM
wpctl set-mute @DEFAULT_AUDIO_SINK@ 1
if [ -n "$HLE" ]; then
  if [ ! -e "$BIOS" ] && [ ! -e /storage/roms/bios/sega_101.bin ] && [ ! -e /storage/roms/bios/mpr-17933.bin ]; then
    : >"$BIOS" && PLACED=1
  fi
  mkdir -p "$TESTDATA/game-settings"
  # PERF=performance|battery: the pause menu's Performance setting for the run; OPTS: more JSON entries
  # ('"opt.yabasanshiro_sh2coretype": "interpreter"').
  echo "{\"opt.yabasanshiro_force_hle_bios\": \"enabled\", \"performance\": \"${PERF:-balanced}\"${OPTS:+, $OPTS}}" >"$TESTDATA/game-settings/saturn.json"
  export POCKETUI_DATA=$TESTDATA
fi
sh "$HERE/syscheck-play.sh" saturn "${CORE:-yabasanshiro}" "$ROM" "$@"

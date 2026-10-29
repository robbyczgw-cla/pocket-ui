#!/bin/sh
# Inside an isolated PipeWire 1.2.6 bench container; 60 s of clicks after guest exit.
set -eu
export XDG_RUNTIME_DIR=/tmp/xdg DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/xdg/bus
OUT=$1
FORCE=${2:-0}
pw-metadata -n settings 0 clock.force-quantum 1024 >/dev/null
# The UI starts its mixer (and the shelf's quantum) and waits 1 s before its first click, as pocket-ui does at launch;
# the recording starts so that the first click comes 0.3 s into it.
if [ "$FORCE" = 0 ]; then HOSTED=; else HOSTED=UI_GUEST_HOSTED=1; fi
env $HOSTED POCKETUI_SOUND_DEBUG=1 audio_ui_guest > "$OUT.ui.log" 2>&1 &
UI=$!
sleep 0.7
pw-record --target bench_sink -P '{ stream.capture.sink = true node.name = bench_rec }' --rate 48000 --channels 2 --format s16 "$OUT.wav" &
REC=$!
sleep 0.3
sleep 5
[ "$FORCE" = 0 ] || pw-metadata -n settings 0 clock.force-quantum 256 >/dev/null
# Silent pulse client uses the same 20 ms tlength as Audio's pulse backend.
POCKET_CORE_AUDIO=pulse audio_tone 5 0 > "$OUT.guest.log" 2>&1 &
GUEST=$!
(sleep 2; pw-metadata -n settings > "$OUT.guest.settings"; pw-top -b -n 2 > "$OUT.guest.pwtop") &
sleep 5
kill -KILL "$GUEST" 2>/dev/null || true
wait "$GUEST" 2>/dev/null || true
# A pocket-core game that pocket-ui started leaves the restore to pocket-ui (POCKET_CORE_QUANTUM_OWNER), whose
# `game_ended` sets the shelf's quantum; nothing else writes it here.
pw-metadata -n settings > "$OUT.settings"
pw-top -b -n 2 > "$OUT.pwtop"
wait "$UI"
sleep 0.3
pw-top -b -n 2 > "$OUT.pwtop.end"
kill -INT "$REC"
wait "$REC" 2>/dev/null || true

#!/bin/sh
# One measurement inside the bench container (README.md): plays `audio_tone` through pocket-core's audio path into
# bench_sink, records the sink's monitor to OUT.wav and saves pw-top's view (ERR column) to OUT.pwtop.
#   speaker:   the game sets its quantum of 256 itself (audio_route, no Bluetooth link here), sink latency 0
#   bluetooth: ROCKNIX's 1024 stays (POCKET_CORE_QUANTUM=0 takes the same branch as a Bluetooth sink in audio_route),
#              the sink reports 140 ms of its own like bluez5's (codec and radio)
# Usage: run-case.sh speaker|bluetooth SECONDS OUT [VAR=value ... for audio_tone]
set -eu
MODE=$1 SECS=$2 OUT=$3
shift 3
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/tmp/xdg}
export DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}
case $MODE in
    speaker) Q=1024 LAT=0 ENV= ;;
    bluetooth) Q=1024 LAT=140000000 ENV=POCKET_CORE_QUANTUM=0 ;;
    *) echo "run-case: speaker or bluetooth" >&2; exit 2 ;;
esac
# ROCKNIX's state between games: 1024 forced.
pw-metadata -n settings 0 clock.force-quantum "$Q" >/dev/null
pw-cli set-param bench_sink ProcessLatency "{ ns = $LAT }" >/dev/null
rm -f "$OUT.wav"
pw-record --target bench_sink -P '{ stream.capture.sink = true node.name = bench_rec }' --rate 48000 --channels 2 --format s16 "$OUT.wav" &
REC=$!
sleep 0.3
env $ENV "$@" audio_tone "$SECS" 1000 > "$OUT.tone.log" 2>&1 &
TONE=$!
sleep 3
pw-top -b -n 2 > "$OUT.pwtop" 2>&1 || true
# The ERR column at the end: xruns of each node over the whole run (a cycle a node missed, played as silence).
(sleep $((${SECS%.*} - 5)); pw-top -b -n 2 > "$OUT.pwtop.end" 2>&1) &
wait $TONE
sleep 0.3
kill -INT $REC; wait $REC 2>/dev/null || true
grep -E 'audio_tone|bench_sink|bench_rec' "$OUT.pwtop.end" | tail -3 > "$OUT.pwtop.short" || true
echo "$MODE $*: $(tr '\n' ' ' < "$OUT.tone.log")"
cat "$OUT.pwtop.short"

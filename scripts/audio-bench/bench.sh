#!/bin/sh
# Off-device audio bench (README.md): ROCKNIX's own PipeWire 1.2.6 and WirePlumber in a container of the virtual
# device's base image, `audio_tone` through pocket-core's audio path, the sink recorded and measured by analyze.py.
# Run from the repo:  scripts/audio-bench/bench.sh [SECONDS] [OUT_DIR]
#   AUDIO_BENCH_HOST  ssh host with docker and the vdev base image, or "local"; from the environment, else from
#                     scripts/local.env (untracked, like WIFI_BENCH_HOST)
#   BENCH_IMAGE  default pocket-vdev-base:20261001
#   BENCH_CASES  default "speaker bluetooth bluetooth-skew pauses"
# Exits 1 when a case breaks a limit (analyze.py). Needs python3 with numpy here, an aarch64 docker host there.
set -eu
SECS=${1:-60}
OUT=${2:-/tmp/audio-bench}
HOST=${AUDIO_BENCH_HOST:-}
IMAGE=${BENCH_IMAGE:-pocket-vdev-base:20261001}
CASES=${BENCH_CASES:-speaker bluetooth bluetooth-skew pauses}
NAME=audio-bench-$$
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
[ -n "$HOST" ] || HOST=$(sed -n 's/^AUDIO_BENCH_HOST=//p' "$ROOT/scripts/local.env" 2>/dev/null | tail -1)
[ -n "$HOST" ] || { echo "bench.sh: set AUDIO_BENCH_HOST (an SSH host with docker and the vdev image, or local), e.g. in scripts/local.env" >&2; exit 2; }
mkdir -p "$OUT"
on() { if [ "$HOST" = local ]; then sh -c "$1"; else ssh "$HOST" "export PATH=/opt/homebrew/bin:/usr/local/bin:\$PATH; $1"; fi; }
put() { if [ "$HOST" = local ]; then cp "$1" "$2"; else scp -q "$1" "$HOST:$2"; fi; }
get() { if [ "$HOST" = local ]; then cp "$1" "$2"; else scp -q "$HOST:$1" "$2"; fi; }

(cd "$ROOT" && PATH=$HOME/.cargo/bin:$HOME/opt/zig:$PATH cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --example audio_tone)
STAGE=/tmp/$NAME
on "mkdir -p $STAGE"
put "$ROOT/target/aarch64-unknown-linux-gnu/release/examples/audio_tone" "$STAGE/audio_tone"
put "$HERE/run-case.sh" "$STAGE/run-case.sh"
put "$HERE/bench-sink.conf" "$STAGE/90-bench.conf"
trap 'on "docker rm -f $NAME >/dev/null 2>&1; rm -rf $STAGE"' EXIT
on "docker run -d --name $NAME --entrypoint /bin/sh $IMAGE -c 'sleep 86400' >/dev/null
    docker exec $NAME mkdir -p /etc/pipewire/pipewire.conf.d /tmp/xdg /tmp/out
    docker cp $STAGE/90-bench.conf $NAME:/etc/pipewire/pipewire.conf.d/90-bench.conf
    docker cp $STAGE/audio_tone $NAME:/usr/local/bin/audio_tone
    docker cp $STAGE/run-case.sh $NAME:/usr/local/bin/run-case.sh
    docker exec $NAME sh -c 'export XDG_RUNTIME_DIR=/tmp/xdg DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/xdg/bus
        dbus-daemon --session --fork --address=\$DBUS_SESSION_BUS_ADDRESS
        (pipewire > /tmp/pw.log 2>&1 &); sleep 1; (wireplumber > /tmp/wp.log 2>&1 &); sleep 2'"
FILES=
for c in $CASES; do
    case $c in
        speaker) args="speaker $SECS /tmp/out/$c" ;;
        bluetooth) args="bluetooth $SECS /tmp/out/$c" ;;
        # The display 0.1 % fast against the sink's clock: the rate control has to make it up for good.
        bluetooth-skew) args="bluetooth $SECS /tmp/out/$c AUDIO_TONE_REFRESH=60.06" ;;
        # Pause menu twice, then quit through it, on Bluetooth; and the same on the speaker.
        pauses) args="bluetooth 20 /tmp/out/$c-bt AUDIO_TONE_PAUSES=5:6,12:12.3"
                on "docker exec $NAME run-case.sh speaker 20 /tmp/out/$c-spk AUDIO_TONE_PAUSES=5:6,12:12.3"
                on "docker cp $NAME:/tmp/out/$c-spk.wav $STAGE/$c-spk.wav"; get "$STAGE/$c-spk.wav" "$OUT/$c-spk.wav"
                FILES="$FILES $OUT/$c-spk.wav"; c=$c-bt ;;
        *) echo "bench: unknown case $c" >&2; exit 2 ;;
    esac
    on "docker exec $NAME run-case.sh $args"
    on "docker cp $NAME:/tmp/out/$c.wav $STAGE/$c.wav"
    get "$STAGE/$c.wav" "$OUT/$c.wav"
    FILES="$FILES $OUT/$c.wav"
done
python3 "$HERE/analyze.py" --expect 1004.5619 $FILES

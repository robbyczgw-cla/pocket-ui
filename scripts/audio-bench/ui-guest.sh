#!/bin/sh
# Off-device standalone/pulse guest regression. See README.md.
set -eu
: "${AUDIO_BENCH_HOST:?Set AUDIO_BENCH_HOST to the isolated Docker host}"
OUT=${1:-/tmp/audio-ui-guest}
ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
NAME=audio-sol-guest-$$
STAGE=/tmp/$NAME
mkdir -p "$OUT"
cd "$ROOT"
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --example audio_ui_guest --example audio_tone
on() { ssh "$AUDIO_BENCH_HOST" "export PATH=/opt/homebrew/bin:/usr/local/bin:\$PATH; $1"; }
trap 'on "docker rm -f $NAME >/dev/null 2>&1; rm -rf $STAGE"' EXIT
on "mkdir -p $STAGE; docker run -d --name $NAME --entrypoint /bin/sh pocket-vdev-base:20261001 -c 'sleep 86400' >/dev/null; docker exec $NAME mkdir -p /etc/pipewire/pipewire.conf.d /tmp/xdg /tmp/out"
for bin in audio_ui_guest audio_tone; do
    scp -q "target/aarch64-unknown-linux-gnu/release/examples/$bin" "$AUDIO_BENCH_HOST:$STAGE/$bin"
    on "docker cp $STAGE/$bin $NAME:/usr/local/bin/$bin"
done
scp -q scripts/audio-bench/bench-sink.conf "$AUDIO_BENCH_HOST:$STAGE/90-bench.conf"
scp -q scripts/audio-bench/ui-guest-case.sh "$AUDIO_BENCH_HOST:$STAGE/case.sh"
scp -q scripts/audio-bench/bench-quantum.sh "$AUDIO_BENCH_HOST:$STAGE/quantum"
on "docker cp $STAGE/quantum $NAME:/usr/sbin/quantum; docker exec $NAME chmod +x /usr/sbin/quantum"
on "docker cp $STAGE/90-bench.conf $NAME:/etc/pipewire/pipewire.conf.d/90-bench.conf; docker cp $STAGE/case.sh $NAME:/usr/local/bin/ui-guest-case.sh; docker exec $NAME sh -c 'export XDG_RUNTIME_DIR=/tmp/xdg DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/xdg/bus; dbus-daemon --session --fork --address=\$DBUS_SESSION_BUS_ADDRESS; pipewire >/tmp/pw.log 2>&1 & sleep 1; wireplumber >/tmp/wp.log 2>&1 & sleep 2; pipewire-pulse >/tmp/pulse.log 2>&1 & sleep 1'"
for mode in pulse forced; do
    force=0
    [ "$mode" != forced ] || force=1
    on "docker exec $NAME sh /usr/local/bin/ui-guest-case.sh /tmp/out/$mode $force"
done
on "docker cp $NAME:/tmp/out/. $STAGE/"
scp -q "$AUDIO_BENCH_HOST:$STAGE/*" "$OUT/"
python3 scripts/audio-bench/analyze-ui-guest.py "$OUT/pulse.wav" "$OUT/forced.wav"

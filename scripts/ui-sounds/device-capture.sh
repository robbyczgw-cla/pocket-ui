#!/bin/sh
# The media sounds through the real UI mixer on a handheld, recorded off the sink and measured here
# (examples/media_sounds_play.rs, capture.py, analyze_capture.py; docs/HOST.md "UI sounds").
#   scripts/ui-sounds/device-capture.sh BIN [VOLUME_STEP] [OUT]     BIN: media_sounds_play built for the device
#   POCKETUI_HOST as for dev.sh (scripts/host.sh). The default sink is recorded (the speaker, or Bluetooth headphones
#   when they are the default). With the default sink muted (pocket-ui plays nothing into it, and neither does this)
#   MS_SINK=null makes a null sink the default for the run and puts the default back afterwards; the speaker's volume
#   and mute are never touched. pocket-ui keeps running. Take the device lock around it.
set -eu
BIN=$1
STEP=${2:-10}
OUT=${3:-/tmp/media-sounds}
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
. "$ROOT/scripts/host.sh"
need_host
scp -q "$BIN" "$HOST:/tmp/media_sounds_play"
scp -q "$HERE/capture.py" "$HOST:/tmp/media-capture.py"
# shellcheck disable=SC2029
ssh "$HOST" "set -u
  old=\$(wpctl status | sed -n '/Sinks:/,/Sources:/p' | grep '\\*' | grep -o '[0-9]\\+\\.' | head -1 | tr -d .); null=
  if [ \"${MS_SINK:-}\" = null ]; then
    for n in \$(wpctl status | grep ' ms_null' | grep -o '[0-9]\\+\\.' | tr -d .); do pw-cli destroy \$n >/dev/null; done
    pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=ms_null node.description=ms_null media.class=Audio/Sink object.linger=true audio.position=[FL FR] node.driver=true }' >/dev/null
    sleep 0.5; null=\$(wpctl status | grep -m1 ' ms_null' | grep -o '[0-9]\\+\\.' | head -1 | tr -d .)
    [ -n \"\$null\" ] && wpctl set-default \$null && echo \"null sink \$null is the default for the run (was \$old)\"
  fi
  target=\$(pactl get-default-sink 2>/dev/null || echo @DEFAULT_AUDIO_SINK@)
  echo \"recording \$target, UI volume step $STEP\"
  rm -f /tmp/media-sounds.*
  XDG_RUNTIME_DIR=\${XDG_RUNTIME_DIR:-/var/run/0-runtime-dir} MEDIA_SOUNDS_VOLUME=$STEP python3 /tmp/media-capture.py /tmp/media-sounds \"\$target\" -- /tmp/media_sounds_play
  if [ -n \"\$null\" ]; then wpctl set-default \$old; pw-cli destroy \$null >/dev/null; echo \"default sink back to \$old, null sink removed\"; fi
  rm -f /tmp/media_sounds_play"
mkdir -p "$(dirname "$OUT")"
for f in wav start events; do scp -q "$HOST:/tmp/media-sounds.$f" "$OUT.$f"; done
python3 "$HERE/analyze_capture.py" "$OUT" "$STEP"

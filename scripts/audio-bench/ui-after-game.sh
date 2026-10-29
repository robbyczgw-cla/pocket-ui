#!/bin/sh
# UI sounds before, in and after a game on a Mini V2, recorded and checked for gaps (README.md "UI sounds after a game").
# Stops pocket-ui (like scripts/latency.sh), runs ui-after-game.py, puts pocket-ui back the normal way, checks the device
# (verify-device.sh) and analyses the recording here. Silent if the speaker is muted: the sink's monitor carries the
# sound before volume and mute (checked on the device: a tone into the muted speaker sink recorded at full level), so
# nothing here touches the volume or the mute.
#   scripts/audio-bench/ui-after-game.sh [GAME] [PLAY_SECONDS] [OUT]     default xbox:jade 20 /tmp/ui-after-game
#   POCKETUI_HOST as for dev.sh (scripts/host.sh). UAG_SINK=null: silent with the speaker muted. pocket-ui plays no menu
#   sound into a muted sink, so the run makes a null sink (uag_null), makes it the default for the run, records it, and
#   puts the default back and removes the null sink afterwards; the speaker's volume and mute are never touched.
set -eu
GAME=${1:-xbox:jade}
PLAY=${2:-20}
OUT=${3:-/tmp/ui-after-game}
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
. "$ROOT/scripts/host.sh"
need_host
scp -q "$ROOT/scripts/latency.py" "$HERE/ui-after-game.py" "$HOST:/tmp/"
# shellcheck disable=SC2029
ssh "$HOST" "DROPIN=/storage/.config/system.d/essway.service.d/10-pocket-ui.conf
  echo \"speaker: \$(wpctl get-volume @DEFAULT_AUDIO_SINK@) (left as it is)\"; echo \"fan before: \$(cat /sys/class/hwmon/hwmon*/pwm1 2>/dev/null | head -1)\"
  old=\$(wpctl status | sed -n '/Sinks:/,/Sources:/p' | grep '\\*' | grep -o '[0-9]\\+\\.' | head -1 | tr -d .); null=
  if [ \"${UAG_SINK:-}\" = null ]; then
    for n in \$(wpctl status | grep ' uag_null' | grep -o '[0-9]\\+\\.' | tr -d .); do pw-cli destroy \$n >/dev/null; done
    pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=uag_null node.description=uag_null media.class=Audio/Sink object.linger=true audio.position=[FL FR] node.driver=true }' >/dev/null
    sleep 0.5; null=\$(wpctl status | grep -m1 ' uag_null' | grep -o '[0-9]\\+\\.' | head -1 | tr -d .)
    [ -n \"\$null\" ] && wpctl set-default \$null && echo \"null sink \$null is the default for the run (was \$old)\"
  fi
  systemctl stop pocketui 2>/dev/null; systemctl reset-failed pocketui 2>/dev/null; systemctl stop essway
  i=0; while pidof pocket-ui >/dev/null && [ \$i -lt 50 ]; do sleep 0.1; i=\$((i+1)); done
  rm -f /tmp/uag-run.*
  [ -n \"\$null\" ] && export UAG_TARGET=uag_null
  python3 /tmp/ui-after-game.py $GAME $PLAY /tmp/uag-run; rc=\$?
  if [ -n \"\$null\" ]; then wpctl set-default \$old; pw-cli destroy \$null >/dev/null; echo \"default sink back to \$old, null sink removed\"; fi
  echo \"fan after: \$(cat /sys/class/hwmon/hwmon*/pwm1 2>/dev/null | head -1)\"
  for p in \$(pidof pocket-ui) \$(pidof pocket-core); do kill \$p 2>/dev/null; done
  i=0; while [ -n \"\$(pidof pocket-ui)\$(pidof pocket-core)\" ] && [ \$i -lt 30 ]; do sleep 0.1; i=\$((i+1)); done
  if [ -f \$DROPIN ]; then rm -f /tmp/pocket-ui.es-only; systemctl reset-failed essway 2>/dev/null; systemctl restart essway;
  else systemctl reset-failed pocketui 2>/dev/null; systemd-run --unit=pocketui /storage/pocket-ui/run-pocket-ui.sh >/dev/null; fi
  echo \"speaker now: \$(wpctl get-volume @DEFAULT_AUDIO_SINK@)\"
  exit \$rc"
mkdir -p "$(dirname "$OUT")"
for f in wav events quantum pwtop-shelf pwtop-game pwtop-after threads; do scp -q "$HOST:/tmp/uag-run.$f" "$OUT.$f" 2>/dev/null || true; done
"$ROOT/scripts/verify-device.sh" --no-local "$HOST" >&2 || echo "ui-after-game: the device is not in its normal state (VERIFY above)" >&2
echo "forced quantum over the run (s, value):"
cat "$OUT.quantum"
python3 "$HERE/analyze_ui.py" "$OUT"

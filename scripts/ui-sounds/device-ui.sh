#!/bin/sh
# The media sounds from the button on a handheld: device-ui.py under capture.py, measured here by analyze_capture.py
# (docs/HOST.md "UI sounds").
#   scripts/ui-sounds/device-ui.sh POCKET_UI_BIN [OUT]     POCKET_UI_BIN: pocket-ui built for the device
#   POCKETUI_HOST as for dev.sh. Stops pocket-ui, runs the given binary from /tmp with a data folder and a library of
#   two self-made test ROMs of its own (scripts/testroms.py), so nothing of the player's is started or written, and puts
#   pocket-ui back afterwards (verify-device.sh). With the default sink muted, MS_SINK=null records a null sink made the
#   default for the run (the speaker's volume and mute are never touched). CORE_BIN=<pocket-core built for the device>:
#   that one is bind-mounted over the installed pocket-core for the run and unmounted after it (nothing is replaced).
#   Take the device lock around it.
set -eu
BIN=$1
OUT=${2:-/tmp/media-ui}
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
. "$ROOT/scripts/host.sh"
need_host
ROMS=$(mktemp -d)
python3 "$ROOT/scripts/testroms.py" "$ROMS" >/dev/null
ssh "$HOST" "rm -rf /tmp/ms-root /tmp/ms-data /tmp/ms-roms /tmp/ms-pocket-core; mkdir -p /tmp/ms-root /tmp/ms-data /tmp/ms-roms/gba /tmp/ms-roms/mastersystem"
scp -q "$ROMS/gba-test.gba" "$HOST:/tmp/ms-roms/gba/"
scp -q "$ROMS/sms-test.sms" "$HOST:/tmp/ms-roms/mastersystem/"
rm -rf "$ROMS"
scp -q "$BIN" "$HOST:/tmp/ms-root/pocket-ui"
[ -n "${CORE_BIN:-}" ] && scp -q "$CORE_BIN" "$HOST:/tmp/ms-pocket-core"
scp -q "$HERE/capture.py" "$HERE/device-ui.py" "$ROOT/scripts/latency.py" "$HOST:/tmp/"
# shellcheck disable=SC2029
ssh "$HOST" "set -u
  for f in /storage/pocket-ui/*; do [ \"\${f##*/}\" = pocket-ui ] || ln -s \"\$f\" /tmp/ms-root/; done
  python3 - <<'PY'
import re
cfg = open('/storage/.config/emulationstation/es_systems.cfg').read()
keep = [s for s in re.findall(r'<system>.*?</system>', cfg, re.S) if re.search(r'<name>(gba|mastersystem)</name>', s)]
keep = [re.sub(r'<path>[^<]*</path>', '<path>/tmp/ms-roms/' + re.search(r'<name>([^<]*)</name>', s).group(1) + '</path>', s) for s in keep]
open('/tmp/ms-roms/es_systems.cfg', 'w').write('<?xml version=\"1.0\"?>\n<systemList>\n' + '\n'.join(keep) + '\n</systemList>\n')
open('/tmp/ms-data/settings.json', 'w').write('{\"theme\": \"shelf\", \"onboarded\": true, \"seen_version\": \"0.1.3\", \"language\": \"en\", \"on_start\": \"home\", \"motion\": \"full\"}')
PY
  old=\$(wpctl status | sed -n '/Sinks:/,/Sources:/p' | grep '\\*' | grep -o '[0-9]\\+\\.' | head -1 | tr -d .); null=
  if [ \"${MS_SINK:-}\" = null ]; then
    for n in \$(wpctl status | grep ' ms_null' | grep -o '[0-9]\\+\\.' | tr -d .); do pw-cli destroy \$n >/dev/null; done
    pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=ms_null node.description=ms_null media.class=Audio/Sink object.linger=true audio.position=[FL FR] node.driver=true }' >/dev/null
    sleep 0.5; null=\$(wpctl status | grep -m1 ' ms_null' | grep -o '[0-9]\\+\\.' | head -1 | tr -d .)
    [ -n \"\$null\" ] && wpctl set-default \$null && echo \"null sink \$null is the default for the run (was \$old)\"
  fi
  target=\$(pactl get-default-sink 2>/dev/null || echo @DEFAULT_AUDIO_SINK@)
  systemctl stop pocketui 2>/dev/null; systemctl reset-failed pocketui 2>/dev/null; systemctl stop essway
  i=0; while pidof pocket-ui >/dev/null && [ \$i -lt 50 ]; do sleep 0.1; i=\$((i+1)); done
  rm -f /tmp/media-ui.*
  core=\$(readlink -f /storage/pocket-ui/pocket-core); bound=
  if [ -f /tmp/ms-pocket-core ]; then mount --bind /tmp/ms-pocket-core \"\$core\" && bound=1 && echo \"pocket-core for the run: /tmp/ms-pocket-core over \$core\"; fi
  python3 /tmp/capture.py /tmp/media-ui \"\$target\" -- python3 /tmp/device-ui.py /tmp/ms-root /tmp/ms-data /tmp/ms-roms/es_systems.cfg; rc=\$?
  if [ -n \"\$null\" ]; then wpctl set-default \$old; pw-cli destroy \$null >/dev/null; echo \"default sink back to \$old, null sink removed\"; fi
  for p in \$(pidof pocket-ui) \$(pidof pocket-core); do kill \$p 2>/dev/null; done
  i=0; while [ -n \"\$(pidof pocket-ui)\$(pidof pocket-core)\" ] && [ \$i -lt 30 ]; do sleep 0.1; i=\$((i+1)); done
  [ -n \"\$bound\" ] && umount \"\$core\" && echo \"pocket-core unmounted, \$core is the installed one again\"; rm -f /tmp/ms-pocket-core
  rm -f /tmp/pocket-ui.es-only; systemctl reset-failed essway 2>/dev/null; systemctl restart essway
  cp /tmp/media-ui-pocket-ui.log /tmp/media-ui.log 2>/dev/null
  rm -rf /tmp/ms-root /tmp/ms-data /tmp/ms-roms /tmp/device-ui.py /tmp/capture.py
  exit \$rc" || echo "device-ui: the run failed (above)" >&2
mkdir -p "$(dirname "$OUT")"
for f in wav start events log; do scp -q "$HOST:/tmp/media-ui.$f" "$OUT.$f" 2>/dev/null || true; done
"$ROOT/scripts/verify-device.sh" --no-local "$HOST" >&2 || echo "device-ui: the device is not in its normal state (VERIFY above)" >&2
python3 "$HERE/analyze_capture.py" "$OUT" 7

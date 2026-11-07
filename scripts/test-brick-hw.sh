#!/bin/bash
# Host test of brick-hw's hardware commands against a fake sysfs tree (docs/TRIMUI-BRICK-HW.md): the mute slider, the motor pin and the
# lights. The C helper is built with the host compiler. What it cannot prove: that the real Brick's files take these values.
#   scripts/test-brick-hw.sh
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
fails=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fails=$((fails + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1: $2"; fi; }
WORK=$(mktemp -d "${TMPDIR:-/tmp}/brick-hw-test.XXXXXX"); trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$WORK"' EXIT
CC=${CC:-cc}
$CC -Wall -Wextra -O2 "$ROOT/deploy/brick/brick-hw.c" -o "$WORK/brick-hw" || { echo "FAIL build"; exit 1; }
HW=$WORK/brick-hw
S=$WORK/sys
put() { mkdir -p "$(dirname "$S/$1")"; printf '%s\n' "$2" >"$S/$1"; }
val() { tr -d '\n' <"$S/$1"; }
for z in l r lr m f1 f2; do put sys/class/led_anim/effect_$z 0; put sys/class/led_anim/effect_rgb_hex_$z 000000; put sys/class/led_anim/effect_cycles_$z 0; done
put sys/class/led_anim/effect_enable 0; put sys/class/led_anim/max_scale 60; put sys/class/led_anim/frame_hex "373737 373737 000000"; put sys/class/led_anim/anim_frames_enable 1
put sys/class/gpio/export ""; put sys/class/gpio/gpio227/value 0; put sys/class/gpio/gpio227/direction in
put sys/class/gpio/gpio243/value 1; put sys/class/gpio/gpio243/direction in
put sys/class/motor/voltage 0
export BRICK_SYS=$S

# --- lights
"$HW" led-set ff0000 50
check "led-set: red on every zone, on the driver's 0..60 scale" '[ "$(val sys/class/led_anim/effect_rgb_hex_lr)" = 3c0000 ] && [ "$(val sys/class/led_anim/effect_rgb_hex_f2)" = 3c0000 ] && [ "$(val sys/class/led_anim/effect_m)" = 4 ]'
check "led-set: 50 percent is 30 of 60, effect enabled, endless" '[ "$(val sys/class/led_anim/max_scale)" = 30 ] && [ "$(val sys/class/led_anim/effect_enable)" = 1 ] && [ "$(val sys/class/led_anim/effect_cycles_f1)" = -1 ]'
check "led-set: the half zones l and r are left alone" '[ "$(val sys/class/led_anim/effect_l)" = 0 ] && [ "$(val sys/class/led_anim/effect_r)" = 0 ]'
"$HW" led-off
check "led-off: dark (scale 0, no effect)" '[ "$(val sys/class/led_anim/max_scale)" = 0 ] && [ "$(val sys/class/led_anim/effect_lr)" = 0 ]'
check "led-off: the driver's raw picture black too (23 LEDs), effects and raw frames off" '[ "$(tr " " "\n" <$S/sys/class/led_anim/frame_hex | grep -c "^000000$")" = 23 ] && [ "$(val sys/class/led_anim/effect_enable)" = 0 ] && [ "$(val sys/class/led_anim/anim_frames_enable)" = 0 ]'
check "led-info lists the driver" '"$HW" led-info | grep -q "effect_enable"'

# --- motor
"$HW" motor-pulse 30 1500
check "motor-pulse: the pin is back to 0 and the voltage was written" '[ "$(val sys/class/gpio/gpio227/value)" = 0 ] && [ "$(val sys/class/motor/voltage)" = 1500 ] && [ "$(val sys/class/gpio/gpio227/direction)" = out ]'
check "motor-info reads the pin and the voltage" '"$HW" motor-info | grep -q "voltage: 1500"'
"$HW" motor-pulse 99999 >/dev/null
check "motor-pulse is capped (2 s at most) and ends off" '[ "$(val sys/class/gpio/gpio227/value)" = 0 ]'

# --- slider
check "slider-get prints the pin" '[ "$("$HW" slider-get)" = 1 ]'
rm -f "$WORK/calls"
cat >"$WORK/volume" <<X
#!/bin/sh
echo "\$1" >>"$WORK/calls"
X
chmod +x "$WORK/volume"
wait_calls() { n=0; while [ "$( (wc -l <"$WORK/calls") 2>/dev/null || echo 0)" -lt "$1" ] && [ $n -lt 40 ]; do sleep 0.1; n=$((n+1)); done; }
BRICK_SLIDER_CMD="$WORK/volume" BRICK_SLIDER_MUTED=0 "$HW" slider-watch 2>/dev/null &
W=$!
wait_calls 1
check "slider-watch: found at 1 = not muted (when 0 is muted): unmute once at start" '[ "$(cat "$WORK/calls")" = unmute ]'
put sys/class/gpio/gpio243/value 0
wait_calls 2
check "slider to 0: mute" '[ "$(sed -n 2p "$WORK/calls")" = mute ]'
sleep 0.6
check "no change: no further call" '[ "$(wc -l <"$WORK/calls")" = 2 ]'
put sys/class/gpio/gpio243/value 1
wait_calls 3
check "slider back: unmute" '[ "$(sed -n 3p "$WORK/calls")" = unmute ]'
kill $W 2>/dev/null; wait $W 2>/dev/null
# the other polarity
rm -f "$WORK/calls"; put sys/class/gpio/gpio243/value 1
BRICK_SLIDER_CMD="$WORK/volume" BRICK_SLIDER_MUTED=1 "$HW" slider-watch 2>/dev/null &
W=$!
wait_calls 1
check "BRICK_SLIDER_MUTED=1: a slider at 1 (down) is muted" '[ "$(cat "$WORK/calls")" = mute ]'
kill $W 2>/dev/null; wait $W 2>/dev/null
# the default without the variable: down (1) is muted, as on the real Brick
rm -f "$WORK/calls"
BRICK_SLIDER_CMD="$WORK/volume" "$HW" slider-watch 2>/dev/null &
W=$!
wait_calls 1
check "no BRICK_SLIDER_MUTED: the slider down (1) mutes" '[ "$(cat "$WORK/calls")" = mute ]'
kill $W 2>/dev/null; wait $W 2>/dev/null
# Settings > Mute slider = Nothing: the slider does not mute, and a speaker already muted by it is un-muted
rm -f "$WORK/calls"; put sys/class/gpio/gpio243/value 0
BRICK_SLIDER_CFG="$WORK/slider-cfg" BRICK_SLIDER_CMD="$WORK/volume" BRICK_SLIDER_MUTED=0 "$HW" slider-watch 2>/dev/null &
W=$!
wait_calls 1
check "slider at the muted position: mute" '[ "$(cat "$WORK/calls")" = mute ]'
echo nothing >"$WORK/slider-cfg"
wait_calls 2
check "Mute slider = Nothing: unmute at once" '[ "$(sed -n 2p "$WORK/calls")" = unmute ]'
put sys/class/gpio/gpio243/value 1; sleep 0.6; put sys/class/gpio/gpio243/value 0; sleep 0.6
check "Nothing: moving the slider calls nothing more" '[ "$(wc -l <"$WORK/calls")" = 2 ]'
echo mute >"$WORK/slider-cfg"
wait_calls 3
check "back to Mute: the slider's position counts again" '[ "$(sed -n 3p "$WORK/calls")" = mute ]'
kill $W 2>/dev/null; wait $W 2>/dev/null
check "slider-watch without a command refuses" '! BRICK_SLIDER_CMD= "$HW" slider-watch 2>/dev/null'
put sys/class/gpio/gpio243/value ""
check "unreadable pin: slider-get fails" '! "$HW" slider-get 2>/dev/null'

# --- the volume stand-in mutes the way the slider needs (stateful fake amixer)
echo 22 >"$WORK/mixer-raw"
cat >"$WORK/amixer" <<X
#!/bin/sh
case " \$* " in *" -s "*) while IFS= read -r line; do eval '"\$0" -c 0 -q '"\$line" || exit 1; done; exit ;; esac
echo "\$*" >>"$WORK/amixer.log"
case "\$*" in
 *cget*"digital volume"*) echo "  : values=\$(cat "$WORK/mixer-raw")" ;;
 *cget*) exit 1 ;;
 *cset*"digital volume"*) for v; do :; done; echo "\$v" >"$WORK/mixer-raw" ;;
esac
X
chmod +x "$WORK/amixer"
# Set the player's level through the current helper: brick-volume itself is an
# OTA rollback encoding, not the current percentage (brick-volume-curve3).
BRICK_AMIXER="$WORK/amixer" BRICK_STATE="$WORK" "$ROOT/deploy/brick/hwroot/usr/bin/volume" 40
before_raw=$(cat "$WORK/mixer-raw")
BRICK_AMIXER="$WORK/amixer" BRICK_STATE="$WORK" "$ROOT/deploy/brick/hwroot/usr/bin/volume" mute
check "volume mute: the speaker and jack switches off" 'grep -q "HpSpeaker Switch off" "$WORK/amixer.log" && grep -q "Headphone Switch off" "$WORK/amixer.log"'
BRICK_AMIXER="$WORK/amixer" BRICK_STATE="$WORK" "$ROOT/deploy/brick/hwroot/usr/bin/volume" unmute
check "volume unmute: both switches on, exact mixer gain and displayed level restored" 'grep -q "HpSpeaker Switch on" "$WORK/amixer.log" && grep -q "Headphone Switch on" "$WORK/amixer.log" && [ "$(cat "$WORK/mixer-raw")" = "$before_raw" ] && BRICK_AMIXER="$WORK/amixer" BRICK_STATE="$WORK" "$ROOT/deploy/brick/hwroot/usr/bin/pactl" get-sink-volume | grep -q " 40% "'

python3 "$ROOT/scripts/test-brick-volume.py" || fails=$((fails + 1))

[ $fails -eq 0 ] && echo "ALL OK" || { echo "$fails FAILED"; exit 1; }

#!/bin/bash
# start.sh on a computer, with the Brick's binaries under qemu-user against the Brick's own glibc 2.33 and libs, and a plain file as
# the screen. It proves the launcher's logic, the load of every binary and the protocol, not the Brick's speed.
#   scripts/brick/qemu-test.sh <qemu-aarch64-static> <sysroot with lib/ and usr/lib/ read from the Brick> <folder with the 4 binaries> <folder with libwayland...>
set -u
qemu=$(realpath "${1:?qemu-aarch64-static}"); sysroot=$(realpath "${2:?sysroot}"); bins=$(realpath "${3:?binaries}"); libs=$(realpath "${4:?libs}")
here=$(cd "$(dirname "$0")" && pwd)
t=$(mktemp -d /tmp/brick-qemu-XXXXXX)
fail=0
check() { if eval "$2"; then echo "ok    $1"; else echo "FAIL  $1"; fail=1; fi; }

# A copy of the folder as it would sit on the card, the binaries as wrappers that run them under qemu.
mk() {
  r=$t/$1; rm -rf "$r"; mkdir -p "$r/bin" "$r/lib"
  cp "$here/start.sh" "$here/env.sh" "$r/"; cp -a "$libs"/*.so.* "$r/lib/"
  for b in pocket-ui pocket-core pocket-fb brick-egl brick-hw; do
    [ -f "$bins/$b" ] || continue
    printf '#!/bin/sh\nexec %s -L %s -E LD_LIBRARY_PATH=%s/lib:%s/usr/lib:%s/lib "$(dirname "$0")/real/%s" "$@"\n' "$qemu" "$sysroot" "$sysroot" "$sysroot" "$r" "$b" >"$r/bin/$b"
    mkdir -p "$r/bin/real"; cp "$bins/$b" "$r/bin/real/$b"; chmod +x "$r/bin/$b"
  done
}
run() { # run <folder> <seconds>: start.sh with a plain file as fb0
  head -c $((1024 * 768 * 4)) /dev/zero >"$t/fb"
  POCKET_VIEW=0 POCKET_FB=$t/fb POCKET_MESSAGE_SECONDS=${MSG_SECONDS:-1} POCKETUI_MOCK=1 POCKETUI_LANG=en SD=$t/sd CORES=$t/sd/cores \
    WAYLAND_DISPLAY= timeout "$2" sh "$t/$1/start.sh" nextui >"$t/$1.out" 2>&1
  echo $? >"$t/$1.code"
}
painted() { [ "$(tr -d '\0' <"$t/fb" | wc -c)" -gt 1000 ]; }

mkdir -p "$t/sd/cores"

echo "== 1 normal start: bridge up, pocket-ui paints, a TERM ends it and everything goes away"
mk good
( run good 45 ) &
sleep 30
check "the bridge's sockets exist while it runs" "[ -S /tmp/pocket-wayland ] && [ -S /tmp/pocket-sway ]"
check "pocket-ui has painted the screen file" painted
wait
check "start.sh ended (timeout's TERM, code 124, or by itself)" "[ \"\$(cat $t/good.code)\" = 124 ] || [ \"\$(cat $t/good.code)\" = 0 ]"
check "the sockets are removed afterwards" "[ ! -e /tmp/pocket-wayland ] && [ ! -e /tmp/pocket-sway ]"
check "no pocket-ui or pocket-fb left of ours" "! pgrep -f '$t/good/bin' >/dev/null"
grep -E "start|end|ended" "$t/good/logs/start.log" | sed 's/^/      /'

echo "== 2 a missing library is said on the screen and in the log"
mk nolib; rm "$t/nolib/lib/libxkbcommon.so.0"
# pocket-fb puts the screen back when the message's time is up, so it is looked at while it is up
( MSG_SECONDS=5 run nolib 30 ) &
sleep 3
check "the message is on the screen" painted
wait
check "exit code 1" "[ \"\$(cat $t/nolib.code)\" = 1 ]"
check "the log names the library" "grep -q 'lib/libxkbcommon.so.0 is missing' $t/nolib/logs/start.log"
check "the screen is back as it was afterwards" "! painted"

echo "== 3 a missing program"
mk noui; rm "$t/noui/bin/pocket-ui"
( MSG_SECONDS=5 run noui 30 ) &
sleep 3
check "the message is on the screen" painted
wait
check "the log names the file" "grep -q 'bin/pocket-ui is missing' $t/noui/logs/start.log"

echo "== 4 no screen device: the log says it, and it ends"
mk nofb
( head -c 10 /dev/zero >/dev/null; POCKET_VIEW=0 POCKET_FB=$t/none timeout 20 sh "$t/nofb/start.sh" nextui >"$t/nofb.out" 2>&1; echo $? >"$t/nofb.code" )
check "exit code 1" "[ \"\$(cat $t/nofb.code)\" = 1 ]"
check "the log says there is no screen device" "grep -q 'No screen device' $t/nofb/logs/start.log"

echo "== 5 low battery stops before drawing"
mk low
head -c $((1024 * 768 * 4)) /dev/zero >"$t/fb"
POCKET_VIEW=0 POCKET_FB=$t/fb POCKET_MESSAGE_SECONDS=1 POCKET_MIN_BATTERY=101 timeout 20 sh "$t/low/start.sh" nextui >/dev/null 2>&1
if ls /sys/class/power_supply/*/capacity >/dev/null 2>&1; then check "the log says the battery" "grep -q 'Battery at' $t/low/logs/start.log"; else echo "skip  5 (this computer has no battery)"; fi

echo "== 5b the GPU asked for where it can't draw: the log says so, the CPU copy paints, no black screen"
mk gl
( POCKET_BRICK_GL=1 run gl 25 ) &
sleep 20
check "pocket-ui has painted the screen file" painted
wait
check "the log says the CPU copy" "grep -q 'GL: .*CPU copy' $t/gl/logs/start.log"
grep -E "GL" "$t/gl/logs/start.log" | sed 's/^/      /'

echo "== 6 the real shelf: the games folder, the systems and the core choices are made, the library finds the game"
mk shelf
mkdir -p "$t/shelf/storage/roms/gba"; printf 'GBA test' >"$t/shelf/storage/roms/gba/Test Game (World).gba"
( run shelf 25 ) &
sleep 20
check "es_systems.cfg and system.cfg are made in our folder" "[ -s $t/shelf/storage/.config/emulationstation/es_systems.cfg ] && grep -q 'gba.core=gpsp' $t/shelf/storage/.config/system/configs/system.cfg"
wait
check "env: NextUI's cores by path and our pocket-core" "grep -q POCKETUI_CORES $t/shelf/env.sh && grep -q POCKETUI_HOST_BIN $t/shelf/env.sh"

echo "== 7 the pak: a stage file runs once, no file starts the real UI, a bad name is ignored"
k=$t/kit; mkdir -p "$k"
printf '#!/bin/sh\necho "kit $*" >>%s/calls\n' "$k" >"$k/brick-kit.sh"; printf '#!/bin/sh\necho "start $*" >>%s/calls\n' "$k" >"$k/start.sh"
printf 'snapshot\n' >"$k/next-stage"; POCKET_UI_KIT=$k sh "$here/pocket-ui-test.pak/launch.sh"
check "the stage runs through the kit's _run" "grep -q 'kit _run snapshot' $k/calls"
check "the file is gone (one-shot)" "[ ! -e $k/next-stage ]"
POCKET_UI_KIT=$k sh "$here/pocket-ui-test.pak/launch.sh"
check "without a file the real UI starts" "grep -q 'start nextui' $k/calls"
printf 'x;rm -rf /\n' >"$k/next-stage"; POCKET_UI_KIT=$k sh "$here/pocket-ui-test.pak/launch.sh"
check "a bad name is not run as a stage" "[ \$(grep -c _run $k/calls) = 1 ]"

echo "== 8 a TERM to start.sh itself ends pocket-ui and the bridge (the night of 2026-11-03: a stop left pocket-ui behind)"
mk term
head -c $((1024 * 768 * 4)) /dev/zero >"$t/fb"
POCKET_VIEW=0 POCKET_FB=$t/fb POCKETUI_MOCK=1 SD=$t/sd CORES=$t/sd/cores WAYLAND_DISPLAY= sh "$t/term/start.sh" nextui >"$t/term.out" 2>&1 &
tp=$!
sleep 20
check "pocket-ui of that folder runs" "pgrep -f '$t/term/bin' >/dev/null"
kill -TERM $tp
sleep 8
check "nothing of that folder is left a few seconds after the TERM" "! pgrep -f '$t/term/bin' >/dev/null"

echo "files: $t"
exit $fail

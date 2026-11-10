#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
set -eu
# Capture the UI before this shell starts its C supervisor. Its watcher stays
# outside the paused game session and compares the owner's proc starttime.
export BRICK_PORTMASTER_OWNER_PID=$PPID
app=${BRICK_APP:?}; pm=${BRICK_PORTMASTER_HOME:?}; runtime=${BRICK_PORTMASTER_RUNTIME:?}
launcher=${1:?port launcher}; shift
[ -f "$launcher" ] || { echo 'PortMaster: game launcher is missing' >&2; exit 1; }
. "$app/portmaster/device-env.sh"
export HOME=${HOME:-$BRICK_DATA/storage/home}
export XDG_CONFIG_HOME=$HOME/.config XDG_STATE_HOME=$HOME/.local/state
# Modern upstream launchers look here first. The forwarding file needs no FAT symlinks;
# the game's own XDG saves remain in the persistent home, never /tmp.
export XDG_DATA_HOME=$HOME/.local/share
mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_DATA_HOME/PortMaster"
printf '%s\n' '. "$BRICK_PORTMASTER_HOME/control.txt"' >"$XDG_DATA_HOME/PortMaster/control.txt"
export PYTHONHOME=$runtime PATH=/bin:$runtime/bin:$app/bin:$PATH
export LD_LIBRARY_PATH=$runtime/lib:${BRICK_FIRMWARE:-/usr/trimui}/lib:/lib:/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
export SDL_GAMECONTROLLERCONFIG_FILE=$pm/pocket-gamecontrollerdb.txt
# TRIMUI Player1 borrows the Xbox USB ID. Use its measured button order, never the generic Xbox map.
# F1/F2 are not sticks on the original Brick. MENU stays with pocket-ui (no guide binding).
guid=${BRICK_SDL_GUID:-0300a3845e0400008e02000014010000}
mapping="$guid,TRIMUI Player1,a:b0,b:b1,x:b2,y:b3,back:b6,start:b7,leftshoulder:b4,rightshoulder:b5,dpup:h0.1,dpdown:h0.4,dpleft:h0.8,dpright:h0.2,"
if [ "$ANALOG_STICKS" = 2 ]; then mapping="${mapping}leftx:a0,lefty:a1,rightx:a3,righty:a4,lefttrigger:a2,righttrigger:a5,leftstick:b9,rightstick:b10,"; fi
printf '%s\n' "$mapping" >"$SDL_GAMECONTROLLERCONFIG_FILE"
export SDL_GAMECONTROLLERCONFIG=$mapping sdl_controllerconfig=$mapping
export SDL_VIDEODRIVER=mali SDL_AUDIODRIVER=alsa SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS=1
export PM_SCRIPTNAME=${launcher##*/}
fb=$app/bin/pocket-fb
registered=0
cleanup() {
  [ "$registered" = 0 ] || "$fb" --ipc-send "$SWAYSOCK" "pocket_port end $$" >/dev/null 2>&1 || :
}
trap cleanup EXIT
trap 'exit 143' TERM HUP INT
if [ -n "${SWAYSOCK:-}" ]; then
  "$fb" --ipc-send "$SWAYSOCK" "pocket_port begin $$" >/dev/null || { echo 'PortMaster: cannot hand over the display' >&2; exit 1; }
  registered=1
fi
set +e
"$app/bin/portmaster-exec" "$launcher" "$@"
rc=$?
exit "$rc"

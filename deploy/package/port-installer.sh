#!/bin/bash
# Install pocket-ui.sh / Uninstall pocket-ui.sh: the installer for people without SSH (PortMaster-style). Copy ONE file
# into roms/ports (ROCKNIX's file sharing, or the SD card in a computer), restart EmulationStation (or the device),
# open Ports and start "Install pocket-ui". The screen shows the checks, the install and the switch to pocket-ui.
#
# This file is only a front end. The checks and the install are deploy/package/install.sh, unchanged: nothing is written
# until every check passed, and running it twice is harmless. The file carries the package (a tar.gz after this text,
# PAYLOAD_BYTES long; make-install-zip.sh appends it). Without a payload it looks for pocket-ui-*-rocknix-install.zip
# next to itself (or in /storage, /storage/roms). After an install it replaces itself with "Uninstall pocket-ui.sh" (the same
# file without the package). Started under the name "Uninstall pocket-ui*" it runs the package's
# uninstall.sh (/storage/pocket-ui/uninstall.sh) the same way.
#
# The screen: ROCKNIX's own text_viewer (full-screen text with a Close button the pad operates; EmulationStation's
# BIOS and download errors use it too), showing the log of a worker started detached, so that closing the viewer early
# never ends an install halfway. The worker is this file with --worker. After the viewer: on success the entry
# removes itself and the interface restarts into pocket-ui (install.sh's own restart is off: it cannot restart essway
# from inside essway, where a port runs), detached through systemd-run.
#
# Tests: deploy/package/test-package.sh (POCKETUI_INSTALL_ROOT, POCKETUI_VIEWER and POCKETUI_FONT are for it).
PAYLOAD_BYTES=0
SELF=$(readlink -f "$0" 2>/dev/null || echo "$0")
R=${POCKETUI_INSTALL_ROOT:-}
VIEWER=${POCKETUI_VIEWER:-text_viewer}
# text_viewer's size: the H700 handhelds' 640x480 (or 720x480) screens hold a quarter of the Mini V2's pixels.
FONT=${POCKETUI_FONT:-}
if [ -z "$FONT" ]; then
  FONT=44
  grep -q '^HW_DEVICE="\{0,1\}H700' "$R/etc/os-release" 2>/dev/null && FONT=22
fi
WORK=$R/tmp/pocket-ui-port
STAGE=$R/storage/.pocket-ui-port
APP=$R/storage/pocket-ui
DROPIN=$R/storage/.config/system.d/essway.service.d/10-pocket-ui.conf
LOG=$WORK/log
DONE=$WORK/done
MODE=install
case ${SELF##*/} in [Uu]ninstall*) MODE=uninstall ;; esac

say() { printf '%s\n' "$@"; }
# The result for the front end: ok (switch the interface), failed or nothing (leave everything as it is).
finish() {
  echo "$1" >"$DONE.new" && mv -f "$DONE.new" "$DONE"
  exit 0
}
failed() {
  say "" "$1" "" "$2"
  finish failed
}

# --- the worker: runs detached, its output is what the screen shows ------------------------------------------------
worker() {
  rm -rf "$STAGE"
  trap 'rm -rf "$STAGE"' EXIT
  if [ "$MODE" = uninstall ]; then
    say "pocket-ui uninstaller" ""
    [ -f "$APP/uninstall.sh" ] || { say "pocket-ui is not installed on this card. Nothing to remove." "" "Press Close to go back to EmulationStation."; finish nothing; }
    say "Removing pocket-ui. Your games, saves and ROCKNIX settings stay." ""
    sh "$APP/uninstall.sh" --no-restart 2>&1 | sed 's/^/  /'
    [ -e "$APP" ] && { say "" "pocket-ui could not be removed completely (see above)." "Press Close to go back."; finish failed; }
    say "" "Done. Press Close: EmulationStation starts in a few seconds."
    finish ok
  fi

  say "pocket-ui installer" ""
  say "Step 1 of 3: looking at this device (nothing is changed yet)"

  # The package: carried by this file, or the zip next to it.
  mkdir -p "$STAGE" || failed "cannot write to the card." "Nothing was changed. Press Close to go back to EmulationStation."
  if [ "$PAYLOAD_BYTES" -gt 0 ]; then
    free=$(df -Pk "$R/storage" 2>/dev/null | awk 'NR==2 {print $4}')
    [ "${free:-0}" -ge 40000 ] 2>/dev/null || failed "there is not enough free space on the card (about 40 MB are needed)." "Nothing was changed. Free some space (delete something in /storage), then try again. Press Close to go back to EmulationStation."
    tail -c "$PAYLOAD_BYTES" "$SELF" | tar xzf - -C "$STAGE" 2>/dev/null ||
      failed "this installer file is damaged (it did not unpack)." "Nothing was changed. Download \"Install pocket-ui.sh\" again and copy it to roms/ports. Press Close to go back to EmulationStation."
  else
    zip=
    for d in "$(dirname "$SELF")" "$R/storage" "$R/storage/roms"; do
      z=$(ls "$d"/pocket-ui-*-rocknix-install.zip 2>/dev/null | sort | tail -n 1)
      [ -z "$z" ] || { zip=$z; break; }
    done
    [ -n "$zip" ] || failed "the installer data is missing: this file is only the starter." "Nothing was changed. Copy pocket-ui-<version>-rocknix-install.zip into the same folder as this file (roms/ports), or download the single complete file \"Install pocket-ui.sh\". Press Close to go back to EmulationStation."
    say "  using ${zip##*/}"
    unzip -q -o "$zip" -d "$STAGE" 2>/dev/null ||
      failed "${zip##*/} did not unpack: it is damaged or incomplete." "Nothing was changed. Download it again. Press Close to go back to EmulationStation."
  fi
  PKG=$(ls -d "$STAGE"/pocket-ui-*-rocknix-install 2>/dev/null | head -n 1)
  [ -f "$PKG/install.sh" ] && [ -f "$PKG/files/VERSION" ] || failed "the package inside is incomplete." "Nothing was changed. Download it again. Press Close to go back to EmulationStation."
  VERSION=$(cat "$PKG/files/VERSION")

  if [ -f "$APP/installed-by-package" ] && [ -f "$DROPIN" ] && [ "$(sed -n 's/^version=//p' "$APP/installed-by-package")" = "$VERSION" ]; then
    say "" "pocket-ui $VERSION is already installed on this card." "Nothing to do. Press Close to go back."
    finish nothing
  fi

  # install.sh's own checks; its sentences are the explanation.
  sh "$PKG/install.sh" --check >"$WORK/check.out" 2>&1
  rc=$?
  if [ $rc -ne 0 ]; then
    reason=$(sed -n 's/^NOT INSTALLED: //p' "$WORK/check.out" | head -n 1)
    [ -n "$reason" ] || reason=$(tail -n 3 "$WORK/check.out")
    failed "pocket-ui was NOT installed: ${reason%.}." "Nothing on the card was changed, EmulationStation stays as it was. Press Close to go back to EmulationStation."
  fi
  # The notes (experimental device, untested version...) as install.sh words them; the passed checks as a count.
  grep '^  note  ' "$WORK/check.out"
  dev=$(sed -n 's/^  \(ok\|note\) \{1,4\}device: \([^,]*\).*/\2/p' "$WORK/check.out" | head -n 1)
  say "  $(grep -c '^  ok  ' "$WORK/check.out") checks passed: ROCKNIX $(sed -n 's/^  ok    ROCKNIX \([^ ]*\) for \([A-Za-z0-9]*\).*/\1 (\2)/p' "$WORK/check.out" | head -n 1)${dev:+, $dev}"
  sleep 1
  say "  Nothing was changed so far."

  say "" "Step 2 of 3: installing pocket-ui $VERSION"
  sh "$PKG/install.sh" --no-restart >"$WORK/install.out" 2>&1
  rc=$?
  if [ $rc -eq 0 ]; then
    say "  programs, start entry and boot screen are in place"
    sleep 1
  else
    reason=$(sed -n 's/^NOT INSTALLED: //p' "$WORK/install.out" | head -n 1)
    [ -n "$reason" ] || reason=$(tail -n 3 "$WORK/install.out")
    failed "the install stopped: ${reason%.}." "Press Close to go back to EmulationStation, then try again. If it stays like this, restart the device."
  fi

  say "" "Step 3 of 3: switching to pocket-ui" "" "pocket-ui $VERSION is installed. Your games and saves were not touched." \
    "Press Close: the screen goes dark for a few seconds, then pocket-ui starts and asks its first questions." \
    "To go back to EmulationStation later: start \"Uninstall pocket-ui\" in Ports (this installer leaves it there), or see the README."
  finish ok
}

if [ "${1:-}" = --worker ]; then
  worker
  exit 0
fi

# --- the front end ------------------------------------------------------------------------------------------------
# EmulationStation's environment, in case this was started some other way (over SSH, for instance).
: "${XDG_RUNTIME_DIR:=/var/run/0-runtime-dir}" "${WAYLAND_DISPLAY:=wayland-1}" "${SDL_VIDEODRIVER:=wayland}"
export XDG_RUNTIME_DIR WAYLAND_DISPLAY SDL_VIDEODRIVER

rm -rf "$WORK"
mkdir -p "$WORK" || exit 1
: >"$LOG"
# What the viewer runs: the log as it grows, until the worker is done.
cat >"$WORK/follow.sh" <<EOF
#!/bin/sh
tail -n +1 -f "$LOG" &
t=\$!
while [ ! -e "$DONE" ]; do sleep 0.3; done
sleep 0.7
kill \$t 2>/dev/null
exit 0
EOF
chmod 755 "$WORK/follow.sh"

if command -v setsid >/dev/null 2>&1; then
  setsid sh "$SELF" --worker >>"$LOG" 2>&1 </dev/null &
else
  sh "$SELF" --worker >>"$LOG" 2>&1 </dev/null &
fi

title="pocket-ui installer"
[ "$MODE" = install ] || title="pocket-ui uninstaller"
if command -v "$VIEWER" >/dev/null 2>&1; then
  "$VIEWER" -w -f "$FONT" -t "$title" -s "$WORK/follow.sh" >/dev/null 2>&1
fi
# Closed early (or no viewer): the worker still finishes by itself; wait for it.
n=0
while [ ! -e "$DONE" ] && [ $n -lt 1200 ]; do sleep 0.5; n=$((n + 1)); done
result=$(cat "$DONE" 2>/dev/null || echo failed)
[ -e "$DONE" ] || { say "pocket-ui installer: the worker did not finish"; }
if ! command -v "$VIEWER" >/dev/null 2>&1; then cat "$LOG"; fi

if [ "$result" = ok ]; then
  # An install removes this entry (it did its job); the interface restarts into pocket-ui, detached from this script,
  # which ends with essway.
  # and leaves "Uninstall pocket-ui.sh" in its place: this file without the payload.
  if [ "$MODE" = install ]; then
    head -c $(( $(wc -c <"$SELF") - PAYLOAD_BYTES )) "$SELF" >"${SELF%/*}/Uninstall pocket-ui.sh" 2>/dev/null && chmod 755 "${SELF%/*}/Uninstall pocket-ui.sh"
    rm -f "$SELF"
  fi
  cmd='sleep 1; rm -f /tmp/pocket-ui.es-only; systemctl reset-failed essway 2>/dev/null; systemctl restart essway'
  if command -v systemd-run >/dev/null 2>&1; then
    systemd-run --no-block --collect --unit=pocket-ui-switch /bin/sh -c "$cmd" >/dev/null 2>&1
  else
    nohup sh -c "$cmd" >/dev/null 2>&1 &
  fi
fi
rm -rf "$WORK" "$STAGE"
exit 0

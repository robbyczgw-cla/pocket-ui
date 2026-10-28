#!/bin/sh
# Takes pocket-ui off a ROCKNIX card again: EmulationStation is the interface as before. Run it on the device as root:
#
#   sh uninstall.sh                removes the start entry and the programs, restarts the interface
#   sh uninstall.sh --keep-programs   only the start entry (pocket-ui stays in /storage/pocket-ui, unused)
#   sh uninstall.sh --no-restart   EmulationStation comes back at the next start of the device
#   sh uninstall.sh --purge        also pocket-ui's settings, logs and history (/storage/.config/pocket-ui)
#
# Games, saves and save states (/storage/roms) are never touched. The boot screen (pocket-splash.service and its link)
# goes with the start entry. What pocket-ui changed outside its folders is put back: the Syncthing drop-in it writes,
# ROCKNIX's smb.conf (kept as smb.conf.rocknix), and the lines in mako's config that hide ROCKNIX's volume box
# while pocket-ui is in front (src/mako_osd.rs). Settings pocket-ui set in
# ROCKNIX's system.cfg (Wi-Fi, volume, SSH switch) stay as they are. Running it again, or without pocket-ui
# installed, is safe.
#
# Tests: deploy/package/test-package.sh (POCKETUI_INSTALL_ROOT is a fake root for it).
set -u
R=${POCKETUI_INSTALL_ROOT:-}
APP=$R/storage/pocket-ui
CONFIG=$R/storage/.config
DROPIN_DIR=$CONFIG/system.d/essway.service.d
DROPIN=$DROPIN_DIR/10-pocket-ui.conf
SPLASH=$CONFIG/system.d/pocket-splash.service
SPLASH_LINK=$CONFIG/system.d/sysinit.target.wants/pocket-splash.service
SYNC_DIR=$CONFIG/system.d/syncthing.service.d
SYNC_DROPIN=$SYNC_DIR/10-pocket-ui.conf
SMB=$CONFIG/smb.conf
MAKO=$CONFIG/mako/config
ES_ONLY=$R/tmp/pocket-ui.es-only

KEEP=0 RESTART=1 PURGE=0
for a in "$@"; do
  case $a in
    --keep-programs) KEEP=1 ;;
    --no-restart) RESTART=0 ;;
    --purge) PURGE=1 ;;
    -h | --help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $a (see sh uninstall.sh --help)" >&2; exit 2 ;;
  esac
done
say() { printf '%s\n' "$@"; }
ok() { say "  ok    $*"; }
die() { say "" "STOPPED: $*" >&2; exit 1; }
[ -n "$R" ] || [ "$(id -u)" = 0 ] || die "run it as root (ROCKNIX's SSH login is root)"

say "== removing pocket-ui"
# The start entry first: whatever happens after this, the next start brings EmulationStation.
if [ -e "$DROPIN" ]; then
  grep -q POCKETUI_AS_ESSWAY "$DROPIN" || die "$DROPIN is not pocket-ui's start entry; left alone"
  rm -f "$DROPIN" || die "cannot remove $DROPIN"
  ok "start entry removed ($DROPIN)"
else
  ok "no start entry ($DROPIN)"
fi
rmdir "$DROPIN_DIR" 2>/dev/null
# The boot screen goes with the start entry (docs/BOOT-SPLASH.md).
if [ -e "$SPLASH" ] || [ -L "$SPLASH_LINK" ]; then
  if [ -e "$SPLASH" ] && ! grep -q 'pocket-splash' "$SPLASH"; then
    die "$SPLASH is not pocket-ui's boot screen; left alone"
  fi
  rm -f "$SPLASH_LINK" "$SPLASH" || die "cannot remove $SPLASH"
  rmdir "$(dirname "$SPLASH_LINK")" 2>/dev/null
  ok "boot screen removed ($SPLASH)"
fi
if [ -e "$SYNC_DROPIN" ]; then
  rm -f "$SYNC_DROPIN" && rmdir "$SYNC_DIR" 2>/dev/null
  ok "pocket-ui's Syncthing drop-in removed"
fi
if head -n 1 "$SMB" 2>/dev/null | grep -q '^# Written by pocket-ui'; then
  # The original pocket-ui kept, or ROCKNIX's default when there was none.
  if [ -f "$SMB.rocknix" ]; then
    mv -f "$SMB.rocknix" "$SMB"
  else
    cp "$R/usr/config/smb.conf" "$SMB"
  fi || die "cannot put ROCKNIX's smb.conf back"
  ok "ROCKNIX's smb.conf back"
fi
# The lines that hide ROCKNIX's volume and brightness box while pocket-ui is in front (src/mako_osd.rs): the block
# between its two marker lines goes, the rest of mako's config stays as it is.
if grep -q '^# BEGIN pocket-ui' "$MAKO" 2>/dev/null; then
  sed '/^# BEGIN pocket-ui/,/^# END pocket-ui/d' "$MAKO" >"$MAKO.new" && mv -f "$MAKO.new" "$MAKO" || die "cannot clean $MAKO"
  makoctl mode -r pocket-ui >/dev/null 2>&1
  makoctl reload >/dev/null 2>&1
  ok "ROCKNIX's volume box is back everywhere (pocket-ui's lines gone from $MAKO)"
fi
systemctl daemon-reload 2>/dev/null
rm -f "$ES_ONLY"

if [ $KEEP = 1 ]; then
  ok "programs kept in /storage/pocket-ui (--keep-programs)"
elif [ -d "$APP" ] || [ -L "$APP" ]; then
  # Only a folder that is pocket-ui's: its launcher or releases are in it.
  if [ -e "$APP/run-pocket-ui.sh" ] || [ -d "$APP/releases" ]; then
    rm -rf "$APP" || die "cannot remove $APP"
    ok "programs removed (/storage/pocket-ui)"
  else
    die "$APP does not look like pocket-ui's folder; left alone"
  fi
else
  ok "no programs (/storage/pocket-ui)"
fi
if [ $PURGE = 1 ]; then
  rm -rf "$CONFIG/pocket-ui"
  ok "settings, logs and history removed (/storage/.config/pocket-ui)"
elif [ -d "$CONFIG/pocket-ui" ]; then
  ok "settings kept in /storage/.config/pocket-ui (a new install picks them up; --purge removes them)"
fi

if grep -q 'essway\.service' /proc/self/cgroup 2>/dev/null && [ -z "$R" ]; then
  RESTART=0
  say "  note  started from inside the interface: restart the device to get EmulationStation back"
fi
if [ $RESTART = 1 ] && [ -f "$R/usr/lib/systemd/system/essway.service" ]; then
  systemctl reset-failed essway 2>/dev/null
  systemctl restart essway
  say "" "Done. EmulationStation is starting. Games and saves are untouched."
else
  say "" "Done. EmulationStation comes back at the next start of the device. Games and saves are untouched."
fi

#!/bin/sh
# Tests of deploy/package/install.sh and uninstall.sh against a fake root (no device, no root rights, no Docker):
#   sh deploy/package/test-package.sh
# The root is a folder with ROCKNIX's os-release, a device-tree model, essway.service, start_es.sh and /storage;
# systemctl is a fake that logs its calls and "loads" the drop-in when the file is there; pocket-ui and pocket-core are
# shell stand-ins that print their version and a compatibility report chosen by the test. Each case runs under every
# shell found of dash, bash (ROCKNIX's sh) and busybox sh. POCKETUI_PACKAGE=<dir> tests the scripts in another folder.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
PKG_SRC=${POCKETUI_PACKAGE:-$HERE}
REPO=$(cd "$HERE/../.." && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
V=9.8.7
fails=0
REAL_DF=$(command -v df)

check() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "FAIL $1"
    fails=$((fails + 1))
  fi
}

# The fakes, in $T/bin (first in PATH).
mkdir -p "$T/bin"
cat >"$T/bin/systemctl" <<'EOF'
#!/bin/sh
echo "systemctl $*" >>"$FAKE_ROOT/calls"
D=$FAKE_ROOT/storage/.config/system.d/essway.service.d/10-pocket-ui.conf
case "$*" in
  "is-enabled essway") echo "${FAKE_ENABLED:-enabled}" ;;
  "show -p DropInPaths --value essway") [ -f "$D" ] && [ -z "${FAKE_IGNORES:-}" ] && echo "/storage/.config/system.d/essway.service.d/10-pocket-ui.conf" ;;
  "show -p Environment --value essway") [ -f "$D" ] && [ -z "${FAKE_IGNORES:-}" ] && sed -n 's/^Environment=//p' "$D" ;;
esac
exit 0
EOF
cat >"$T/bin/makoctl" <<'EOF'
#!/bin/sh
echo "makoctl $*" >>"$FAKE_ROOT/calls"
EOF
cat >"$T/bin/systemd-run" <<'EOF'
#!/bin/sh
echo "systemd-run $*" >>"$FAKE_ROOT/calls"
EOF
# ROCKNIX's text_viewer, as far as the port installer uses it: -s <script> runs the script and shows its output (here:
# collected in $VIEWER_OUT); the real one then waits for the player to press Close.
cat >"$T/bin/fake-viewer" <<'EOF'
#!/bin/sh
while [ $# -gt 0 ]; do
  case $1 in -s) script=$2; shift ;; esac
  shift
done
echo "viewer opened" >>"$VIEWER_OUT"
[ -z "${script:-}" ] || sh "$script" >>"$VIEWER_OUT" 2>&1
echo "viewer closed" >>"$VIEWER_OUT"
EOF
cat >"$T/bin/df" <<EOF
#!/bin/sh
if [ -n "\${FAKE_FREE_KB:-}" ]; then
  echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
  echo "/dev/fake 100000 0 \$FAKE_FREE_KB 0% /storage"
else
  exec $REAL_DF "\$@"
fi
EOF
chmod 755 "$T/bin/"*
export PATH="$T/bin:$PATH"

# The package as make-install-zip.sh lays it out, with stand-in programs.
make_pkg() {
  P=$T/pkg
  rm -rf "$P"
  mkdir -p "$P/files"
  cp "$PKG_SRC/install.sh" "$PKG_SRC/uninstall.sh" "$P/"
  for b in pocket-ui pocket-core pocket-splash; do
    cat >"$P/files/$b" <<EOF
#!/bin/sh
# stand-in $b
case "\$1" in
  --version) echo "\${FAKE_VERSION_$(echo $b | tr - _):-$b $V}" ;;
  --check-os-update)
    echo "Checking \$2 for pocket-ui $V"
    case "\${FAKE_COMPAT:-ok}" in
      ok) echo "info   start_es.sh is known"; echo "BLOCKS This system update contains the tool that rewrites the internal bootloader. It was not installed."; echo "REFUSED system 20261001: This system update contains the tool that rewrites the internal bootloader. It was not installed."; exit 1 ;;
      clean) echo "system 20261001: would be installed" ;;
      warns) echo "WARNS  no wifictl: Wi-Fi settings won't work"; echo "system 20261001: would be installed, with the notes above" ;;
      glibc) echo "BLOCKS pocket-ui needs glibc 2.38 or later (libc.so.6), the new system has glibc 2.37"; echo "REFUSED system 20261001: This system update would break pocket-ui"; exit 1 ;;
      broken) echo "REFUSED The new system has no version information. It was not installed."; exit 1 ;;
    esac ;;
esac
EOF
  done
  cp "$REPO/deploy/portmaster-setup.sh" "$REPO/deploy/run-pocket-ui.sh" "$P/files/"
  cp "$REPO/deploy/essway-pocket-ui.conf" "$P/files/10-pocket-ui.conf"
  cp "$REPO/deploy/pocket-splash.service" "$P/files/pocket-splash.service"
  chmod 644 "$P/files/"*
  # The licence texts as scripts/licence-files.sh lays them out (stand-in contents; the real ones: LICENCE_FILES below).
  mkdir -p "$P/licences"
  for f in LICENSE NOTICE.md FONTS-OFL.txt THIRD-PARTY-CRATES.md THIRD-PARTY-LICENSES.txt; do echo "text of $f" >"$P/licences/$f"; done
  echo "$V" >"$P/files/VERSION"
  (cd "$P/files" && sha256sum pocket-ui pocket-core pocket-splash portmaster-setup.sh run-pocket-ui.sh 10-pocket-ui.conf pocket-splash.service VERSION >SHA256SUMS)
}

# A ROCKNIX 20261001 card on a Mini V2 after its first start.
make_root() {
  R=$T/root
  rm -rf "$R"
  mkdir -p "$R/etc" "$R/sys/firmware/devicetree/base" "$R/usr/lib/systemd/system" "$R/usr/bin" "$R/usr/config" \
    "$R/storage/.config/system.d" "$R/storage/roms/gba" "$R/tmp"
  cat >"$R/etc/os-release" <<'EOF'
OS_NAME="ROCKNIX"
OS_VERSION="20261001"
HW_DEVICE="SM8250"
HW_ARCH="aarch64"
EOF
  printf 'Retroid Pocket Mini V2\000' >"$R/sys/firmware/devicetree/base/model"
  echo '[Service]' >"$R/usr/lib/systemd/system/essway.service"
  printf '#!/bin/sh\n' >"$R/usr/bin/start_es.sh"
  chmod 755 "$R/usr/bin/start_es.sh"
  echo '[global] rocknix default' >"$R/usr/config/smb.conf"
  echo 'save' >"$R/storage/roms/gba/game.srm"
  : >"$R/calls"
}

# Every path under the root with its type, mode, link target and checksum: "nothing changed" compares two of these.
tree() { (cd "$T/root" && find . ! -name calls -exec sh -c 'for p; do
  if [ -L "$p" ]; then echo "$p -> $(readlink "$p")"; elif [ -f "$p" ]; then echo "$p $(stat -c %a "$p") $(cksum <"$p")"; else echo "$p/"; fi
done' sh {} + | sort); }

inst() { (cd "$T" && FAKE_ROOT=$T/root POCKETUI_INSTALL_ROOT=$T/root $SH "$T/pkg/install.sh" "$@" >"$T/out" 2>&1); }
uninst() { (cd "$T" && FAKE_ROOT=$T/root POCKETUI_INSTALL_ROOT=$T/root $SH "$T/root/storage/pocket-ui/uninstall.sh" "$@" >"$T/out" 2>&1); }
uninst_pkg() { (cd "$T" && FAKE_ROOT=$T/root POCKETUI_INSTALL_ROOT=$T/root $SH "$T/pkg/uninstall.sh" "$@" >"$T/out" 2>&1); }
said() { grep -q -- "$1" "$T/out"; }
APP=storage/pocket-ui
DROPIN=storage/.config/system.d/essway.service.d/10-pocket-ui.conf
SPLASH=storage/.config/system.d/pocket-splash.service
SPLASH_LINK=storage/.config/system.d/sysinit.target.wants/pocket-splash.service

# The port installer (port-installer.sh as "Install pocket-ui.sh" in roms/ports): with the package as its payload
# ("payload"), or as the bare starter ("thin"; with a second argument "zip" the package's zip lies next to it).
PORTS=storage/roms/ports
make_port() {
  mkdir -p "$T/root/$PORTS"
  if [ "$1" = payload ]; then
    (cd "$T" && tar -czf port.tgz --transform 's,^pkg,pocket-ui-9.8.7-rocknix-install,' pkg)
    sed "s/^PAYLOAD_BYTES=0\$/PAYLOAD_BYTES=$(wc -c <"$T/port.tgz" | tr -d ' ')/" "$PKG_SRC/port-installer.sh" >"$T/root/$PORTS/Install pocket-ui.sh"
    cat "$T/port.tgz" >>"$T/root/$PORTS/Install pocket-ui.sh"
  else
    cp "$PKG_SRC/port-installer.sh" "$T/root/$PORTS/Install pocket-ui.sh"
    if [ "${2:-}" = zip ]; then
      rm -rf "$T/zipsrc" && mkdir "$T/zipsrc" && cp -r "$T/pkg" "$T/zipsrc/pocket-ui-9.8.7-rocknix-install"
      (cd "$T/zipsrc" && python3 -I -c "
import os, zipfile
with zipfile.ZipFile('$T/root/$PORTS/pocket-ui-9.8.7-rocknix-install.zip', 'w') as z:
    for d, _, fs in os.walk('pocket-ui-9.8.7-rocknix-install'):
        for f in fs:
            p = os.path.join(d, f)
            i = zipfile.ZipInfo.from_file(p)
            i.external_attr = (0o100000 | os.stat(p).st_mode) << 16
            z.writestr(i, open(p, 'rb').read())
")
    fi
  fi
  chmod 755 "$T/root/$PORTS/Install pocket-ui.sh"
}
# Starts a port the way EmulationStation does (runemu.sh: bash <script>): $1 is the file's name in roms/ports.
port() {
  : >"$T/viewer.out"
  (cd "$T" && FAKE_ROOT=$T/root POCKETUI_INSTALL_ROOT=$T/root POCKETUI_VIEWER=$T/bin/fake-viewer VIEWER_OUT=$T/viewer.out $SH "$T/root/$PORTS/$1" >"$T/out" 2>&1)
}
shown() { grep -q -- "$1" "$T/viewer.out"; }

# A refusal: exit 1, the message, and not one file changed. $1 name, $2 expected text, rest: install.sh options.
refused() {
  name=$1 text=$2
  shift 2
  before=$(tree)
  inst "$@"
  rc=$?
  check "$SH: refused, $name (exit 1)" '[ $rc = 1 ]'
  check "$SH: refused, $name: says why" 'said "NOT INSTALLED: .*$text"' || sed 's/^/     /' "$T/out"
  check "$SH: refused, $name: nothing changed" '[ "$(tree)" = "$before" ] && ! grep -q "daemon-reload\|restart" "$T/root/calls"'
}

SHELLS=
for s in dash bash "busybox sh"; do
  command -v ${s%% *} >/dev/null && SHELLS="$SHELLS${SHELLS:+:}$s"
done
IFS_OLD=$IFS
IFS=:
for SH in $SHELLS; do
  IFS=$IFS_OLD
  echo "== $SH"

  # --check: every check, nothing written.
  make_pkg
  make_root
  before=$(tree)
  inst --check
  rc=$?
  check "$SH: --check passes on ROCKNIX 20261001, Mini V2 (exit 0)" '[ $rc = 0 ] && said "All checks passed"' || sed 's/^/     /' "$T/out"
  check "$SH: --check changes nothing" '[ "$(tree)" = "$before" ] && ! grep -q "daemon-reload\|restart" "$T/root/calls"'

  # A fresh install.
  inst
  rc=$?
  check "$SH: install (exit 0)" '[ $rc = 0 ] && said "Installed. pocket-ui is starting now"' || sed 's/^/     /' "$T/out"
  r=$T/root
  check "$SH: releases/$V holds the programs, mode 755" \
    'for f in pocket-ui pocket-core pocket-splash portmaster-setup.sh; do cmp -s "$T/pkg/files/$f" "$r/$APP/releases/$V/$f" && [ "$(stat -c %a "$r/$APP/releases/$V/$f")" = 755 ] || exit 1; done'
  check "$SH: current -> releases/$V, the three links through current" \
    '[ "$(readlink "$r/$APP/current")" = "releases/$V" ] && for f in pocket-ui pocket-core portmaster-setup.sh; do [ "$(readlink "$r/$APP/$f")" = "current/$f" ] || exit 1; done'
  check "$SH: no previous on a fresh install" '[ ! -e "$r/$APP/previous" ] && [ ! -L "$r/$APP/previous" ]'
  check "$SH: launcher is deploy/run-pocket-ui.sh, executable" 'cmp -s "$REPO/deploy/run-pocket-ui.sh" "$r/$APP/run-pocket-ui.sh" && [ -x "$r/$APP/run-pocket-ui.sh" ]'
  check "$SH: drop-in is deploy/essway-pocket-ui.conf, mode 644" 'cmp -s "$REPO/deploy/essway-pocket-ui.conf" "$r/$DROPIN" && [ "$(stat -c %a "$r/$DROPIN")" = 644 ]'
  check "$SH: boot screen: deploy/pocket-splash.service, mode 644, linked from sysinit.target.wants" \
    'cmp -s "$REPO/deploy/pocket-splash.service" "$r/$SPLASH" && [ "$(stat -c %a "$r/$SPLASH")" = 644 ] && [ "$(readlink "$r/$SPLASH_LINK")" = ../pocket-splash.service ]'
  check "$SH: boot screen: never started by the install" '! grep -q "pocket-splash" "$r/calls"'
  check "$SH: licences: all five texts in /storage/pocket-ui/licences, as packaged, mode 644" \
    'for f in LICENSE NOTICE.md FONTS-OFL.txt THIRD-PARTY-CRATES.md THIRD-PARTY-LICENSES.txt; do cmp -s "$T/pkg/licences/$f" "$r/$APP/licences/$f" && [ "$(stat -c %a "$r/$APP/licences/$f")" = 644 ] || exit 1; done && [ -z "$(find "$r/$APP" -name "licences.new")" ]'
  check "$SH: uninstall.sh copied next to the programs" 'cmp -s "$T/pkg/uninstall.sh" "$r/$APP/uninstall.sh"'
  check "$SH: no staging folder left" '[ -z "$(find "$r/storage" -name ".pocket-ui-staging*")" ]'
  check "$SH: daemon-reload, then essway restarted" \
    'grep -n "daemon-reload" "$r/calls" | head -1 | grep -q . && grep -q "systemctl restart essway" "$r/calls" && [ "$(grep -n "daemon-reload" "$r/calls" | head -1 | cut -d: -f1)" -lt "$(grep -n "restart essway" "$r/calls" | cut -d: -f1)" ]'
  check "$SH: games untouched" '[ "$(cat "$r/storage/roms/gba/game.srm")" = save ]'

  # Again: idempotent.
  before=$(tree | grep -v installed-by-package)
  : >"$r/calls"
  inst
  rc=$?
  check "$SH: second install (exit 0), says what is already in place" '[ $rc = 0 ] && said "already in place" && said "start entry already in place" && said "boot screen already in place"' || sed 's/^/     /' "$T/out"
  check "$SH: second install changes no file" '[ "$(tree | grep -v installed-by-package)" = "$before" ]'

  # Upgrade over a dev build (dev.sh's layout), a stale update-pending, an older launcher.
  make_root
  mkdir -p "$r/$APP/releases/dev"
  echo old >"$r/$APP/releases/dev/pocket-ui"
  ln -s releases/dev "$r/$APP/current"
  ln -s current/pocket-ui "$r/$APP/pocket-ui"
  echo 1.0 >"$r/$APP/update-pending"
  echo old >"$r/$APP/run-pocket-ui.sh"
  inst --no-restart
  rc=$?
  check "$SH: upgrade (exit 0)" '[ $rc = 0 ] && said "starts the next time"' || sed 's/^/     /' "$T/out"
  check "$SH: upgrade: previous -> releases/dev, current -> releases/$V" '[ "$(readlink "$r/$APP/previous")" = releases/dev ] && [ "$(readlink "$r/$APP/current")" = "releases/$V" ]'
  check "$SH: upgrade: the old release stays" '[ "$(cat "$r/$APP/releases/dev/pocket-ui")" = old ]'
  check "$SH: upgrade: launcher replaced, update-pending gone" 'cmp -s "$REPO/deploy/run-pocket-ui.sh" "$r/$APP/run-pocket-ui.sh" && [ ! -e "$r/$APP/update-pending" ]'
  check "$SH: --no-restart: essway not restarted" '! grep -q "restart essway" "$r/calls" && grep -q daemon-reload "$r/calls"'

  # The same version, another build (a rebuilt package): replaced.
  echo different >"$r/$APP/releases/$V/pocket-core"
  inst --no-restart
  check "$SH: same version, other build: replaced" 'cmp -s "$T/pkg/files/pocket-core" "$r/$APP/releases/$V/pocket-core" && [ -z "$(find "$r/$APP/releases" -name ".old-*")" ]'

  # Refusals: nothing changes.
  make_root
  sed -i 's/SM8250/RK3566/' "$r/etc/os-release"
  refused "another SoC" "ROCKNIX for RK3566"
  make_root
  sed -i 's/aarch64/x86_64/' "$r/etc/os-release"
  refused "another architecture" "needs ROCKNIX for SM8250"
  make_root
  sed -i 's/ROCKNIX/Batocera/' "$r/etc/os-release"
  refused "not ROCKNIX" "not ROCKNIX"
  make_root
  rm "$r/etc/os-release"
  refused "no os-release" "not ROCKNIX"
  # A package without its licence texts is incomplete (they have to travel with the programs).
  make_root
  rm "$T/pkg/licences/FONTS-OFL.txt"
  refused "a package without the fonts' licence" "licences/FONTS-OFL.txt is missing"
  make_pkg
  make_root
  : >"$T/pkg/licences/THIRD-PARTY-LICENSES.txt"
  refused "a package with an empty crate licence file" "licences/THIRD-PARTY-LICENSES.txt is missing"
  make_pkg
  make_root
  sed -i 's/20261001/20261101/' "$r/etc/os-release"
  refused "an untested ROCKNIX version" "ROCKNIX 20261101 is not a version"
  inst --allow-untested-rocknix --check
  rc=$?
  check "$SH: --allow-untested-rocknix lets it through when the compatibility check passes" '[ $rc = 0 ] && said "untested"'
  make_root
  FAKE_COMPAT=glibc
  export FAKE_COMPAT
  sed -i 's/20261001/20261101/' "$r/etc/os-release"
  refused "untested version allowed, but the compatibility check fails" "lacks something pocket-ui needs" --allow-untested-rocknix
  check "$SH: ... and it names what is missing" 'said "missing  pocket-ui needs glibc 2.38"'
  unset FAKE_COMPAT
  for experimental in 'Retroid Pocket 5' 'Retroid Pocket 5 Visionox' 'Retroid Pocket Flip2' 'Retroid Pocket Flip2 Visionox' 'Retroid Pocket Mini' 'Retroid Pocket 6' 'Retroid Pocket 6 TOP-DPAD' 'Retroid Pocket Nova' 'AYN Odin 2' 'AYN Odin 2 Mini' 'AYN Odin 2 Portal'; do
    make_root
    case "$experimental" in 'Retroid Pocket 6'*|'Retroid Pocket Nova'|'AYN Odin 2'*) sed -i 's/SM8250/SM8550/' "$r/etc/os-release" ;; esac
    printf '%s\000' "$experimental" >"$r/sys/firmware/devicetree/base/model"
    inst --check
    rc=$?
    check "$SH: the $experimental passes as experimental, without an option" '[ $rc = 0 ] && said "device: $experimental, experimental" && said "Report a problem"'
  done
  make_root
  printf 'AYN Thor Lite\000' >"$r/sys/firmware/devicetree/base/model"
  refused "another device" "this device is 'AYN Thor Lite'"
  inst --allow-other-device --check
  rc=$?
  check "$SH: --allow-other-device lets it through" '[ $rc = 0 ] && said "allowed by --allow-other-device"'
  make_root
  rm "$r/sys/firmware/devicetree/base/model"
  refused "no device tree model" "this device is 'unknown'"
  make_root
  rm "$r/usr/lib/systemd/system/essway.service"
  refused "no essway.service" "essway.service is missing"
  make_root
  rm "$r/usr/bin/start_es.sh"
  refused "no start_es.sh" "start_es.sh is missing"
  make_root
  FAKE_ENABLED=masked refused "essway masked" "essway is masked"
  make_root
  rmdir "$r/storage/.config/system.d"
  refused "ROCKNIX's first start has not run" "start ROCKNIX normally once"
  make_root
  mkdir -p "$r/storage/.pocket-seed"
  refused "a pocket-ui image still setting up" "pocket-ui SD image"
  make_root
  # BusyBox's sh runs its own df applet, not the fake one in PATH.
  if [ "$SH" = "busybox sh" ]; then
    echo "skip $SH: not enough room (df is a BusyBox applet here)"
  else
    FAKE_FREE_KB=1000 refused "not enough room" "not enough room on /storage"
  fi
  make_root
  FAKE_VERSION_pocket_core="pocket-core 9.8.6" refused "pocket-core is another version" "pocket-core does not run here"
  make_root
  printf '\177ELF garbage' >"$T/pkg/files/pocket-ui"
  (cd "$T/pkg/files" && sha256sum pocket-ui pocket-core pocket-splash portmaster-setup.sh run-pocket-ui.sh 10-pocket-ui.conf pocket-splash.service VERSION >SHA256SUMS)
  refused "a program that cannot run here" "pocket-ui does not run here"
  make_pkg
  make_root
  FAKE_COMPAT=glibc refused "the compatibility check finds something missing" "lacks something pocket-ui needs"
  make_root
  FAKE_COMPAT=broken refused "the compatibility check cannot read the system" "could not read this system"
  make_root
  echo tampered >>"$T/pkg/files/run-pocket-ui.sh"
  refused "a damaged package" "package is damaged"
  make_pkg
  make_root
  rm "$T/pkg/files/VERSION"
  refused "an incomplete package" "files/VERSION is missing"
  make_pkg

  # The compatibility check's notes and a clean report pass.
  make_root
  FAKE_COMPAT=warns inst --check
  rc=$?
  check "$SH: notes of the compatibility check are shown, install allowed" '[ $rc = 0 ] && said "note  no wifictl"'
  FAKE_COMPAT=clean inst --check
  rc=$?
  check "$SH: a report without the bootloader line passes too" '[ $rc = 0 ]'

  # systemd does not take the drop-in: removed again.
  make_root
  FAKE_IGNORES=1 inst
  rc=$?
  check "$SH: drop-in not loaded: exit 1, drop-in and its folder removed, no boot screen, essway not restarted" \
    '[ $rc = 1 ] && [ ! -e "$r/$SPLASH" ] && [ ! -L "$r/$SPLASH_LINK" ] && [ ! -e "$r/$DROPIN" ] && [ ! -d "$(dirname "$r/$DROPIN")" ] && ! grep -q "restart essway" "$r/calls" && said "did not take the start entry"'

  # Uninstall after an install, with what pocket-ui changes at run time.
  make_root
  inst >/dev/null
  mkdir -p "$r/storage/.config/pocket-ui" "$r/storage/.config/system.d/syncthing.service.d"
  echo '{}' >"$r/storage/.config/pocket-ui/settings.json"
  echo '[Service]' >"$r/storage/.config/system.d/syncthing.service.d/10-pocket-ui.conf"
  echo '[global] the user had this' >"$r/storage/.config/smb.conf.rocknix"
  printf '# Written by pocket-ui; ROCKNIX'"'"'s original is kept next to it as smb.conf.rocknix.\n[global]\n' >"$r/storage/.config/smb.conf"
  touch "$r/tmp/pocket-ui.es-only"
  # mako's config as ROCKNIX wrote it plus the block pocket-ui adds (src/mako_osd.rs): the block goes, ROCKNIX's lines stay.
  mkdir -p "$r/storage/.config/mako"
  printf 'max-visible=1\nwidth=500\n# BEGIN pocket-ui (hides it)\n[mode=pocket-ui app-name=mako-notify]\ninvisible=1\n# END pocket-ui\n' >"$r/storage/.config/mako/config"
  : >"$r/calls"
  uninst
  rc=$?
  check "$SH: uninstall (exit 0)" '[ $rc = 0 ] && said "EmulationStation is starting"' || sed 's/^/     /' "$T/out"
  check "$SH: uninstall: drop-in and its folder gone" '[ ! -e "$r/$DROPIN" ] && [ ! -d "$(dirname "$r/$DROPIN")" ]'
  check "$SH: uninstall: boot screen's unit, link and wants folder gone" '[ ! -e "$r/$SPLASH" ] && [ ! -L "$r/$SPLASH_LINK" ] && [ ! -d "$(dirname "$r/$SPLASH_LINK")" ]'
  check "$SH: uninstall: /storage/pocket-ui gone" '[ ! -e "$r/$APP" ]'
  check "$SH: uninstall: Syncthing drop-in gone" '[ ! -e "$r/storage/.config/system.d/syncthing.service.d" ]'
  check "$SH: uninstall: the user's smb.conf back" '[ "$(cat "$r/storage/.config/smb.conf")" = "[global] the user had this" ] && [ ! -e "$r/storage/.config/smb.conf.rocknix" ]'
  check "$SH: uninstall: pocket-ui's lines gone from mako's config, ROCKNIX's stay, mako told" \
    '[ "$(cat "$r/storage/.config/mako/config")" = "$(printf "max-visible=1\nwidth=500")" ] && grep -q "makoctl mode -r pocket-ui" "$r/calls" && grep -q "makoctl reload" "$r/calls"'
  check "$SH: uninstall: settings and games kept" '[ -f "$r/storage/.config/pocket-ui/settings.json" ] && [ "$(cat "$r/storage/roms/gba/game.srm")" = save ]'
  check "$SH: uninstall: daemon-reload, essway restarted" 'grep -q daemon-reload "$r/calls" && grep -q "restart essway" "$r/calls"'
  before=$(tree)
  uninst_pkg
  rc=$?
  check "$SH: uninstall again: exit 0, nothing changes" '[ $rc = 0 ] && [ "$(tree)" = "$before" ]' || sed 's/^/     /' "$T/out"
  uninst_pkg --purge --no-restart
  check "$SH: --purge removes the settings" '[ ! -e "$r/storage/.config/pocket-ui" ] && [ "$(cat "$r/storage/roms/gba/game.srm")" = save ]'

  # smb.conf written by pocket-ui with no original kept: ROCKNIX's default comes back.
  make_root
  printf '# Written by pocket-ui; ...\n' >"$r/storage/.config/smb.conf"
  uninst_pkg --no-restart
  check "$SH: uninstall: no original smb.conf kept: ROCKNIX's default" '[ "$(cat "$r/storage/.config/smb.conf")" = "[global] rocknix default" ]'

  # A drop-in that is not ours, a folder that is not ours: left alone.
  make_root
  mkdir -p "$(dirname "$r/$DROPIN")"
  echo '[Service]' >"$r/$DROPIN"
  uninst_pkg
  rc=$?
  check "$SH: a foreign drop-in of that name is left alone (exit 1)" '[ $rc = 1 ] && [ -f "$r/$DROPIN" ] && said "not pocket-ui"'
  make_root
  echo '[Service]' >"$r/$SPLASH"
  uninst_pkg
  rc=$?
  check "$SH: a foreign pocket-splash.service is left alone (exit 1)" '[ $rc = 1 ] && [ -f "$r/$SPLASH" ] && said "not pocket-ui"'
  make_root
  mkdir -p "$r/$APP"
  echo mine >"$r/$APP/notes.txt"
  uninst_pkg
  rc=$?
  check "$SH: a /storage/pocket-ui that is not pocket-ui's is left alone (exit 1)" '[ $rc = 1 ] && [ -f "$r/$APP/notes.txt" ]'

  # --keep-programs.
  make_root
  inst >/dev/null
  : >"$r/calls"
  uninst --keep-programs --no-restart
  check "$SH: --keep-programs removes only the start entry and the boot screen" '[ ! -e "$r/$DROPIN" ] && [ ! -e "$r/$SPLASH" ] && [ ! -L "$r/$SPLASH_LINK" ] && [ -x "$r/$APP/run-pocket-ui.sh" ] && ! grep -q "restart essway" "$r/calls"'

  # The port installer (no SSH): Install pocket-ui.sh in roms/ports, started from EmulationStation's Ports.
  make_pkg
  make_root
  r=$T/root
  make_port payload
  port "Install pocket-ui.sh"
  rc=$?
  check "$SH: port: launch -> checks -> install -> switch (exit 0, the screen showed every step)" \
    '[ $rc = 0 ] && shown "Step 1 of 3" && shown "checks passed" && shown "Step 2 of 3" && shown "Step 3 of 3" && shown "pocket-ui $V is installed" && shown "viewer closed"' || sed 's/^/     /' "$T/viewer.out" "$T/out"
  check "$SH: port: installed like install.sh does (releases/$V, current, drop-in, boot screen)" \
    '[ "$(readlink "$r/$APP/current")" = "releases/$V" ] && cmp -s "$REPO/deploy/essway-pocket-ui.conf" "$r/$DROPIN" && [ -L "$r/$SPLASH_LINK" ]'
  check "$SH: port: install.sh's own restart is off, the switch is detached through systemd-run and restarts essway" \
    '! grep -q "^systemctl restart essway" "$r/calls" && grep -q "^systemd-run --no-block .* restart essway" "$r/calls"'
  check "$SH: port: leaves \"Uninstall pocket-ui.sh\" in its place (the same script without the package)" \
    '[ -x "$r/$PORTS/Uninstall pocket-ui.sh" ] && [ "$(wc -c <"$r/$PORTS/Uninstall pocket-ui.sh")" -lt 20000 ]'
  check "$SH: port: the entry removed itself, no staging or work folder left, games untouched" \
    '[ ! -e "$r/$PORTS/Install pocket-ui.sh" ] && [ -z "$(find "$r/storage" "$r/tmp" -name ".pocket-ui-port*" -o -name "pocket-ui-port")" ] && [ "$(cat "$r/storage/roms/gba/game.srm")" = save ]'
  # Twice: the second launch (a fresh copy of the file) changes nothing and says so.
  make_port payload
  before=$(tree)
  : >"$r/calls"
  port "Install pocket-ui.sh"
  check "$SH: port: second run says it is already installed, changes nothing, no restart" \
    'shown "already installed" && [ "$(tree)" = "$before" ] && ! grep -q "restart essway" "$r/calls"' || sed 's/^/     /' "$T/viewer.out"

  # Failures: a sentence what does not fit and how to get back, nothing written, no switch, the entry stays.
  portfail() {
    name=$1 text=$2
    before=$(tree)
    : >"$r/calls"
    port "Install pocket-ui.sh"
    check "$SH: port: $name: the screen says why and how to get back" 'shown "$text" && shown "Press Close to go back to EmulationStation"' || sed 's/^/     /' "$T/viewer.out"
    check "$SH: port: $name: nothing changed, no restart, the entry stays" '[ "$(tree)" = "$before" ] && ! grep -q "restart" "$r/calls" && [ -f "$r/$PORTS/Install pocket-ui.sh" ]'
  }
  make_root
  sed -i 's/20261001/20261101/' "$r/etc/os-release"
  make_port payload
  portfail "an untested ROCKNIX version" "ROCKNIX 20261101 is not a version"
  make_root
  printf 'AYN Thor Lite\000' >"$r/sys/firmware/devicetree/base/model"
  make_port payload
  portfail "another device" "this device is 'AYN Thor Lite'"
  make_root
  sed -i 's/ROCKNIX/Batocera/' "$r/etc/os-release"
  make_port payload
  portfail "not ROCKNIX" "not ROCKNIX"
  if [ "$SH" != "busybox sh" ]; then
    make_root
    make_port payload
    export FAKE_FREE_KB=1000
    portfail "not enough room" "not enough free space"
    unset FAKE_FREE_KB
  fi
  make_root
  make_port payload
  head -c -2000 "$r/$PORTS/Install pocket-ui.sh" >"$T/short" && cat "$T/short" >"$r/$PORTS/Install pocket-ui.sh"
  portfail "a damaged file" "damaged"

  # Without the payload: the zip next to it, or a sentence what is missing.
  if command -v unzip >/dev/null; then
    make_root
    make_port thin zip
    port "Install pocket-ui.sh"
    check "$SH: port: starter + zip next to it installs" 'shown "using pocket-ui-9.8.7-rocknix-install.zip" && shown "pocket-ui $V is installed" && [ "$(readlink "$r/$APP/current")" = "releases/$V" ]' || sed 's/^/     /' "$T/viewer.out"
  fi
  make_root
  make_port thin
  portfail "the starter alone" "installer data is missing"

  # The uninstaller: "Uninstall pocket-ui" in Ports (the file is port-installer.sh under that name).
  # (the one the install left in roms/ports: a second install first, in a fresh root)
  make_root
  make_port payload
  port "Install pocket-ui.sh"
  : >"$r/calls"
  port "Uninstall pocket-ui.sh"
  check "$SH: port: Uninstall removes pocket-ui, keeps games, switches back to EmulationStation" \
    'shown "EmulationStation starts" && [ ! -e "$r/$APP" ] && [ ! -e "$r/$DROPIN" ] && [ "$(cat "$r/storage/roms/gba/game.srm")" = save ] && grep -q "^systemd-run .* restart essway" "$r/calls" && ! grep -q "^systemctl restart essway" "$r/calls"' || sed 's/^/     /' "$T/viewer.out"
  : >"$r/calls"
  port "Uninstall pocket-ui.sh"
  check "$SH: port: Uninstall on a card without pocket-ui says so, no restart" 'shown "not installed" && ! grep -q "restart" "$r/calls"'
  IFS=:
done
IFS=$IFS_OLD

# The real licence folder (scripts/licence-files.sh, what make-install-zip.sh and make-image.sh put in): the repository's
# own texts, every crate of the list with a text, the fonts' OFL. Needs cargo (third-party.sh reads cargo metadata).
if [ -z "${POCKETUI_PACKAGE:-}" ] && command -v cargo >/dev/null; then
  if sh "$REPO/scripts/licence-files.sh" "$T/lic" >/dev/null 2>&1; then
    check "licence-files.sh: LICENSE, NOTICE.md and the fonts' OFL are the repository's" \
      'cmp -s "$REPO/LICENSE" "$T/lic/LICENSE" && cmp -s "$REPO/NOTICE.md" "$T/lic/NOTICE.md" && cmp -s "$REPO/assets/fonts/OFL.txt" "$T/lic/FONTS-OFL.txt"'
    check "licence-files.sh: the OFL is the SIL Open Font License" 'grep -q "SIL OPEN FONT LICENSE Version 1.1" "$T/lic/FONTS-OFL.txt"'
    check "licence-files.sh: the crate list and the texts name the same crates (a text for every crate but the few without a file)" \
      'n=$(grep -c "^| " "$T/lic/THIRD-PARTY-CRATES.md"); t=$(grep -c "^-\{100\}\$" "$T/lic/THIRD-PARTY-LICENSES.txt"); [ "$t" -ge 100 ] && [ "$n" -ge 100 ]'
  else
    check "licence-files.sh writes the licence folder" false
  fi
else
  echo "skip licence-files.sh (needs cargo; or POCKETUI_PACKAGE is set)"
fi

echo
if [ $fails = 0 ]; then echo "all package tests passed ($SHELLS)"; else echo "$fails package tests failed"; exit 1; fi

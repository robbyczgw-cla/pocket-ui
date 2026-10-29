#!/bin/sh
# Installs pocket-ui on a ROCKNIX card that is already in use (Retroid Pocket Mini V2, ROCKNIX SM8250; the Pocket 5,
# Flip 2 and first Mini on the same image are experimental, docs/DEVICES.md). Run it on the
# device as root, from the unpacked package (README.txt):
#
#   sh install.sh             checks, installs, then restarts the interface: pocket-ui comes up
#   sh install.sh --check     only the checks; nothing changes
#   sh install.sh --no-restart   installs, but pocket-ui only comes up at the next start of the device
#   --allow-untested-rocknix  a ROCKNIX older than the tested one, if pocket-ui's own check passes (newer ones need no option)
#   --allow-other-device      another SM8250 device than these four (untested, unknown to pocket-ui)
#
# What it writes, and only after every check passed: /storage/pocket-ui/ (the programs, as the updater lays them out)
# and, in /storage/.config/system.d, the start entry essway.service.d/10-pocket-ui.conf, which makes ROCKNIX start
# pocket-ui instead of EmulationStation, and the boot screen pocket-splash.service with its link in
# sysinit.target.wants/ (docs/BOOT-SPLASH.md). Nothing on /flash, nothing of ROCKNIX's system, no bootloader. Running it again is safe:
# what is already in place stays. uninstall.sh takes it out again.
#
# Tests: deploy/package/test-package.sh (a fake root; POCKETUI_INSTALL_ROOT, POCKETUI_MODEL_FILE are for it).
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
R=${POCKETUI_INSTALL_ROOT:-}
# The ROCKNIX version this release was tested on. Newer ones are accepted (pocket-ui's check still has to pass); older ones are not.
TESTED_ROCKNIX="20261001"
MODEL_WANTED="Retroid Pocket Mini V2"
# The Mini V2's siblings on the same ROCKNIX SM8250 image, as their device trees name them (src/device.rs): pocket-ui
# knows them, nobody on the project has tested on one.
MODELS_EXPERIMENTAL="Retroid Pocket 5|Retroid Pocket 5 Visionox|Retroid Pocket Flip2|Retroid Pocket Flip2 Visionox|Retroid Pocket Mini|Retroid Pocket 6|Retroid Pocket 6 TOP-DPAD|Retroid Pocket Nova|AYN Odin 2|AYN Odin 2 Mini|AYN Odin 2 Portal"
MODEL_FILE=${POCKETUI_MODEL_FILE:-$R/sys/firmware/devicetree/base/model}
APP=$R/storage/pocket-ui
DROPIN_DIR=$R/storage/.config/system.d/essway.service.d
DROPIN=$DROPIN_DIR/10-pocket-ui.conf
UNITS=$R/storage/.config/system.d
SPLASH=$UNITS/pocket-splash.service
SPLASH_LINK=$UNITS/sysinit.target.wants/pocket-splash.service
ES_ONLY=$R/tmp/pocket-ui.es-only
F=$HERE/files

CHECK_ONLY=0 RESTART=1 ANY_ROCKNIX=0 ANY_DEVICE=0
for a in "$@"; do
  case $a in
    --check) CHECK_ONLY=1 ;;
    --no-restart) RESTART=0 ;;
    --allow-untested-rocknix) ANY_ROCKNIX=1 ;;
    --allow-other-device) ANY_DEVICE=1 ;;
    -h | --help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $a (see sh install.sh --help)" >&2; exit 2 ;;
  esac
done

say() { printf '%s\n' "$@"; }
ok() { say "  ok    $*"; }
note() { say "  note  $*"; }
STAGE=
die() {
  say "" "NOT INSTALLED: $*" >&2
  [ -z "$STAGE" ] || rm -rf "$STAGE"
  say "Nothing of ROCKNIX was changed; EmulationStation stays as it was." >&2
  exit 1
}
field() { sed -n "s/^$1=\"\{0,1\}\([^\"]*\)\"\{0,1\}\$/\1/p" "$2" 2>/dev/null | head -1; }

[ -f "$F/VERSION" ] || die "files/VERSION is missing: unpack the whole package and run install.sh from its folder"
VERSION=$(cat "$F/VERSION")
say "pocket-ui $VERSION for ROCKNIX (Retroid Pocket Mini V2; Pocket 5, Flip 2, Mini, Pocket 6, Nova: experimental)"
say "== checks (nothing is changed yet)"

# The package itself: every file there and as built.
[ -n "$R" ] || [ "$(id -u)" = 0 ] || die "run it as root (ROCKNIX's SSH login is root)"
(cd "$F" && sha256sum -c SHA256SUMS >/dev/null 2>&1) || die "the package is damaged or incomplete (files/SHA256SUMS does not match); download and unpack it again"
ok "package complete (files/SHA256SUMS)"
# The licence texts travel with the programs (scripts/licence-files.sh); a package without them is not complete.
LIC=$HERE/licences
for f in LICENSE NOTICE.md FONTS-OFL.txt THIRD-PARTY-CRATES.md THIRD-PARTY-LICENSES.txt; do
  [ -s "$LIC/$f" ] || die "licences/$f is missing from the package: unpack the whole package and run install.sh from its folder"
done

# The system: ROCKNIX SM8250/SM8550 on aarch64; only the Mini V2 has hardware verification.
OSR=$R/etc/os-release
[ -f "$OSR" ] || die "no /etc/os-release: this is not ROCKNIX"
[ "$(field OS_NAME "$OSR")" = ROCKNIX ] || die "this system is $(field OS_NAME "$OSR" || echo unknown), not ROCKNIX"
dev=$(field HW_DEVICE "$OSR") arch=$(field HW_ARCH "$OSR") ver=$(field OS_VERSION "$OSR")
{ [ "$dev" = SM8250 ] || [ "$dev" = SM8550 ]; } && [ "$arch" = aarch64 ] || die "this is ROCKNIX for $dev/$arch; pocket-ui needs ROCKNIX for SM8250 or SM8550 (aarch64)"
# ROCKNIX versions are dates (20261001). The tested one and any newer one go ahead (the compatibility check below is the
# real contract and still blocks); an older one is asked to update first.
case $ver in *[!0-9]* | '') vernum=0 ;; *) vernum=$ver ;; esac
if [ "$ver" = "$TESTED_ROCKNIX" ]; then
  ok "ROCKNIX $ver for $dev (Mini V2 tested; other devices experimental)"
elif [ "$vernum" -gt "$TESTED_ROCKNIX" ] 2>/dev/null; then
  ok "ROCKNIX $ver for $dev"
  note "tested with ROCKNIX $TESTED_ROCKNIX; your ROCKNIX is newer. pocket-ui's compatibility check passed, so it goes ahead"
elif [ $ANY_ROCKNIX = 1 ]; then
  note "ROCKNIX $ver is older than the tested $TESTED_ROCKNIX; allowed by --allow-untested-rocknix"
else
  die "this ROCKNIX ($ver) is older than the one pocket-ui was tested with ($TESTED_ROCKNIX). Please update ROCKNIX first (its own updater, or a fresh image from rocknix.org), then run this again"
fi
model=$(tr -d '\000' <"$MODEL_FILE" 2>/dev/null)
experimental=0
case "|$MODELS_EXPERIMENTAL|" in *"|$model|"*) [ -n "$model" ] && experimental=1 ;; esac
if [ "$model" = "$MODEL_WANTED" ]; then
  ok "device: $model"
elif [ $experimental = 1 ]; then
  note "device: $model, experimental: pocket-ui hasn't been tested on one yet. Most things should work; if something doesn't, Settings -> System -> About -> Report a problem"
elif [ $ANY_DEVICE = 1 ]; then
  note "device: ${model:-unknown}, not the $MODEL_WANTED; allowed by --allow-other-device"
else
  die "this device is '${model:-unknown}'; pocket-ui is made for the $MODEL_WANTED (and, experimental, the Retroid Pocket 5, Flip 2, first Mini, Pocket 6 and Nova; --allow-other-device tries it anyway)"
fi

# ROCKNIX's interface unit, which the drop-in takes over, and the fallback to EmulationStation.
[ -f "$R/usr/lib/systemd/system/essway.service" ] || die "ROCKNIX's essway.service is missing"
[ -x "$R/usr/bin/start_es.sh" ] || die "/usr/bin/start_es.sh is missing (pocket-ui falls back to EmulationStation with it)"
[ "$(systemctl is-enabled essway 2>/dev/null)" != masked ] || die "essway is masked (ROCKNIX desktop mode?); switch back to the normal interface first"
[ -d "$R/storage/.config/system.d" ] || die "/storage/.config/system.d is missing: start ROCKNIX normally once (its first start sets it up), then run this again"
[ ! -d "$R/storage/.pocket-seed" ] || die "this card still finishes setting up the pocket-ui SD image: restart the device once, then run this again"
ok "essway.service, start_es.sh, /storage/.config/system.d"

# Room on /storage: the programs once more next to an older version, and some to spare.
need=$(du -sk "$F" | cut -f1)
need=$((need * 2 + 10240))
free=$(df -Pk "$R/storage" 2>/dev/null | awk 'NR == 2 { print $4 }')
[ -n "$free" ] && [ "$free" -ge "$need" ] || die "not enough room on /storage: ${free:-?} KB free, $need KB needed"
ok "room on /storage: $((free / 1024)) MB free"

# The programs run on this system and are the version of the package (as the updater checks, docs/UPDATES.md).
# Staged next to /storage/pocket-ui, on the same file system (moved in with one rename), and gone again after --check.
STAGE=$R/storage/.pocket-ui-staging-$VERSION
rm -rf "$STAGE"
mkdir -p "$STAGE" || die "cannot create $STAGE"
for f in pocket-ui pocket-core pocket-splash portmaster-setup.sh; do
  cp "$F/$f" "$STAGE/$f" && chmod 755 "$STAGE/$f" || die "cannot copy $f to $STAGE"
done
for b in pocket-ui pocket-core pocket-splash; do
  got=$("$STAGE/$b" --version 2>&1)
  [ "$got" = "$b $VERSION" ] || die "$b does not run here (it said: '$(printf '%s' "$got" | head -c 200)'); this system lacks what it needs"
done
ok "pocket-ui, pocket-core and pocket-splash $VERSION run on this system"

# pocket-ui's own check of the running system: the libraries and symbol versions both programs link to, the programs
# and ROCKNIX scripts they call, the cores, ROCKNIX's functions (src/os_compat.rs, the one an OS update goes through).
# The bootloader guard of that check is about update tars and refuses every ROCKNIX system, so it is left out here.
report=$("$STAGE/pocket-ui" --check-os-update "${R:-/}" 2>&1)
rc=$?
blocks=$(printf '%s\n' "$report" | grep '^BLOCKS ' | grep -v -e 'rewrites the internal bootloader' -e 'would touch the bootloader or a partition')
if [ -n "$blocks" ]; then
  printf '%s\n' "$blocks" | sed 's/^BLOCKS /  missing  /'
  die "this ROCKNIX lacks something pocket-ui needs (above)"
fi
if [ $rc -ne 0 ] && ! printf '%s\n' "$report" | grep -q '^REFUSED system '; then
  printf '%s\n' "$report" | tail -n 5 | sed 's/^/  /'
  die "pocket-ui's compatibility check could not read this system"
fi
printf '%s\n' "$report" | grep '^WARNS ' | sed 's/^WARNS  */  note  /'
ok "pocket-ui's compatibility check of this system"

if [ $CHECK_ONLY = 1 ]; then
  rm -rf "$STAGE"
  say "" "All checks passed. Nothing was changed (--check). Run sh install.sh to install."
  exit 0
fi

say "== installing"
# The programs: releases/<version>, current and the fixed links, as scripts/dev.sh and the updater lay them out.
REL=$APP/releases/$VERSION
mkdir -p "$APP/releases" || die "cannot create $APP/releases"
same_release() {
  for f in pocket-ui pocket-core pocket-splash portmaster-setup.sh; do cmp -s "$STAGE/$f" "$REL/$f" || return 1; done
}
if [ -d "$REL" ] && same_release; then
  rm -rf "$STAGE"
  ok "releases/$VERSION already in place"
else
  if [ -d "$REL" ]; then
    rm -rf "$APP/releases/.old-$VERSION"
    mv "$REL" "$APP/releases/.old-$VERSION" || die "cannot move the old releases/$VERSION aside"
  fi
  mv "$STAGE" "$REL" || die "cannot put releases/$VERSION in place"
  rm -rf "$APP/releases/.old-$VERSION"
  ok "releases/$VERSION"
fi
STAGE=
cp "$F/run-pocket-ui.sh" "$APP/run-pocket-ui.sh.new" && chmod 755 "$APP/run-pocket-ui.sh.new" &&
  mv -f "$APP/run-pocket-ui.sh.new" "$APP/run-pocket-ui.sh" || die "cannot write the launcher $APP/run-pocket-ui.sh"
ok "launcher run-pocket-ui.sh"
# Licences of pocket-ui, its Rust crates and the fonts: /storage/pocket-ui/licences (Settings > About names the folder).
rm -rf "$APP/licences.new"
{ mkdir -p "$APP/licences.new" && cp "$LIC/"* "$APP/licences.new/" && chmod 644 "$APP/licences.new/"* &&
  rm -rf "$APP/licences" && mv "$APP/licences.new" "$APP/licences"; } || die "cannot write the licence texts $APP/licences"
ok "licences (/storage/pocket-ui/licences)"
# The Xbox core (hakuX, docs/XBOX.md) when the package has it: /storage/pocket-ui/hakux, next to the releases, so an update of
# pocket-ui leaves it alone. Checked against the package's sums first.
if [ -d "$HERE/hakux" ]; then
  (cd "$HERE/hakux" && sha256sum -c SHA256SUMS >/dev/null 2>&1) || die "hakux/ does not match its SHA256SUMS: unpack the package again"
  rm -rf "$APP/hakux.new"
  { mkdir -p "$APP/hakux.new" && cp -r "$HERE/hakux/." "$APP/hakux.new/" && rm -f "$APP/hakux.new/SHA256SUMS" &&
    chmod 755 "$APP/hakux.new/hakux_libretro.so" && rm -rf "$APP/hakux" && mv "$APP/hakux.new" "$APP/hakux"; } || die "cannot write $APP/hakux"
  ok "Xbox core (/storage/pocket-ui/hakux)"
fi
# A copy of uninstall.sh stays with the programs, for when the package folder is gone.
cp "$HERE/uninstall.sh" "$APP/uninstall.sh" 2>/dev/null && chmod 755 "$APP/uninstall.sh"
old=$(readlink "$APP/current" 2>/dev/null)
if [ "$old" != "releases/$VERSION" ]; then
  [ -z "$old" ] || ln -sfn "$old" "$APP/previous"
  ln -sfn "releases/$VERSION" "$APP/current" || die "cannot point current at releases/$VERSION"
  ok "current -> releases/$VERSION${old:+ (previous -> $old)}"
fi
for f in pocket-ui pocket-core portmaster-setup.sh; do
  [ -L "$APP/$f" ] || rm -f "$APP/$f"
  ln -sfn "current/$f" "$APP/$f"
done
# A version left half-installed by the updater would be rolled back by the launcher; this one was checked above.
rm -f "$APP/update-pending"
printf 'version=%s\ninstalled=%s\n' "$VERSION" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$APP/installed-by-package"

# The one file that makes ROCKNIX start pocket-ui: a drop-in for essway (docs/INSTALL-DUAL-BOOT.md, phase 6).
if cmp -s "$F/10-pocket-ui.conf" "$DROPIN"; then
  ok "start entry already in place ($DROPIN)"
else
  mkdir -p "$DROPIN_DIR" && cp "$F/10-pocket-ui.conf" "$DROPIN.new" && chmod 644 "$DROPIN.new" && mv -f "$DROPIN.new" "$DROPIN" ||
    die "cannot write $DROPIN"
  ok "start entry $DROPIN"
fi
systemctl daemon-reload
if ! systemctl show -p DropInPaths --value essway 2>/dev/null | grep -q 10-pocket-ui.conf ||
  ! systemctl show -p Environment --value essway 2>/dev/null | grep -q POCKETUI_AS_ESSWAY=1; then
  rm -f "$DROPIN"
  rmdir "$DROPIN_DIR" 2>/dev/null
  systemctl daemon-reload
  die "systemd did not take the start entry; it was removed again (the programs stay in /storage/pocket-ui, unused)"
fi
ok "systemd took the start entry"

# The boot screen (docs/BOOT-SPLASH.md): its unit and the link that starts it early in the boot. Never started from
# here: it shows from the next start of the device.
if cmp -s "$F/pocket-splash.service" "$SPLASH" && [ "$(readlink "$SPLASH_LINK" 2>/dev/null)" = ../pocket-splash.service ]; then
  ok "boot screen already in place ($SPLASH)"
else
  { cp "$F/pocket-splash.service" "$SPLASH.new" && chmod 644 "$SPLASH.new" && mv -f "$SPLASH.new" "$SPLASH" &&
    mkdir -p "$UNITS/sysinit.target.wants" && ln -sfn ../pocket-splash.service "$SPLASH_LINK"; } ||
    die "cannot write the boot screen's unit $SPLASH"
  systemctl daemon-reload
  ok "boot screen $SPLASH (from the next start)"
fi

# Restarting essway ends EmulationStation (and a running game) and starts pocket-ui. Not from inside essway: that
# would end this script halfway (an installer started from EmulationStation's menus).
if grep -q 'essway\.service' /proc/self/cgroup 2>/dev/null && [ -z "$R" ]; then
  RESTART=0
  note "started from inside the interface: restart the device to start pocket-ui"
fi
if [ $RESTART = 1 ]; then
  rm -f "$ES_ONLY"
  systemctl reset-failed essway 2>/dev/null
  systemctl restart essway
  say "" "Installed. pocket-ui is starting now. Uninstall: sh /storage/pocket-ui/uninstall.sh"
else
  say "" "Installed. pocket-ui starts the next time the device starts. Uninstall: sh /storage/pocket-ui/uninstall.sh"
fi

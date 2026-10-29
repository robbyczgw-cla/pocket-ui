#!/bin/sh
# Tests of deploy/fat/mount-storage.sh.in (the "two files on the card" install) against a fake /flash and /storage:
#   sh deploy/fat/test-hook.sh
# The hook runs under BusyBox sh with a PATH that holds ONLY the applets ROCKNIX's initramfs has (read from its busybox),
# so a tool that is missing there (find, cmp, date, chmod, touch, mktemp ...) fails here too. mount_part, mount and
# progress are fakes that log their calls, as ROCKNIX's init would provide them; PU_FLASH / PU_STORAGE point the hook at
# the fake folders. POCKETUI_FAT_DIR=<folder of make-fat-install.sh's output> adds a run with the real two files. The tar is built as scripts/make-fat-install.sh does. About 2 s, part of the gate.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
command -v busybox >/dev/null || { echo "busybox is needed for this test"; exit 1; }
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0
V=9.8.7
check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

# The applets of ROCKNIX's initramfs busybox that a hook can use.
mkdir "$T/applets"
BB=$(command -v busybox)
for a in cat cp cut df dd gunzip gzip grep head ln ls md5sum mkdir mv readlink rm sed sleep stat sync tail tar tee tr wc; do
  ln -s "$BB" "$T/applets/$a"
done

# A tree as the build makes it, with stand-in programs. With $2 = hakux it also has the Xbox core as
# `scripts/make-fat-install.sh --hakux` lays it out (scripts/hakux-files.sh).
build_tar() { # $1 = version, $2 = hakux
  rm -rf "$T/tree"
  R=$T/tree/pocket-ui/releases/$1
  mkdir -p "$R" "$T/tree/pocket-ui/licences" "$T/tree/.config/system.d/essway.service.d" "$T/tree/.config/system.d/sysinit.target.wants"
  for b in pocket-ui pocket-core pocket-splash portmaster-setup.sh; do printf '#!/bin/sh\necho %s\n' "$b" >"$R/$b"; chmod 755 "$R/$b"; done
  echo L >"$T/tree/pocket-ui/licences/LICENSE"
  ln -s "releases/$1" "$T/tree/pocket-ui/current"
  for f in pocket-ui pocket-core portmaster-setup.sh; do ln -s "current/$f" "$T/tree/pocket-ui/$f"; done
  echo '[Service]' >"$T/tree/.config/system.d/essway.service.d/10-pocket-ui.conf"
  echo '[Unit]' >"$T/tree/.config/system.d/pocket-splash.service"
  ln -s ../pocket-splash.service "$T/tree/.config/system.d/sysinit.target.wants/pocket-splash.service"
  if [ "${2:-}" = hakux ]; then
    H=$T/tree/pocket-ui/hakux
    mkdir -p "$H/licenses" "$H/patches"
    echo core >"$H/hakux_libretro.so"; chmod 755 "$H/hakux_libretro.so"
    echo 'savestate = "false"' >"$H/hakux_libretro.info"
    echo source >"$H/SOURCE.txt"; echo build >"$H/BUILD-INFO"
    echo gpl >"$H/licenses/COPYING"
    echo patch >"$H/patches/0001-apu-dsp-preference-per-apu-state.patch"
    (cd "$H" && find . -type f | sort | xargs sha256sum >SHA256SUMS)
  fi
  tar --format=gnu --sort=name --owner=0 --group=0 --numeric-owner -C "$T/tree" -cf - . | gzip -n -9 >"$T/seed.tar.gz"
}
make_hook() { # $1 = version, $2 = md5, $3 = need_kb
  sed -e "s|@VERSION@|$1|g" -e "s|@SEED_MD5@|$2|g" -e "s|@NEED_KB@|$3|g" "$HERE/mount-storage.sh.in" >"$T/mount-storage.sh"
}

# A card after flashing the official image: FAT with the two files, STORAGE 32 MB with the resize marker.
# $1 = resized (marker gone)
setup() {
  rm -rf "$T/flash" "$T/storage" "$T/calls"
  mkdir -p "$T/flash" "$T/storage/roms/gba" "$T/storage/.config"
  echo save >"$T/storage/roms/gba/game.srm"
  rm -rf "$T/storage/.config"
  [ "${1:-}" = resized ] && mkdir -p "$T/storage/.config/emuelec" && echo keep >"$T/storage/.config/emuelec/settings"
  [ "${1:-}" = resized ] || : >"$T/storage/.please_resize_me"
  cp "$T/mount-storage.sh" "$T/flash/mount-storage.sh"
  cp "$T/seed.tar.gz" "$T/flash/pocket-ui-seed.tar.gz"
  : >"$T/calls"
}
fake_df='#!/bin/sh
echo "Filesystem 1K-blocks Used Available Use% Mounted on"
echo "/dev/fake 100000 0 ${FAKE_FREE_KB:-5000000} 0% /storage"'
printf '%s\n' "$fake_df" >"$T/dfdir.sh"

boot() {
  env -i PATH="$T/applets" PU_FLASH="$T/flash" PU_STORAGE="$T/storage" PU_DT="${PU_DT_DIR:-$T/no-dt}" FAKE_FREE_KB="${FAKE_FREE_KB:-5000000}" T="$T" \
    "$BB" sh -c '
    mount_part() { echo "mount_part $*" >>$T/calls; }
    mount() { echo "mount $*" >>$T/calls; }
    progress() { echo "progress $*" >>$T/calls; }
    df() { echo "Filesystem 1K-blocks Used Available Use% Mounted on"; echo "/dev/fake 100000 0 $FAKE_FREE_KB 0% /storage"; }
    disk=LABEL=STORAGE
    mount_storage() { if [ -f $PU_FLASH/mount-storage.sh ]; then . $PU_FLASH/mount-storage.sh; else mount_part "$disk" /storage rw,noatime; fi; }
    mount_storage
  ' >"$T/out" 2>&1
}

build_tar $V hakux
MD5=$(md5sum "$T/seed.tar.gz" | cut -d' ' -f1)
make_hook $V "$MD5" 200000

echo "== first boot: STORAGE not grown yet"
setup
boot
check "mounts STORAGE as ROCKNIX does" 'grep -qx "mount_part LABEL=STORAGE /storage rw,noatime" "$T/calls"'
check "does nothing: no pocket-ui, no .config, marker stays" '[ ! -e "$T/storage/pocket-ui" ] && [ ! -e "$T/storage/.config" ] && [ -e "$T/storage/.please_resize_me" ]'
check "hook and tar stay" '[ -f "$T/flash/mount-storage.sh" ] && [ -f "$T/flash/pocket-ui-seed.tar.gz" ]'
check "no log written, /flash not touched" '[ ! -e "$T/flash/pocket-ui-install.log" ] && ! grep -q "^mount " "$T/calls"'
check "no stray output" '[ ! -s "$T/out" ]'

echo "== second boot: STORAGE grown"
setup resized
boot
check "no shell error" '! grep -qi "not found\|error\|syntax" "$T/out"'
check "programs in releases/$V" '[ -x "$T/storage/pocket-ui/releases/$V/pocket-ui" ] && [ -x "$T/storage/pocket-ui/releases/$V/pocket-splash" ]'
check "current and fixed links" '[ "$(readlink "$T/storage/pocket-ui/current")" = "releases/$V" ] && [ "$(readlink "$T/storage/pocket-ui/pocket-ui")" = current/pocket-ui ] && [ -x "$T/storage/pocket-ui/pocket-core" ]'
check "licences" '[ -f "$T/storage/pocket-ui/licences/LICENSE" ]'
check "Xbox core in pocket-ui/hakux with what must travel with it" '[ -x "$T/storage/pocket-ui/hakux/hakux_libretro.so" ] && [ -f "$T/storage/pocket-ui/hakux/hakux_libretro.info" ] && [ -f "$T/storage/pocket-ui/hakux/licenses/COPYING" ] && [ -f "$T/storage/pocket-ui/hakux/patches/0001-apu-dsp-preference-per-apu-state.patch" ] && [ -f "$T/storage/pocket-ui/hakux/SHA256SUMS" ]'
check "drop-in, boot screen unit and its link" '[ -f "$T/storage/.config/system.d/essway.service.d/10-pocket-ui.conf" ] && [ "$(readlink "$T/storage/.config/system.d/sysinit.target.wants/pocket-splash.service")" = ../pocket-splash.service ]'
check "existing settings and saves untouched" '[ "$(cat "$T/storage/.config/emuelec/settings")" = keep ] && [ "$(cat "$T/storage/roms/gba/game.srm")" = save ]'
check "no staging or running marker left" '[ ! -e "$T/storage/.pocket-ui-stage" ] && [ ! -e "$T/storage/.pocket-ui-install-running" ]'
check "hook and tar deleted" '[ ! -e "$T/flash/mount-storage.sh" ] && [ ! -e "$T/flash/pocket-ui-seed.tar.gz" ]'
check "/flash rw only for the changes, ro again" '[ "$(grep "^mount " "$T/calls" | tr "\n" "|")" = "mount -o remount,rw $T/flash|mount -o remount,ro $T/flash|mount -o remount,rw $T/flash|mount -o remount,ro $T/flash|" ]'
check "logs on /flash and /storage say installed" 'grep -q "installed" "$T/flash/pocket-ui-install.log" && grep -q "installed" "$T/storage/pocket-ui-install.log"'
check "progress line shown" 'grep -q "^progress pocket-ui: installing" "$T/calls"'

echo "== third boot: hook gone, ROCKNIX mounts /storage itself"
: >"$T/calls"
boot
check "plain mount" '[ "$(cat "$T/calls")" = "mount_part LABEL=STORAGE /storage rw,noatime" ]'

echo "== second run of the same files (idempotent)"
setup resized
boot
cp "$T/mount-storage.sh" "$T/flash/mount-storage.sh"; cp "$T/seed.tar.gz" "$T/flash/pocket-ui-seed.tar.gz"
echo mine >"$T/storage/pocket-ui/mine"
: >"$T/calls"
boot
check "nothing changed, own file kept" '[ "$(cat "$T/storage/pocket-ui/mine")" = mine ] && [ -x "$T/storage/pocket-ui/releases/$V/pocket-ui" ]'
check "hook and tar removed again" '[ ! -e "$T/flash/mount-storage.sh" ] && [ ! -e "$T/flash/pocket-ui-seed.tar.gz" ]'

echo "== another pocket-ui already on the card (installed by the SSH package)"
setup resized
mkdir -p "$T/storage/pocket-ui/releases/1.0.0"
ln -s releases/1.0.0 "$T/storage/pocket-ui/current"
boot
check "current stays on the old one" '[ "$(readlink "$T/storage/pocket-ui/current")" = releases/1.0.0 ] && [ ! -e "$T/storage/pocket-ui/releases/$V" ]'
check "hook and tar removed, log says untouched" '[ ! -e "$T/flash/mount-storage.sh" ] && grep -q "already installed" "$T/flash/pocket-ui-install.log"'

echo "== bad checksum (tar damaged)"
setup resized
head -c 300 "$T/seed.tar.gz" >"$T/flash/pocket-ui-seed.tar.gz"
boot
check "nothing installed in /storage" '[ ! -e "$T/storage/pocket-ui" ] && [ ! -e "$T/storage/.pocket-ui-stage" ]'
check "mount done, ROCKNIX goes on" 'grep -qx "mount_part LABEL=STORAGE /storage rw,noatime" "$T/calls"'
check "hook and tar stay" '[ -f "$T/flash/mount-storage.sh" ] && [ -f "$T/flash/pocket-ui-seed.tar.gz" ]'
check "reason in the log on /flash" 'grep -q "NOT INSTALLED" "$T/flash/pocket-ui-install.log" && grep -q "damaged" "$T/flash/pocket-ui-install.log"'
check "reason also in /storage" 'grep -q "NOT INSTALLED" "$T/storage/pocket-ui-install.log"'
check "existing files untouched" '[ "$(cat "$T/storage/.config/emuelec/settings")" = keep ]'
echo "-- then the user copies the right tar"
cp "$T/seed.tar.gz" "$T/flash/pocket-ui-seed.tar.gz"
boot
check "installs at the next boot" '[ -x "$T/storage/pocket-ui/current/pocket-ui" ] && [ ! -e "$T/flash/mount-storage.sh" ]'

echo "== tar missing"
setup resized
rm "$T/flash/pocket-ui-seed.tar.gz"
boot
check "no-op with a reason in the log" '[ ! -e "$T/storage/pocket-ui" ] && [ -f "$T/flash/mount-storage.sh" ] && grep -q "not on this partition" "$T/flash/pocket-ui-install.log"'

echo "== STORAGE still small (resize refused)"
setup resized
FAKE_FREE_KB=20000 boot
check "no-op with a reason in the log" '[ ! -e "$T/storage/pocket-ui" ] && [ -f "$T/flash/mount-storage.sh" ] && grep -q "not grown" "$T/flash/pocket-ui-install.log"'

echo "== unreadable tar with a matching checksum (not gzip)"
echo "not a tar" >"$T/junk"
JM=$(md5sum "$T/junk" | cut -d' ' -f1)
make_hook $V "$JM" 200000
setup resized
cp "$T/junk" "$T/flash/pocket-ui-seed.tar.gz"
boot
check "no-op, reason in the log" '[ ! -e "$T/storage/pocket-ui" ] && grep -q "cannot be read" "$T/flash/pocket-ui-install.log"'

echo "== a tar without the programs (wrong content)"
rm -rf "$T/tree"; mkdir -p "$T/tree/x"; echo y >"$T/tree/x/y"
tar -C "$T/tree" -czf "$T/bad.tar.gz" .
BM=$(md5sum "$T/bad.tar.gz" | cut -d' ' -f1)
make_hook $V "$BM" 200000
setup resized
cp "$T/bad.tar.gz" "$T/flash/pocket-ui-seed.tar.gz"
boot
check "no-op, no half install, reason in the log" '[ ! -e "$T/storage/pocket-ui" ] && [ ! -e "$T/storage/.pocket-ui-stage" ] && grep -q "unpacking" "$T/flash/pocket-ui-install.log"'

# A device tree as the kernel shows it: model and NUL-separated compatible.
dt() { # $1 = model, $2 = first compatible, $3 = SoC
  rm -rf "$T/dt" && mkdir "$T/dt"
  printf '%s\000' "$1" >"$T/dt/model"
  printf '%s\000%s\000' "$2" "$3" >"$T/dt/compatible"
  PU_DT_DIR=$T/dt
}
build_tar $V
MD5=$(md5sum "$T/seed.tar.gz" | cut -d' ' -f1)
make_hook $V "$MD5" 200000
for m in "Anbernic RG28XX|anbernic,rg28xx" "Anbernic RG35XX SP|anbernic,rg35xx-sp" "Anbernic RG40XX V|anbernic,rg40xx-V" "Anbernic RG-SP|anbernic,rg-sp" "Anbernic RG34XX-SP|anbernic,rg34xx-sp"; do
  echo "== H700 card: ${m%%|*}"
  dt "${m%%|*}" "${m##*|}" allwinner,sun50i-h700
  setup resized
  boot
  check "${m%%|*}: installed, the log names the device" '[ -x "$T/storage/pocket-ui/current/pocket-ui" ] && [ ! -e "$T/flash/mount-storage.sh" ] && grep -q "device: ${m%%|*} (Anbernic H700, experimental)" "$T/flash/pocket-ui-install.log"'
done
for m in "Anbernic RG CubeXX|anbernic,rgcubexx" "Anbernic RG99XX|anbernic,rg99xx" "Trimui Smart Pro|trimui,smart-pro"; do
  echo "== H700 card, no profile: ${m%%|*}"
  dt "${m%%|*}" "${m##*|}" allwinner,sun50i-h700
  setup resized
  boot
  check "${m%%|*}: refused with a sentence, nothing installed, files stay, ROCKNIX goes on" '[ ! -e "$T/storage/pocket-ui" ] && [ -f "$T/flash/mount-storage.sh" ] && [ -f "$T/flash/pocket-ui-seed.tar.gz" ] && grep -q "has no profile for it" "$T/flash/pocket-ui-install.log" && grep -q "NOT INSTALLED: this H700 handheld is .${m%%|*}." "$T/flash/pocket-ui-install.log" && grep -q "^mount_part LABEL=STORAGE" "$T/calls"'
done
echo "== a Snapdragon device is not asked about H700 profiles"
dt "Retroid Pocket Mini V2" "retroidpocket,rpminiv2" qcom,sm8250
setup resized
boot
check "Mini V2: installed as before" '[ -x "$T/storage/pocket-ui/current/pocket-ui" ]'
echo "== H700 first boot (not grown yet): silent, as everywhere"
dt "Anbernic RG99XX" "anbernic,rg99xx" allwinner,sun50i-h700
setup
boot
check "nothing written on the first start" '[ ! -e "$T/flash/pocket-ui-install.log" ]'
unset PU_DT_DIR

if [ -n "${POCKETUI_FAT_DIR:-}" ]; then
  echo "== the real files from scripts/make-fat-install.sh ($POCKETUI_FAT_DIR)"
  setup resized
  cp "$POCKETUI_FAT_DIR/mount-storage.sh" "$POCKETUI_FAT_DIR/pocket-ui-seed.tar.gz" "$T/flash/"
  boot
  check "installed: programs, links, drop-in; hook and tar gone" '[ -x "$T/storage/pocket-ui/current/pocket-ui" ] && [ -x "$T/storage/pocket-ui/pocket-core" ] && [ -f "$T/storage/.config/system.d/essway.service.d/10-pocket-ui.conf" ] && [ ! -e "$T/flash/mount-storage.sh" ] && [ ! -e "$T/flash/pocket-ui-seed.tar.gz" ]'
  # make-fat-install.sh --hakux (the card zip): the Xbox core has to arrive as in the SSH package.
  if tar -tzf "$POCKETUI_FAT_DIR/pocket-ui-seed.tar.gz" | grep -qx './pocket-ui/hakux/hakux_libretro.so'; then
    check "Xbox core from the tar (--hakux): installed, with its licences" '[ -x "$T/storage/pocket-ui/hakux/hakux_libretro.so" ] && [ -f "$T/storage/pocket-ui/hakux/licenses/COPYING" ] && (cd "$T/storage/pocket-ui/hakux" && sha256sum -c SHA256SUMS >/dev/null)'
  fi
fi

echo
if [ $fails -eq 0 ]; then echo "all passed"; else echo "$fails failed"; exit 1; fi

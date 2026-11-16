#!/bin/bash
# Tests of deploy/image/games-card.sh: a games card from ArkOS, dArkOS or a stock R36S in the second slot is used as it is,
# every other card goes through ROCKNIX's automount (docs/SD-IMAGE.md, docs/OTHER-LAYOUTS.md). No root: mount, umount,
# blkid, modprobe, systemd-tmpfiles, busybox and automount are stand-ins that log their calls; a "mount" of a card is a link
# to its folder under $T/cards, so whatever the script writes through it lands on the card and the card's checksum shows it.
#   bash deploy/image/test-games-card.sh        (seconds; run by deploy/image/test-mount-storage.sh, part of the gate)
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0
check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

S=$T/storage EXT=$T/storage/games-external ROMS=$T/storage/roms INT=$T/storage/games-internal RUN=$T/run
mkdir -p "$T/bin"
tool() { { printf '#!/bin/bash\nT=%q\n' "$T"; cat; } >"$T/bin/$1"; chmod 755 "$T/bin/$1"; }
tool busybox <<'EOF'
c=$1; shift; exec "$T/bin/$c" "$@"
EOF
tool blkid <<'EOF'
cat "$T/blkid"
EOF
tool modprobe <<'EOF'
echo "modprobe $*" >>"$T/calls"
EOF
tool automount <<'EOF'
echo "automount $*" >>"$T/calls"
EOF
# ROCKNIX's system folders, written wherever $T/storage/roms leads.
tool systemd-tmpfiles <<'EOF'
echo "systemd-tmpfiles $*" >>"$T/calls"
for d in gba snes psx bios; do mkdir -p "$T/storage/roms/$d"; done
EOF
# mount [-o OPTS] [-t TYPE] DEV DIR, mount --bind FROM DIR: the directory becomes a link (to the card or to FROM).
tool mount <<'EOF'
echo "mount $*" >>"$T/calls"
ro=0 bind=0 type=exfat a=()
while [ $# -gt 0 ]; do
  case $1 in
    -o) case ,$2, in *,ro,*) ro=1 ;; esac; shift 2 ;;
    -t) type=$2; shift 2 ;;
    --bind) bind=1; shift ;;
    *) a+=("$1"); shift ;;
  esac
done
src=${a[0]} dst=${a[1]}
if [ $bind = 1 ]; then
  [ ! -e "$T/fail-bind" ] && [ -d "$src" ] && rmdir "$dst" || exit 1
  ln -s "$src" "$dst"
  echo "/dev/mmcblk0p2 $dst ext4 rw 0 0" >>"$T/mounts"
  exit 0
fi
card=$T/cards/${src##*/}
[ -d "$card" ] || exit 1
[ $ro = 0 ] || [ ! -e "$T/fail-probe" ] || exit 1
[ $ro = 1 ] || [ ! -e "$T/fail-rw" ] || exit 1
rmdir "$dst" || exit 1
ln -s "$card" "$dst"
echo "$src $dst $type $([ $ro = 1 ] && echo ro || echo rw) 0 0" >>"$T/mounts"
EOF
tool umount <<'EOF'
echo "umount $*" >>"$T/calls"
rc=1
for a in "$@"; do
  case $a in -*) continue ;; esac
  grep -q " $a " "$T/mounts" || continue
  [ -e "$T/stuck" ] && [ "$a" = "$T/run/probe" ] && exit 1
  n=$(grep -n " $a " "$T/mounts" | tail -n 1 | cut -d: -f1)
  sed -i "${n}d" "$T/mounts"
  [ -L "$a" ] && rm "$a" && mkdir "$a"
  rc=0
done
exit $rc
EOF
cat >"$T/profile" <<'EOF'
get_setting() { sed -n "s/^$1=//p" "$T/settings" 2>/dev/null | tail -n 1; }
set_setting() { echo "$1=$2" >>"$T/settings"; echo "set_setting $1 $2" >>"$T/calls"; }
EOF

# A device the way the handheld boots: the system card in slot 1 (mmcblk0), 32 GB.
reset() {
  rm -rf "$S" "$RUN" "$T/cards" "$T/sys" "$T/calls" "$T/settings" "$T/fail-"* "$T/stuck" "$T/log"
  mkdir -p "$S/roms" "$INT/roms" "$RUN" "$T/cards" "$T/sys"
  : >"$T/calls"
  : >"$T/dirs.conf"
  printf '/dev/mmcblk0p1 /flash vfat ro 0 0\n/dev/mmcblk0p2 %s ext4 rw 0 0\n' "$S" >"$T/mounts"
  printf '/dev/mmcblk0p1: LABEL="ROCKNIX" UUID="1111-1111" TYPE="vfat" PARTUUID="a-01"\n/dev/mmcblk0p2: LABEL="STORAGE" UUID="2222" TYPE="ext4" PARTUUID="a-02"\n' >"$T/blkid"
  printf 'major minor  #blocks  name\n\n 179        0   31166976 mmcblk0\n 179        1    2097152 mmcblk0p1\n 179        2   29000000 mmcblk0p2\n' >"$T/partitions"
}
# card DEV KBYTES: a second card's device (mmcblk1) with its size; its partitions with part.
card() { printf ' 179       32 %10d %s\n' "$2" "$1" >>"$T/partitions"; }
part() { # part NAME TYPE LABEL: a partition on the second card, its tree in $T/cards/NAME
  printf '/dev/%s: LABEL="%s" UUID="u-%s" TYPE="%s" PARTUUID="b-%s"\n' "$1" "$3" "$1" "$2" "$1" >>"$T/blkid"
  mkdir -p "$T/cards/$1"
}
f() { mkdir -p "$(dirname "$T/cards/$1")"; printf '%s\n' "${2:-$1}" >"$T/cards/$1"; }
run() {
  env -i PATH="$T/bin:/usr/bin:/bin" T="$T" PU_MOUNTS="$T/mounts" PU_PARTITIONS="$T/partitions" PU_SYSBLOCK="$T/sys" \
    PU_STORAGE="$S" PU_EXTERNAL="$EXT" PU_ROMS="$ROMS" PU_INTERNAL="$INT" PU_RUN="$RUN" PU_LOG="$T/log" \
    PU_AUTOMOUNT="$T/bin/automount" PU_PROFILE="$T/profile" PU_BUSYBOX="$T/bin/busybox" PU_DIRS_CONF="$T/dirs.conf" \
    PU_BIOS_MAX="${BIOS_MAX:-1000}" bash "$HERE/games-card.sh"
  echo $? >"$T/rc"
}
# Every name, size, mode and byte under the card's folders.
sum() { (cd "$T/cards" && find . -printf '%p %s %m %y\n' | LC_ALL=C sort && find . -type f -exec md5sum {} + | LC_ALL=C sort) | md5sum; }
automounted() { grep -q '^automount' "$T/calls"; }
arkos_used() { # DEV: mounted rw on games-external, roms bound to internal, marker, no automount, probe gone
  grep -q "^$1 $EXT exfat rw" "$T/mounts" && [ "$(readlink "$EXT")" = "$T/cards/${1##*/}" ] &&
    [ "$(grep " $ROMS " "$T/mounts" | tail -n 1 | cut -d' ' -f1)" = /dev/mmcblk0p2 ] && [ "$(readlink "$ROMS")" = "$INT/roms" ] &&
    [ "$(cat "$RUN/games-card-root" 2>/dev/null)" = "$EXT" ] && ! automounted && ! grep -q " $RUN/probe " "$T/mounts" &&
    [ "$(cat "$T/rc")" = 0 ]
}
mounted_rw() { grep -q "^mount $1 $EXT\$" "$T/calls"; }

stock_r36s() { # BOOT (FAT), rootfs (ext4), EASYROMS (exFAT) with the system folders and bios/ at its root
  card mmcblk1 62333952
  part mmcblk1p1 vfat BOOT
  part mmcblk1p2 ext4 rootfs
  part mmcblk1p3 exfat EASYROMS
  f mmcblk1p1/Image; f mmcblk1p1/rk3326-r35s-linux.dtb
  f "mmcblk1p3/gba/Pokemon - Emerald Version (USA).gba"; f mmcblk1p3/snes/Chrono.sfc; f mmcblk1p3/psx/FF7.chd
  f mmcblk1p3/bios/gba_bios.bin card-gba; f mmcblk1p3/bios/scph1001.bin card-psx; f mmcblk1p3/bios/dc/dc_boot.bin card-dc
  f "mmcblk1p3/ports/Cave Story.sh"; f mmcblk1p3/themes/x.zip
}

echo "== stock R36S card in the second slot (BOOT, rootfs, EASYROMS with gba/ snes/ psx/ bios/ at its root)"
reset; stock_r36s
mkdir -p "$INT/roms/bios"; echo internal-psx >"$INT/roms/bios/scph1001.bin"
before=$(sum)
run
check "EASYROMS mounted read-write on games-external; /storage/roms is the internal folder; marker names games-external" 'arkos_used /dev/mmcblk1p3'
check "mounted the way ROCKNIX mounts (busybox mount, no options)" 'mounted_rw /dev/mmcblk1p3'
check "looked at read-only first" 'grep -q "^mount -o ro /dev/mmcblk1p3 $RUN/probe\$" $T/calls'
check "nothing written on the card (no .tmp-rocknix, no roms/, no system folders): byte for byte the same" '[ "$(sum)" = "$before" ] && [ ! -e $T/cards/mmcblk1p3/roms ] && [ ! -e $T/cards/mmcblk1p3/.tmp-rocknix ]'
check "ROCKNIX's system folders made in the internal folder" 'grep -q "^systemd-tmpfiles --create $T/dirs.conf" $T/calls && [ -d $INT/roms/gba ]'
check "BIOS files copied into the internal bios folder, subfolders too" '[ "$(cat $INT/roms/bios/gba_bios.bin)" = card-gba ] && [ "$(cat $INT/roms/bios/dc/dc_boot.bin)" = card-dc ]'
check "a BIOS file already there is not overwritten" '[ "$(cat $INT/roms/bios/scph1001.bin)" = internal-psx ]'
check "no half-copied file left" '[ -z "$(find $INT -name "*.pocket-part")" ]'
check "the log names the card and the copied files" 'grep -q "mmcblk1p3 (exfat): an ArkOS card" $T/log && grep -q "bios: copied gba_bios.bin" $T/log && ! grep -q "bios: copied scph1001.bin" $T/log'
check "ROCKNIX's merged-storage setting: internal, as automount sets it without a card" 'grep -qx "set_setting system.merged.device internal" $T/calls'
check "the file systems loaded first, as automount does" 'grep -q "^modprobe exfat" $T/calls'
check "never fails the unit" '[ "$(cat $T/rc)" = 0 ]'

echo "== the same card again (udev restarts the unit): still used as it is, nothing copied twice"
: >"$T/calls"; echo "== old" >"$T/log"
run
check "unmounted and mounted again, the ArkOS way" 'arkos_used /dev/mmcblk1p3 && grep -q "^umount -f $EXT" $T/calls && [ "$(grep -c " $EXT " $T/mounts)" = 1 ]'
check "card still byte for byte the same" '[ "$(sum)" = "$before" ]'
check "no BIOS copied the second time" '! grep -q "bios: copied" $T/log'
check "merged-storage setting left as it is" '! grep -q set_setting $T/calls'

echo "== dArkOS games card (SD2: one exFAT partition, the system folders at its root)"
reset; card mmcblk1 125000000; part mmcblk1p1 exfat GAMES
f mmcblk1p1/gba/a.gba; f mmcblk1p1/nes/b.nes; f mmcblk1p1/n64/c.z64; f mmcblk1p1/BIOS/gba_bios.bin card-gba
before=$(sum)
run
check "mounted read-write on games-external, internal roms, marker" 'arkos_used /dev/mmcblk1p1 && mounted_rw /dev/mmcblk1p1'
check "nothing written on the card" '[ "$(sum)" = "$before" ]'
check "a BIOS folder called BIOS works too (exFAT ignores case)" '[ "$(cat $INT/roms/bios/gba_bios.bin)" = card-gba ]'

echo "== ArkOS card on NTFS"
reset; card mmcblk1 62333952; part mmcblk1p1 ntfs GAMES
f mmcblk1p1/GBA/a.gba; f mmcblk1p1/SNES/b.sfc
run
check "mounted with the kernel's ntfs3, as automount does" 'grep -q "^mount -t ntfs3 /dev/mmcblk1p1 $EXT\$" $T/calls && [ "$(cat $RUN/games-card-root)" = "$EXT" ] && ! automounted'

echo "== cards that go through ROCKNIX's automount, unchanged"
reset; card mmcblk1 62333952; part mmcblk1p1 exfat ROMS; f mmcblk1p1/roms/gba/a.gba; f mmcblk1p1/roms/snes/b.sfc; f mmcblk1p1/gba/x.gba; f mmcblk1p1/psx/y.chd
echo /storage/games-external >"$RUN/games-card-root"
before=$(sum)
run
check "ROCKNIX-style card (roms/ at its root, even with system folders beside it): automount" 'automounted && [ ! -e $RUN/games-card-root ]'
check "  and the script itself wrote nothing on it, mounted nothing in the end" '[ "$(sum)" = "$before" ] && [ "$(grep -c "^/dev/mmcblk1" $T/mounts)" = 0 ] && ! grep -q -- "--bind" $T/calls'

reset; card mmcblk1 62333952; part mmcblk1p1 exfat ANBERNIC; f mmcblk1p1/Roms/GBA/a.gba; f mmcblk1p1/Roms/SFC/b.sfc; f mmcblk1p1/BIOS/x.bin
run
check "Anbernic stock card (Roms/GBA): automount" 'automounted && [ ! -e $RUN/games-card-root ]'

reset; card mmcblk1 62333952; part mmcblk1p1 exfat NEW
run
check "blank card: automount" 'automounted && [ ! -e $RUN/games-card-root ]'

reset; card mmcblk1 62333952; part mmcblk1p1 exfat MUSIC; f mmcblk1p1/gba/a.gba; f mmcblk1p1/music/a.mp3
run
check "only one known system folder: automount" 'automounted'

reset; card mmcblk1 62333952; part mmcblk1p1 exfat X; f mmcblk1p1/gba; f mmcblk1p1/snes
run
check "files called gba and snes are not folders: automount" 'automounted'

reset
run
check "no second card: automount" 'automounted && [ ! -e $RUN/games-card-root ] && ! grep -q "^mount" $T/calls'

reset; card mmcblk1 7000000; part mmcblk1p1 exfat SMALL; f mmcblk1p1/gba/a.gba; f mmcblk1p1/snes/b.sfc
run
check "a card under 8 GiB (automount ignores it): automount, not even looked at" 'automounted && ! grep -q "^mount" $T/calls'

reset; card mmcblk1 62333952; part mmcblk1p1 exfat A; f mmcblk1p1/gba/a.gba; f mmcblk1p1/snes/b.sfc; ln -s x "$T/sys/mmcblk1boot0"
run
check "an Android boot device: automount" 'automounted && ! grep -q "^mount" $T/calls'

reset; stock_r36s; : >"$T/fail-probe"
before=$(sum)
run
check "the card can't be looked at: automount" 'automounted && [ ! -e $RUN/games-card-root ] && [ "$(sum)" = "$before" ]'

reset; stock_r36s; echo system.automount=0 >"$T/settings"
run
check "automount switched off in ROCKNIX: automount (internal), no look at the card" 'automounted && ! grep -q "^mount" $T/calls'

reset; stock_r36s; echo system.gamesdevice=/dev/sda1 >"$T/settings"
run
check "a games device set in ROCKNIX: automount" 'automounted && ! grep -q "^mount" $T/calls'

reset; stock_r36s; : >"$S/.please_resize_me"
run
check "first start (STORAGE not grown): automount" 'automounted && ! grep -q "^mount" $T/calls'

echo "== failures on the ArkOS way: internal only, the card untouched, never automount"
reset; stock_r36s; : >"$T/fail-rw"
before=$(sum)
run
check "read-write mount refused: internal folder bound, no marker, no automount" '! automounted && [ "$(readlink "$ROMS")" = "$INT/roms" ] && [ ! -e $RUN/games-card-root ] && ! grep -q " $EXT " $T/mounts'
check "  card byte for byte the same, nothing copied" '[ "$(sum)" = "$before" ] && [ ! -e $INT/roms/bios/gba_bios.bin ] && [ "$(cat $T/rc)" = 0 ]'
check "  the log says why" 'grep -q "could not be mounted on $EXT" $T/log'

reset; stock_r36s; : >"$T/stuck"
before=$(sum)
run
check "the probe can't be unmounted: internal only, no second mount, no automount" '! automounted && ! grep -q "^mount /dev/mmcblk1p3 $EXT" $T/calls && [ "$(readlink "$ROMS")" = "$INT/roms" ] && [ ! -e $RUN/games-card-root ] && [ "$(sum)" = "$before" ]'

reset; stock_r36s; : >"$T/fail-bind"
before=$(sum)
run
check "internal bind refused: ROCKNIX's folders not made through /storage/roms, card untouched" '! grep -q systemd-tmpfiles $T/calls && [ "$(sum)" = "$before" ] && [ "$(cat $T/rc)" = 0 ]'

echo "== BIOS limits"
reset; stock_r36s; f mmcblk1p3/bios/huge.bin "$(head -c 2000 /dev/zero | tr '\0' x)"
before=$(sum)
run
check "a BIOS file over the size limit stays on the card only" '[ ! -e $INT/roms/bios/huge.bin ] && grep -q "bios: huge.bin left on the card" $T/log && [ -e $INT/roms/bios/gba_bios.bin ] && [ "$(sum)" = "$before" ]'

echo
if [ $fails -eq 0 ]; then echo "all passed"; else echo "$fails failed"; exit 1; fi

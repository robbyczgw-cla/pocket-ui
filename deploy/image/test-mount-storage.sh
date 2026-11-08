#!/bin/sh
# Tests of the SD image's initramfs hook deploy/image/mount-storage.sh: the first-start seed, the guard against
# system updates pocket-ui did not stage (docs/SD-IMAGE.md "Updates on the card"), and the GAMES drive's two scripts
# games-first-start.sh and games-mount.sh (docs/SD-IMAGE.md "The GAMES drive").
#   sh deploy/image/test-mount-storage.sh
# The hook runs under BusyBox sh with a PATH that holds ONLY applets ROCKNIX's initramfs has (busybox-init.conf of
# 20261001: no date, no test, no printf applet; ash has them as builtins), so a tool missing there fails here too.
# mount_part, mount and progress are fakes that log their calls, as ROCKNIX's init would provide them; PU_STORAGE points
# the hook at a fake /storage. About 1 s, part of the gate.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
command -v busybox >/dev/null || { echo "busybox is needed for this test"; exit 1; }
BB=$(command -v busybox)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
S=$T/storage
FL=$T/flash
fails=0
check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

mkdir "$T/applets"
for a in basename cat cp cut grep head ls mkdir mv rm sha256sum stat sync tr; do ln -s "$BB" "$T/applets/$a"; done

DROPIN=.config/system.d/essway.service.d/10-pocket-ui.conf

# A fresh card as the image leaves it. $1 = resize: STORAGE not grown yet (first boot).
setup() {
  rm -rf "$S" "$FL" "$T/calls"
  mkdir -p "$S/pocket-ui" "$S/.pocket-seed/$(dirname $DROPIN)" "$FL"
  echo '[Service]' >"$S/.pocket-seed/$DROPIN"
  chmod 644 "$S/.pocket-seed/$DROPIN"
  cp "$HERE/mount-storage.sh" "$FL/"
  [ "${1:-}" = resize ] && : >"$S/.please_resize_me"
  : >"$T/calls"
}

# Sources mount-storage.sh as ROCKNIX's init does (inside mount_storage, $disk set), with logging fakes.
boot() {
  env -i PATH="$T/applets" PU_STORAGE="$S" PU_FLASH="$FL" PU_SYSROOT="$T/sysroot" T="$T" FL="$FL" "$BB" sh -c '
    mount_part() { echo "mount_part $*" >>$T/calls; }
    mount() { echo "mount $*" >>$T/calls; }
    progress() { echo "progress $*" >>$T/calls; }
    # The card is a fixture: record the barrier without flushing unrelated host writes.
    sync() { echo "sync $*" >>$T/calls; }
    disk=LABEL=STORAGE
    check_update() { echo "check_update" >>$T/calls; }
    mount_storage() { if [ -f $FL/mount-storage.sh ]; then . $FL/mount-storage.sh; else mount_part "$disk" /storage rw,noatime; fi; }
    # init runs its boot steps by name, mount_storage before check_update.
    for BOOT_STEP in mount_storage check_update; do ${BOOT_STEP}; done
  '
}

echo "== first boot: STORAGE still 32 MB, ROCKNIX will grow it"
setup resize
boot
check "mounts STORAGE as ROCKNIX does" 'grep -qx "mount_part LABEL=STORAGE /storage rw,noatime" $T/calls'
check "no .config yet (fs-resize would refuse)" '[ ! -e $S/.config ]'
check "seed stays" '[ -f "$S/.pocket-seed/$DROPIN" ]'
check "hook stays on /flash" '[ -f $FL/mount-storage.sh ]'
check "/flash not touched" '! grep -q "^mount " $T/calls'

echo "== second boot: STORAGE grown"
rm -f $S/.please_resize_me
: >$T/calls
boot
check "mounts STORAGE" 'grep -qx "mount_part LABEL=STORAGE /storage rw,noatime" $T/calls'
check "drop-in in place" '[ "$(cat "$S/$DROPIN" 2>/dev/null)" = "[Service]" ]'
check "drop-in mode 644" '[ "$(stat -c %a "$S/$DROPIN" 2>/dev/null)" = 644 ]'
check "seed gone" '[ ! -e $S/.pocket-seed ]'
check "pocket-ui untouched" '[ -d $S/pocket-ui ]'
check "hook stays: it guards every start" '[ -f $FL/mount-storage.sh ]'
check "/flash never written" '! grep -q "^mount " $T/calls'

echo "== later boots: same mount, nothing else"
: >$T/calls
boot
check "plain mount, ROCKNIX's update check as usual" '[ "$(cat $T/calls)" = "$(printf "mount_part LABEL=STORAGE /storage rw,noatime\ncheck_update")" ]'
check "no .update-refused without updates" '[ ! -e $S/.update-refused ]'

echo "== existing settings are never overwritten by a seed left over"
setup
rm -rf $S/.pocket-seed
mkdir -p $S/.config/pocket-ui
echo '{}' >$S/.config/pocket-ui/settings.json
boot
check "copies nothing over existing settings" '[ "$(cat $S/.config/pocket-ui/settings.json)" = "{}" ] && [ ! -e "$S/$DROPIN" ]'

echo "== existing files in .config survive the move"
setup
mkdir -p $S/.config/system.d
echo keep >$S/.config/system.d/other.conf
boot
check "other file kept" '[ "$(cat $S/.config/system.d/other.conf)" = keep ]'
check "drop-in added" '[ -f "$S/$DROPIN" ]'

# The update guard. ROCKNIX's check_update runs after this hook and installs *.tar, *.img, *.img.gz or KERNEL+SYSTEM
# from $S/.update. TAR is a ROCKNIX-named update as EmulationStation's updater or pocket-ui would put it there.
U=$S/.update
R=$S/.update-refused
TAR=ROCKNIX-SM8250.aarch64-20261201.tar
# A grown card after its first starts, with an update tar ($1 = its contents).
updated() {
  setup
  rm -rf $S/.pocket-seed
  mkdir -p $U $S/roms
  echo game >$S/roms/game.gb
  printf '%s' "$1" >"$U/$TAR"
}
# The approval src/update.rs stage_os writes: "<sha256>  <name>".
approve() { echo "$(sha256sum "$U/$TAR" | cut -d' ' -f1)  $TAR" >$U/.pocket-ui-approved; }
# What check_update would find (init: ls -1 *.tar / *.img.gz / *.img, or KERNEL+SYSTEM).
installable() { ls $U/*.tar $U/*.img $U/*.img.gz 2>/dev/null | grep -q . || { [ -f $U/KERNEL ] && [ -f $U/SYSTEM ]; }; }

echo "== guard: a tar pocket-ui staged and approved is left for ROCKNIX"
updated "our update"
approve
boot
check "tar stays" '[ "$(cat $U/$TAR)" = "our update" ]'
check "approval stays for this start" '[ -f $U/.pocket-ui-approved ]'
check "nothing refused" '[ ! -e $R ]'
check "says it checked" 'grep -q "^progress pocket-ui: checking the system update" $T/calls'

echo "== guard: a tar without approval (EmulationStation's updater) is moved, not installed"
updated "rocknix update"
boot
check "nothing left to install" '! installable'
check "moved to .update-refused, unchanged" '[ "$(cat $R/$TAR)" = "rocknix update" ]'
check "the reason is logged" 'grep -q "^$TAR -> $TAR: not staged by pocket-ui" $R/refused.log'
check "user files untouched" '[ "$(cat $S/roms/game.gb)" = game ]'
check "said on screen" 'grep -q "^progress pocket-ui: system update $TAR not installed" $T/calls'

echo "== guard: a tar dropped over the network share next to an approved one"
updated "our update"
approve
printf 'dropped' >"$U/ROCKNIX-SM8250.aarch64-20270101.tar"
boot
check "ours stays" '[ "$(cat $U/$TAR)" = "our update" ]'
check "the dropped one is moved" '[ ! -e $U/ROCKNIX-SM8250.aarch64-20270101.tar ] && [ "$(cat $R/ROCKNIX-SM8250.aarch64-20270101.tar)" = dropped ]'
check "logged as not approved" 'grep -q "^ROCKNIX-SM8250.aarch64-20270101.tar -> .*not the update pocket-ui staged" $R/refused.log'

echo "== guard: the approved name, but other contents (replaced over the share)"
updated "our update"
approve
printf 'replaced' >"$U/$TAR"
boot
check "nothing left to install" '! installable'
check "moved, unchanged" '[ "$(cat $R/$TAR)" = replaced ]'
check "logged as a checksum mismatch" 'grep -q "checksum is not the one pocket-ui checked" $R/refused.log'
check "the spent approval goes" '[ ! -e $U/.pocket-ui-approved ]'

echo "== guard: an empty or broken approval refuses"
updated "our update"
: >$U/.pocket-ui-approved
boot
check "empty approval: moved" '! installable && [ -f $R/$TAR ]'
updated "our update"
echo "garbage" >$U/.pocket-ui-approved
boot
check "garbage approval: moved" '! installable && [ -f $R/$TAR ]'
updated "our update"
printf '%s  %s' "$(sha256sum "$U/$TAR" | cut -d' ' -f1)" "$TAR" >$U/.pocket-ui-approved
boot
check "approval without a final newline still counts" '[ -f $U/$TAR ] && [ ! -e $R ]'

echo "== guard: disk images and a loose KERNEL + SYSTEM are always refused"
updated "x"
rm -f "$U/$TAR"
printf img >$U/ROCKNIX-SM8250.aarch64-20261201.img
printf gz >$U/ROCKNIX-SM8250.aarch64-20261201.img.gz
printf k >$U/KERNEL
printf s >$U/SYSTEM
printf m >$U/SYSTEM.md5
boot
check "nothing left to install" '! installable'
check "all five moved, unchanged" '[ "$(cat $R/ROCKNIX-SM8250.aarch64-20261201.img $R/ROCKNIX-SM8250.aarch64-20261201.img.gz $R/KERNEL $R/SYSTEM $R/SYSTEM.md5)" = imggzksm ]'

echo "== guard: a second refusal of the same name keeps the first"
updated "first"
boot
printf 'second' >"$U/$TAR"
boot
check "both kept" '[ "$(cat $R/$TAR)" = first ] && [ "$(cat $R/$TAR.1)" = second ]'

echo "== guard: other files in .update stay where they are"
updated "x"
echo note >$U/readme.txt
mkdir -p $U/.tmp
boot
check "readme and ROCKNIX's .tmp untouched" '[ "$(cat $U/readme.txt)" = note ] && [ -d $U/.tmp ]'

echo "== guard: a file that cannot be moved to .update-refused is renamed where it is"
updated "rocknix update"
: >$R
boot
check "renamed, so check_update no longer sees it" '! installable && [ "$(cat $U/$TAR.refused)" = "rocknix update" ]'

echo "== guard: the in-place fallback never overwrites an earlier refused file"
updated "first update"
: >$R
boot
printf 'second update' >"$U/$TAR"
boot
check "both kept" '[ "$(cat $U/$TAR.refused)" = "first update" ] && [ "$(cat $U/$TAR.refused.1)" = "second update" ]'
check "nothing left to install" '! installable'
check "ROCKNIX's update check still runs (nothing foreign left)" '[ "$(grep -c "^check_update$" $T/calls)" = 2 ]'

echo "== guard: nothing can be moved at all (STORAGE read-only or damaged)"
if [ "$(id -u)" = 0 ]; then
  echo "skip (root ignores the read-only folder this case uses)"
else
  updated "rocknix update"
  : >$R
  chmod 555 $U
  boot
  chmod 755 $U
  check "the foreign tar is still there, untouched" '[ "$(cat $U/$TAR)" = "rocknix update" ]'
  check "ROCKNIX's update check is skipped for this start" '! grep -q "^check_update$" $T/calls'
  check "said on screen, start goes on" 'grep -q "^progress pocket-ui: .*update check skipped" $T/calls'
  chmod 755 $U; rm -f $R
  boot
  check "next start with a writable STORAGE: moved, update check back" '! installable && [ -f $R/$TAR ] && grep -q "^check_update$" $T/calls'
  updated "our update"
  approve
  chmod 555 $U
  boot
  chmod 755 $U
  check "an approved tar on a read-only STORAGE still installs (check not skipped)" '[ -f $U/$TAR ] && grep -q "^check_update$" $T/calls'
fi

echo "== guard: no .update at all"
setup
rm -rf $S/.pocket-seed $U
boot
check "nothing created" '[ ! -e $U ] && [ ! -e $R ]'

echo "== first start with a GAMES drive: the hook starts games-first-start.sh instead of ROCKNIX's fs-resize"
setup resize
cp "$HERE/games-first-start.sh" "$FL/"
chmod 755 "$FL/games-first-start.sh"
boot
check "bound over fs-resize" 'grep -qx "mount --bind $FL/games-first-start.sh $T/sysroot/usr/lib/rocknix/fs-resize" $T/calls'
setup
cp "$HERE/games-first-start.sh" "$FL/"
chmod 755 "$FL/games-first-start.sh"
boot
check "not at later starts" '! grep -q "^mount " $T/calls'
setup resize
cp "$HERE/games-first-start.sh" "$FL/"
chmod 644 "$FL/games-first-start.sh"
boot
check "not when /flash can't run it: ROCKNIX's script runs" '! grep -q "^mount " $T/calls'

# games-first-start.sh with stand-ins for what changes the card. $G/table holds the partition table, $G/fs what's on
# p3 ("exfat GAMES", "ext4 old" or nothing), $G/size the card in MiB; every tool call can be the moment the power goes.
G=$T/g
mkdir -p "$T/gtools"
for a in basename cat cp cut grep head ls mkdir mv rm sync tr sleep wc touch; do ln -s "$BB" "$T/gtools/$a"; done
tool() {
  { printf '#!%s sh\nG=%s\necho "$(basename "$0") $*" >>$G/calls\n' "$BB" "$G"
    cat <<'POWER'
n=$(cat $G/n 2>/dev/null); n=$((${n:-0} + 1)); echo $n >$G/n
if [ "$n" = "$(cat $G/cut-at 2>/dev/null)" ]; then echo cut >$G/was-cut; kill -9 $(cat $G/top); exit 1; fi
POWER
    cat; } >"$T/gtools/$1"
  chmod 755 "$T/gtools/$1"
}
tool parted <<'TOOL'
case "$*" in *print*)
  echo "BYT;"; while read -r n a b; do echo "$n:${a}s:${b}s:0s:::;"; done <$G/table; exit 0 ;; esac
case "$*" in *resizepart*) [ ! -e $G/refuse-table ] || exit 1
  e=${6:-0}; for a in "$@"; do last=$a; done
  grep -v '^2 ' $G/table >$G/t; a=$(grep '^2 ' $G/table | cut -d" " -f2); { cat $G/t; echo "2 $a ${last%s}"; } | sort >$G/table
  echo $((${last%s} - a + 1)) >$G/block/mmcblk0p2/size ;; esac
case "$*" in *mkpart*) [ ! -e $G/refuse-table ] || exit 1
  set -- $*; shift $(($# - 2)); echo "3 $1 $2" >>$G/table; mkdir -p $G/block/mmcblk0p3; echo 1 >$G/block/mmcblk0p3/size ;; esac
case "$*" in *" rm 3"*) grep -v '^3 ' $G/table >$G/t; cp $G/t $G/table; rm -rf $G/block/mmcblk0p3 ;; esac
exit 0
TOOL
tool blkid <<'TOOL'
set -- $* ; want=$5
[ -s $G/fs ] || exit 2
set -- $(cat $G/fs); [ "$want" = TYPE ] && echo "$1" || echo "$2"
TOOL
tool mkfs.exfat <<'TOOL'
[ ! -e $G/refuse-mkfs ] || exit 1
echo x >>$G/formatted; rm -f $G/games; echo "exfat GAMES" >$G/fs
TOOL
tool fsck.exfat <<'TOOL'
[ ! -e $G/fsck-bad ] || [ "$(cat $G/fs)" != "exfat GAMES" ] || [ -e $G/formatted ]
TOOL
tool mount <<'TOOL'
for a in "$@"; do [ "$a" = "$G/storage" ] && { echo "/dev/mmcblk0p2 $G/storage ext4 rw 0 0" >$G/mounts; exit 0; }; done
for a in "$@"; do [ "$a" = "$G/work" ] && { mkdir -p $G/work; exit 0; }; done
exit 1
TOOL
tool umount <<'TOOL'
for a in "$@"; do [ "$a" = "$G/storage" ] && { : >$G/mounts; exit 0; }; done
exit 0
TOOL
for t in partprobe e2fsck resize2fs tune2fs; do tool $t </dev/null; done
tool reboot <<'TOOL'
echo reboot >>$G/end
TOOL

newcard() {
  rm -rf "$G"
  mkdir -p "$G/storage" "$G/block/mmcblk0" "$G/block/mmcblk0p2"
  : >"$G/storage/.please_resize_me"
  printf '1 32768 4227071\n2 4227072 4292607\n' >"$G/table"
  : >"$G/fs"
  echo $((${1:-64000} * 2048)) >"$G/block/mmcblk0/size"
  echo 65536 >"$G/block/mmcblk0p2/size"
  echo "/dev/mmcblk0p2 $G/storage ext4 rw 0 0" >"$G/mounts"
  printf 'd\t/storage/roms/gba\t\t\t0777 root root - -\nd\t/storage/roms/snes\t\t\t0777 root root - -\nd\t/tmp/x\n' >"$G/dirs"
  # Stands for ROCKNIX's own script: what it does to the card is not this test's.
  printf '#!%s sh\necho stock >>%s/end\nrm -f %s/storage/.please_resize_me\n' "$BB" "$G" "$G" >"$G/stock"
  chmod 755 "$G/stock"
}
firststart() {
  env -i PATH="$T/gtools" PU_STORAGE="$G/storage" PU_STOCK="$G/stock" PU_MOUNTS="$G/mounts" PU_BLOCK="$G/block" PU_DIRS="$G/dirs" \
    PU_LOG="$G/log" PU_WORK="$G/work" T="$T" HERE="$HERE" "$BB" sh -c '
      echo $$ >"$T/g/top"
      # BusyBox may run its own applet before anything on PATH: these must be the stand-ins.
      mount() { "$T/gtools/mount" "$@"; }
      umount() { "$T/gtools/umount" "$@"; }
      blkid() { "$T/gtools/blkid" "$@"; }
      reboot() { "$T/gtools/reboot" "$@"; }
      sync() { echo "sync $*" >>"$T/g/calls"; }
      . "$HERE/games-first-start.sh"
    ' >/dev/null 2>&1
}
split_right() {
  [ "$(cat "$G/table")" = "$(printf '1 32768 4227071\n2 4227072 16809983\n3 16809984 131071999')" ]
}
marker_gone() { [ ! -e "$G/storage/.please_resize_me" ]; }

echo "== games-first-start.sh: a 64 GB card"
newcard
firststart
check "p2 is 6 GiB, p3 starts on 4 MiB, ends on a whole MiB" 'split_right && [ $(((16809983 - 4227072 + 1) / 2048)) = 6144 ] && [ $((16809984 % 8192)) = 0 ]'
check "p3 is type 7 (parted's name for it: ntfs), exact as asked" 'grep -q "mkpart primary ntfs 16809984 131071999" $G/calls && grep -q -- "-a none" $G/calls'
check "formatted once as exFAT GAMES, folders ready" '[ "$(wc -l <$G/formatted)" = 1 ] && [ "$(cat $G/fs)" = "exfat GAMES" ] && [ -d $G/work/roms/gba ] && [ -d $G/work/roms/snes ] && [ ! -e $G/work/roms/x ]'
check "says where games go, on the drive" 'grep -q "roms" "$G/work/READ ME - games go in roms.txt"'
check "restarts with the marker gone, no state left, ROCKNIX's script not used" 'marker_gone && [ "$(cat $G/end)" = reboot ] && [ ! -e $G/storage/.pocket-games-state ] && [ ! -e $G/storage/.pocket-games-tries ]'
check "never writes to /flash" '! grep -q flash $G/calls'
check "grows STORAGE after the table, before the GAMES drive" '[ "$(grep -n "^resize2fs" $G/calls | cut -d: -f1)" -gt "$(grep -n "mkpart" $G/calls | cut -d: -f1)" ] && [ "$(grep -n "^resize2fs" $G/calls | cut -d: -f1)" -lt "$(grep -n "^mkfs.exfat" $G/calls | cut -d: -f1)" ]'

echo "== power cut at every step of a first start: the next start finishes the job"
newcard
firststart
steps=$(cat "$G/n")
bad=0
c=1
while [ $c -le $steps ]; do
  newcard
  echo $c >"$G/cut-at"
  firststart
  [ -e "$G/was-cut" ] || { echo "     step $c: no cut"; bad=1; }
  # Gone is fine only when everything was done before the cut.
  if marker_gone && ! { split_right && [ "$(cat "$G/fs")" = "exfat GAMES" ]; }; then echo "     step $c: marker gone early"; bad=1; fi
  # The player switches on again: the kernel reads the table, init mounts STORAGE, and fs-resize.target runs only with the marker.
  rm -f "$G/cut-at" "$G/n"
  echo "/dev/mmcblk0p2 $G/storage ext4 rw 0 0" >"$G/mounts"
  marker_gone || firststart
  split_right && [ "$(cat "$G/fs")" = "exfat GAMES" ] && marker_gone || { echo "     step $c: table $(tr '\n' ',' <"$G/table") fs $(cat "$G/fs") end $(cat "$G/end" 2>/dev/null)"; bad=1; }
  c=$((c + 1))
done
check "$steps cut points, each recovers to the same card" '[ $bad = 0 ] && [ "$steps" -gt 10 ]'

echo "== after a cut, games already on a good GAMES are never formatted away"
newcard
firststart
n=$(wc -l <"$G/formatted")
: >"$G/storage/.please_resize_me"
: >"$G/storage/.pocket-games-state"
echo table >"$G/storage/.pocket-games-state"
: >"$G/games"
firststart
check "no second format, games stay" '[ "$(wc -l <$G/formatted)" = "$n" ] && [ -e $G/games ]'

echo "== a card flashed again with the old GAMES still behind the new table"
newcard
echo "exfat GAMES" >"$G/fs"
: >"$G/games"
firststart
check "same split, GAMES and its games stay" 'split_right && [ ! -e $G/formatted ] && [ -e $G/games ] && marker_gone'

echo "== a half-made GAMES from a cut during formatting is made again"
newcard
printf '1 32768 4227071\n2 4227072 16809983\n3 16809984 131071999\n' >"$G/table"
echo 12582912 >"$G/block/mmcblk0p2/size"
mkdir -p "$G/block/mmcblk0p3"
echo 1 >"$G/block/mmcblk0p3/size"
echo formatting >"$G/storage/.pocket-games-state"
firststart
check "formatted" '[ "$(wc -l <$G/formatted)" = 1 ] && [ "$(cat $G/fs)" = "exfat GAMES" ] && marker_gone'

echo "== a third partition that isn't ours"
newcard
printf '1 32768 4227071\n2 4227072 4292607\n3 16809984 131071999\n' >"$G/table"
echo "ext4 mine" >"$G/fs"
firststart
check "left alone: ROCKNIX's script, no format, table as it was" '[ "$(cat $G/end)" = stock ] && [ ! -e $G/formatted ] && [ "$(cat $G/fs)" = "ext4 mine" ] && grep -q "^3 16809984" $G/table && ! grep -q mkpart $G/calls && ! grep -q " rm 3" $G/calls'

echo "== cards too small to split"
newcard 7400
firststart
check "8 GB card: ROCKNIX's script, table untouched, marker still there for it" '[ "$(cat $G/end)" = stock ] && [ ! -e $G/formatted ] && ! grep -q "resizepart\|mkpart" $G/calls'
newcard 12288
firststart
check "just big enough (12 GiB): split" 'grep -q mkpart $G/calls && [ "$(tail -1 $G/end)" = reboot ]'

echo "== what goes wrong ends in ROCKNIX's script, with today's layout and no state left"
newcard
: >"$G/refuse-table"
firststart
check "the table can't be changed" '[ "$(cat $G/end)" = stock ] && [ ! -e $G/formatted ] && [ ! -e $G/storage/.pocket-games-state ] && grep -q "/storage ext4\|$G/storage" $G/mounts'
newcard
: >"$G/refuse-mkfs"
firststart
check "formatting fails: p3 goes again, STORAGE gets the card" '[ "$(cat $G/end)" = stock ] && ! grep -q "^3 " $G/table && grep -q "rm 3" $G/calls && grep -q "$G/storage" $G/mounts'
newcard
: >"$G/storage/.pocket-games-tries"; printf xxx >"$G/storage/.pocket-games-tries"
firststart
check "three tries used up: no loop of restarts" '[ "$(cat $G/end)" = stock ] && ! grep -q mkpart $G/calls'
newcard
mkdir "$G/storage/.config"
firststart
check "a card already set up: ROCKNIX's refusal, nothing changed" '[ "$(cat $G/end)" = stock ] && ! grep -q "resizepart\|mkpart" $G/calls'

echo "== games-mount.sh: the drive's own mount"
M=$T/m
mkdir -p "$M/tools"
for a in basename cat cp cut grep rm od sync tr; do ln -s "$BB" "$M/tools/$a"; done
mtool() { { printf '#!%s sh\nM=%s\necho "$(basename "$0") $*" >>$M/calls\n' "$BB" "$M"; cat; } >"$M/tools/$1"; chmod 755 "$M/tools/$1"; }
mtool blkid <<'TOOL'
cat $M/label
TOOL
mtool umount <<'TOOL'
for a in "$@"; do grep -v " $a " $M/mounts >$M/m2; cp $M/m2 $M/mounts; done
TOOL
mtool mount <<'TOOL'
case "$*" in *--bind*) ;; *) [ ! -e $M/refuse ] || case "$*" in *noatime*) exit 1 ;; esac
  echo "$(echo "$*" | grep -o '/dev/[a-z0-9]*') /e exfat rw 0 0" >>$M/mounts ;; esac
TOOL
mtool timeout <<'TOOL'
:
TOOL
# The flags byte, 106 bytes in, as dd would read it.
printf '#!%s sh\nprintf "\\\\$(cat %s/flags)"\n' "$BB" "$M" >"$M/tools/dd"
chmod 755 "$M/tools/dd"
gamesmount() {
  env -i PATH="$M/tools" PU_MOUNTS="$M/mounts" PU_EXTERNAL=/e PU_ROMS=/r PU_INTERNAL=/i M="$M" HERE="$HERE" "$BB" sh -c '
      mount() { "$M/tools/mount" "$@"; }
      umount() { "$M/tools/umount" "$@"; }
      blkid() { "$M/tools/blkid" "$@"; }
      timeout() { "$M/tools/timeout" "$@"; }
      dd() { "$M/tools/dd" "$@"; }
      . "$HERE/games-mount.sh"
    ' >/dev/null 2>&1
}
minit() {
  rm -f "$M/calls" "$M/refuse"
  printf '/dev/mmcblk0p2 /storage ext4 rw 0 0\n%s /e %s rw 0 0\n' "$1" "$2" >"$M/mounts"
  echo "$3" >"$M/label"
  echo 000 >"$M/flags"
}
minit /dev/mmcblk0p3 exfat GAMES
gamesmount
check "GAMES on the boot card: mounted again, then bound to roms" 'grep -q "mount -t exfat -o noatime,fmask=0022,dmask=0022" $M/calls && grep -q -- "--bind /e/roms /r\|--bind /i/roms /r" $M/calls'
check "a clean drive isn't checked" '! grep -q "^timeout" $M/calls'
minit /dev/mmcblk0p3 exfat GAMES
echo 002 >"$M/flags"
gamesmount
check "a drive not closed properly is checked first" 'grep -q "^timeout 120 fsck.exfat -p /dev/mmcblk0p3" $M/calls'
minit /dev/mmcblk0p3 exfat GAMES
: >"$M/refuse"
gamesmount
check "the new mount refused: the plain one comes back" 'grep -qx "mount -t exfat /dev/mmcblk0p3 /e" $M/calls && grep -q " /e " $M/mounts'
minit /dev/mmcblk1p1 exfat GAMES
gamesmount
check "a card in the second slot stays as automount left it, even one called GAMES" '[ "$(grep -c "^mount\|^umount" $M/calls)" = 0 ]'
minit /dev/mmcblk0p3 ext4 GAMES
gamesmount
check "not exFAT: nothing" '[ ! -e $M/calls ]'
printf '/dev/mmcblk0p2 /storage ext4 rw 0 0\n' >"$M/mounts"
rm -f "$M/calls"
gamesmount
check "no drive mounted (games in STORAGE): nothing" '[ ! -e $M/calls ]'

echo
if [ $fails -eq 0 ]; then echo "all passed"; else echo "$fails failed"; exit 1; fi

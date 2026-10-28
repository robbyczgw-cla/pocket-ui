#!/bin/sh
# Runs inside the pocket-vdev-extract container (see scripts/vdev.sh image).
# Unpacks /in/rocknix.img (GPT -> FAT -> SYSTEM squashfs), drops what a virtual Mini V2 never
# needs (standalone emulators, wine, firmware, huge cores) and writes the rootfs as a tar to stdout.
set -eu
exec 3>&1 1>&2   # progress to stderr, the tar to fd 3

W=/work
mkdir -p $W/a $W/b
echo "[extract] GPT image -> system partition"
7z x -y -o$W/a /in/rocknix.img '*system*' >/dev/null
echo "[extract] FAT -> SYSTEM"
7z x -y -o$W/b $W/a/*system* SYSTEM >/dev/null
rm -rf $W/a
echo "[extract] squashfs -> rootfs"
unsquashfs -no-progress -no-xattrs -d $W/rootfs $W/b/SYSTEM >/dev/null
rm -rf $W/b

cd $W/rootfs
echo "[extract] trimming"
rm -rf usr/lib/wine usr/lib32 usr/lib/kernel-overlays usr/lib/firmware usr/lib/steam usr/lib/vlc \
  usr/lib/compat usr/lib/librenderdoc.so \
  usr/share/scummvm usr/config/scummvm usr/share/armsx2-sa usr/share/aethersx2-sa usr/share/Cemu \
  usr/share/slang-shaders usr/share/soundfonts usr/share/themes \
  usr/config/ppsspp \
  usr/bin/scummvm usr/bin/duckstation-sa usr/bin/rclone usr/bin/azahar usr/bin/cemu usr/bin/rpcs3-sa \
  usr/bin/vita3k-sa usr/bin/ppsspp usr/bin/flycast usr/bin/SkyEmu usr/bin/box64 usr/bin/syncthing \
  usr/sbin/tailscale usr/sbin/tailscaled
# libretro-database: only the cheats (cht) of the consoles pocket-core hosts, about 70 MB (src/cheats.rs reads them)
if [ -d usr/share/libretro-database ]; then
  find usr/share/libretro-database -mindepth 1 -maxdepth 1 ! -name cht -exec rm -rf {} +
  find usr/share/libretro-database/cht -mindepth 1 -maxdepth 1 \
    ! -name 'Nintendo - Game Boy' ! -name 'Nintendo - Game Boy Color' ! -name 'Nintendo - Game Boy Advance' \
    ! -name 'Nintendo - Nintendo Entertainment System' ! -name 'Nintendo - Super Nintendo Entertainment System' \
    ! -name 'Sega - Mega Drive - Genesis' ! -name 'Sega - Master System - Mark III' ! -name 'Sega - Game Gear' \
    ! -name 'NEC - PC Engine - TurboGrafx 16' ! -name 'Atari - 2600' ! -name 'Sony - PlayStation' -exec rm -rf {} +
fi
# cores over 20 MB (mame, fbneo, scummvm, flycast, puae, ...): not what the shelf runs
find usr/lib/libretro -name '*.so' -size +20M -print -delete | sed 's/^/[extract] dropped /'

echo "[extract] tar"
tar -C $W/rootfs -cf - . >&3

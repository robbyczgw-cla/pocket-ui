#!/usr/bin/env bash
# Test fixture only. Uses adb root on the dedicated emulator; NEVER a handheld installer.
# Usage: prepare-retroarch-emulator.sh RetroArch.apk gearsystem_libretro_android.so.zip
set -euo pipefail
cd "$(dirname "$0")/../.."
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
ADB="$SDK/platform-tools/adb"
SERIAL="${ANDROID_SERIAL:-emulator-5580}"
case "$SERIAL" in emulator-[0-9]*) ;; *) echo 'Refusing a non-emulator serial' >&2; exit 2;; esac
[ "$("$ADB" -s "$SERIAL" shell getprop ro.kernel.qemu | tr -d '\r')" = 1 ] || exit 2
[ "$#" = 2 ] || { echo 'Supply official RetroArch APK and Android ARM64 Gearsystem zip' >&2; exit 2; }
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
unzip -q "$2" -d "$TMP"
CORE="$TMP/gearsystem_libretro_android.so"
test -f "$CORE"
"$ADB" -s "$SERIAL" install -r "$1"
"$ADB" -s "$SERIAL" root
"$ADB" -s "$SERIAL" wait-for-device
"$ADB" -s "$SERIAL" shell mkdir -p /data/data/com.retroarch/cores /data/data/com.retroarch/files
"$ADB" -s "$SERIAL" push "$CORE" /data/data/com.retroarch/cores/ > /dev/null
"$ADB" -s "$SERIAL" shell 'chown -R $(stat -c %u /data/data/com.retroarch):$(stat -c %g /data/data/com.retroarch) /data/data/com.retroarch/cores /data/data/com.retroarch/files; restorecon -RF /data/data/com.retroarch/cores /data/data/com.retroarch/files'
"$ADB" -s "$SERIAL" unroot
"$ADB" -s "$SERIAL" wait-for-device

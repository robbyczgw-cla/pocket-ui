#!/usr/bin/env bash
# Isolated ARM64 AVD. Never reuses another project's emulator image/data.
set -euo pipefail
cd "$(dirname "$0")/../.."
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
NAME="${ANDROID_AVD_NAME:-pocket-android-mvp}"
PORT="${ANDROID_EMULATOR_PORT:-5580}"
WIDTH="${ANDROID_DISPLAY_WIDTH:-1280}"
HEIGHT="${ANDROID_DISPLAY_HEIGHT:-720}"
DENSITY="${ANDROID_DISPLAY_DENSITY:-240}"
case "$NAME" in *[!a-zA-Z0-9_-]*|'') echo 'Invalid AVD name' >&2; exit 2;; esac
for number in "$PORT" "$WIDTH" "$HEIGHT" "$DENSITY"; do case "$number" in *[!0-9]*|'') exit 2;; esac; done
export ANDROID_AVD_HOME="$PWD/android/build/avd"
AVD="$ANDROID_AVD_HOME/$NAME.avd"
mkdir -p "$AVD"
cat > "$ANDROID_AVD_HOME/$NAME.ini" <<INI
avd.ini.encoding=UTF-8
path=$AVD
target=android-35
INI
cat > "$AVD/config.ini" <<INI
AvdId=$NAME
avd.ini.displayname=$NAME
abi.type=arm64-v8a
hw.cpu.arch=arm64
hw.cpu.ncore=4
hw.ramSize=2048
hw.lcd.width=$WIDTH
hw.lcd.height=$HEIGHT
hw.lcd.density=$DENSITY
hw.gpu.enabled=yes
hw.gpu.mode=swiftshader
hw.keyboard=yes
hw.mainKeys=no
image.sysdir.1=$ANDROID_HOME/system-images/android-35/google_apis/arm64-v8a/
disk.dataPartition.size=2G
showDeviceFrame=no
tag.id=google_apis
INI
exec "$ANDROID_HOME/emulator/emulator" -avd "$NAME" -port "$PORT" -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader -memory 2048 -cores 4 "$@"

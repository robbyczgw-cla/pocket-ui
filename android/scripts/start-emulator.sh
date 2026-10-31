#!/usr/bin/env bash
# Isolated ARM64 AVD. Never reuses another project's emulator image/data.
set -euo pipefail
cd "$(dirname "$0")/../.."
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_AVD_HOME="$PWD/android/build/avd"
AVD="$ANDROID_AVD_HOME/pocket-android-spike.avd"
mkdir -p "$AVD"
cat > "$ANDROID_AVD_HOME/pocket-android-spike.ini" <<INI
avd.ini.encoding=UTF-8
path=$AVD
target=android-35
INI
cat > "$AVD/config.ini" <<INI
AvdId=pocket-android-spike
avd.ini.displayname=Pocket Android Spike
abi.type=arm64-v8a
hw.cpu.arch=arm64
hw.cpu.ncore=4
hw.ramSize=2048
hw.lcd.width=1280
hw.lcd.height=720
hw.lcd.density=240
hw.gpu.enabled=yes
hw.gpu.mode=swiftshader
hw.keyboard=yes
hw.mainKeys=no
image.sysdir.1=$ANDROID_HOME/system-images/android-35/google_apis/arm64-v8a/
disk.dataPartition.size=2G
showDeviceFrame=no
tag.id=google_apis
INI
exec "$ANDROID_HOME/emulator/emulator" -avd pocket-android-spike -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader -memory 2048 -cores 4 "$@"

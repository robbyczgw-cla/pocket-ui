#!/usr/bin/env bash
# Dedicated emulator only. Real SAF + our player, release APK (no run-as/debuggable assumption).
set -euo pipefail
cd "$(dirname "$0")/../.."
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
ADB="$SDK/platform-tools/adb"
SERIAL="${ANDROID_SERIAL:-emulator-5580}"
OUT="${ANDROID_EVIDENCE_DIR:-$PWD/android/build/evidence}"
APK="${ANDROID_APK:-$PWD/android/build/pocket-android-mvp.apk}"
case "$SERIAL" in emulator-[0-9]*) ;; *) echo 'Refusing a non-emulator serial' >&2; exit 2;; esac
[ "$("$ADB" -s "$SERIAL" shell getprop ro.kernel.qemu | tr -d '\r')" = 1 ] || { echo 'Target is not an emulator' >&2; exit 2; }
mkdir -p "$OUT"
"$ADB" -s "$SERIAL" install -r "$APK"
python3 scripts/android/player-smoke.py "$ADB" "$SERIAL" "$OUT/player"
python3 scripts/android/classic-smoke.py "$ADB" "$SERIAL" "$OUT/classic-cores"
if [ "${ANDROID_SMOKE_RETROARCH:-0}" = 1 ]; then
    python3 scripts/android/retroarch-smoke.py "$ADB" "$SERIAL" "$OUT/retroarch"
fi
printf 'Evidence: %s\n' "$OUT"

#!/bin/sh
# Shizuku's binder interfaces (RikkaApps/Shizuku-API, MIT; android/aidl/moe/shizuku/server) as the Java that
# android/java/moe/shizuku/server holds. Run after updating the .aidl files; the build compiles the Java, it needs no aidl.
set -eu
cd "$(dirname "$0")/../.."
ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
AIDL="$ANDROID_HOME/build-tools/${ANDROID_BUILD_TOOLS:-35.0.0}/aidl"
FRAMEWORK="$ANDROID_HOME/platforms/android-35/framework.aidl"
for f in android/aidl/moe/shizuku/server/*.aidl; do "$AIDL" -p"$FRAMEWORK" -Iandroid/aidl -oandroid/java "$f"; done
# No build machine's paths in the sources (scripts/check-leaks.sh).
sed -i.bak 's#^ \* Using: .*# * Using: aidl (build-tools 35.0.0) -p framework.aidl -Iandroid/aidl -oandroid/java android/aidl/moe/shizuku/server/<name>.aidl (scripts/android/shizuku-aidl.sh)#' android/java/moe/shizuku/server/*.java
rm -f android/java/moe/shizuku/server/*.java.bak

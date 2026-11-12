#!/bin/sh
# Compile-check Android launcher and player before tests when the Rust target and NDK are installed.
# android: `cargo check --target aarch64-linux-android` of android/ (launcher) and android/player/ (player), each with its own
#          Cargo.lock, and so the shared src/ under cfg(target_os = "android"). Needs the Rust target and an NDK
#          (ANDROID_NDK_HOME, else the newest ndk/ under ANDROID_HOME or /opt/android-sdk); without them it says SKIPPED
#          and passes. GATE_ANDROID_TARGET_DIR puts its build files elsewhere than CARGO_TARGET_DIR.
# Exit 0 ok (or skipped), 1 a check failed (the compiler's errors are printed).
set -u
cd "$(dirname "$0")/.." || exit 2
export PATH=$HOME/.cargo/bin:$PATH
parts=${*:-android}

for part in $parts; do
    start=$(date +%s)
    case $part in
    android)
        if ! rustup target list --installed 2>/dev/null | grep -qx aarch64-linux-android; then
            echo "check-platforms: android SKIPPED (no Rust target aarch64-linux-android here)"; continue
        fi
        ndk=${ANDROID_NDK_HOME:-}
        if [ -z "$ndk" ]; then
            for sdk in "${ANDROID_HOME:-}" "$HOME/Library/Android/sdk" "$HOME/android/sdk" /opt/android-sdk; do
                [ -n "$sdk" ] && [ -d "$sdk/ndk" ] || continue
                ndk=$(ls -d "$sdk"/ndk/*/ 2>/dev/null | sort -V | tail -1); ndk=${ndk%/}
                [ -n "$ndk" ] && break
            done
        fi
        bin=$(ls -d "$ndk"/toolchains/llvm/prebuilt/*/bin 2>/dev/null | head -1)
        if [ -z "$bin" ]; then echo "check-platforms: android SKIPPED (no Android NDK here)"; continue; fi
        # The C parts (the log shim) need the NDK's compiler; API 26 as in scripts/android/build-apk.sh.
        export CC_aarch64_linux_android="$bin/aarch64-linux-android26-clang" AR_aarch64_linux_android="$bin/llvm-ar"
        export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
        for m in android/Cargo.toml android/player/Cargo.toml; do
            out=$(CARGO_TARGET_DIR=${GATE_ANDROID_TARGET_DIR:-${CARGO_TARGET_DIR:-$PWD/target}} \
                cargo check --locked --target aarch64-linux-android --manifest-path "$m" -q 2>&1) || {
                echo "check-platforms: android ($m) FAILED"; echo "$out" | grep -E -A6 '^error' | head -40
                # A crate added to the root Cargo.toml: the Android crates' own lock files need it too.
                echo "$out" | grep -q 'needs to be updated' && echo "check-platforms: run \`cargo update -w --manifest-path $m\` and commit its Cargo.lock"
                exit 1; }
        done
        echo "check-platforms: android ok ($(( $(date +%s) - start )) s)"
        ;;
    *) echo "check-platforms: unknown part $part (android)" >&2; exit 2 ;;
    esac
done
exit 0

#!/bin/sh
# Shared compile-first stage for the merge gate and the queue client's precheck.
# No linking, packaging or test execution. The caller owns its heavy/target slot.
set -eu
cd "${1:-$(dirname "$0")/..}"
profile=${GATE_PROFILE:-gate}
case $profile in release) pflag=--release ;; *) pflag="--profile $profile" ;; esac
start=$(date +%s)
stage=linux
trap 'rc=$?; [ "$rc" -eq 0 ] || echo "compile-first: $stage FAILED ($(( $(date +%s) - start )) s)"' EXIT
echo 'compile-first: linux --all-targets (including examples)'
cargo check $pflag --locked --all-targets
if grep -Eq '^[[:space:]]*brick[[:space:]]*=' Cargo.toml; then
    stage=brick
    echo 'compile-first: brick --all-targets'
    cargo check $pflag --locked --all-targets --features brick
fi
# Check only configurations this tree still ships. A branch may remove a
# platform (and its check-platforms argument) while another train is in flight.
parts=
if [ -f android/Cargo.toml ] || [ -f android/player/Cargo.toml ]; then parts=android; fi
if [ -n "$parts" ] && [ -f scripts/check-platforms.sh ]; then
    stage=platforms
    sh scripts/check-platforms.sh $parts
fi
echo "compile-first: ok ($(( $(date +%s) - start )) s)"

#!/bin/sh
# One supervised lane; all children inherit the caller's process group and CPU affinity.
set -u
lane=$1 name=$2 RCD=$3
unset POCKETUI_SNAPSHOT_CACHE_DIR
L=/tmp/gate-launcher-$name.log V=/tmp/gate-verify-$name.log C=/tmp/gate-licences-$name.log
P=/tmp/gate-package-$name.log Q=/tmp/gate-grub-$name.log G=/tmp/gate-image-grub-$name.log F=/tmp/gate-fat-hook-$name.log
date +%s >"$RCD/$lane.start"
case $lane in
 launcher)
    sh deploy/test-run-pocket-ui.sh >"$L" 2>&1; echo $? >"$RCD/l"
    ;;
 small)
    { sh scripts/test-verify-device.sh && python3 scripts/test-device-portability.py && python3 scripts/test-device-transfer.py; } >"$V" 2>&1; echo $? >"$RCD/v"
    bash scripts/test-check-core-licences.sh >"$C" 2>&1; echo $? >"$RCD/c"
    sh deploy/package/test-package.sh >"$P" 2>&1; echo $? >"$RCD/p"
    sh deploy/test-grub-quiet.sh >"$Q" 2>&1; echo $? >"$RCD/q"
    { sh scripts/image/test-grub-checks.sh && sh scripts/image/test-rocknix-source.sh && sh scripts/image/test-h700-pins.sh && sh scripts/image/test-storage-budget.sh && python3 -I scripts/image/test-receipts.py; } >"$G" 2>&1; echo $? >"$RCD/g"
    { sh deploy/fat/test-hook.sh && sh deploy/image/test-mount-storage.sh; } >"$F" 2>&1; echo $? >"$RCD/f"
    ;;
 *) echo "gate: unknown shell lane $lane" >&2; exit 2 ;;
esac
date +%s >"$RCD/$lane.end"

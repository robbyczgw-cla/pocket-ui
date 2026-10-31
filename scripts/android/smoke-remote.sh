#!/usr/bin/env bash
# Run from the Linux checkout after preparing the dedicated emulator on the selected Mac.
# Usage: smoke-remote.sh <ssh-host>; exports to ~/work/drop/export/web/android-mvp by default.
set -euo pipefail
cd "$(dirname "$0")/../.."
[ "$#" = 1 ] || { echo 'Usage: smoke-remote.sh <ssh-host>' >&2; exit 2; }
HOST=$1
case "$HOST" in -*|*[!a-zA-Z0-9@._-]*) echo 'Invalid SSH host' >&2; exit 2;; esac
OUT="${ANDROID_EXPORT_DIR:-$HOME/work/drop/export/web/android-mvp}"
[ -z "$(git status --porcelain)" ] || { echo 'Commit this worktree before exporting a build and its exact source.' >&2; exit 2; }
REV=$(git rev-parse HEAD)
mkdir -p "$OUT"
# Never a raw archive of the private tree: the cleaned, scanned export tree only (fails -> no archive, no smoke run).
scripts/source-archive.sh "$OUT/pocket-ui-$REV.tar.gz" . "$REV" "pocket-ui-$REV" || exit 1
ssh -o BatchMode=yes "$HOST" 'mkdir -p "$HOME/agent-work/android-mvp"'
rsync -az --exclude .git --exclude target --exclude build ./ "$HOST:agent-work/android-mvp/"
ssh -o BatchMode=yes "$HOST" 'cd "$HOME/agent-work/android-mvp" && ANDROID_RELEASE_SOURCES=1 scripts/android/build-apk.sh && scripts/android/smoke-test.sh' > "$OUT/build-smoke.log" 2>&1
# Export evidence, not downloaded third-party ROMs; starter provenance remains in JSON.
rsync -az --include '*/' --include '*.png' --include '*.xml' --include '*.txt' --include '*.log' --include '*.json' --include '*.s16le' --include '*.wav' --exclude '*' "$HOST:agent-work/android-mvp/android/build/evidence/" "$OUT/"
rsync -az "$HOST:agent-work/android-mvp/android/build/cores/corresponding-source/" "$OUT/corresponding-source/"
rsync -az "$HOST:agent-work/android-mvp/android/build/cores/manifest.json" "$OUT/core-manifest.json"
rsync -az "$HOST:agent-work/android-mvp/android/build/pocket-android-mvp.apk" "$HOST:agent-work/android-mvp/android/build/pocket-android-mvp.apk.sha256" "$OUT/"
printf 'Verified build/smoke evidence: %s\n' "$OUT"

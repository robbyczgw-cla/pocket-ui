#!/bin/sh
# Shared full selection for pre-release, nightly and RC. A fresh main run can issue a pre-cut receipt.
# Usage: scripts/full-suite.sh [--receipt FILE] [tree]
set -eu
receipt=
if [ "${1:-}" = --receipt ]; then receipt=$2; shift 2; fi
case $receipt in ''|/*) ;; *) receipt=$PWD/$receipt ;; esac
cd "${1:-$(dirname "$0")/..}"
if [ -n "$receipt" ]; then
    sha=$(git rev-parse HEAD)
    tree=$(git rev-parse 'HEAD^{tree}')
    [ -z "$(git status --porcelain --untracked-files=normal)" ] || { echo 'full-suite: receipt requires a clean checkout' >&2; exit 2; }
    case ${GATE_PROFILE:-gate} in gate|release) ;; *) echo 'full-suite: receipt requires gate or release profile' >&2; exit 2 ;; esac
    # Never leave an old green result at the requested output after a failed fresh run.
    rm -f "$receipt"
fi
start=$(date +%s)
GATE_FULL=1 sh scripts/gate.sh "$PWD"
if [ -n "$receipt" ]; then
    [ "$sha" = "$(git rev-parse HEAD)" ] && [ "$tree" = "$(git rev-parse 'HEAD^{tree}')" ] &&
        [ -z "$(git status --porcelain --untracked-files=normal)" ] || { echo 'full-suite: checkout changed during run; no receipt' >&2; exit 2; }
    python3 - "$receipt" "$sha" "$tree" "${GATE_PROFILE:-gate}" "$(( $(date +%s) - start ))" <<'PY'
import json, os, sys, tempfile, time
path, sha, tree, profile, seconds = sys.argv[1:]
fd, tmp = tempfile.mkstemp(prefix='.full-suite-', dir=os.path.dirname(path))
with os.fdopen(fd, 'w') as f:
    json.dump(dict(kind='full-suite', suite='gate-full-v1', verdict='green', mode='full',
                   sha=sha, tree=tree, profile=profile, seconds=int(seconds), finished=int(time.time())), f, indent=2)
os.replace(tmp, path)
PY
    echo "full-suite: green receipt $receipt ($sha)"
fi

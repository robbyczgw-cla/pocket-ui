#!/usr/bin/env bash
# Local pinned NDK builds only. Source and licence evidence: android/cores.lock.json.
set -euo pipefail
cd "$(dirname "$0")/../.."
export PATH="/opt/homebrew/bin:$PATH"
exec python3 scripts/android/build-cores.py

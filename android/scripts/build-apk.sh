#!/usr/bin/env bash
# Compatibility entry for the spike's instructions.
set -euo pipefail
exec "$(dirname "$0")/../../scripts/android/build-apk.sh" "$@"

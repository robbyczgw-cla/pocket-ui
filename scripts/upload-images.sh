#!/bin/sh
# Upload only a hash-verified, checked image release (docs/SD-IMAGE.md).
#   scripts/upload-images.sh vVERSION IMAGE_DIR --repo OWNER/REPO [--dry-run]
# This deliberately excludes the unrelated installer SHA256SUMS and local receipts/logs.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
exec python3 -I "$ROOT/scripts/image/receipt.py" upload "$@"

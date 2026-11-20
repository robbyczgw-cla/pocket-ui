#!/bin/bash
# Native JVM companion: no Android SDK, account or device needed.
set -euo pipefail
cd "$(dirname "$0")/../../.."
. scripts/android/test/jvm-env.sh
reader_require_jvm "${1:-}"
if [ "${1:-}" != --bios-only ]; then reader_require_proc "${1:-}"; fi
reader_test_dir=$(mktemp -d)
trap 'rm -rf "$reader_test_dir"' EXIT
rustc --edition=2021 -C opt-level=1 scripts/android/test/file-fault-corpus.rs -o "$reader_test_dir/corpus"
"$reader_test_dir/corpus" "$reader_test_dir/faults"
javac --release 8 -d "$reader_test_dir/classes" android/java/xyz/pocketui/app/BiosFiles.java scripts/android/test/BiosFilesTest.java
python3 - "$reader_test_dir/classes" "$reader_test_dir/faults" <<'PYTHON'
import subprocess,sys
subprocess.run(['java','-cp',sys.argv[1],'xyz.pocketui.app.BiosFilesTest',sys.argv[2]],check=True,timeout=15)
PYTHON

# The same policy also exercises the six Android Java file boundaries.
if [ "${1:-}" != --bios-only ]; then bash scripts/android/test/java-reader-rule.sh; fi

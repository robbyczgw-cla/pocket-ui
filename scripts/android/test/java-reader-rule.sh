#!/bin/bash
# Real production file readers; narrow host Android API doubles; no SDK/device/network.
set -euo pipefail
cd "$(dirname "$0")/../../.."
. scripts/android/test/jvm-env.sh
reader_require_jvm "${1:-}"
reader_require_proc "${1:-}"
python3 -I scripts/android/test/test-reader-watchdog.py -q
reader_test_dir=$(mktemp -d)
trap 'rm -rf "$reader_test_dir"' EXIT
rustc --edition=2021 -C opt-level=1 scripts/android/test/file-fault-corpus.rs -o "$reader_test_dir/corpus"
# Gate PATHs need not include ripgrep. A failing process substitution also
# escapes set -e; build the checked source list with the required Python instead.
python3 -I - "$reader_test_dir/java-sources" <<'PYTHON'
from pathlib import Path
import sys
sources = sorted(str(path) for path in Path('scripts/android/test/jvm').rglob('*.java'))
if not sources:
    raise SystemExit('Missing JVM file-reader test sources')
Path(sys.argv[1]).write_text('\n'.join(sources) + '\n')
PYTHON
javac --release 8 -d "$reader_test_dir/classes" "@$reader_test_dir/java-sources" \
  android/java/xyz/pocketui/app/{GameCopy,GameDirect,SaveBackup,SaveCopy,UpdateInstaller,Views,NetworkRule}.java \
  android/java/xyz/pocketui/app/{PadChord,PadMenu,EmulatorKeys,PressGate,TargetKey,LaunchNote,PressSlot}.java \
  scripts/android/test/{GameDirectTest,JavaReaderRule,ViewsTest,ExtrasKeysTest}.java
java -Xmx128m -cp "$reader_test_dir/classes" xyz.pocketui.app.JavaReaderRule --seeds "$reader_test_dir/seeds"
"$reader_test_dir/corpus" "$reader_test_dir/faults" "$reader_test_dir/seeds"
java -Xmx128m -cp "$reader_test_dir/classes" xyz.pocketui.app.GameDirectTest
java -Xmx128m -cp "$reader_test_dir/classes" xyz.pocketui.app.ViewsTest
# Android extras: the pad chord, the pause menu over other emulator apps and their keys (docs/ANDROID-EXTRAS.md).
java -Xmx128m -cp "$reader_test_dir/classes" xyz.pocketui.app.ExtrasKeysTest
python3 -I scripts/android/test/run-java-reader-rule.py "$reader_test_dir/classes" "$reader_test_dir/faults"

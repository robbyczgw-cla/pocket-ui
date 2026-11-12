#!/usr/bin/env bash
# The public Android preview: builds the release APK, its corresponding-source bundle and the licence file from a PUBLIC tree
# (the output of scripts/export-public.sh, copied to the Apple Silicon build host), signed with the release key.
#
#   scripts/android/release-apk.sh --version X.Y.Z --out DIR [--build N]
#
#   --version X.Y.Z   the Android preview's version: versionName X.Y.Z-android.N, versionCode X*1000000+Y*10000+Z*100+N
#   --build N         N (default 1, or the counter in android/AndroidManifest.xml for the same version): raise it for a second build
#   --out DIR         must not exist or be empty; receives the release assets (fixed names, so Obtainium can match them)
#
# Output (DIR): pocket-ui-android.apk, pocket-ui-android.apk.sha256, pocket-ui-android-core-sources.tar (one corresponding-source
# archive per core + manifest.json + patches/), pocket-ui-android-SOURCE-OFFER.txt, pocket-ui-android-licences.txt (the offer, LICENSE, NOTICE.md). Nothing is published. Afterwards, on the same machine, from the
# PRIVATE tree: scripts/android/release-check.sh DIR/pocket-ui-android.apk --sources <tree>/android/build/cores/corresponding-source
# --previous-code <last published versionCode>   (docs/ANDROID-RELEASE.md).
#
# The release key stays where build-apk.sh looks for it ($ANDROID_SIGNING_DIR, default ~/.local/share/pocket-ui/android-signing/
# release.jks + store-password). build-apk.sh would CREATE a missing key, and an APK signed by another key is another app to Android
# (no update): this script stops first if the key is not there. Never copy or print the key.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd -P)
cd "$ROOT"
VERSION="" OUT="" BUILD=""
while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION=${2:-}; shift ;;
        --out) OUT=${2:-}; shift ;;
        --build) BUILD=${2:-}; shift ;;
        *) echo "usage: $0 --version X.Y.Z --out DIR [--build N]" >&2; exit 2 ;;
    esac
    shift
done
case "$VERSION" in [0-9]*.[0-9]*.[0-9]*) ;; *) echo "release-apk: --version X.Y.Z is required" >&2; exit 2 ;; esac
[ -n "$OUT" ] || { echo "release-apk: --out DIR is required" >&2; exit 2; }
if [ -e "$OUT" ] && [ -n "$(ls -A "$OUT" 2>/dev/null)" ]; then echo "release-apk: $OUT is not empty" >&2; exit 2; fi
PRIVATE=${ANDROID_SIGNING_DIR:-$HOME/.local/share/pocket-ui/android-signing}
[ -f "$PRIVATE/release.jks" ] && [ -f "$PRIVATE/store-password" ] || { echo "release-apk: the release key is not in $PRIVATE (a new one would be another app); stopping" >&2; exit 1; }
[ -f scripts/check-leaks.sh ] && echo "release-apk: note: this is the private tree; a release is built from the exported public tree" >&2
mkdir -p "$OUT"
unset ANDROID_QA_VERSION
export ANDROID_VERSION="$VERSION" ANDROID_RELEASE_SOURCES=1
[ -n "$BUILD" ] && export ANDROID_BUILD="$BUILD"
scripts/android/build-apk.sh
B=android/build
APK=$B/pocket-android-mvp.apk
[ -f "$APK" ] || { echo "release-apk: the build produced no APK" >&2; exit 1; }
cp "$APK" "$OUT/pocket-ui-android.apk"
# Local output only; the catalog must be signed separately with the existing store key before use.
cp -R "$B/core-downloads" "$OUT/core-downloads"
( cd "$OUT" && python3 - <<'PY'
import hashlib
from pathlib import Path
Path("pocket-ui-android.apk.sha256").write_text(hashlib.sha256(Path("pocket-ui-android.apk").read_bytes()).hexdigest() + "  pocket-ui-android.apk\n")
PY
)
# The corresponding sources of the cores, exactly the archives whose hashes the APK's cores.json records.
# The written source offer: every core with licence, upstream commit, source archive hash and patches (also inside the APK).
VERSION_NAME=$(sed -n 's/.*android:versionName="\([^"]*\)".*/\1/p' android/AndroidManifest.xml | head -1)
python3 -I scripts/android/core_licences.py check --cache "${ANDROID_CORE_CACHE:-$B/cores}" --downloads "$B/core-downloads" --sources "${ANDROID_CORE_CACHE:-$B/cores}/corresponding-source"
python3 -I scripts/android/core_licences.py offer --cache "${ANDROID_CORE_CACHE:-$B/cores}" --downloads "$B/core-downloads" --version "$VERSION_NAME" ${ANDROID_RELEASE_URL:+--release-url "$ANDROID_RELEASE_URL"} --out "$OUT/pocket-ui-android-SOURCE-OFFER.txt"
python3 - "${ANDROID_CORE_CACHE:-$B/cores}" "$OUT" <<'PY'
import json, sys, tarfile
from pathlib import Path
cores, out = Path(sys.argv[1]), Path(sys.argv[2])
manifest = json.loads((cores / "manifest.json").read_text())
with tarfile.open(out / "pocket-ui-android-core-sources.tar", "w") as tar:
    tar.add(cores / "manifest.json", arcname="manifest.json")
    for patch in sorted(Path("android/patches").glob("*.patch")):
        tar.add(patch, arcname="patches/" + patch.name)
    for core in manifest["cores"]:
        tar.add(cores / "corresponding-source" / core["corresponding_source"], arcname="corresponding-source/" + core["corresponding_source"])
lines = [(out / "pocket-ui-android-SOURCE-OFFER.txt").read_text(), "-" * 78, ""]
for name in ["LICENSE", "NOTICE.md", "android/CORE-LICENCES.md"]:
    lines += ["== %s ==" % name, Path(name).read_text(), ""]
(out / "pocket-ui-android-licences.txt").write_text("\n".join(lines))
PY
echo
echo "release-apk: assets in $OUT:"; ls -l "$OUT"
echo "release-apk: next, from the private tree: scripts/android/release-check.sh $OUT/pocket-ui-android.apk --sources $ROOT/$B/cores/corresponding-source --previous-code <last published versionCode>"

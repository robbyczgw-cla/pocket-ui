#!/usr/bin/env bash
# Native ARM64 release APK; no Gradle, hosted CI or device required. See docs/ANDROID-MVP.md.
set -euo pipefail
cd "$(dirname "$0")/../.."
# Dedicated release identity, kept outside every checkout. Password never appears in argv or logs.
PRIVATE="${ANDROID_SIGNING_DIR:-$HOME/.local/share/pocket-ui/android-signing}"
KEY="$PRIVATE/release.jks"
PASSWORD="$PRIVATE/store-password"
# A missing key is never replaced quietly: an APK signed by another key is another app to Android (no update, saves stay
# with the old one). QA and test packages bring their own key in ANDROID_SIGNING_DIR (release.jks + store-password); a
# throwaway key for a one-off test build is made on purpose with ANDROID_ALLOW_NEW_KEY=1. First thing, so nothing is built or
# rewritten before it. ANDROID_BUILD_KEY_CHECK_ONLY=1 stops right after this check (used by the test; builds nothing).
if [ ! -f "$KEY" ] && [ "${ANDROID_ALLOW_NEW_KEY:-}" != 1 ]; then
  echo "build-apk: there is no signing key at $KEY." >&2
  echo "  A new key would make this APK another app to Android (no update over the one players have). Stopping." >&2
  echo "  - Release build: put the release key back there (or point ANDROID_SIGNING_DIR at it). Never create a new one." >&2
  echo "  - QA or test build with its own key: ANDROID_SIGNING_DIR=<folder with release.jks and store-password>." >&2
  echo "  - A throwaway test key on purpose: ANDROID_ALLOW_NEW_KEY=1 (never for a build players install)." >&2
  exit 1
fi
[ "${ANDROID_BUILD_KEY_CHECK_ONLY:-}" != 1 ] || { echo "build-apk: signing key check passed"; exit 0; }
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.2.12479018}"
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17}"
export PATH="$HOME/.cargo/bin:$JAVA_HOME/bin:$PATH"
BT="$ANDROID_HOME/build-tools/${ANDROID_BUILD_TOOLS:-35.0.0}"
JAR="$ANDROID_HOME/platforms/android-35/android.jar"
OUT="$PWD/android/build"
# The APK identity follows the shared release: a new Cargo version restarts the build counter at 1 and rewrites the manifest
# (a bump can't break the build); the same version keeps its counter. versionCode is derived from both.
python3 - <<'PYVERSION'
import os,re
from pathlib import Path
version=re.search(r'^version = "([^" ]+)"',Path('Cargo.toml').read_text(),re.M).group(1)
version=os.environ.get('ANDROID_VERSION') or os.environ.get('ANDROID_QA_VERSION') or version   # a public preview: ANDROID_VERSION=X.Y.Z (scripts/android/release-apk.sh)
assert re.fullmatch(r'\d+\.\d+\.\d+',version),'Invalid Android QA version'
path=Path('android/AndroidManifest.xml');text=path.read_text()
name=re.search(r'android:versionName="([^"]*)"',text).group(1)
build=int(os.environ.get('ANDROID_BUILD') or (name.rsplit('.',1)[1] if name.startswith(version+'-android.') else 1))   # ANDROID_BUILD=N: a second build of the same version
major,minor,patch=map(int,version.split('.'))
assert 0<build<100,'APK build counter out of range'
code=major*1_000_000+minor*10_000+patch*100+build
new=re.sub(r'android:versionCode="\d+"','android:versionCode="%d"'%code,text,1)
new=re.sub(r'android:versionName="[^"]*"','android:versionName="%s-android.%d"'%(version,build),new,1)
if new!=text:
    path.write_text(new);print('android manifest set to %s-android.%d (%d)'%(version,build,code))
PYVERSION
for tool in cargo javac keytool zip; do command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }; done
for tool in d8 aapt zipalign apksigner; do test -x "$BT/$tool" || { echo "Missing $BT/$tool" >&2; exit 1; }; done
test -f "$JAR" && test -d "$ANDROID_NDK_HOME" || { echo 'Install Android SDK 35 and NDK 27.2.12479018' >&2; exit 1; }
# Only generated directories below this fixed output root are reset.
rm -rf "$OUT/classes" "$OUT/apk" "$OUT/dex"
mkdir -p "$OUT/classes" "$OUT/apk/lib/arm64-v8a" "$OUT/dex"
# An explicit prebuilt cache is read-only and checked against the complete lock by package-cores.py.
if [ -z "${ANDROID_CORE_CACHE:-}" ]; then
  ANDROID_RELEASE_SOURCES=1 scripts/android/build-cores.sh
  ANDROID_CORE_CACHE="$OUT/cores"
fi
export ANDROID_CORE_CACHE
python3 scripts/android/package-cores.py "$OUT"
# The download channel for the extra systems: android/core-channel.url (or ANDROID_CORE_CHANNEL_URL for one build). Empty = "Coming soon".
if [ -n "${ANDROID_CORE_CHANNEL_URL:-}" ]; then printf '%s\n' "$ANDROID_CORE_CHANNEL_URL" > "$OUT/apk/assets/core-channel.url"; else cp android/core-channel.url "$OUT/apk/assets/core-channel.url"; fi
python3 -I scripts/android/core_licences.py channel "$OUT/apk/assets/core-channel.url" || { echo 'build-apk: the core-channel.url is not usable' >&2; exit 1; }
mkdir -p "$OUT/apk/assets/licences"
cp "$ANDROID_CORE_CACHE"/licences/* "$OUT/apk/assets/licences/"
cp android/licences/*.txt android/licences/sources.json "$OUT/apk/assets/licences/"
cp LICENSE NOTICE.md "$OUT/apk/assets/licences/"
cp android/CORE-LICENCES.md "$OUT/apk/assets/licences/ANDROID-CORES.md"
cp android/licences/README.md "$OUT/apk/assets/licences/ANDROID-RUST-NOTICES.md"
cp assets/fonts/OFL.txt "$OUT/apk/assets/licences/FONTS-OFL.txt"
# Every core, bundled or downloadable, has its licence texts, pinned source revision, source archive and patches, or the build stops
# (as the Linux images do for their packages). The APK carries the written offer, the list the Licences page shows and the patches.
python3 -I scripts/android/core_licences.py check --cache "$ANDROID_CORE_CACHE" --downloads "$OUT/core-downloads" || { echo 'build-apk: a core lacks its licence, source or patches' >&2; exit 1; }
VERSION_NAME=$(sed -n 's/.*android:versionName="\([^"]*\)".*/\1/p' android/AndroidManifest.xml | head -1)
python3 -I scripts/android/core_licences.py offer --cache "$ANDROID_CORE_CACHE" --downloads "$OUT/core-downloads" --version "$VERSION_NAME" ${ANDROID_RELEASE_URL:+--release-url "$ANDROID_RELEASE_URL"} --out "$OUT/apk/assets/licences/SOURCE-OFFER.txt"
python3 -I scripts/android/core_licences.py list --cache "$ANDROID_CORE_CACHE" --downloads "$OUT/core-downloads" --out "$OUT/apk/assets/licences/CORES.tsv"
python3 -I scripts/android/core_licences.py patches --cache "$ANDROID_CORE_CACHE" --out "$OUT/apk/assets/licences/patches"
# Audit the Apache-licensed launcher separately from the private GPL-compatible player process.
scripts/third-party.sh --target aarch64-linux-android --manifest-path android/Cargo.toml --texts "$OUT/apk/assets/licences/RUST-LICENSES.txt" --out "$OUT/apk/assets/licences/RUST-CRATES.md" > "$OUT/licence-report.log" 2>&1 || true
test -s "$OUT/apk/assets/licences/RUST-LICENSES.txt" && test -s "$OUT/apk/assets/licences/RUST-CRATES.md"
# The player is a separate library/process without winit: check its linked dependency graph independently.
scripts/third-party.sh --target aarch64-linux-android --manifest-path android/player/Cargo.toml --texts "$OUT/apk/assets/licences/PLAYER-RUST-LICENSES.txt" --out "$OUT/apk/assets/licences/PLAYER-RUST-CRATES.md" > "$OUT/player-licence-report.log" 2>&1

# Build paths out of the native libraries (the home folder of whoever builds): scripts/release-rustflags.sh, checked below.
REMAP_ROOT=$(pwd -P) . scripts/release-rustflags.sh
# 16 KB memory pages (Android 15+ devices): align our own libraries' segments like the cores' (build-cores.py). Checked by release-check.sh.
export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-z,max-page-size=16384"
cargo ndk -t arm64-v8a --platform 26 -o "$OUT/apk/lib" build --locked --release --manifest-path android/Cargo.toml
cargo ndk -t arm64-v8a --platform 26 -o "$OUT/apk/lib" build --locked --release --manifest-path android/player/Cargo.toml
# Every packaged native library: our own and the cores.
sh scripts/check-binary-paths.sh "$OUT"/apk/lib/arm64-v8a/*.so || { echo 'build-apk: the native libraries carry build paths' >&2; exit 1; }
javac --release 8 -classpath "$JAR" -d "$OUT/classes" android/java/xyz/pocketui/app/*.java
"$BT/d8" --min-api 26 --lib "$JAR" --output "$OUT/dex" "$OUT/classes/xyz/pocketui/app/"*.class
"$BT/aapt" package -f -M android/AndroidManifest.xml -S android/res -I "$JAR" -F "$OUT/unsigned.apk"
cp "$OUT/dex/classes.dex" "$OUT/apk/"
(cd "$OUT/apk" && zip -qr "$OUT/unsigned.apk" .)
"$BT/zipalign" -f -P 16 4 "$OUT/unsigned.apk" "$OUT/aligned.apk"   # stored .so files on 16 KB; ours are deflated (extractNativeLibs=true), so ELF alignment is what counts
if [ ! -f "$KEY" ]; then
  (umask 077
   mkdir -p "$PRIVATE"
   test -f "$PASSWORD" || python3 -c 'import secrets; print(secrets.token_urlsafe(48))' > "$PASSWORD"
   chmod 700 "$PRIVATE"
   chmod 600 "$PASSWORD"
   keytool -genkeypair -keystore "$KEY" -storepass:file "$PASSWORD" -keypass:file "$PASSWORD" -alias pocket-ui -dname 'CN=Pocket UI Android' -keyalg RSA -keysize 3072 -validity 10000 >/dev/null 2>&1
   chmod 600 "$KEY")
fi
test -f "$PASSWORD" || { echo 'Missing signing password file' >&2; exit 1; }
"$BT/apksigner" sign --ks "$KEY" --ks-pass "file:$PASSWORD" --out "$OUT/pocket-android-mvp.apk" "$OUT/aligned.apk"
"$BT/apksigner" verify --verbose "$OUT/pocket-android-mvp.apk"
python3 - "$OUT/pocket-android-mvp.apk" <<'PY'
import hashlib,sys
from pathlib import Path
p=Path(sys.argv[1]); assert p.stat().st_size<30_000_000,'APK exceeds the 30 MB budget'; digest=hashlib.sha256(p.read_bytes()).hexdigest()
p.with_suffix('.apk.sha256').write_text(digest+'  '+p.name+'\n')
print(str(p)+'  SHA256 '+digest)
PY

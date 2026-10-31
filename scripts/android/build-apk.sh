#!/usr/bin/env bash
# Native ARM64 release APK; no Gradle, hosted CI or device required. See docs/ANDROID-MVP.md.
set -euo pipefail
cd "$(dirname "$0")/../.."
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
import re
from pathlib import Path
version=re.search(r'^version = "([^" ]+)"',Path('Cargo.toml').read_text(),re.M).group(1)
path=Path('android/AndroidManifest.xml');text=path.read_text()
name=re.search(r'android:versionName="([^"]*)"',text).group(1)
build=int(name.rsplit('.',1)[1]) if name.startswith(version+'-android.') else 1
major,minor,patch=map(int,version.split('.'))
assert 0<build<100,'APK build counter out of range'
code=major*1_000_000+minor*10_000+patch*100+build
new=re.sub(r'android:versionCode="\d+"','android:versionCode="%d"'%code,text,1)
new=re.sub(r'android:versionName="[^"]*"','android:versionName="%s-android.%d"'%(version,build),new,1)
if new!=text:
    path.write_text(new);print('android manifest set to %s-android.%d (%d)'%(version,build,code))
PYVERSION
# Dedicated release identity, kept outside every checkout. Password never appears in argv or logs.
PRIVATE="${ANDROID_SIGNING_DIR:-$HOME/.local/share/pocket-ui/android-signing}"
KEY="$PRIVATE/release.jks"
PASSWORD="$PRIVATE/store-password"
for tool in cargo javac keytool zip; do command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }; done
for tool in d8 aapt zipalign apksigner; do test -x "$BT/$tool" || { echo "Missing $BT/$tool" >&2; exit 1; }; done
test -f "$JAR" && test -d "$ANDROID_NDK_HOME" || { echo 'Install Android SDK 35 and NDK 27.2.12479018' >&2; exit 1; }
# Only generated directories below this fixed output root are reset.
rm -rf "$OUT/classes" "$OUT/apk" "$OUT/dex"
mkdir -p "$OUT/classes" "$OUT/apk/lib/arm64-v8a" "$OUT/dex"
scripts/android/build-cores.sh
# Copy only the manifest's current cores: a removed experimental .so must never leak into an APK.
python3 - "$OUT" <<'PYCORE'
import hashlib,json,shutil,sys
from pathlib import Path
out=Path(sys.argv[1]);cores=out/'cores'
for core in json.loads((cores/'manifest.json').read_text())['cores']:
    source=cores/'lib'/('lib'+core['id']+'_libretro.so')
    assert hashlib.sha256(source.read_bytes()).hexdigest()==core['sha256']
    shutil.copyfile(source,out/'apk/lib/arm64-v8a'/source.name)
PYCORE
mkdir -p "$OUT/apk/assets/licences"
cp android/build/cores/licences/* "$OUT/apk/assets/licences/"
cp android/licences/*.txt android/licences/sources.json "$OUT/apk/assets/licences/"
cp android/build/cores/manifest.json "$OUT/apk/assets/cores.json"
cp LICENSE NOTICE.md "$OUT/apk/assets/licences/"
cp android/CORE-LICENCES.md "$OUT/apk/assets/licences/ANDROID-CORES.md"
cp android/licences/README.md "$OUT/apk/assets/licences/ANDROID-RUST-NOTICES.md"
cp assets/fonts/OFL.txt "$OUT/apk/assets/licences/FONTS-OFL.txt"
# Audit the Apache-licensed launcher separately from the private GPL-compatible player process.
scripts/third-party.sh --target aarch64-linux-android --manifest-path android/Cargo.toml --texts "$OUT/apk/assets/licences/RUST-LICENSES.txt" --out "$OUT/apk/assets/licences/RUST-CRATES.md" > "$OUT/licence-report.log" 2>&1 || true
test -s "$OUT/apk/assets/licences/RUST-LICENSES.txt" && test -s "$OUT/apk/assets/licences/RUST-CRATES.md"
# The player is a separate library/process without winit: check its linked dependency graph independently.
scripts/third-party.sh --target aarch64-linux-android --manifest-path android/player/Cargo.toml --texts "$OUT/apk/assets/licences/PLAYER-RUST-LICENSES.txt" --out "$OUT/apk/assets/licences/PLAYER-RUST-CRATES.md" > "$OUT/player-licence-report.log" 2>&1

# Build paths out of the native libraries (the home folder of whoever builds): scripts/release-rustflags.sh, checked below.
REMAP_ROOT=$(pwd -P) . scripts/release-rustflags.sh
cargo ndk -t arm64-v8a --platform 26 -o "$OUT/apk/lib" build --locked --release --manifest-path android/Cargo.toml
cargo ndk -t arm64-v8a --platform 26 -o "$OUT/apk/lib" build --locked --release --manifest-path android/player/Cargo.toml
# Every packaged native library: our own and the cores.
sh scripts/check-binary-paths.sh "$OUT"/apk/lib/arm64-v8a/*.so || { echo 'build-apk: the native libraries carry build paths' >&2; exit 1; }
javac --release 8 -classpath "$JAR" -d "$OUT/classes" android/java/dev/pocket/spike/*.java
"$BT/d8" --min-api 26 --lib "$JAR" --output "$OUT/dex" "$OUT/classes/dev/pocket/spike/"*.class
"$BT/aapt" package -f -M android/AndroidManifest.xml -S android/res -I "$JAR" -F "$OUT/unsigned.apk"
cp "$OUT/dex/classes.dex" "$OUT/apk/"
(cd "$OUT/apk" && zip -qr "$OUT/unsigned.apk" .)
"$BT/zipalign" -f -p 4 "$OUT/unsigned.apk" "$OUT/aligned.apk"
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
p=Path(sys.argv[1]); assert p.stat().st_size<100_000_000,'APK exceeds the 100 MB budget'; digest=hashlib.sha256(p.read_bytes()).hexdigest()
p.with_suffix('.apk.sha256').write_text(digest+'  '+p.name+'\n')
print(str(p)+'  SHA256 '+digest)
PY

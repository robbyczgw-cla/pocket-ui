#!/bin/bash
# The Brick card's network parts (docs/TRIMUI-BRICK-REMOTE.md, docs/TRIMUI-BRICK-SYNC.md): Tailscale, curl, dropbear and Syncthing, each pinned to one version and checksum.
#   scripts/brick-remote-parts.sh OUT [WORK]           everything, for aarch64 (the Brick)
#   scripts/brick-remote-parts.sh --dropbear TARGET OUT  only dropbear, for a zig target (x86_64-linux-musl for the host tests)
# Result in OUT: bin/{tailscale,tailscaled,curl,dropbearmulti,syncthing}, licences/{tailscale,curl,dropbear,syncthing}/, sources/ (dropbear's source,
# our patch and options; SOURCES.txt says where the others' source is), PARTS.txt (name, version, sha256 of each program).
# Tailscale, Syncthing and curl are the upstream builds as they are (official static arm64 Tailscale, BSD-3-Clause; Syncthing, MPL-2.0; stunnel's static curl, curl
# licence and the libraries listed in licences/curl). dropbear (MIT) is built here with zig, static against musl, with
# deploy/brick/dropbear/pocket-ui.patch: the password is the one the device made (DROPBEAR_PASSWORD_FILE), not the firmware's.
# Needs curl, tar with xz and bzip2, sha256sum; for dropbear zig (0.13+), make, patch. Any download that differs from its pinned
# checksum stops the script.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(dirname "$HERE")

TAILSCALE_VERSION=1.104.5
TAILSCALE_SHA=e900b0e6e25023a1e504b2c237ce2a32ea0535e006f8c1a1097a490886493b6c          # tailscale_1.104.5_arm64.tgz (= pkgs.tailscale.com's .sha256)
TAILSCALE_LICENSE_SHA=a7ca6186a7963a0a60740f6047760eecd7a0234e8c38bd7e1e0bbcb324bda45b  # LICENSE at tag v1.104.5
TAILSCALE_DEPS_SHA=fe5729149b37198fad6c6227f2923760f3fa1540ad99a7b8d1981cebb80d65d1     # licenses/tailscale.md at tag v1.104.5
CURL_RELEASE=8.23.0-1
CURL_SHA=e20f26e5b87c1883608f286f86bdabf39d3b8556e8a8e75dfe205e14bd206199               # curl-linux-aarch64-musl-8.23.0-1.tar.xz
CURL_BIN_SHA=3a7c6818343f1b5184047bb484cb3cff7d5ede35a2713a6ef61030b422d8d6fc           # the curl inside (the release notes' table)
CURL_DEV_SHA=ed2e3162aef9c4a7116ba63db34f8bbb677c12d4f66c9b52230f851c34ff6609           # curl-linux-aarch64-dev-8.23.0-1.tar.xz (its licences/)
# curl's two LGPL libraries, statically inside it: their source goes on the card (LGPL-3.0 section 4), with the build scripts of that release.
LIBIDN2_VERSION=2.3.8
LIBIDN2_SRC_SHA=f557911bf6171621e1f72ff35f5b1825bb35b52ed45325dcdee931e5d3c0787a        # libidn2-2.3.8.tar.gz (the dev package's idn2.h says 2.3.8)
LIBUNISTRING_VERSION=1.5
LIBUNISTRING_SRC_SHA=fe89007a8a414ae4790b816b86859e877d3ab135db3c37f06541ff7e85ac344b   # libunistring-1.5.tar.xz (the dev package: _LIBUNISTRING_VERSION 0x010500)
CURL_SCRIPTS_SHA=811f4bcee86e5ca31f42fc5e2057810cf229ceea16b8c06889adb519d311fe2a       # github.com/stunnel/static-curl tag 8.23.0-1, source archive
SYNCTHING_VERSION=2.1.7
SYNCTHING_SHA=48e456690cd2b97e7d26b02b5ac35f62a3b36172267b144cfeaf02c993ab955c          # syncthing-linux-arm64-v2.1.7.tar.gz (= the release's sha256sum.txt.asc)
SYNCTHING_BIN_SHA=3381c439af70c31db557d02581d0665d279f9efac1d0ea6806835a5b80dc65c7      # the static arm64 syncthing inside
SYNCTHING_LICENSE_SHA=3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04  # LICENSE.txt (MPL-2.0) inside
DROPBEAR_VERSION=2026.94
DROPBEAR_SHA=e098034a843699200c8c977a991fff73159735bf795d5f72ef672c41a6b1ae81           # dropbear-2026.94.tar.bz2 (= the release's SHA256SUM)

die() { echo "brick-remote-parts: $*" >&2; exit 1; }
fetch() { # url sha file
  if [ ! -f "$3" ] || [ "$(sha256sum "$3" | cut -d' ' -f1)" != "$2" ]; then
    rm -f "$3"; curl -fsSL --retry 3 -o "$3.part" "$1" || die "download failed: $1"; mv "$3.part" "$3"
  fi
  [ "$(sha256sum "$3" | cut -d' ' -f1)" = "$2" ] || die "$(basename "$3") does not match its pinned sha256 (got $(sha256sum "$3" | cut -d' ' -f1))"
}

build_dropbear() { # zig-target out work
  local target=$1 out=$2 work=$3
  command -v zig >/dev/null || die "zig is not on PATH"
  fetch "https://matt.ucc.asn.au/dropbear/releases/dropbear-$DROPBEAR_VERSION.tar.bz2" "$DROPBEAR_SHA" "$work/dropbear-$DROPBEAR_VERSION.tar.bz2"
  local src=$work/dropbear-$target
  rm -rf "$src"; mkdir -p "$src" "$out/bin"
  tar -xjf "$work/dropbear-$DROPBEAR_VERSION.tar.bz2" -C "$src" --strip-components=1
  patch -d "$src" -p1 -s <"$ROOT/deploy/brick/dropbear/pocket-ui.patch"
  cp "$ROOT/deploy/brick/dropbear/localoptions.h" "$src/localoptions.h"
  ( cd "$src" && ./configure --host="${target%%-*}-linux-musl" CC="zig cc -target $target" AR="zig ar" RANLIB="zig ranlib" LDFLAGS=-s \
      --enable-static --disable-zlib --disable-lastlog --disable-utmp --disable-utmpx --disable-wtmp --disable-wtmpx \
      --disable-pututline --disable-pututxline --disable-harden >"$work/dropbear-$target-configure.log" 2>&1 ) || die "dropbear configure failed: $work/dropbear-$target-configure.log"
  make -C "$src" -j"${BRICK_JOBS:-4}" PROGRAMS="dropbear dropbearkey scp" MULTI=1 STATIC=1 >"$work/dropbear-$target-make.log" 2>&1 || die "dropbear build failed: $work/dropbear-$target-make.log"
  cp "$src/dropbearmulti" "$out/bin/dropbearmulti"
  mkdir -p "$out/licences/dropbear"; cp "$src/LICENSE" "$out/licences/dropbear/LICENSE"
}

if [ "${1:-}" = --dropbear ]; then
  TARGET=${2:?zig target}; OUT=${3:?out dir}; mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
  WORK=$(mktemp -d "${TMPDIR:-/tmp}/brick-dropbear.XXXXXX"); trap 'rm -rf "$WORK"' EXIT
  build_dropbear "$TARGET" "$OUT" "$WORK"
  echo "brick-remote-parts: $OUT/bin/dropbearmulti ($TARGET)"
  exit 0
fi

OUT=${1:?out dir}; mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
WORK=${2:-$OUT/.work}; mkdir -p "$WORK"; WORK=$(cd "$WORK" && pwd)
rm -rf "$OUT/bin" "$OUT/licences" "$OUT/sources"; mkdir -p "$OUT/bin" "$OUT/licences/tailscale" "$OUT/licences/curl" "$OUT/licences/syncthing" "$OUT/sources"

# Tailscale: the official static build, both programs as they come.
fetch "https://pkgs.tailscale.com/stable/tailscale_${TAILSCALE_VERSION}_arm64.tgz" "$TAILSCALE_SHA" "$WORK/tailscale_${TAILSCALE_VERSION}_arm64.tgz"
tar -xzf "$WORK/tailscale_${TAILSCALE_VERSION}_arm64.tgz" -C "$WORK"
cp "$WORK/tailscale_${TAILSCALE_VERSION}_arm64/tailscale" "$WORK/tailscale_${TAILSCALE_VERSION}_arm64/tailscaled" "$OUT/bin/"
fetch "https://raw.githubusercontent.com/tailscale/tailscale/v$TAILSCALE_VERSION/LICENSE" "$TAILSCALE_LICENSE_SHA" "$OUT/licences/tailscale/LICENSE"
fetch "https://raw.githubusercontent.com/tailscale/tailscale/v$TAILSCALE_VERSION/licenses/tailscale.md" "$TAILSCALE_DEPS_SHA" "$OUT/licences/tailscale/DEPENDENCIES.md"

# Syncthing: the official static build as it comes (MPL-2.0; its source is the tag on GitHub, nothing of it is changed).
fetch "https://github.com/syncthing/syncthing/releases/download/v$SYNCTHING_VERSION/syncthing-linux-arm64-v$SYNCTHING_VERSION.tar.gz" "$SYNCTHING_SHA" "$WORK/syncthing-$SYNCTHING_VERSION.tar.gz"
tar -xzf "$WORK/syncthing-$SYNCTHING_VERSION.tar.gz" -C "$WORK"
[ "$(sha256sum "$WORK/syncthing-linux-arm64-v$SYNCTHING_VERSION/syncthing" | cut -d' ' -f1)" = "$SYNCTHING_BIN_SHA" ] || die "the syncthing inside the archive is not the pinned one"
[ "$(sha256sum "$WORK/syncthing-linux-arm64-v$SYNCTHING_VERSION/LICENSE.txt" | cut -d' ' -f1)" = "$SYNCTHING_LICENSE_SHA" ] || die "Syncthing's LICENSE.txt is not the pinned one"
cp "$WORK/syncthing-linux-arm64-v$SYNCTHING_VERSION/syncthing" "$OUT/bin/"
cp "$WORK/syncthing-linux-arm64-v$SYNCTHING_VERSION/LICENSE.txt" "$WORK/syncthing-linux-arm64-v$SYNCTHING_VERSION/AUTHORS.txt" "$OUT/licences/syncthing/"

# curl: one static program with its own CA bundle (the Brick's firmware has neither a curl we know of nor CA certificates we can rely on).
fetch "https://github.com/stunnel/static-curl/releases/download/$CURL_RELEASE/curl-linux-aarch64-musl-$CURL_RELEASE.tar.xz" "$CURL_SHA" "$WORK/curl-$CURL_RELEASE.tar.xz"
fetch "https://github.com/stunnel/static-curl/releases/download/$CURL_RELEASE/curl-linux-aarch64-dev-$CURL_RELEASE.tar.xz" "$CURL_DEV_SHA" "$WORK/curl-dev-$CURL_RELEASE.tar.xz"
rm -rf "$WORK/curl"; mkdir -p "$WORK/curl"; tar -xJf "$WORK/curl-$CURL_RELEASE.tar.xz" -C "$WORK/curl" curl
[ "$(sha256sum "$WORK/curl/curl" | cut -d' ' -f1)" = "$CURL_BIN_SHA" ] || die "the curl inside the archive is not the one the release lists"
cp "$WORK/curl/curl" "$OUT/bin/curl"
tar -xJf "$WORK/curl-dev-$CURL_RELEASE.tar.xz" -C "$WORK/curl" curl-aarch64/licenses
cp "$WORK"/curl/curl-aarch64/licenses/* "$OUT/licences/curl/"
fetch "https://ftp.gnu.org/gnu/libidn/libidn2-$LIBIDN2_VERSION.tar.gz" "$LIBIDN2_SRC_SHA" "$OUT/sources/libidn2-$LIBIDN2_VERSION.tar.gz"
fetch "https://ftp.gnu.org/gnu/libunistring/libunistring-$LIBUNISTRING_VERSION.tar.xz" "$LIBUNISTRING_SRC_SHA" "$OUT/sources/libunistring-$LIBUNISTRING_VERSION.tar.xz"
fetch "https://github.com/stunnel/static-curl/archive/refs/tags/$CURL_RELEASE.tar.gz" "$CURL_SCRIPTS_SHA" "$OUT/sources/static-curl-$CURL_RELEASE.tar.gz"

# dropbear: built here, with our patch.
PATH=${ZIG_DIR:+$ZIG_DIR:}$PATH build_dropbear aarch64-linux-musl "$OUT" "$WORK"
cp "$WORK/dropbear-$DROPBEAR_VERSION.tar.bz2" "$ROOT/deploy/brick/dropbear/pocket-ui.patch" "$OUT/sources/"
cp "$ROOT/deploy/brick/dropbear/localoptions.h" "$OUT/sources/dropbear-localoptions.h"

cat >"$OUT/sources/SOURCES.txt" <<TXT
Where the source of each network part on this card is.

dropbear $DROPBEAR_VERSION (MIT, licences/dropbear/LICENSE): dropbear-$DROPBEAR_VERSION.tar.bz2 here, as released at
  https://matt.ucc.asn.au/dropbear/releases/ (sha256 $DROPBEAR_SHA), with pocket-ui.patch and dropbear-localoptions.h applied,
  built static against musl with zig (scripts/brick-remote-parts.sh in pocket-ui's source).
Tailscale $TAILSCALE_VERSION (BSD-3-Clause, licences/tailscale/LICENSE; its Go dependencies and their licences: DEPENDENCIES.md):
  the official static arm64 build from https://pkgs.tailscale.com/stable/ (sha256 $TAILSCALE_SHA), unchanged.
  Source: https://github.com/tailscale/tailscale/tree/v$TAILSCALE_VERSION
Syncthing $SYNCTHING_VERSION (MPL-2.0, licences/syncthing/LICENSE.txt; authors: AUTHORS.txt): the official static arm64 build from
  https://github.com/syncthing/syncthing/releases/tag/v$SYNCTHING_VERSION (sha256 $SYNCTHING_SHA), unchanged. MPL-2.0 asks that the source of the
  files under it stay available: https://github.com/syncthing/syncthing/tree/v$SYNCTHING_VERSION. Its Go dependencies keep their own
  licences (listed in the project's repository, go.mod).
curl 8.23.0 ($CURL_RELEASE, static, licences/curl/: curl, OpenSSL, zlib-ng, brotli, zstd, c-ares, libidn2, libunistring, libpsl, libssh2,
  nghttp2, ngtcp2, nghttp3, the CA bundle): https://github.com/stunnel/static-curl/releases/tag/$CURL_RELEASE (sha256 $CURL_SHA), unchanged.
  Built from the upstream releases of each library by the scripts at https://github.com/stunnel/static-curl (tag $CURL_RELEASE).
  libidn2 $LIBIDN2_VERSION and libunistring $LIBUNISTRING_VERSION are LGPL-3.0-or-later / GPL-2.0-or-later and linked into curl statically:
  their source is here (libidn2-$LIBIDN2_VERSION.tar.gz, libunistring-$LIBUNISTRING_VERSION.tar.xz, as released at https://ftp.gnu.org/gnu/),
  with the scripts that built this curl (static-curl-$CURL_RELEASE.tar.gz), so it can be rebuilt with changed versions of them.
  The other libraries' source: their upstream releases named in those scripts.
TXT
{
  for f in tailscale tailscaled curl dropbearmulti syncthing; do
    case $f in tailscale*) v=$TAILSCALE_VERSION ;; syncthing) v=$SYNCTHING_VERSION ;; curl) v=$CURL_RELEASE ;; *) v=$DROPBEAR_VERSION+pocket-ui ;; esac
    echo "$f $v $(sha256sum "$OUT/bin/$f" | cut -d' ' -f1)"
  done
} >"$OUT/PARTS.txt"
chmod 755 "$OUT"/bin/*
echo "brick-remote-parts: $OUT ($(du -sh "$OUT/bin" | cut -f1) of programs)"
cat "$OUT/PARTS.txt"

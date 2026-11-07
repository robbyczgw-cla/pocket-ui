#!/bin/sh
# Builds the signed update channel for pocketui.xyz from a folder of release artefacts (docs/UPDATE-CHANNEL.md):
#   scripts/release-channel.sh --check OUT/updates/<channel>/update.json      (after the GitHub upload, see below)
#   scripts/release-channel.sh --repo OWNER/NAME [--version V] [--in DIR] [--out DIR] [--channel NAME]
#                              [--key FILE] [--notes TEXT | --notes-file FILE] [--previous FILE]
#                              [--os-tar FILE --os-version V] [--base-url URL] [--github-primary] [--allow-foreign-key]
#     --repo        the public GitHub repository whose release v<version> will hold the files
#     --version     plain X.Y.Z; brick alone also accepts X.Y.Z.W hotfixes (default: from --in)
#     --gba-core    pinned ARM64 gpSP build folder (licence and corresponding source included)
#     --in          the artefacts (default release-out/): bin/pocket-ui and bin/pocket-core (or a ready
#                   pocket-ui-<v>-aarch64.tar.gz), optional pocket-ui-<v>-rocknix-install.zip,
#                   pocket-ui-<v>-rp-mini-v2.img.xz, NOTES.txt and store/ (a signed catalog folder)
#     --out         the folder to deploy (default site-updates/, gitignored); emptied first
#     --channel     channel folder under /updates/ (default stable)
#     --key         the private signing key (default $POCKETUI_SIGNING_KEY, else ~/.config/pocket-ui/store-signing.key)
#     --previous    the update.json that is live now: entries this release doesn't replace (the system update) are
#                   carried over; its signature is checked first
#     --os-tar      a system update tar of our own image build, with --os-version (its version, YYYYMMDD)
#     --base-url    where the files will be (default https://github.com/OWNER/NAME/releases/download/v<version>)
#     --direct-app  the app on pocketui.xyz first, the same bytes at --base-url as its signed mirror (80 MB guard;
#                   nothing deployed). The default unless --base-url is given: pocket-ui up to 0.2.x knows no mirrors
#                   and downloads only `url`, so the app must not depend on GitHub's download host, which some
#                   players' networks don't reach.
#     --github-primary  the app only at --base-url (GitHub), for a test channel
#     --allow-foreign-key  sign with a key pocket-ui does not have built in (tests); devices would refuse the result
#
# --check downloads every file the list names, the way a device does (https only, redirects followed), and compares
# size and sha256: run it after the upload to GitHub and before the list goes to Vercel. A pocketui.xyz file that is not
# live yet is compared in the folder next to the list, which goes live with it.
#
# Publishes nothing: it writes OUT and prints the upload and deploy commands. Needs store-tool (STORE_TOOL=<path>, or
# it is built with cargo), tar, gzip, od.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REPO=
VERSION=
IN=$ROOT/release-out
OUT=$ROOT/site-updates
CHANNEL=stable
KEY=${POCKETUI_SIGNING_KEY:-$HOME/.config/pocket-ui/store-signing.key}
NOTES=
GBA_CORE_DIR=
NOTES_FILE=
PREVIOUS=
OS_TAR=
OS_VERSION=
BASE_URL=
DIRECT_APP=
FOREIGN=0
CHECK=
# The website (tests put a file:// folder here).
SITE=${POCKETUI_SITE_URL:-https://pocketui.xyz}
while [ $# -gt 0 ]; do
  case $1 in
    --direct-app) DIRECT_APP=1 ;;
    --github-primary) DIRECT_APP=0 ;;
    --check) CHECK=$2; shift ;;
    --repo) REPO=$2; shift ;;
    --version) VERSION=$2; shift ;;
    --gba-core) GBA_CORE_DIR=$2; shift ;;
    --in) IN=$2; shift ;;
    --out) OUT=$2; shift ;;
    --channel) CHANNEL=$2; shift ;;
    --key) KEY=$2; shift ;;
    --notes) NOTES=$2; shift ;;
    --notes-file) NOTES_FILE=$2; shift ;;
    --previous) PREVIOUS=$2; shift ;;
    --os-tar) OS_TAR=$2; shift ;;
    --os-version) OS_VERSION=$2; shift ;;
    --base-url) BASE_URL=$2; shift ;;
    --allow-foreign-key) FOREIGN=1 ;;
    -h | --help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
die() { echo "release-channel: $*" >&2; exit 1; }

tool() {
  if [ -n "${STORE_TOOL:-}" ]; then
    TOOL=$STORE_TOOL
  else
    (cd "$ROOT" && cargo build --release -q --bin store-tool)
    TOOL=$ROOT/target/release/store-tool
  fi
  [ -x "$TOOL" ] || die "no store-tool at $TOOL"
}

if [ -n "$CHECK" ]; then
  [ -f "$CHECK" ] || die "no list at $CHECK"
  tool
  T=$(mktemp -d)
  trap 'rm -rf "$T"' EXIT
  bad=0
  # One line per file: kind, version, size, sha256, url (store-tool list).
  "$TOOL" list "$CHECK" >"$T/list" || die "$CHECK can't be read"
  while IFS='	' read -r kind version size sha url; do
    where=
    if ! curl -fsSL --proto =https,file --proto-redir =https --connect-timeout 20 --speed-limit 1 --speed-time 30 --max-time 300 -o "$T/f" "$url"; then
      # The website's own copy is deployed together with the list: before that, the one in the list's folder counts
      # (also below it: the Brick's packages are in updates/brick/assets/).
      in_channel=${url#"$SITE/updates/$(basename "$(dirname "$CHECK")")/"}
      local_copy=$(dirname "$CHECK")/$in_channel
      if [ "$in_channel" = "$url" ] || case $in_channel in *..*) true ;; *) false ;; esac || [ ! -f "$local_copy" ]; then
        echo "  MISSING  $kind $version  $url"; bad=1; continue
      fi
      cp "$local_copy" "$T/f"
      where=" (not live yet: the copy next to the list, deployed with it)"
    fi
    got=$("$TOOL" hash "$T/f")
    if [ "$got" = "$sha  $size bytes" ]; then
      echo "  ok       $kind $version  $url$where"
      if [ "$kind" = app ] && [ ! -e "$T/assets-added" ]; then
        python3 "$ROOT/scripts/list-update-assets.py" "$T/f" >>"$T/list" || die "the app's asset list cannot be checked"
        touch "$T/assets-added"
      fi
    else echo "  CHANGED  $kind $version  $url ($got)"; bad=1; fi
    rm -f "$T/f"
  done <"$T/list"
  [ $bad = 0 ] || die "the list names files that are not there as signed: do not deploy it"
  echo "release-channel: every file of $CHECK is there and matches"
  exit 0
fi

case $REPO in */*) ;; *) echo "--repo OWNER/NAME is needed (the public GitHub repository)" >&2; exit 2 ;; esac
case $REPO in *[!A-Za-z0-9._/-]* | */*/*) echo "bad repository: '$REPO'" >&2; exit 2 ;; esac
case $CHANNEL in '' | *[!a-z0-9-]*) echo "bad channel: '$CHANNEL'" >&2; exit 2 ;; esac
[ -d "$IN" ] || die "no artefact folder $IN"
IN=$(cd "$IN" && pwd)
if [ -z "$VERSION" ]; then
  # The install package and the image are named after the version; the first one found decides.
  for f in "$IN"/pocket-ui-*-rocknix-install.zip "$IN"/pocket-ui-*-rp-mini-v2.img.xz "$IN"/pocket-ui-*-aarch64.tar.gz; do
    [ -f "$f" ] || continue
    VERSION=$(basename "$f" | sed -e 's/^pocket-ui-//' -e 's/-rocknix-install\.zip$//' -e 's/-rp-mini-v2\.img\.xz$//' -e 's/-aarch64\.tar\.gz$//')
    break
  done
fi
# Plain X.Y.Z: every pocket-ui compares versions by their numbers only (update::newer, the same up to 0.2.x), so 0.3.0-rc1
# would count as newer than 0.3.0 (which would then never be offered to it) and 0.3 as older than 0.3.0.
VERSION_BAD=
case $VERSION in
  ''|*[!0-9.]*|.*|*.|*..*|*.*.*.*.*) VERSION_BAD=1 ;;
  *.*.*.*) [ "$CHANNEL" = brick ] || VERSION_BAD=1 ;;
  *.*.*) ;;
  *) VERSION_BAD=1 ;;
esac
[ -z "$VERSION_BAD" ] || { echo "bad or no version: '$VERSION' (give --version, plain X.Y.Z; brick also accepts X.Y.Z.W)" >&2; exit 2; }
[ -z "$OS_TAR" ] || case $OS_VERSION in '' | *[!0-9]*) echo "--os-tar needs --os-version YYYYMMDD" >&2; exit 2 ;; esac
[ -f "$KEY" ] || die "no signing key at $KEY (docs/UPDATE-CHANNEL.md, 'The signing key')"
# The app on the website first, unless the files go somewhere of their own (--base-url) or a test channel wants GitHub only.
[ -n "$DIRECT_APP" ] || { if [ -n "$BASE_URL" ]; then DIRECT_APP=0; else DIRECT_APP=1; fi; }
[ -n "$BASE_URL" ] || BASE_URL=https://github.com/$REPO/releases/download/v$VERSION
BASE_URL=${BASE_URL%/}
case $BASE_URL in https://* | file://*) ;; *) die "the files must be on https (pocket-ui refuses anything else): $BASE_URL" ;; esac
case $OUT in / | "$ROOT" | "$IN" | /dev | /dev/*) die "refusing to empty $OUT" ;; esac

tool

# GitHub refuses release files of 2 GiB or more.
GH_LIMIT=2147483648
size_of() { wc -c <"$1" | tr -d ' '; }
check_size() {
  [ "$(size_of "$1")" -lt "$GH_LIMIT" ] || die "$(basename "$1") is $(size_of "$1") bytes; GitHub Releases takes files under 2 GiB"
}

# pocket-ui and pocket-core must be 64-bit little-endian ELF for aarch64 (e_machine 183) and carry the version.
check_binary() {
  head=$(od -An -tx1 -N20 "$1" | tr -d ' \n')
  case $head in 7f454c460201*) ;; *) die "$1 is not a 64-bit little-endian ELF program" ;; esac
  [ "$(printf %s "$head" | cut -c37-40)" = b700 ] || die "$1 is not an aarch64 program"
  # The device checks `--version` itself before switching; this catches a build of another version here already.
  grep -aq "$VERSION" "$1" || die "$1 does not carry version $VERSION"
}

# The key first, before hashing gigabytes: a signature made with it must verify against src/store.rs's PUBLIC_KEY.
# The scratch folders ($P, $S, $X below) go on every way out, also when a check ends the script with `die`.
P=$(mktemp -d)
trap 'rm -rf "${P:-}" "${S:-}" "${X:-}"' EXIT
printf 'pocket-ui release key check\n' >"$P/probe"
"$TOOL" sign "$KEY" "$P/probe" >/dev/null || { rm -rf "$P"; die "can't sign with $KEY"; }
if [ $FOREIGN = 0 ] && ! "$TOOL" verify "$P/probe" >/dev/null 2>&1; then
  rm -rf "$P"
  die "$KEY is not the key whose public half is in src/store.rs (PUBLIC_KEY): devices would refuse the list. Nothing written."
fi
rm -rf "$P"

rm -rf "$OUT"
mkdir -p "$OUT/updates/$CHANNEL" "$OUT/github"
OUT=$(cd "$OUT" && pwd)
CH=$OUT/updates/$CHANNEL
GH=$OUT/github
: >"$GH/assets.txt"
asset() { printf '%s\n' "$1" >>"$GH/assets.txt"; }

# 1. The app update: the tarball the Updates page installs.
APP=pocket-ui-$VERSION-aarch64.tar.gz
if [ -f "$IN/$APP" ]; then
  cp "$IN/$APP" "$GH/$APP"
else
  for b in pocket-ui pocket-core; do [ -f "$IN/bin/$b" ] || die "neither $APP nor bin/$b in $IN"; done
  S=$(mktemp -d)
  install -m 755 "$IN/bin/pocket-ui" "$IN/bin/pocket-core" "$ROOT/deploy/portmaster-setup.sh" "$S/"
  # The boot screen (docs/BOOT-SPLASH.md) when it was built; a release without it shows no boot screen, nothing else.
  FILES="pocket-ui pocket-core portmaster-setup.sh"
  if [ -f "$IN/bin/pocket-splash" ]; then
    install -m 755 "$IN/bin/pocket-splash" "$S/"
    FILES="pocket-ui pocket-core pocket-splash portmaster-setup.sh"
  fi
  if [ -n "$GBA_CORE_DIR" ]; then
    sh "$ROOT/scripts/gba-core-files.sh" "$GBA_CORE_DIR" "$S/cores"
    FILES="$FILES cores"
  fi
  EPOCH=${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || echo 315532800)}
  # The same bytes for the same files: fixed order, owner and time (GNU tar; another tar packs without them).
  if tar --version 2>/dev/null | grep -q GNU; then
    tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$EPOCH" -cf - -C "$S" $FILES | gzip -n9 >"$GH/$APP"
  else
    COPYFILE_DISABLE=1 tar -cf - -C "$S" $FILES | gzip -n9 >"$GH/$APP"
  fi
fi
# Whichever way it came, look at what is inside: the device links these names (pocket-splash runs through current/).
X=$(mktemp -d)
tar -xzf "$GH/$APP" -C "$X" || die "$APP can't be unpacked"
for b in pocket-ui pocket-core; do [ -f "$X/$b" ] || die "$APP has no $b"; check_binary "$X/$b"; done
if [ -n "$GBA_CORE_DIR" ]; then
  sh "$ROOT/scripts/gba-core-files.sh" "$GBA_CORE_DIR" "$X/.expected-cores"
  diff -r "$X/.expected-cores" "$X/cores" >/dev/null 2>&1 || die "the app archive lacks the verified ARM64 gpSP, licence or source"
fi
rm -rf "$X"
asset "$GH/$APP"

# 2. The channel list: carried over from the live one, then each file of this release, signed after each step.
INDEX=$CH/update.json
if [ -n "$PREVIOUS" ]; then
  [ -f "$PREVIOUS" ] && [ -f "$PREVIOUS.sig" ] || die "--previous needs the file and its .sig: $PREVIOUS"
  if [ $FOREIGN = 1 ]; then "$TOOL" verify "$PREVIOUS" "$KEY" >/dev/null; else "$TOOL" verify "$PREVIOUS" >/dev/null; fi ||
    die "the previous list $PREVIOUS is not signed by the expected key"
  cp "$PREVIOUS" "$INDEX"
fi
[ -z "$NOTES_FILE" ] || NOTES=$(cat "$NOTES_FILE")
[ -n "$NOTES" ] || [ ! -f "$IN/NOTES.txt" ] || NOTES=$(cat "$IN/NOTES.txt")
add() { # kind version file
  check_size "$3"
  "$TOOL" update "$KEY" "$INDEX" "$1" "$2" "$3" "$BASE_URL/$(basename "$3")" "$NOTES" >/dev/null
  printf '  %-12s %-10s %s\n' "$1" "$2" "$BASE_URL/$(basename "$3")"
}
echo "release-channel: pocket-ui $VERSION, channel $CHANNEL"
add app "$VERSION" "$GH/$APP"
if [ "$DIRECT_APP" = 1 ]; then
  [ "$(size_of "$GH/$APP")" -le 80000000 ] || die "app exceeds the 80 MB direct-hosting budget"
  cp "$GH/$APP" "$CH/$APP"
  "$TOOL" update "$KEY" "$INDEX" app "$VERSION" "$GH/$APP" "$SITE/updates/$CHANNEL/$APP" "$NOTES" >/dev/null
  "$TOOL" mirror "$KEY" "$INDEX" app "$BASE_URL/$APP" >/dev/null
  printf '  %-12s %-10s %s (first; the line above is its mirror)\n' app "$VERSION" "$SITE/updates/$CHANNEL/$APP"
fi
if [ -n "$OS_TAR" ]; then
  [ -f "$OS_TAR" ] || die "no system tar $OS_TAR"
  # The device takes only a *.tar name (update::stage_os).
  case $(basename "$OS_TAR") in *[!A-Za-z0-9._+-]* | *..*) die "odd system tar name: $(basename "$OS_TAR")" ;; *.tar) ;; *) die "a system update is a .tar: $OS_TAR" ;; esac
  add os "$OS_VERSION" "$OS_TAR"
  asset "$(cd "$(dirname "$OS_TAR")" && pwd)/$(basename "$OS_TAR")"
fi
for kind in install_zip sd_image; do
  case $kind in install_zip) f=$IN/pocket-ui-$VERSION-rocknix-install.zip ;; *) f=$IN/pocket-ui-$VERSION-rp-mini-v2.img.xz ;; esac
  [ -f "$f" ] || { echo "  $kind: none in $IN (left out)"; continue; }
  add "$kind" "$VERSION" "$f"
  asset "$f"
  # The image's notes go along, under the same name as make-image.sh writes them.
  if [ $kind = sd_image ] && [ -f "${f%.img.xz}.txt" ]; then asset "${f%.img.xz}.txt"; fi
done
# The loose Ports scripts and the other zips of the release page. GitHub turns a space in an asset's name into a dot
# ("Install pocket-ui.sh" becomes Install.pocket-ui.sh), so the file is uploaded and listed in SHA256SUMS under the dotted
# name: `sha256sum -c` then finds what was downloaded. They work under either name (port-installer.sh looks at "Uninstall*").
for f in "Install pocket-ui.sh" "Uninstall pocket-ui.sh"; do
  [ -f "$IN/$f" ] || continue
  g=$(printf '%s' "$f" | tr ' ' '.')
  cp "$IN/$f" "$GH/$g"
  asset "$GH/$g"
done
for f in "$IN/pocket-ui-$VERSION-ports-installer.zip" "$IN/pocket-ui-$VERSION-rocknix-card.zip"; do
  [ -f "$f" ] && asset "$f"
done

# 3. The signature must be one pocket-ui accepts: the public key in src/store.rs.
if [ $FOREIGN = 1 ]; then
  "$TOOL" verify "$INDEX" "$KEY" >/dev/null || die "the list does not verify"
  echo "release-channel: WARNING: signed with a key pocket-ui does not have built in (--allow-foreign-key); devices refuse this list"
elif ! "$TOOL" verify "$INDEX" >/dev/null 2>&1; then
  rm -rf "$OUT"
  die "$KEY is not the key whose public half is in src/store.rs (PUBLIC_KEY): devices would refuse the list. Nothing written."
fi

# 4. The store catalog, when there is one: a signed folder from scripts/make-test-catalog.sh or by hand (docs/STORE.md).
if [ -f "$IN/store/catalog.json" ]; then
  if [ $FOREIGN = 1 ]; then "$TOOL" verify "$IN/store/catalog.json" "$KEY" >/dev/null; else "$TOOL" verify "$IN/store/catalog.json" >/dev/null; fi ||
    die "$IN/store/catalog.json is not signed by the expected key"
  mkdir -p "$OUT/updates/store"
  cp -R "$IN/store/." "$OUT/updates/store/"
  echo "  store        catalog.json ($(du -sk "$OUT/updates/store" | cut -f1) KB with its packages)"
fi

# 5. Checksums for the release page, the headers for Vercel, and what to do next.
(cd "$GH" && while read -r f; do
  printf '%s  %s\n' "$("$TOOL" hash "$f" | cut -d' ' -f1)" "$(basename "$f")"
done <assets.txt) >"$GH/SHA256SUMS"
asset "$GH/SHA256SUMS"
printf '%s\n' "$NOTES" >"$GH/notes.md"
cat >"$OUT/vercel-headers.json" <<'EOF'
{
  "headers": [
    {
      "source": "/updates/(.*)",
      "headers": [
        { "key": "Cache-Control", "value": "no-cache, no-store, must-revalidate" },
        { "key": "Pragma", "value": "no-cache" }
      ]
    }
  ]
}
EOF
SITE_KB=$(du -sk "$OUT/updates" | cut -f1)
[ "$SITE_KB" -lt 20480 ] || echo "release-channel: WARNING: updates/ is $SITE_KB KB; Vercel Hobby takes 100 MB per deployment in all. Put big store packages on GitHub Releases"
cat >"$OUT/NEXT-STEPS.txt" <<EOF
pocket-ui $VERSION, channel $CHANNEL. Nothing has been published.

1. GitHub release (the files the list points to must be there first):
   gh release create v$VERSION --repo $REPO --title "pocket-ui $VERSION" --notes-file $GH/notes.md \\
     \$(cat $GH/assets.txt)
2. Check the uploads against the signed list (downloads every file once, as a device does):
   scripts/release-channel.sh --check $INDEX
3. Copy $OUT/updates/ into the website's folder (updates/ next to index.html; with the app it carries, keep the files
   of earlier versions that are there), keep the /updates/ rule of vercel-headers.json in its vercel.json, deploy the
   website, and run step 2 again: now every file is the live one.
EOF
echo "release-channel: wrote $OUT ($(wc -l <"$GH/assets.txt" | tr -d ' ') files for the GitHub release); next: $OUT/NEXT-STEPS.txt"

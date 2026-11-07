#!/bin/bash
# Publishes a Brick card build to the private update channel (docs/TRIMUI-BRICK-REMOTE.md). PRIVATE: the channel is only reachable
# inside the owner's tailnet; never copy it, its address or the Brick to the public site or the public repository.
#   scripts/brick-publish.sh CARD [--channel NAME] [--notes TEXT] [--web DIR] [--full-check]
# Public channel (docs/UPDATE-CHANNEL.md, "The Brick card"): prepares, publishes nothing.
#   scripts/brick-publish.sh CARD --public OWNER/NAME --out DIR [--tag vX.Y.Z] [--zip CARD.zip] [--notes TEXT]
#   writes DIR/release/ (the tarball, and the card zip when given: files for the GitHub release <tag> of OWNER/NAME) and
#   DIR/updates/brick/update.json + .sig (for pocketui.xyz/updates/brick/, the address a public card asks, update::BRICK_DEFAULT_URL),
#   app and immutable per-file core/data packages on pocketui.xyz first, GitHub fallback. A beta card is refused.
#   After the upload: scripts/release-channel.sh --check DIR/updates/brick/update.json, then deploy the list with the website.
# CARD        what scripts/brick-make-card.sh made (OUT/card), or its pocket-ui/app folder
# --channel   the channel's folder name (default beta): <web>/brick-updates/<channel>/
# --web       the folder `tailscale serve` shares at / on this machine (default ~/work/drop/export/web)
# --notes     shown on the Brick's Updates page
# --full-check  also downloads the tarball back over the tailnet and compares its sha256 (default: only the list and signature)
# What it does: checks the program folder (the files the card's start scripts need, aarch64 programs that carry the version in
# app/RELEASE, a version newer than the channel's), packs it reproducibly (sorted, no owners, no times), puts the tarball next to the
# list, signs the list with the store key (store-tool, the same key and checks as docs/UPDATE-CHANNEL.md; the key is never printed),
# switches update.json and its .sig by rename, keeps the last three tarballs (a Brick in the middle of a download finishes it), and
# reads the list back through https://<this machine's tailnet name>/brick-updates/<channel>/update.json.
# Env: POCKETUI_SIGNING_KEY (default ~/.config/pocket-ui/store-signing.key), STORE_TOOL (a built store-tool; else target/*/store-tool,
# else `cargo build --bin store-tool`), BRICK_CHANNEL_BASE (the https base instead of this machine's tailnet name, for the tests).
# Mirror (BRICK_MIRROR, default none): a machine whose `tailscale serve` has /brick-updates (a static folder ~/brick-updates there,
# a launchd/systemd static server of the maintainer's). The tarball is copied there too and the signed list names it by its absolute address, so a
# Brick on that machine's network downloads it directly instead of through a relay; the list itself stays here. BRICK_MIRROR is
# that machine's ssh and tailnet host name, or a local folder (the tests) with BRICK_MIRROR_BASE as its address. If the mirror
# can't be reached or doesn't give the tarball back, the list names this machine's copy as before.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
die() { echo "brick-publish: $*" >&2; exit 1; }
CARD=${1:?card folder}; shift
CHANNEL=beta; NOTES=""; WEB=$HOME/work/drop/export/web; FULL=0; PUBLIC=""; OUTDIR=""; TAG=""; ZIP=""
while [ $# -gt 0 ]; do
  case $1 in
    --public) PUBLIC=${2:?}; shift 2 ;;
    --out) OUTDIR=${2:?}; shift 2 ;;
    --tag) TAG=${2:?}; shift 2 ;;
    --zip) ZIP=${2:?}; shift 2 ;;
    --channel) CHANNEL=${2:?}; shift 2 ;;
    --notes) NOTES=${2-}; shift 2 ;;
    --web) WEB=${2:?}; shift 2 ;;
    --full-check) FULL=1; shift ;;
    *) die "unknown option $1" ;;
  esac
done
case $CHANNEL in ''|*[!a-z0-9-]*) die "a channel name is lower-case letters, digits and -" ;; esac
KEY=${POCKETUI_SIGNING_KEY:-$HOME/.config/pocket-ui/store-signing.key}
[ -r "$KEY" ] || die "no signing key at $KEY"
if [ -d "$CARD/pocket-ui/app" ]; then APP=$CARD/pocket-ui/app; else APP=$CARD; fi
APP=$(cd "$APP" && pwd)

# --- the program folder -----------------------------------------------------------------------------------------------
VERSION=$(head -n 1 "$APP/RELEASE" 2>/dev/null | tr -d '[:space:]')
[ -n "$VERSION" ] || die "$APP/RELEASE is missing or empty"
case $VERSION in *[0-9]*) ;; *) die "version $VERSION has no number to count up" ;; esac
for f in start.sh RELEASE bin/pocket-ui bin/pocket-core bin/pocket-fb bin/brick-hw msg/starting.png bin/tailscale bin/tailscaled bin/curl bin/dropbearmulti bin/syncthing \
  hwroot/usr/bin/brick-ssh hwroot/usr/bin/brick-tailscale hwroot/usr/bin/brick-syncthing; do
  [ -s "$APP/$f" ] || die "the program folder has no $f"
done
[ -e "$APP/.ready" ] && die "$APP has a .ready mark: that is a folder from a Brick, not a build"
sh -n "$APP/start.sh" || die "start.sh is not valid sh"
for b in pocket-ui pocket-core pocket-fb; do
  # ELF, 64-bit, little-endian, e_machine 183 (aarch64)
  [ "$(od -An -tx1 -j0 -N5 "$APP/bin/$b" | tr -d ' \n')" = 7f454c4602 ] && [ "$(od -An -tu2 -j18 -N2 "$APP/bin/$b" | tr -d ' ')" = 183 ] || die "bin/$b is not an aarch64 program"
done
for b in pocket-ui pocket-core; do
  grep -aqF "$VERSION" "$APP/bin/$b" || die "bin/$b was not built as $VERSION (POCKETUI_VERSION); the Brick would refuse it"
done
if [ -n "$PUBLIC" ]; then
  case $PUBLIC in */*) ;; *) die "--public OWNER/NAME: the public GitHub repository" ;; esac
  case $PUBLIC in *[!A-Za-z0-9._/-]*|*/*/*) die "bad repository: $PUBLIC" ;; esac
  [ -n "$OUTDIR" ] || die "--public needs --out DIR"
  [ -e "$APP/update-url" ] && die "this build asks the channel in app/update-url ($(head -n 1 "$APP/update-url")): a public card must have none (build the card without BRICK_UPDATE_URL)"
  case $VERSION in
    ''|*[!0-9.]*|.*|*.|*..*|*.*.*.*.*) die "version $VERSION is not plain X.Y.Z or a Brick-only X.Y.Z.W hotfix" ;;
    *.*.*) ;;
    *) die "version $VERSION is not plain X.Y.Z or a Brick-only X.Y.Z.W hotfix" ;;
  esac
  grep -aqF 'asset-packages.json' "$APP/bin/pocket-ui" || die "this card's updater cannot install small asset packages; rebuild the public card with the new updater first"
  [ -z "$ZIP" ] || [ -s "$ZIP" ] || die "no card zip at $ZIP"
elif [ -s "$APP/update-url" ] && ! grep -q "/brick-updates/$CHANNEL/update.json" "$APP/update-url"; then
  echo "brick-publish: note: this build looks for updates at $(cat "$APP/update-url"), not in channel $CHANNEL" >&2
fi

# --- store-tool ----------------------------------------------------------------------------------------------------------
ST=${STORE_TOOL:-}
if [ -z "$ST" ]; then
  for c in "$ROOT/target/release/store-tool" "$ROOT/target/debug/store-tool" "${CARGO_TARGET_DIR:-/nonexistent}/debug/store-tool" "${CARGO_TARGET_DIR:-/nonexistent}/release/store-tool"; do
    [ -x "$c" ] && { ST=$c; break; }
  done
fi
if [ -z "$ST" ]; then
  (cd "$ROOT" && nice -n 19 cargo build -q -j4 --bin store-tool) || die "store-tool does not build"
  ST=${CARGO_TARGET_DIR:-$ROOT/target}/debug/store-tool
fi

# --- the public channel: files for the GitHub release and the website, nothing uploaded ------------------------------------
if [ -n "$PUBLIC" ]; then
  TAG=${TAG:-v$VERSION}
  case $TAG in *[!A-Za-z0-9.+-]*|'') die "bad tag: $TAG" ;; esac
  NAME=pocket-ui-brick-$VERSION.tar.gz
  BASE=https://github.com/$PUBLIC/releases/download/$TAG
  mkdir -p "$OUTDIR"; OUTDIR=$(cd "$OUTDIR" && pwd)
  rm -rf "$OUTDIR/release" "$OUTDIR/updates/brick"; mkdir -p "$OUTDIR/release" "$OUTDIR/updates/brick"
  python3 "$ROOT/scripts/brick-update-pack.py" "$APP" "$OUTDIR/release" "$OUTDIR/updates/brick" "$BASE" "$VERSION" \
    || die "the small app / asset packages could not be prepared"
  # GitHub Releases take files under 2 GiB.
  for f in "$OUTDIR/release/$NAME" ${ZIP:+"$ZIP"}; do [ "$(wc -c <"$f" | tr -d ' ')" -lt 2147483648 ] || die "$(basename "$f") is 2 GiB or more: GitHub refuses it"; done
  "$ST" update "$KEY" "$OUTDIR/updates/brick/update.json" app "$VERSION" "$OUTDIR/release/$NAME" "https://pocketui.xyz/updates/brick/$NAME" "$NOTES" >/dev/null
  "$ST" mirror "$KEY" "$OUTDIR/updates/brick/update.json" app "$BASE/$NAME" >/dev/null
  if [ -n "$ZIP" ]; then
    cp "$ZIP" "$OUTDIR/release/"
    "$ST" update "$KEY" "$OUTDIR/updates/brick/update.json" install_zip "$VERSION" "$ZIP" "$BASE/$(basename "$ZIP")" "$NOTES" >/dev/null
  fi
  "$ST" verify "$OUTDIR/updates/brick/update.json" ${BRICK_PUBLISH_VERIFY_KEY:+"$BRICK_PUBLISH_VERIFY_KEY"} >/dev/null \
    || die "the list does not verify with the key pocket-ui trusts (store::PUBLIC_KEY): wrong signing key?"
  echo "brick-publish: public $VERSION prepared in $OUTDIR (nothing uploaded). Next, by hand:"
  echo "  gh release upload $TAG $OUTDIR/release/* --repo $PUBLIC"
  echo "  scripts/release-channel.sh --check $OUTDIR/updates/brick/update.json"
  echo "  then check every asset package and copy $OUTDIR/updates/brick/ to the website's updates/brick/ and deploy it"
  exit 0
fi

# --- the channel ---------------------------------------------------------------------------------------------------------
DIR=$WEB/brick-updates/$CHANNEL
mkdir -p "$DIR"
if [ -s "$DIR/update.json" ]; then
  LIVE=$("$ST" list "$DIR/update.json" | awk -F'\t' '$1 == "app" { print $2 }')
  if [ -n "$LIVE" ]; then
    python3 -I - "$VERSION" "$LIVE" <<'PY' || die "version $VERSION is not newer than $LIVE, which the channel has: a Brick would not take it"
import re, sys
nums = lambda s: [int(p) for p in re.split(r"\D+", s) if p]
sys.exit(0 if nums(sys.argv[1]) > nums(sys.argv[2]) else 1)
PY
  fi
fi
NAME=pocket-ui-brick-$VERSION.tar.gz
TMP=$(mktemp -d "${TMPDIR:-/tmp}/brick-publish.XXXXXX"); trap 'rm -rf "$TMP"' EXIT
# Reproducible: the same folder gives the same bytes (sorted names, no owners, no times, gzip without a name or time).
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 --format=ustar -C "$APP" -cf - . | gzip -9n >"$TMP/$NAME"
# --- the mirror (optional): the tarball there first, checked by its size over the tailnet ---------------------------------
TARBALL_URL=$NAME
MIRROR=${BRICK_MIRROR:-}
if [ -n "$MIRROR" ]; then
  MBASE=${BRICK_MIRROR_BASE:-}
  if [ -z "$MBASE" ]; then
    MDNS=$(tailscale status --json 2>/dev/null | python3 -I -c '
import json, sys
want = sys.argv[1].lower()
for p in json.load(sys.stdin).get("Peer", {}).values():
    if p.get("HostName", "").lower() == want or p.get("DNSName", "").lower().startswith(want + "."):
        print(p["DNSName"].rstrip(".")); break' "$MIRROR" 2>/dev/null || true)
    [ -n "$MDNS" ] && MBASE=https://$MDNS/brick-updates
  fi
  mirrored=
  if [ -n "$MBASE" ]; then
    case $MIRROR in
      /*) mkdir -p "$MIRROR/$CHANNEL" && cp "$TMP/$NAME" "$MIRROR/$CHANNEL/.$NAME.part" && mv -f "$MIRROR/$CHANNEL/.$NAME.part" "$MIRROR/$CHANNEL/$NAME" && mirrored=1 ;;
      *) ssh -o BatchMode=yes -o ConnectTimeout=10 "$MIRROR" "mkdir -p brick-updates/$CHANNEL" \
           && scp -q -o BatchMode=yes "$TMP/$NAME" "$MIRROR:brick-updates/$CHANNEL/.$NAME.part" \
           && ssh -o BatchMode=yes "$MIRROR" "mv -f brick-updates/$CHANNEL/.$NAME.part brick-updates/$CHANNEL/$NAME && ls -1t brick-updates/$CHANNEL/pocket-ui-brick-*.tar.gz | tail -n +4 | while read -r old; do rm -f \"\$old\"; done" \
           && mirrored=1 ;;
    esac
  fi
  size=$(wc -c <"$TMP/$NAME" | tr -d ' ')
  if [ -n "$mirrored" ] && [ "$(curl -fsSI --max-time 20 "$MBASE/$CHANNEL/$NAME" 2>/dev/null | tr -d '\r' | awk 'tolower($1) == "content-length:" { print $2 }' | tail -n 1)" = "$size" ]; then
    TARBALL_URL=$MBASE/$CHANNEL/$NAME
  else
    echo "brick-publish: note: the mirror $MIRROR did not take or give back the tarball; the list names this machine's copy" >&2
  fi
fi
# The list is made and checked aside; only then do the tarball and the list go into the channel (the tarball first).
"$ST" update "$KEY" "$TMP/update.json" app "$VERSION" "$TMP/$NAME" "$TARBALL_URL" "$NOTES" >/dev/null
# Against the key built into pocket-ui (store::PUBLIC_KEY); BRICK_PUBLISH_VERIFY_KEY (a key file) is for the tests' throwaway key only.
"$ST" verify "$TMP/update.json" ${BRICK_PUBLISH_VERIFY_KEY:+"$BRICK_PUBLISH_VERIFY_KEY"} >/dev/null || die "the list does not verify with the key pocket-ui trusts (store::PUBLIC_KEY): wrong signing key? Nothing was published."
cp "$TMP/$NAME" "$DIR/.$NAME.part" && mv -f "$DIR/.$NAME.part" "$DIR/$NAME"
cp "$TMP/update.json" "$DIR/.update.json.new"; cp "$TMP/update.json.sig" "$DIR/.update.json.sig.new"
# The signature first, then the list: a Brick reads the list, then its signature; the window between the two renames is a moment.
mv -f "$DIR/.update.json.sig.new" "$DIR/update.json.sig"
mv -f "$DIR/.update.json.new" "$DIR/update.json"
# The cores' source, which the update itself no longer carries (it is on a card from the zip in pocket-ui/sources/cores): the same archives
# in the channel's sources/ folder, next to the update. Archives are named by core and commit, so the ones there stay and new ones are added.
if [ -d "$CARD/pocket-ui/sources/cores" ]; then
  mkdir -p "$DIR/sources"
  for a in "$CARD"/pocket-ui/sources/cores/*.tar.gz; do [ -e "$a" ] && { [ -s "$DIR/sources/$(basename "$a")" ] || cp "$a" "$DIR/sources/"; }; done
fi
# The last three tarballs stay.
ls -1t "$DIR"/pocket-ui-brick-*.tar.gz 2>/dev/null | tail -n +4 | while read -r old; do rm -f "$old"; done

# --- read it back over the tailnet --------------------------------------------------------------------------------------
BASE=${BRICK_CHANNEL_BASE:-}
if [ -z "$BASE" ]; then
  DNS=$(tailscale status --json 2>/dev/null | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["Self"]["DNSName"].rstrip("."))' 2>/dev/null || true)
  [ -n "$DNS" ] || die "published in $DIR, but this machine's tailnet name is unknown (is Tailscale up?)"
  BASE=https://$DNS
fi
URL=$BASE/brick-updates/$CHANNEL/update.json
curl -fsS --max-time 20 "$URL" -o "$TMP/live.json" && cmp -s "$TMP/live.json" "$DIR/update.json" || die "published in $DIR, but $URL does not give it back"
curl -fsS --max-time 20 "$URL.sig" -o "$TMP/live.sig" && cmp -s "$TMP/live.sig" "$DIR/update.json.sig" || die "$URL.sig does not give the signature back"
if [ "$FULL" = 1 ]; then
  case $TARBALL_URL in *://*) BACK=$TARBALL_URL ;; *) BACK=$BASE/brick-updates/$CHANNEL/$NAME ;; esac
  curl -fsS --max-time 600 "$BACK" -o "$TMP/back.tar.gz"
  cmp -s "$TMP/back.tar.gz" "$DIR/$NAME" || die "the tarball over the tailnet ($BACK) is not the one published"
fi
case $TARBALL_URL in *://*) FROM=" (tarball from the mirror $MIRROR)" ;; *) FROM= ;; esac
echo "brick-publish: $VERSION in channel $CHANNEL ($(du -h "$DIR/$NAME" | cut -f1))$FROM: $URL"

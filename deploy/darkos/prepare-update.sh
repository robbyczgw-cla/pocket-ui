#!/bin/sh
# Prepare the existing Brick component format in a dArkOS channel. No network writes. Usage: prepare-update.sh APP OUT RELEASE_BASE_URL
# CHANNEL: the website folder under updates/ (darkos = the releases' channel, darkos-test = betas; default darkos-test).
# APP_BASE_URL: where the app tarball is (default RELEASE_BASE_URL; a release puts it on vX.Y.Z, the packages stay on RELEASE_BASE_URL).
# STORE_TOOL: built store-tool; POCKETUI_SIGNING_KEY: local signing key (never copied); NOTES: the list's notes line.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
APP=$1; OUT=$2; BASE=$3
CHANNEL=${CHANNEL:-darkos-test}; APP_BASE=${APP_BASE_URL:-$BASE}
case $CHANNEL in darkos|darkos-test) ;; *) echo "CHANNEL must be darkos or darkos-test" >&2; exit 1 ;; esac
[ "$(cat "$APP/DARKOS-PORT")" = 1 ] || { echo 'not a dArkOS app' >&2; exit 1; }
VERSION=$(cat "$APP/RELEASE")
ST=${STORE_TOOL:-$REPO/target/debug/store-tool}
[ -x "$ST" ] || { echo 'Set STORE_TOOL to a built store-tool.' >&2; exit 1; }
KEY=${POCKETUI_SIGNING_KEY:-$HOME/.config/pocket-ui/store-signing.key}
python3 -I "$REPO/scripts/brick-update-pack.py" --platform darkos --mirror-channel "$CHANNEL" "$APP" "$OUT/release" "$OUT/updates/$CHANNEL" "$BASE" "$VERSION"
NOTES=${NOTES:-dArkOS test port: additional systems and signed port-only updates.}
python3 -I - "$OUT" "$VERSION" "$APP_BASE" "$NOTES" "$CHANNEL" <<'PY'
import hashlib,json,sys
from pathlib import Path
out=Path(sys.argv[1]);version=sys.argv[2];base=sys.argv[3].rstrip('/');channel=sys.argv[5]
name='pocket-ui-darkos-'+version+'.tar.gz';p=out/'release'/name
entry=dict(version=version,url=base+'/'+name,size=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest(),
           mirrors=['https://pocketui.xyz/updates/'+channel+'/'+name],notes=sys.argv[4])
manifest=dict(format=1,target='darkos-port',app=entry)
(out/'updates'/channel/'update.json').write_text(json.dumps(manifest,indent=2)+'\n')
PY
"$ST" sign "$KEY" "$OUT/updates/$CHANNEL/update.json" >/dev/null
"$ST" verify "$OUT/updates/$CHANNEL/update.json" >/dev/null
printf '%s\n' "Prepared and signature verified: $OUT/updates/$CHANNEL/update.json (nothing uploaded)."

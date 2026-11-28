#!/bin/sh
# Offline emulator-route check of a release's payloads (docs/DEVICE-TESTS.md "Emulator routes of the release payloads").
#   scripts/release-routes.sh VERSION WORKDIR KIND=PATH...
# KIND: rocknix-install, rocknix-card, rocknix-ota, rocknix-update, brick-card, brick-update, darkos-zip, darkos-update
# (scripts/release-routes.py). Keep WORKDIR's name the same between releases: the build host keeps its copy by that name.
# Bundles the payloads where they were built (seconds), runs the release's own pocket-ui in one test tree per family, payload and
# device on the aarch64 build host (its Linux container, chroot) and compares with scripts/release-routes.json. Exit 0 green, 1 a wrong
# route, 2 bad inputs, 3 no build-host slot. The receipt: WORKDIR/target/air/release-routes-VERSION.json.
# Env: ROUTE_BASES (the base listings, `release-routes.py base`; default $HOME/pocket-review/route-bases),
#      AIR_BUILD (default $HOME/pocket-review/tools/air-build.sh), ROUTES_ALLOW_MISSING=1 (skip families without payloads),
#      ROUTES_PROBE_ROCKNIX / ROUTES_PROBE_BRICK (tests of the check only: another pocket-ui than the payloads', said in the receipt;
#      the Brick one also stands in for dArkOS's, the same `--features brick` build).
set -eu
[ $# -ge 3 ] || { sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
V=$1; W=$2; shift 2
HERE=$(cd "$(dirname "$0")" && pwd)
BASES=${ROUTE_BASES:-$HOME/pocket-review/route-bases}
AIR_BUILD=${AIR_BUILD:-$HOME/pocket-review/tools/air-build.sh}
t0=$(date +%s)
mkdir -p "$W"
W=$(cd "$W" && pwd)
set -- ${ROUTES_PROBE_ROCKNIX:+--probe-rocknix "$ROUTES_PROBE_ROCKNIX"} ${ROUTES_PROBE_BRICK:+--probe-brick "$ROUTES_PROBE_BRICK"} "$@"
python3 -I "$HERE/release-routes.py" bundle --version "$V" --bases "$BASES" --out "$W/bundle" "$@" || exit 2
cp "$HERE/release-routes.py" "$HERE/release-routes.json" "$W/"
# tools/air-build.sh copies only folders that look like a worktree
[ -f "$W/Cargo.toml" ] || printf '# marker for tools/air-build.sh: this folder is the route check bundle, not a crate\n' >"$W/Cargo.toml"
t1=$(date +%s)
allow=; [ "${ROUTES_ALLOW_MISSING:-0}" = 1 ] && allow=--allow-missing-payloads
R=$W/target/air/release-routes-$V.json
rm -f "$R"
rc=0
"$AIR_BUILD" --host air -t 600 -f "release-routes-$V.json" -x "$W" \
  python3 -I release-routes.py run bundle release-routes.json --receipt "/target/release-routes-$V.json" --zero $allow || rc=$?
# the run exits 0 when it wrote its receipt (--zero); the verdict is in there
if [ $rc -eq 0 ]; then
  v=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["verdict"])' "$R" 2>/dev/null || echo missing)
  [ "$v" = ok ] || rc=1
elif [ $rc -eq 7 ]; then rc=1; fi   # air-build 7: no receipt came back (the run failed before writing it)
echo "release routes: bundle $((t1 - t0)) s, host run $(( $(date +%s) - t1 )) s, total $(( $(date +%s) - t0 )) s, receipt $R (exit $rc)"
exit $rc

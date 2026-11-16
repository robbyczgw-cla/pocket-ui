#!/bin/sh
# Test of scripts/run-tmp.sh (the run's own TMPDIR): the folder is made, goes away at the end however the run ends, what a
# test leaves in it is reported, a red run keeps a few small files, the preflight removes what dead runs left and cleans old
# folders first when the base is nearly full - and never touches a run that is alive. Runs on a base of its own.
# Alone: sh scripts/test-run-tmp.sh
HERE=$(cd "$(dirname "$0")" && pwd)
B=$(mktemp -d "${TMPDIR:-/tmp}/test-run-tmp.XXXXXX") || exit 2
trap 'chmod -R u+rwX "$B" 2>/dev/null; rm -rf "$B"' EXIT
fail=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fail=1; }
run() { # run <script text>: a fresh shell that sources the helper on the test base
    PUI_TMP_BASE=$B PUI_RUN_TMP= sh -c ". '$HERE/run-tmp.sh'; $1" 2>&1
}
runs() { find "$B" -maxdepth 1 -name 'pui-run-*' | wc -l; }

# 1. TMPDIR points into a folder below the base while the run lasts; none is left after a normal end.
out=$(run 'run_tmp_begin t; echo "in:$TMPDIR"; [ -d "$TMPDIR" ] && echo present')
case $out in *"in:$B/pui-run-t."*present*) ok "TMPDIR is a run folder below the base" ;; *) bad "TMPDIR is a run folder: $out" ;; esac
[ "$(runs)" -eq 0 ] && ok "the folder is gone after a normal end" || bad "folder left after a normal end"

# 2. also after exit 1 anywhere, and after a TERM.
run 'run_tmp_begin t; exit 1' >/dev/null
[ "$(runs)" -eq 0 ] && ok "gone after exit 1" || bad "folder left after exit 1"
PUI_TMP_BASE=$B PUI_RUN_TMP= sh -c ". '$HERE/run-tmp.sh'; run_tmp_begin t; echo \$\$ >'$B/pid'; sleep 30 & wait" >/dev/null 2>&1 &
n=0; while [ ! -s "$B/pid" ] && [ $n -lt 50 ]; do sleep 0.1; n=$((n + 1)); done
kill -TERM "$(cat "$B/pid")"; wait
[ "$(runs)" -eq 0 ] && ok "gone after SIGTERM" || bad "folder left after SIGTERM"

# 3. RED BEFORE: a test that leaves folders in TMPDIR is told so (one note, the names), and nothing stays.
out=$(run 'run_tmp_begin t; mkdir "$TMPDIR/pui-rigs-cozy-1234" "$TMPDIR/pui-rigs-cozy-5678" "$TMPDIR/pocketui-x-AbCdEf"; echo leak > "$TMPDIR/pui-rigs-cozy-1234/a.log"')
case $out in *"left 3 entries"*pui-rigs-cozy-*) ok "leftovers are reported with their names" ;; *) bad "leftovers reported: $out" ;; esac
[ "$(runs)" -eq 0 ] && ok "leftovers are removed with the run" || bad "leftovers stayed"
out=$(run 'run_tmp_begin t; mkdir "$TMPDIR/clean-test"; rmdir "$TMPDIR/clean-test"')
case $out in *"left "*) bad "a clean run is said to leak: $out" ;; *) ok "a clean run says nothing" ;; esac

# 4. A red run keeps a few small files, bounded (30), one set per label.
out=$(run 'run_tmp_begin t; mkdir "$TMPDIR/d"; i=0; while [ $i -lt 40 ]; do echo x >"$TMPDIR/d/f$i.png"; i=$((i+1)); done; exit 1')
kept=$(ls "$B/gate-artifacts-t" 2>/dev/null | wc -l)
[ "$kept" -eq 30 ] && ok "a red run keeps 30 files, no more" || bad "kept $kept files: $out"
run 'run_tmp_begin t; echo x >"$TMPDIR/only.log"; exit 1' >/dev/null
[ "$(ls "$B/gate-artifacts-t" | wc -l)" -eq 1 ] && ok "the next red run replaces the set" || bad "artifacts not replaced"
run 'run_tmp_begin t; echo x >"$TMPDIR/only.log"' >/dev/null
rm -rf "$B/gate-artifacts-t"

# 5. The preflight removes run folders of dead owners and keeps those of live ones.
mkdir "$B/pui-run-dead.AAAAAA" "$B/pui-run-live.BBBBBB"; echo 99999999 >"$B/pui-run-dead.AAAAAA/.owner"
sleep 30 & live=$!; echo $live >"$B/pui-run-live.BBBBBB/.owner"
out=$(run 'run_tmp_begin t')
kill $live 2>/dev/null; wait $live 2>/dev/null
[ ! -d "$B/pui-run-dead.AAAAAA" ] && [ -d "$B/pui-run-live.BBBBBB" ] && ok "a dead run's folder goes, a live run's stays" || bad "dead/live: $out"
case $out in *"removed 1 run folder"*) ok "and it says so" ;; *) bad "no message: $out" ;; esac
rm -rf "$B/pui-run-live.BBBBBB"

# 6. Nearly full base: old run folders (even of a live-looking owner) and old fixed-name test folders go first, it is said,
#    the run starts anyway (never red because of the disk). Recent folders of live runs stay.
mkdir "$B/pui-run-old.CCCCCC" "$B/pui-run-new.DDDDDD" "$B/pui-rigs-cozy-1" "$B/pui-rigs-cozy-2" "$B/other-thing"
sleep 30 & live=$!; echo $live >"$B/pui-run-old.CCCCCC/.owner"; echo $live >"$B/pui-run-new.DDDDDD/.owner"
touch -d '20 hours ago' "$B/pui-run-old.CCCCCC/.owner"; touch -d '5 hours ago' "$B/pui-rigs-cozy-1"
out=$(PUI_TMP_MIN_FREE_MB=999999999 run 'run_tmp_begin t; echo "started:$TMPDIR"')
kill $live 2>/dev/null; wait $live 2>/dev/null
case $out in *"cleaning old run folders first"*"started:$B/pui-run-t."*) ok "a full base: says it cleans, then starts" ;; *) bad "full base: $out" ;; esac
[ ! -d "$B/pui-run-old.CCCCCC" ] && [ -d "$B/pui-run-new.DDDDDD" ] && ok "only the old run folder went" || bad "old/new run folders"
[ ! -d "$B/pui-rigs-cozy-1" ] && [ -d "$B/pui-rigs-cozy-2" ] && [ -d "$B/other-thing" ] && ok "old test leftovers went, fresh and foreign folders stayed" || bad "legacy cleanup"
rm -rf "$B"/pui-run-* "$B/pui-rigs-cozy-2" "$B/other-thing"

# 7. A run inside a run (release-check -> gate) joins it: no second folder, the inner end removes nothing.
out=$(run 'run_tmp_begin outer; outer=$TMPDIR; sh -c ". '"'$HERE/run-tmp.sh'"'; run_tmp_begin inner; [ \"\$TMPDIR\" = \"$outer\" ] && echo joined; run_tmp_end 0; [ -d \"$outer\" ] && echo still-there"')
case $out in *joined*still-there*) ok "a nested run joins the outer one and does not remove it" ;; *) bad "nested: $out" ;; esac
[ "$(runs)" -eq 0 ] && ok "and the outer end removes it" || bad "nested folder left"

# 8. Read-only trees (a test that chmods) are removed too.
run 'run_tmp_begin t; mkdir -p "$TMPDIR/ro/sub"; touch "$TMPDIR/ro/sub/f"; chmod 500 "$TMPDIR/ro/sub" "$TMPDIR/ro"' >/dev/null
[ "$(runs)" -eq 0 ] && ok "read-only leftovers are removed" || bad "read-only tree stayed"
exit $fail

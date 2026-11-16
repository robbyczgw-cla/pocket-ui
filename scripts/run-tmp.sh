# Sourced (`. scripts/run-tmp.sh`) by every script that runs many tests: scripts/gate.sh (merge gate, nightly, release check
# through it), scripts/release-check.sh, scripts/crash-sweep.sh, scripts/preflight.sh and the probe of tools/remote-gate.sh.
#
#   run_tmp_begin <label>   a folder of this run's own below the temp base, TMPDIR pointing at it, removed again when the
#                           script ends (EXIT trap: also on a red run, a kill with TERM/INT/HUP, `exit` anywhere)
#   run_tmp_end [status]    what the EXIT trap does: note what the tests left in it, keep the few files a red run needs,
#                           remove the folder. A script that sets its own EXIT trap calls it from there.
#
# Why: 14.11.2026 the merge trains went red with "write png: Disk quota exceeded": the build machine's /tmp (a 7.3 GB tmpfs) was 82 %
# full of folders the tests had left (8,700 of them). Every test and tool used the shared /tmp, so leftovers piled up across
# runs. Now a run can leave nothing behind, whatever its tests do.
#
#  - Preflight (every begin): runs whose owner is gone (killed, rebooted) are removed. Less than PUI_TMP_MIN_FREE_MB (2048) free
#    in the base: older run folders (PUI_TMP_STALE_HOURS, 12), old kept artifacts and the old fixed-name test folders of
#    earlier versions go too, and it is said. A run is never red because an earlier one filled the disk.
#  - A red run keeps at most 30 small pictures/logs (each under 1 MB) of its folder in <base>/gate-artifacts-<label>/, one set
#    per label (the next red run of that label replaces it; the preflight removes it after a day).
#  - A run that leaves things in TMPDIR is told so ("tests left N entries"): a note, the folder goes anyway.
#  - Inside a run (PUI_RUN_TMP set and still there: release-check.sh -> gate.sh) begin does nothing, the outer run owns it.
# Settings: PUI_TMP_BASE (default /tmp), PUI_TMP_MIN_FREE_MB, PUI_TMP_STALE_HOURS. The label stays short: a Unix socket path of
# the tests (wayland stub) is limited to 107 bytes.

run_tmp_free_mb() {
    df -Pk "${PUI_TMP_BASE:-/tmp}" 2>/dev/null | awk 'NR == 2 { print int($4 / 1024) }'
}

# Removes the run folder $1 (read-only trees included). Prints its size in MB.
run_tmp_remove() {
    mb=$(du -sm "$1" 2>/dev/null | awk '{ print $1 }')
    chmod -R u+rwX "$1" 2>/dev/null
    rm -rf "$1"
    echo "${mb:-0}"
}

# Removes run folders whose owner process is gone. $1 = also those older than N hours whatever their owner says (0 = no).
run_tmp_sweep() {
    base=${PUI_TMP_BASE:-/tmp}
    removed=0 freed=0
    for d in "$base"/pui-run-*; do
        if [ ! -d "$d" ] || [ "$d" = "${PUI_RUN_TMP:-}" ]; then continue; fi
        owner=$(cat "$d/.owner" 2>/dev/null)
        stale=0
        case $owner in
            ''|*[!0-9]*) stale=1 ;;
            *) kill -0 "$owner" 2>/dev/null || stale=1 ;;
        esac
        if [ "$stale" -eq 0 ] && [ "${1:-0}" -gt 0 ] && [ -n "$(find "$d/.owner" -maxdepth 0 -mmin +$(( $1 * 60 )) 2>/dev/null)" ]; then stale=1; fi
        if [ "$stale" -ne 1 ]; then continue; fi
        freed=$(( freed + $(run_tmp_remove "$d") ))
        removed=$(( removed + 1 ))
    done
    RUN_TMP_REMOVED=$removed RUN_TMP_FREED_MB=$freed
}

# The fixed-name test folders earlier versions of the tests left in the base (only ours, only when old).
run_tmp_old_leftovers() {
    base=${PUI_TMP_BASE:-/tmp}
    n=0
    for d in $(find "$base" -maxdepth 1 \( -name 'pui-rigs-*' -o -name 'pui-sticker-*' -o -name 'pocketui-*' -o -name 'pocket-portmaster-*' \
        -o -name 'pocket-games-card-*' -o -name 'device-check*' -o -name 'test-device-check.*' -o -name 'hakux-inst-*' -o -name 'kids-aspect-*' \
        -o -name 'svt[0-9]*' -o -name 'svp[0-9]*' -o -name 'gc[0-9]*-*' -o -name 'snapshot-equivalence-*' -o -name 'no-copy-*' \) -mmin +180 2>/dev/null); do
        chmod -R u+rwX "$d" 2>/dev/null
        rm -rf "$d"
        n=$(( n + 1 ))
    done
    echo "$n"
}

run_tmp_begin() {
    base=${PUI_TMP_BASE:-/tmp}
    if [ -n "${PUI_RUN_TMP:-}" ] && [ -d "$PUI_RUN_TMP" ]; then return 0; fi
    PUI_RUN_TMP=
    label=${1:-run}
    mkdir -p "$base" 2>/dev/null
    run_tmp_sweep 0
    if [ "$RUN_TMP_REMOVED" -ne 0 ]; then echo "run-tmp: removed $RUN_TMP_REMOVED run folder(s) of runs that are gone ($RUN_TMP_FREED_MB MB)"; fi
    min=${PUI_TMP_MIN_FREE_MB:-2048}
    free=$(run_tmp_free_mb)
    if [ -n "$free" ] && [ "$free" -lt "$min" ]; then
        echo "run-tmp: only $free MB free in $base (needs $min): cleaning old run folders first"
        run_tmp_sweep "${PUI_TMP_STALE_HOURS:-12}"
        for a in "$base"/gate-artifacts-*; do if [ -d "$a" ]; then rm -rf "$a"; fi; done
        old=$(run_tmp_old_leftovers)
        free=$(run_tmp_free_mb)
        echo "run-tmp: removed $RUN_TMP_REMOVED old run folder(s) ($RUN_TMP_FREED_MB MB) and $old old test folder(s); $free MB free now"
        if [ "$free" -lt "$min" ]; then echo "run-tmp: still under $min MB free in $base after cleaning; going on (something else fills it)"; fi
    fi
    # Kept artifacts of red runs: one day.
    find "$base" -maxdepth 1 -name 'gate-artifacts-*' -mmin +1440 -exec rm -rf {} + 2>/dev/null
    PUI_RUN_TMP=$(mktemp -d "$base/pui-run-$label.XXXXXX") || { echo "run-tmp: cannot make a run folder in $base; using the shared temp folder"; return 0; }
    echo "$$" >"$PUI_RUN_TMP/.owner"
    PUI_RUN_LABEL=$label PUI_RUN_BASE=$base PUI_RUN_OWNER=$$
    TMPDIR=$PUI_RUN_TMP
    export PUI_RUN_TMP PUI_RUN_LABEL PUI_RUN_BASE PUI_RUN_OWNER TMPDIR
    trap 'run_tmp_end $?' EXIT
    trap 'exit 143' HUP INT TERM
}

run_tmp_end() {
    status=${1:-0}
    # Only the script that began the run ends it (a child inherits the variables, not the right).
    if [ "${PUI_RUN_OWNER:-}" != "$$" ] || [ -z "${PUI_RUN_TMP:-}" ] || [ ! -d "$PUI_RUN_TMP" ]; then return 0; fi
    left=$(find "$PUI_RUN_TMP" -mindepth 1 -maxdepth 1 ! -name .owner 2>/dev/null | wc -l)
    if [ "$left" -gt 0 ]; then
        echo "run-tmp: the run left $left entr$([ "$left" -eq 1 ] && echo y || echo ies) in its TMPDIR (removed): $(find "$PUI_RUN_TMP" -mindepth 1 -maxdepth 1 ! -name .owner -printf '%f\n' 2>/dev/null | sed -E 's/[A-Za-z0-9]{6}$|[0-9]+$/*/' | sort | uniq -c | sort -rn | head -8 | awk '{ printf "%s%s x%s", sep, $2, $1; sep = ", " }')"
    fi
    if [ "$status" -ne 0 ] && [ "$left" -gt 0 ]; then
        keep="$PUI_RUN_BASE/gate-artifacts-$PUI_RUN_LABEL"
        rm -rf "$keep"
        files=$(find "$PUI_RUN_TMP" -type f \( -name '*.png' -o -name '*.jpg' -o -name '*.log' -o -name '*.txt' -o -name '*.json' \) -size -1024k 2>/dev/null | head -30)
        if [ -n "$files" ]; then
            mkdir -p "$keep" 2>/dev/null
            printf '%s\n' "$files" | while read -r f; do
                cp "$f" "$keep/$(echo "${f#"$PUI_RUN_TMP"/}" | tr '/' '_')" 2>/dev/null
            done
            echo "run-tmp: red run: up to 30 small files kept in $keep"
        fi
    fi
    run_tmp_remove "$PUI_RUN_TMP" >/dev/null
    PUI_RUN_TMP=
}

#!/bin/sh
# Release check: the checks that protect players but no longer block a merge (TEST-AUDIT-2026-11-04.md). Run it before a
# release, on the release candidate's tree:  sh scripts/release-check.sh
# They are #[ignore = "release check: ..."] tests, so the merge gate skips them and this runs them:
#   - every string of src/ has a German line (fix: scripts/i18n-extract.py --stub)
#   - the German and Spanish snapshot matrix shows no untranslated words
#   - every language has every line: Spanish, Portuguese and French complete (fill the gaps in one commit; scripts/bookkeeping.sh lists them)
#   - every "Settings › A › B" path in docs, strings and tables names a real category and row
#   - each category's line under Settings names its rows in order
#   - the D-pad explorer (tests/nav_explorer.rs) at the sizes and text size the gate leaves out
#   - the programs still fit into the SD image's 128 MiB STORAGE (ROCKNIX's 32 MiB, grown by build.sh) (scripts/image/storage-budget.sh; too little growth room is only a NOTE). It needs
#     the AArch64 release programs: POCKET_STORAGE_BIN_DIR=<folder with pocket-ui, pocket-core, pocket-splash>, or a build in
#     target/aarch64-unknown-linux-gnu*/release. Without them it fails: a release must not outgrow the image silently (0.3.1 did).
# Bookkeeping no player notices (documented variables and modules, the docs index, code-shape caps, manual page length) is
# printed as warnings by scripts/bookkeeping.sh at the end; it does not fail the check.
# Placeholder integrity, German complete, language choice, clipping, saves, launch, quit, update and input stay in the gate.
# First, the complete gate (GATE_FULL=1: every suite, the timing tier, the Brick corpus and UI walk, Android, the
# German/Spanish matrix, the shell suites). The merge gate runs a fast subset since 2026-11-10; the nightly run on main
# and this check run all of it. RELEASE_CHECK_FULL_GATE=0 skips it only when that exact tree already passed a full gate.
# RELEASE_CHECK_RECEIPT=<file>: when the check is green, write a receipt (kind=release-check result=ok commit tree full
# volume_sha256) to it. RELEASE_CHECK_TEXT_ONLY=<receipt> (a text-only fix after a green release check, docs/dev/release.md):
# the change since that receipt's full run must be text only (scripts/text-only-diff.py), else the check stops. Then the full
# gate is not run again (it passed on that commit; a text change does not reach it, the text checks below do) and the
# volume evidence must match the binary that check verified (only text changed since). Both are printed, never silent.
cd "$(dirname "$0")/.." || exit 2
# Its own TMPDIR for the whole check (the full gate below joins it), removed at the end: scripts/run-tmp.sh
# (the fixture trees of scripts/test-release-fast.sh have no copy of it)
if [ -f scripts/run-tmp.sh ]; then . scripts/run-tmp.sh; run_tmp_begin rc; fi
rv() { sed -n "s/^$2=//p" "$1" | head -1; }   # a value of a receipt
TEXT_ONLY=${RELEASE_CHECK_TEXT_ONLY:-} FULL_COMMIT=$(git rev-parse HEAD 2>/dev/null) VOLUME_SHA=
if [ -n "$TEXT_ONLY" ]; then
    [ -s "$TEXT_ONLY" ] && [ "$(rv "$TEXT_ONLY" kind)" = release-check ] && [ "$(rv "$TEXT_ONLY" result)" = ok ] \
        || { echo "release check: RELEASE_CHECK_TEXT_ONLY=$TEXT_ONLY is not the receipt of a green release check"; echo "RELEASE CHECK: FAILED (text-only)"; exit 1; }
    FULL_COMMIT=$(rv "$TEXT_ONLY" full); VOLUME_SHA=$(rv "$TEXT_ONLY" volume_sha256)
    echo "release check: TEXT-ONLY since the full release check of $(printf %.12s "$FULL_COMMIT"):"
    python3 -I scripts/text-only-diff.py "$FULL_COMMIT" HEAD | sed 's/^/  /'
    python3 -I scripts/text-only-diff.py "$FULL_COMMIT" HEAD >/dev/null \
        || { echo "RELEASE CHECK: FAILED (not a text-only change since $(printf %.12s "$FULL_COMMIT"): run the whole check, without RELEASE_CHECK_TEXT_ONLY)"; exit 1; }
    [ -n "$VOLUME_SHA" ] || { echo "RELEASE CHECK: FAILED (the receipt names no verified volume binary)"; exit 1; }
fi
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
BUDGET_BIN=${POCKET_STORAGE_BIN_DIR:-}
if [ -z "$BUDGET_BIN" ]; then
    for d in target/aarch64-unknown-linux-gnu*/release; do [ -s "$d/pocket-ui" ] && BUDGET_BIN=$d; done
fi
if [ -z "$BUDGET_BIN" ]; then
    echo "release check: no AArch64 programs to measure for the SD image's STORAGE budget: set POCKET_STORAGE_BIN_DIR to the release programs' folder"
    echo "RELEASE CHECK: FAILED"; exit 1
fi
sh scripts/image/storage-budget.sh "$BUDGET_BIN" || { echo "RELEASE CHECK: FAILED (STORAGE budget)"; exit 1; }
# Hardware evidence is made on an explicitly allocated idle device through
# device-check.sh --volume; the build host need not have access to that device.
# It must be the exact release binary, not an earlier good build or a unit fake.
if [ -z "${POCKET_VOLUME_CHECK_EVIDENCE:-}" ]; then
    echo "release check: missing H700 volume curve evidence; run scripts/device-check.sh --volume (docs/DEVICE-TESTS.md)"
    echo "RELEASE CHECK: FAILED (device volume curve)"; exit 1
fi
if [ -n "$TEXT_ONLY" ]; then
    echo "release check: TEXT-ONLY: the volume evidence must be of the binary the full check verified ($(printf %.12s "$VOLUME_SHA"), commit $(printf %.12s "$FULL_COMMIT")); only text changed since"
    python3 scripts/volume-device-check.py --verify-evidence "$POCKET_VOLUME_CHECK_EVIDENCE" \
        --release-sha "$VOLUME_SHA" || { echo "RELEASE CHECK: FAILED (device volume curve)"; exit 1; }
else
    python3 scripts/volume-device-check.py --verify-evidence "$POCKET_VOLUME_CHECK_EVIDENCE" \
        --release-binary "$BUDGET_BIN/pocket-ui" || { echo "RELEASE CHECK: FAILED (device volume curve)"; exit 1; }
    VOLUME_SHA=$(sha256sum "$BUDGET_BIN/pocket-ui" | cut -d' ' -f1)
fi
echo "release check: Brick and Retroid volume profiles TODO (capture their own mixer/DAPM paths)"
if [ -n "$TEXT_ONLY" ]; then
    echo "release check: TEXT-ONLY: full gate not run again (green on $(printf %.12s "$FULL_COMMIT")); the text checks below run"
elif [ "${RELEASE_CHECK_FULL_GATE:-1}" != 0 ]; then
    GATE_FULL=1 sh scripts/gate.sh "$(pwd)" || { echo "RELEASE CHECK: FAILED (full gate, GATE_FULL=1)"; exit 1; }
fi
python3 scripts/doc-missing.py || echo "release check: documentation is missing entries (above)"
# The same list as scripts/pre-release.sh (scripts/release-text-tests.txt), so a pre-release step that was green finds it all.
# shellcheck disable=SC2046
cargo test --profile "${GATE_PROFILE:-gate}" --lib --bins --test i18n_snapshots -q -- --include-ignored \
    $(grep -v '^#' scripts/release-text-tests.txt) \
    && cargo test --profile "${GATE_PROFILE:-gate}" --test nav_explorer -q -- --ignored \
    || { echo "RELEASE CHECK: FAILED"; exit 1; }
sh scripts/bookkeeping.sh || { echo "RELEASE CHECK: FAILED (bookkeeping did not build)"; exit 1; }
if [ -n "${RELEASE_CHECK_RECEIPT:-}" ]; then
    { echo "kind=release-check"; echo "result=ok"; echo "mode=$([ -n "$TEXT_ONLY" ] && echo text-only || echo full)"
      echo "commit=$(git rev-parse HEAD)"; echo "tree=$(git rev-parse 'HEAD^{tree}')"; echo "full=$FULL_COMMIT"
      echo "volume_sha256=$VOLUME_SHA"; echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"; } >"$RELEASE_CHECK_RECEIPT" \
        || { echo "RELEASE CHECK: FAILED (cannot write $RELEASE_CHECK_RECEIPT)"; exit 1; }
    echo "release check: receipt $RELEASE_CHECK_RECEIPT"
fi
echo "RELEASE CHECK: ok"

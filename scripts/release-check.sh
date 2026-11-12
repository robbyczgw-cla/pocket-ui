#!/bin/sh
# Release check: the bookkeeping tests that no longer block a merge (TEST-AUDIT-2026-11-04.md). Run it before a
# release, on the release candidate's tree:  sh scripts/release-check.sh
# They are #[ignore = "release check: ..."] tests, so the merge gate skips them and this runs them:
#   - every string of src/ has a German line (fix: scripts/i18n-extract.py --stub)
#   - the German and Spanish snapshot matrix shows no untranslated words
#   - every environment variable and source module is documented (fix: scripts/doc-missing.py)
#   - every "Settings › A › B" path in docs, strings and tables names a real category and row
#   - each category's line under Settings names its rows in order
#   - the manual's pages are not too long for the screen (editorial advice)
#   - the D-pad explorer (tests/nav_explorer.rs) at the sizes and text size the gate leaves out
#   - the programs still fit into the SD image's 32 MiB STORAGE with room to grow (scripts/image/storage-budget.sh). It needs
#     the AArch64 release programs: POCKET_STORAGE_BIN_DIR=<folder with pocket-ui, pocket-core, pocket-splash>, or a build in
#     target/aarch64-unknown-linux-gnu*/release. Without them it fails: a release must not outgrow the image silently (0.3.1 did).
# Placeholder integrity, language choice, clipping, saves, launch, quit, update and input stay in the gate.
# First, the complete gate (GATE_FULL=1: every suite, the timing tier, the Brick corpus and UI walk, Android, the
# German/Spanish matrix, the shell suites). The merge gate runs a fast subset since 2026-11-10; the nightly run on main
# and this check run all of it. RELEASE_CHECK_FULL_GATE=0 skips it only when that exact tree already passed a full gate.
cd "$(dirname "$0")/.." || exit 2
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
if [ "${RELEASE_CHECK_FULL_GATE:-1}" != 0 ]; then
    GATE_FULL=1 sh scripts/gate.sh "$(pwd)" || { echo "RELEASE CHECK: FAILED (full gate, GATE_FULL=1)"; exit 1; }
fi
python3 scripts/doc-missing.py || echo "release check: documentation is missing entries (above)"
cargo test --profile "${GATE_PROFILE:-gate}" --lib --bins --test i18n_snapshots -q -- --include-ignored \
    every_string_in_the_source_has_a_german_line_or_is_listed \
    every_string_in_the_german_snapshots_is_translated_or_a_name \
    every_string_in_the_spanish_snapshots_is_translated_or_a_name \
    developing_md_names_every_source_module \
    every_environment_variable_is_documented \
    settings_paths_on_the_screen_use_the_glossarys_arrow \
    every_path_in_settings_names_a_category_and_one_of_its_rows \
    every_category_line_names_its_rows_in_order \
    manual_pages_are_not_too_long_for_the_screen \
    && cargo test --profile "${GATE_PROFILE:-gate}" --test nav_explorer -q -- --ignored \
    && echo "RELEASE CHECK: ok" || { echo "RELEASE CHECK: FAILED"; exit 1; }

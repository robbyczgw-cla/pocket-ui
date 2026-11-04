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
# Placeholder integrity, language choice, clipping, saves, launch, quit, update and input stay in the gate.
cd "$(dirname "$0")/.." || exit 2
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
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
    && echo "RELEASE CHECK: ok" || { echo "RELEASE CHECK: FAILED"; exit 1; }

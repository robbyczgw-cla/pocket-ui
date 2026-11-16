# Releasing

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you cut a release or build release assets.*

## Releasing the app

For the public channel: `scripts/release-channel.sh --repo <owner>/pocket-ui --in release-out` makes and signs the list
from the release artefacts and writes `site-updates/` for the website, `--check` compares the uploaded files with it
([UPDATE-CHANNEL.md](../UPDATE-CHANNEL.md#releasing-step-by-step)). For a channel of your own,
`scripts/release-app.sh <version> <out-dir> "notes"` builds with that version, packs a tarball and adds it to a
signed `update.json`. Signing needs the private key (default `~/.config/pocket-ui/store-signing.key`, never
committed). The public key is compiled into `src/store.rs`. See [UPDATES.md](../UPDATES.md) and [STORE.md](../STORE.md).

Before a release, first fill missing translations: `sh scripts/bookkeeping.sh` lists missing es/pt lines (and the other
bookkeeping warnings); one agent fills them in one commit before the release check. Then run `sh scripts/release-check.sh`
(the release checks the merge gate skips, see [Documentation checks](tests.md#documentation-checks)): it fails on a
missing Spanish or Portuguese line and prints the bookkeeping as warnings only.

### Order: text checks first, export once

The public export (`scripts/export-public.sh`) runs every test of the tree and takes about an hour; a text find after it
costs another hour. So the text checks come first, on the release branch, before any export:

1. `sh scripts/pre-release.sh --receipt pre.txt`: the translation audit, `--lib --bins` complete (a test red only side by
   side gets one run alone), the release check's text tests (`scripts/release-text-tests.txt`), the text-fit suites with
   their release matrices (Large text in every language on every panel size), the Brick's Settings walk and text cases,
   the D-pad explorer at every size, and the bookkeeping as warnings. Each step prints `ok`, `RED` or `WARN` and its log;
   a red step does not stop the others, so one run lists everything. Fix on the branch and run it again until it is green.
2. Export with `--receipt export.txt`, then the release programs, then `RELEASE_CHECK_RECEIPT=rc.txt sh scripts/release-check.sh`.

### Text-only fixes

A fix after a green export that changes only text (translation tables `assets/i18n/*`, `docs/**`, the top-level Markdown
files; `python3 scripts/text-only-diff.py BASE [TARGET]` decides and prints every file) does not need the whole test run again:

1. `sh scripts/pre-release.sh --since export.txt --receipt pre2.txt`: refuses when anything else changed since the full
   run; runs the text checks again (text fit, Brick and explorer only when a translation table or manual page changed, and
   prints why a step is skipped).
2. `scripts/export-public.sh … --text-only-since export.txt --pre-release pre2.txt --receipt export2.txt`: the private-data
   scan, the release-tree and licence checks as always; no cargo build/test, the report says `TEXT  cargo test  TEXT-ONLY
   since …` with the number of changed files and the pre-release steps.
3. New release programs from the public tree (they hold the text), then `RELEASE_CHECK_TEXT_ONLY=rc.txt
   RELEASE_CHECK_RECEIPT=rc2.txt sh scripts/release-check.sh`: no second full gate; the STORAGE budget, the text tests, the
   explorer and the bookkeeping run; the volume evidence must be the one of the binary the full check verified (under 24 h).

A code change, a test or a script in between means the full order again. The receipts are small `key=value` files;
`scripts/test-release-fast.sh` tests the refusals.

Its device prerequisite is the silent H700 volume curve from `scripts/device-check.sh --volume`: deploy the
exact release binary, explicitly allocate the idle device, record `volume-proof.json`, then set
`POCKET_VOLUME_CHECK_EVIDENCE` to that file for the release check. It checks the real DAPM signal path,
10..100% within ±1.5 dB, exact restoration and a matching binary hash; [procedure and limits](../DEVICE-TESTS.md#silent-release-volume-curve).
Brick and Retroid volume profiles remain explicit TODOs until their own mixer topologies are captured.

The public channel is `https://pocketui.xyz/updates/stable/update.json` (`update::DEFAULT_URL`), with the files on
GitHub Releases; `scripts/release-channel.sh` makes and signs its list, nothing is published yet
([UPDATE-CHANNEL.md](../UPDATE-CHANNEL.md)).

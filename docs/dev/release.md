# Releasing

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you cut a release or build release assets.*

## Releasing the app

For the public channel: `scripts/release-channel.sh --repo <owner>/pocket-ui --in release-out` makes and signs the list
from the release artefacts and writes `site-updates/` for the website, `--check` compares the uploaded files with it
([UPDATE-CHANNEL.md](../UPDATE-CHANNEL.md#releasing-step-by-step)). For a channel of your own,
`scripts/release-app.sh <version> <out-dir> "notes"` builds with that version, packs a tarball and adds it to a
signed `update.json`. Signing needs the private key (default `~/.config/pocket-ui/store-signing.key`, never
committed). The public key is compiled into `src/store.rs`. See [UPDATES.md](../UPDATES.md) and [STORE.md](../STORE.md).

Before a release run `sh scripts/release-check.sh` (the bookkeeping tests the merge gate skips, see
[Documentation checks](tests.md#documentation-checks)).

The public channel is `https://pocketui.xyz/updates/stable/update.json` (`update::DEFAULT_URL`), with the files on
GitHub Releases; `scripts/release-channel.sh` makes and signs its list, nothing is published yet
([UPDATE-CHANNEL.md](../UPDATE-CHANNEL.md)).

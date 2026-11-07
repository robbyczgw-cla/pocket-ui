# Compile-time isolation of the Brick build

The `brick` Cargo feature selects the TrimUI implementation at compile time. A build without
features must retain the ROCKNIX implementation, including its embedded assets. Returning
`false` from a shared `brick_card()` helper is insufficient: reading environment variables,
changing a struct or enum, or calling a new wrapper can still change the standard binary.

## Ownership

- `untar.rs` keeps the original in-memory parser for standard builds. `untar/brick.rs` holds
  streaming tar/gzip extraction and FAT-compatible file creation; its module and exports
  compile only with `brick`. The Brick parser and its refusal/cleanup tests remain intact.
- `update/brick.rs` holds atomic replacement of the card's app directory, space checks,
  rollback markers and program validation. `update.rs` compiles the card installer, private
  channel environment and Tailscale proxy only with `brick`; the standard fetch/download
  paths retain the public updater's implementation.
- SSH markers, Tailscale CLI/socket overrides, loopback admission for userspace networking,
  F keys, motor state, battery percentages and load diagnostics are selected with `cfg`.
  Standard pad/hotkey/rumble/sensor layouts retain their original fields and variants.
- The two translation tables in `assets/i18n/` remain the standard tables. Only a Brick build
  appends `brick-de.tsv` and `brick-es.tsv`; their key/value union preserves the former Brick
  table. Its phone upload page uses `assets/phone-upload/brick-app.js`; the standard page
  keeps `app.js` and does not offer cover uploads.
- The card's own sleep and heat guard (update 5: `heat.rs`, `Sleep::spawn_heat_guard`, `Msg::Hot`/`Why::Hot`, the power-key-only
  wake, `Power::set_dark` and `Extras::dark` for a game behind a dark screen) and the beta version text (`update::brick_short`,
  `brick_long`) compile only with `brick`.
- The manual's include-only card variants live in `docs/user/brick-*.md.inc`. `manual.rs` includes those only
  with `brick`; standard builds embed the unchanged pages under `docs/user/`.

Small alternative functions deliberately retain the original standard implementation rather
than routing standard calls through a new Brick-aware helper. Shared fixes should be reviewed
as separate standard-build changes before replacing either implementation.

## Verification baseline and status

The reference is main at `f3374b0b`; this branch contains it (merge `08516a39`).

## Binary comparison

`pocket-ui`, `pocket-core` and `pocket-splash` were built twice on one machine, one after the other, from
the same source path (a scratch folder, `git archive` of each tree), toolchain 1.99.0,
`cargo zigbuild --locked --release --target aarch64-unknown-linux-gnu.2.38` with
`scripts/release-rustflags.sh`, the same `CARGO_TARGET_DIR` and the same `POCKETUI_VERSION` and
`POCKETUI_COMMIT`. One normalisation, applied to both trees: `build.rs` stores a fixed `POCKETUI_BUILD_UNIX`
instead of the clock. No feature flags in either build.

Result (`scripts/brick-cfg-compare.py <reference> <branch>`): the files have the same size and `.text`,
`.rodata`, `.data`, `.eh_frame`, the symbol-relevant dynamic sections and the headers are byte-identical.
The only differing bytes are in `.data.rel.ro`: 333 words in `pocket-ui`, 45 in `pocket-core`, 17 in
`pocket-splash`. Each is the line number of a `core::panic::Location`; the column is unchanged in every
one. The cfg attributes and `mod` lines added to shared files move source lines, so the line numbers in the
panic locations move; no instruction, string or data differs. Exact byte identity would need the branch's
shared files to keep every line number, which the cfg attributes make impossible.

An earlier run against a newer main (which had other changes) showed real code differences; that was the
wrong reference, not a branch difference. Always build the reference from the commit merged into the branch.

## Status

Host `cargo check --locked --all-targets` passes with and without `--features brick`.
Brick suite (`cargo test --profile gate --features brick`, build host): 3422 passed, 0 failed.
`scripts/test-brick-card.sh`: all passed. Full default gate on the build host with `GATE_LEAKS_DONE=1`: ok (3332 passed).
The gate ran with `TMPDIR` on the disk: `phone_upload::tests::too_large_or_no_room_is_said_before_a_byte_is_written`
reads the free space of the 1 GB `/tmp` tmpfs twice and fails when anything there is deleted between the two reads
(not related to this branch; with a disk `TMPDIR` the disc is larger than any ROM and the answer is fixed).

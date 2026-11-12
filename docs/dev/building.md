# Building and tools

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you build, cross-build or package pocket-ui / pocket-core, or need the binaries table.*

## Building

The device runs ROCKNIX with glibc 2.41. Link against an older glibc so a binary does not depend on a symbol the
image lacks:

```sh
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core
```

The output is in `target/aarch64-unknown-linux-gnu/release/`.

Release uses full LTO with one codegen unit, opt-level 3, stripped symbols and aborting panics. The gate overrides
LTO and codegen units for build speed. Size attribution, the alternatives considered, and the measured runtime
comparison are in [BINARY-SIZE.md](../BINARY-SIZE.md); do not optimise the shared core/audio code for size alone.

The scripts that build release binaries (`dev.sh deploy`, `vdev.sh deploy`, `release-app.sh`, `make-image.sh`,
`make-install-zip.sh`, `syscheck.sh`, the build check of `export-public.sh`) first source `scripts/release-rustflags.sh`.
It adds `--remap-path-prefix` for the repository (`/pocket-ui`), the cargo home (`/cargo`) and the Rust toolchain
(`/rustup`), so panic locations in the binaries do not carry the build user's home folder. Only release builds use it;
debug, test and gate builds are unchanged. When you run `cargo zigbuild --release` by hand for something that ships, run
`. scripts/release-rustflags.sh` first. `scripts/check-binary-paths.sh <binary>...` fails if a binary still contains
`/home/`, `/Users/` or the build paths; `make-image.sh`, `make-install-zip.sh`, `release-app.sh` and `export-public.sh` run it.
(`profile.trim-paths` would do the same in `Cargo.toml`, but it is still nightly-only in cargo 1.99.) Do not use
`env!("CARGO_MANIFEST_DIR")` outside `#[cfg(test)]`: the compiled-in string is not remapped.

### A new ROCKNIX version

pocket-ui depends on ROCKNIX's scripts, settings and libraries, not only on its kernel. Before moving the device (or
our own image) to a newer ROCKNIX, compare the two:

```sh
scripts/rocknix-diff.sh 20261001 origin/next          # two ROCKNIX git refs: the source tree
scripts/rocknix-diff.sh old/SYSTEM new/ROCKNIX-SM8250.aarch64-20261101.tar   # two images (tar, SYSTEM or folder)
scripts/rocknix-diff.sh -v 20261001 origin/next       # with the diff of every changed script
```

It lists what changed in what pocket-ui depends on (the UI-start scripts the launcher repeats, the scripts it calls,
ROCKNIX's shell functions, `system.cfg` defaults, es_systems entries, cores, standalone start scripts, the SM8250 and
Mini V2 quirks, library and package versions) and ends with a checklist; the steps are in
[UPDATES.md](../UPDATES.md#a-rocknix-bump). Refs come from a clone in `~/.cache/rocknix-distribution` (`ROCKNIX_SRC`),
made on first use; images need `unsquashfs` (`brew install squashfs`) or Docker with the virtual device's tool image.

Then check the new system against everything pocket-ui relies on, one system at a time:

```sh
unsquashfs -d /tmp/rocknix-root SYSTEM                   # the SYSTEM file of the image's first partition or update tar
scripts/rocknix-contract.sh --root /tmp/rocknix-root     # an unpacked (or mounted) root
scripts/rocknix-contract.sh --ref origin/next            # ROCKNIX's source tree (only the rows it can map)
scripts/rocknix-contract.sh --host root@<device>         # a running device, read-only, over one SSH session
```

One line per promise (the lock style of `set_setting`, the functions `start_es.sh` calls, the arguments `runemu.sh`
parses, es_systems folders and extensions, cores, services, ...): `ok`, `CHANGED` or `MISSING` with the evidence and
the code that relies on it, then `CONTRACT: ok` (exit 0) or `CONTRACT: <n> changed` (exit 1). The rows are
`scripts/rocknix-contract.txt`, explained in [ROCKNIX-CONTRACT.md](../ROCKNIX-CONTRACT.md); `cargo test` keeps them in
step with the code and runs the script's self-test (`scripts/test-rocknix-contract.sh`).

The package has five binaries:

| Binary | Source | What it is |
|---|---|---|
| `pocket-ui` | `src/main.rs` | The shelf, Settings, first-start setup, the pause menu |
| `pocket-core` | `src/bin/pocket_core.rs` | The libretro host that runs a game |
| `pocket-splash` | `src/bin/pocket_splash.rs` | The boot screen on the frame buffer from systemd's start until sway takes over; `--snapshot` renders a frame without a screen ([BOOT-SPLASH.md](../BOOT-SPLASH.md)) |
| `store-tool` | `src/bin/store_tool.rs` | Signs catalogs and update lists, checks a signature (`verify`), lists the files of an update list (`list`), prints file hashes. Runs on your computer |
| `pocket-fb` | `src/bin/pocket_fb.rs` | Only with `--features brick` ([TRIMUI-BRICK.md](../TRIMUI-BRICK.md)): Wayland and sway's IPC on a bare framebuffer |
| `brick-egl` | `src/bin/brick_egl.rs` | Only with `--features brick`: the Brick's GPU test (`--probe`, `--panel`) |
| `hello-pad` | `src/bin/hello_pad.rs` | A small program that shows button presses, for checking input |

`cargo build` and `cargo test` also work on macOS, where the Linux-only parts (evdev input, the gamma protocol) are
compiled out. That is enough for unit tests and for rendering screenshots.

`POCKETUI_VERSION=<version>` at build time sets the version that `pocket-ui --version` and the Updates page
report; without it the version from `Cargo.toml` is used. `POCKETUI_COMMIT=<sha>` names the commit in Report a
problem's `system.txt`; `scripts/release-app.sh`, `make-image.sh`, `make-install-zip.sh` and `dev.sh deploy` set it.

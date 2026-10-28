# Sourced by the scripts that build release binaries (dev.sh, vdev.sh, make-image.sh, make-install-zip.sh,
# release-app.sh, syscheck.sh, export-public.sh): `. "$ROOT/scripts/release-rustflags.sh"`.
# Maps the build machine's folders to fixed names, so panic locations and debug strings in the binaries do not
# carry the build user's home folder (the account name): the repository -> /pocket-ui, the cargo registry and
# git checkouts -> /cargo, the Rust toolchain -> /rustup. `--remap-path-prefix` is stable; `trim-paths` is not
# (cargo 1.99). Only release builds use it, so debug and gate builds keep their incremental caches.
# Check the result with scripts/check-binary-paths.sh.
_rr_root=${REMAP_ROOT:-$(cd "$(dirname "${BASH_SOURCE:-$0}")/.." && pwd -P)}   # REMAP_ROOT: a copy of the tree built elsewhere
_rr_cargo=${CARGO_HOME:-$HOME/.cargo}
_rr_rustup=${RUSTUP_HOME:-$HOME/.rustup}
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$_rr_root=/pocket-ui --remap-path-prefix=$_rr_cargo=/cargo --remap-path-prefix=$_rr_rustup=/rustup"
# The C shim (build.rs, src/log_shim.c) records __FILE__ through the C compiler.
CFLAGS="${CFLAGS:+$CFLAGS }-ffile-prefix-map=$_rr_root=/pocket-ui"
export RUSTFLAGS CFLAGS
unset _rr_root _rr_cargo _rr_rustup

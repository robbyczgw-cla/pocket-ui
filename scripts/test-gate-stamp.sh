#!/bin/sh
# Proof for gate.sh's GATE_SOURCES_STAMPED=1 (tools/remote-gate.sh): the caller's `rsync -a --no-times --omit-dir-times
# --checksum --delete` stamps exactly the files whose content differs, with the time of the copy, so cargo rebuilds
# every changed file and nothing else, without gate.sh touching all of src/, tests/, examples/ and assets/ first.
# A tiny crate (no dependencies, offline) stands in for pocket-ui: a changed source, a changed include_str! data file and a
# changed document OUTSIDE the old touch list (docs/) must show in the built program, even when the changed file carries an
# OLDER time than the shared target's last build, and when two target slots alternate. An unchanged tree must not recompile.
set -eu
for tool in cargo rsync; do command -v "$tool" >/dev/null 2>&1 || { echo "FAIL gate stamping: $tool is not installed"; exit 1; }; done
T=$(mktemp -d "${TMPDIR:-/tmp}/gate-stamp.XXXXXX")
trap 'rm -rf "$T"' EXIT HUP INT TERM
WT=$T/wt DST=$T/gate/train-1
mkdir -p "$WT/src" "$WT/data" "$WT/docs" "$DST"
cat >"$WT/Cargo.toml" <<'TOML'
[package]
name = "stamp"
version = "0.1.0"
edition = "2021"
TOML
printf 'mod m;\nfn main() { print!("{}|{}|{}", include_str!("../data/v.txt"), m::s(), include_str!("../docs/note.md")); }\n' >"$WT/src/main.rs"
printf 'pub fn s() -> &'"'"'static str { "src-1" }\n' >"$WT/src/m.rs"
printf 'data-1' >"$WT/data/v.txt"
printf 'doc-1' >"$WT/docs/note.md"
sync_tree() {
    rsync -a --no-times --omit-dir-times --checksum --delete "$WT"/ "$DST"/ || { echo "FAIL rsync"; exit 1; }
}
build() {  # build TARGET_DIR -> prints the program's output
    ( cd "$DST" && CARGO_TARGET_DIR=$1 cargo build --offline --locked >"$T/build.log" 2>&1 || CARGO_TARGET_DIR=$1 cargo build --offline >"$T/build.log" 2>&1 ) \
        || { echo "FAIL cargo build"; cat "$T/build.log"; exit 1; }
    "$1/debug/stamp"
}
expect() {  # expect LABEL TARGET_DIR EXPECTED
    got=$(build "$2")
    [ "$got" = "$3" ] || { echo "FAIL $1: built '$got', the tree says '$3'"; exit 1; }
    echo "ok $1"
}
old() { touch -d 2020-01-01 "$1"; }   # an edit that carries a time BEFORE the target's last build
sync_tree
expect "first build" "$T/slot-a" "data-1|src-1|doc-1"
# 1. a changed source file, older than the build
printf 'pub fn s() -> &'"'"'static str { "src-2" }\n' >"$WT/src/m.rs"; old "$WT/src/m.rs"
sync_tree; expect "changed source with an old time is rebuilt" "$T/slot-a" "data-1|src-2|doc-1"
# 2. a changed include_str! data file
printf 'data-2' >"$WT/data/v.txt"; old "$WT/data/v.txt"
sync_tree; expect "changed data file with an old time is rebuilt" "$T/slot-a" "data-2|src-2|doc-1"
# 3. a changed document outside src/tests/examples/assets (the old touch never reached it)
printf 'doc-2' >"$WT/docs/note.md"; old "$WT/docs/note.md"
sync_tree; expect "changed document outside the old touch list is rebuilt" "$T/slot-a" "data-2|src-2|doc-2"
# 4. nothing changed: nothing recompiles
sync_tree
( cd "$DST" && CARGO_TARGET_DIR=$T/slot-a cargo build --offline >"$T/again.log" 2>&1 ) || { cat "$T/again.log"; exit 1; }
if grep -q 'Compiling stamp' "$T/again.log"; then echo "FAIL an unchanged tree recompiled"; cat "$T/again.log"; exit 1; fi
echo "ok an unchanged tree recompiles nothing"
# 5. two target slots take turns (slot B is built from a later tree, then slot A meets a tree it has never seen)
printf 'data-3' >"$WT/data/v.txt"; old "$WT/data/v.txt"
sync_tree; expect "second slot, first build" "$T/slot-b" "data-3|src-2|doc-2"
printf 'pub fn s() -> &'"'"'static str { "src-3" }\n' >"$WT/src/m.rs"; old "$WT/src/m.rs"
sync_tree; expect "second slot sees the next change" "$T/slot-b" "data-3|src-3|doc-2"
expect "first slot, several changes behind" "$T/slot-a" "data-3|src-3|doc-2"
# 6. a file changed back to an earlier content
printf 'data-1' >"$WT/data/v.txt"; old "$WT/data/v.txt"
sync_tree; expect "a file changed back is rebuilt" "$T/slot-a" "data-1|src-3|doc-2"
expect "and in the other slot" "$T/slot-b" "data-1|src-3|doc-2"
# 7. a deleted and a new module
printf 'pub fn s() -> &'"'"'static str { "src-4" }\n' >"$WT/src/m2.rs"; printf 'mod m2;\nfn main() { print!("{}|{}|{}", include_str!("../data/v.txt"), m2::s(), include_str!("../docs/note.md")); }\n' >"$WT/src/main.rs"; rm "$WT/src/m.rs"; old "$WT/src/main.rs"
sync_tree; expect "a new module replaces an old one" "$T/slot-a" "data-1|src-4|doc-2"
# Control: the same edit copied WITH its file time (the old 30.10. setup) is not seen by cargo. If this ever rebuilt, the
# checks above would prove nothing about the stamping.
CTL=$T/gate/control
mkdir -p "$CTL"
printf 'mod m2;\nfn main() { print!("{}|{}|{}", include_str!("../data/v.txt"), m2::s(), include_str!("../docs/note.md")); }\n' >"$WT/src/main.rs"
printf 'pub fn s() -> &'"'"'static str { "src-5" }\n' >"$WT/src/m2.rs"; old "$WT/src/m2.rs"
rsync -a --checksum --delete "$WT"/ "$CTL"/
( cd "$CTL" && CARGO_TARGET_DIR=$T/slot-c cargo build --offline >/dev/null 2>&1 )
printf 'pub fn s() -> &'"'"'static str { "src-6" }\n' >"$WT/src/m2.rs"; old "$WT/src/m2.rs"
touch -d 2020-01-01 "$WT/src/m2.rs"
rsync -a --checksum --delete "$WT"/ "$CTL"/
( cd "$CTL" && CARGO_TARGET_DIR=$T/slot-c cargo build --offline >/dev/null 2>&1 )
[ "$("$T/slot-c/debug/stamp")" = "data-1|src-5|doc-2" ] || { echo "FAIL control: a copy that keeps file times was rebuilt, so this test cannot tell"; exit 1; }
echo "ok control: copying the file time along leaves cargo on the old content (why --no-times)"
echo "gate stamping: all checks passed"

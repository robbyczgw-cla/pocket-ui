#!/bin/sh
# What changed between two ROCKNIX versions in the parts pocket-ui depends on, as a checklist for a ROCKNIX bump
# (docs/UPDATES.md, "A ROCKNIX bump"): the UI-start scripts the launcher repeats (start_es.sh, es_settings), the scripts
# pocket-ui calls or imitates, ROCKNIX's shell functions, system.cfg keys, es_systems entries, libretro cores, start
# scripts of standalone emulators, the SM8250 and Mini V2 quirks, and the versions of the libraries pocket-ui loads.
#
#   scripts/rocknix-diff.sh [-v] <old> <new>
#
# <old> and <new> are of one kind:
#   - ROCKNIX git refs of github.com/ROCKNIX/distribution (a tag such as 20261001, a commit, origin/next): compared in
#     the source tree. The clone is ROCKNIX_SRC (default ~/.cache/rocknix-distribution), made on first use without
#     file contents (they come as needed) and fetched when a ref is missing.
#   - images: an update tar (ROCKNIX-SM8250.aarch64-<version>.tar), a SYSTEM squashfs, or an unpacked system folder.
#     A squashfs needs unsquashfs (brew install squashfs), or Docker with the virtual device's tool image
#     pocket-vdev-extract (scripts/vdev.sh image builds it).
# -v shows the diff of every changed script; without it only start_es.sh's and es_settings'.
# Exit code 0 always when both sides could be read (it is a report); 2 when not.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC=${ROCKNIX_SRC:-$HOME/.cache/rocknix-distribution}
VERBOSE=0
[ "${1:-}" = -v ] && { VERBOSE=1; shift; }
[ $# = 2 ] || { sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
die() { echo "rocknix-diff: $*" >&2; exit 2; }
command -v python3 >/dev/null || die "needs python3"

# Under the home folder: Docker on a Mac (colima) shares that, not the system's temporary folder.
mkdir -p "$HOME/.cache"
W=$(mktemp -d "$HOME/.cache/rocknix-diff.XXXXXX")
trap 'rm -rf "$W"' EXIT

# label | path in the image | path in the source tree under projects/ROCKNIX/packages/ (a trailing / is a folder).
# The files pocket-ui calls, reads or repeats; keep it in step with os_compat::FILES (src/os_compat.rs).
TABLE='start_es.sh|usr/bin/start_es.sh|ui/emulationstation/sources/start_es.sh
es_settings|usr/bin/es_settings|ui/emulationstation/sources/es_settings
runemu.sh|usr/bin/runemu.sh|rocknix/sources/scripts/runemu.sh
001-functions|etc/profile.d/001-functions|rocknix/profile.d/001-functions
999-export|etc/profile.d/999-export|hardware/quirks/profile.d/999-export
system.cfg|usr/config/system/configs/system.cfg|rocknix/config/system/configs/system.cfg
setsettings.sh|usr/bin/setsettings.sh|rocknix/sources/scripts/setsettings.sh
wifictl|usr/bin/wifictl|rocknix/sources/scripts/wifictl
rocknix-bluetooth|usr/bin/rocknix-bluetooth|rocknix/sources/scripts/rocknix-bluetooth
steamos-set-timezone|usr/bin/steamos-polkit-helpers/steamos-set-timezone|emulators/standalone/steam/scripts/steamos-polkit-helpers/steamos-set-timezone
factoryreset|usr/bin/factoryreset|rocknix/sources/scripts/factoryreset
rocknix-fake-suspend|usr/bin/rocknix-fake-suspend|rocknix/sources/scripts/rocknix-fake-suspend
input_sense|usr/bin/input_sense|sysutils/system-utils/sources/scripts/input_sense
post-update|usr/share/post-update|rocknix/sources/post-update
001-sync-modules|usr/lib/autostart/common/001-sync-modules|misc/modules/autostart/001-sync-modules
111-sway-init|usr/lib/autostart/common/111-sway-init|wayland/compositor/sway/autostart/111-sway-init
sway-config|usr/lib/sway/sway-config|wayland/compositor/sway/scripts/sway-config
essway.service|usr/lib/systemd/system/essway.service|ui/emulationstation/system.d/essway.service
sway.service|usr/lib/systemd/system/sway.service|wayland/compositor/sway/system.d/sway.service
SM8250 quirks|usr/lib/autostart/quirks/platforms/SM8250/|hardware/quirks/platforms/SM8250/
Mini V2 quirks|usr/lib/autostart/quirks/devices/Retroid Pocket Mini V2/|hardware/quirks/devices/Retroid Pocket Mini V2/'
# Libraries pocket-ui links to or opens (os_compat::DLOPEN), and the packages they come from in the source tree.
LIBS='libc.so.6 libm.so.6 ld-linux-aarch64.so.1 libwayland-client.so.0 libwayland-egl.so.1 libxkbcommon.so.0 libEGL.so.1 libasound.so.2 libpulse-simple.so.0'
PACKAGES='glibc sway wlroots wayland libxkbcommon mesa pipewire wireplumber networkmanager iwd systemd bash squashfs-tools retroarch emulationstation'

# --- sides -----------------------------------------------------------------------------------------------------------
kind() {
  if [ -d "$1" ]; then echo dir
  elif [ -f "$1" ]; then case $1 in *.tar) echo tar ;; *) echo squashfs ;; esac
  else echo ref
  fi
}

# unsquashfs, native or in Docker (the image's folder mounted read-only at /in, the output folder at /out).
unsq() {
  if command -v unsquashfs >/dev/null 2>&1; then
    unsquashfs "$@"
  elif command -v docker >/dev/null 2>&1 && docker image inspect pocket-vdev-extract >/dev/null 2>&1; then
    img=$UNSQ_IMG
    docker run --rm -v "$(cd "$(dirname "$img")" && pwd):/in:ro" -v "$UNSQ_OUT:/out" pocket-vdev-extract unsquashfs "$@"
  else
    die "a squashfs needs unsquashfs (brew install squashfs) or Docker with pocket-vdev-extract (scripts/vdev.sh image)"
  fi
}

# An image side: files/ (the files of TABLE at their paths), list.txt (every path, links as "path -> target").
from_squashfs() {
  img=$1 out=$2
  UNSQ_IMG=$img UNSQ_OUT=$out
  if command -v unsquashfs >/dev/null 2>&1; then in=$img; dest=$out/files; else in=/in/$(basename "$img"); dest=/out/files; fi
  export UNSQ_IMG UNSQ_OUT
  unsq -lls "$in" | sed -n 's#^\(.\).* squashfs-root/\(.*\)$#\1 \2#p' >"$out/ls.txt" || die "can't list $img"
  sed 's/^. //' "$out/ls.txt" >"$out/list.txt"
  # The paths to unpack: TABLE's files, every file in its folders, the cores' list needs none, libc for its version.
  : >"$out/want.txt"
  echo "$TABLE" | while IFS='|' read -r _ ipath _; do
    case $ipath in
      */) grep -F "$ipath" "$out/list.txt" | grep -v ' -> ' | sed -n "s#^\($ipath[^/]*\)\$#\1#p" >>"$out/want.txt" ;;
      *) grep -qx "$ipath" "$out/list.txt" && echo "$ipath" >>"$out/want.txt" ;;
    esac
  done
  echo usr/config/emulationstation/es_systems.cfg >>"$out/want.txt"
  grep -x 'usr/lib/libc.so.6' "$out/list.txt" >>"$out/want.txt" || true
  # unsquashfs -ef: the paths to extract, one per line.
  cp "$out/want.txt" "$out/files.ef"
  ef=$out/files.ef
  command -v unsquashfs >/dev/null 2>&1 || ef=/out/files.ef
  unsq -q -n -f -d "$dest" -ef "$ef" "$in" >/dev/null 2>&1 || unsq -f -d "$dest" -ef "$ef" "$in" >/dev/null || die "can't unpack $img"
}

from_dir() {
  dir=$1 out=$2
  (cd "$dir" && find . \( -type f -o -type l -o -type d \) | sed 's#^\./##' | grep -v '^\.$') >"$out/list.txt"
  mkdir -p "$out/files"
  echo "$TABLE" | while IFS='|' read -r _ ipath _; do
    case $ipath in
      */) [ -d "$dir/$ipath" ] && mkdir -p "$out/files/$ipath" && cp "$dir/$ipath"* "$out/files/$ipath" 2>/dev/null || true ;;
      *) [ -f "$dir/$ipath" ] && mkdir -p "$out/files/$(dirname "$ipath")" && cp "$dir/$ipath" "$out/files/$ipath" ;;
    esac
  done
  for f in usr/config/emulationstation/es_systems.cfg usr/lib/libc.so.6; do
    [ -f "$dir/$f" ] && mkdir -p "$out/files/$(dirname "$f")" && cp "$dir/$f" "$out/files/$f"
  done
  : >"$out/ls.txt"
  for l in $(cd "$dir" && find usr/lib -maxdepth 1 -type l 2>/dev/null); do echo "l $l -> $(readlink "$dir/$l")" >>"$out/ls.txt"; done
}

from_image() {
  case $(kind "$1") in
    dir) from_dir "$1" "$2" ;;
    squashfs) from_squashfs "$1" "$2" ;;
    tar)
      sys=$(tar -tf "$1" | grep -m1 '/target/SYSTEM$') || die "no target/SYSTEM in $1"
      mkdir -p "$2/tar"
      tar -xf "$1" -C "$2/tar" "$sys" || die "can't unpack $sys from $1"
      from_squashfs "$2/tar/$sys" "$2"
      rm -rf "$2/tar"
      ;;
  esac
  # cores: the libretro cores in the image
  sed -n 's#^usr/lib/libretro/\([^/]*\)_libretro\.so$#\1#p' "$2/list.txt" | sort -u >"$2/cores.txt"
  sed -n 's#^usr/bin/\(start_[^/]*\.sh\)$#\1#p' "$2/list.txt" | sort -u >"$2/start-scripts.txt"
  # libraries: where each soname points
  : >"$2/libs.txt"
  for lib in $LIBS; do
    t=$(sed -n "s#^l usr/lib/$lib -> ##p" "$2/ls.txt" | head -n 1)
    if [ -n "$t" ]; then echo "$lib -> $t"
    elif grep -qx "usr/lib/$lib" "$2/list.txt"; then echo "$lib (file)"
    else echo "$lib MISSING"
    fi >>"$2/libs.txt"
  done
  if [ -f "$2/files/usr/lib/libc.so.6" ]; then
    v=$(LC_ALL=C grep -ao 'release version 2\.[0-9]*' "$2/files/usr/lib/libc.so.6" | head -n 1 | sed 's/release version //')
    echo "glibc ${v:-unknown}" >>"$2/libs.txt"
  fi
}

# A source side: files/ like an image's, emulators.txt (es_systems as the build makes it: add_emu_core and
# add_es_system lines), cores.txt (LIBRETRO_CORES), start-scripts.txt, libs.txt (package versions).
from_ref() {
  ref=$1 out=$2
  if [ ! -d "$SRC/.git" ]; then
    echo "cloning ROCKNIX's distribution into $SRC (once, without file contents)" >&2
    git clone -q --filter=blob:none --no-checkout https://github.com/ROCKNIX/distribution.git "$SRC" || die "clone failed"
  fi
  if ! git -C "$SRC" rev-parse -q --verify "$ref^{commit}" >/dev/null; then
    git -C "$SRC" fetch -q --tags origin || die "fetch failed"
    git -C "$SRC" rev-parse -q --verify "$ref^{commit}" >/dev/null || die "no ROCKNIX ref $ref"
  fi
  P=projects/ROCKNIX/packages
  git -C "$SRC" ls-tree -r --name-only "$ref" >"$out/tree.txt"
  echo "$TABLE" | while IFS='|' read -r _ ipath spath; do
    case $spath in
      */)
        grep -F "$P/$spath" "$out/tree.txt" | sed -n "s#^$P/$spath\([^/]*\)\$#\1#p" | while IFS= read -r f; do
          mkdir -p "$out/files/$ipath"
          git -C "$SRC" show "$ref:$P/$spath$f" >"$out/files/$ipath$f"
        done
        ;;
      *)
        if grep -qx "$P/$spath" "$out/tree.txt"; then
          mkdir -p "$out/files/$(dirname "$ipath")"
          git -C "$SRC" show "$ref:$P/$spath" >"$out/files/$ipath"
        fi
        ;;
    esac
  done
  git -C "$SRC" show "$ref:$P/virtual/emulators/package.mk" >"$out/emulators.mk" 2>/dev/null || : >"$out/emulators.mk"
  grep -E '^[[:space:]]*add_(emu_core|es_system)[[:space:]]' "$out/emulators.mk" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' >"$out/emulators.txt" || true
  grep -oE 'LIBRETRO_CORES\+?="[^"]*' "$out/emulators.mk" | sed 's/^[^"]*"//' | tr ' ' '\n' | grep . | sed 's/-lr$//' | sort -u >"$out/cores.txt" || true
  sed -n 's#.*/\(start_[^/]*\.sh\)$#\1#p' "$out/tree.txt" | sort -u >"$out/start-scripts.txt"
  : >"$out/libs.txt"
  # The project's package.mk first (ROCKNIX overrides), then the generic one; every PKG_VERSION in it (some differ per
  # device, e.g. sway 1.9/1.11), joined by "/".
  for pkg in $PACKAGES; do
    v=
    for mk in $(grep -E "^(projects/ROCKNIX/)?packages/.*/$pkg/package.mk\$" "$out/tree.txt" | sort -r); do
      v=$(git -C "$SRC" show "$ref:$mk" | sed -n 's/^[[:space:]]*PKG_VERSION="\{0,1\}\([^"]*\)"\{0,1\}.*/\1/p' | paste -sd/ -)
      [ -n "$v" ] && break
    done
    echo "$pkg ${v:-?}" >>"$out/libs.txt"
  done
}

k1=$(kind "$1") k2=$(kind "$2")
case "$k1$k2" in
  refref) mode=source ;;
  ref*|*ref) die "compare two ROCKNIX refs, or two images (tar, SYSTEM or folder), not one of each" ;;
  *) mode=image ;;
esac
mkdir -p "$W/old/files" "$W/new/files"
for side in old new; do
  if [ $side = old ]; then what=$1; else what=$2; fi
  if [ $mode = source ]; then from_ref "$what" "$W/$side"; else from_image "$what" "$W/$side"; fi
done

# --- the report ------------------------------------------------------------------------------------------------------
cat >"$W/report.py" <<'PY'
import difflib, hashlib, os, re, sys, xml.etree.ElementTree as ET
w, mode, verbose, root, old_name, new_name, table = sys.argv[1], sys.argv[2], sys.argv[3] == "1", sys.argv[4], sys.argv[5], sys.argv[6], sys.argv[7]
OLD, NEW = os.path.join(w, "old"), os.path.join(w, "new")
todo = []

def read(side, path):
    try:
        with open(os.path.join(side, "files", path), "rb") as f:
            return f.read()
    except OSError:
        return None

def lines(side, name):
    try:
        with open(os.path.join(side, name), encoding="utf-8", errors="replace") as f:
            return [l.rstrip("\n") for l in f if l.strip()]
    except OSError:
        return []

def src_text(*parts):
    """The text of these files and folders (.rs, .sh, .conf); in .rs files without the test module at the end."""
    out = []
    for base in parts:
        p = os.path.join(root, base)
        files = [p] if os.path.isfile(p) else [os.path.join(d, f) for d, _, fs in os.walk(p) for f in fs if f.endswith((".rs", ".sh", ".conf"))]
        for f in files:
            with open(f, encoding="utf-8", errors="replace") as fh:
                t = fh.read()
            if f.endswith(".rs"):
                t = t.split("#[cfg(test)]", 1)[0]
            out.append(t)
    return "\n".join(out)

# What pocket-ui itself names: the code and the launcher, not the tests (which copy ROCKNIX's functions).
POCKET = src_text("src", "deploy/run-pocket-ui.sh")
def used(word):
    return re.search(r"(?<![A-Za-z0-9_.])" + re.escape(word) + r"(?![A-Za-z0-9_])", POCKET) is not None

def diff(a, b, label):
    a = (a or b"").decode("utf-8", "replace").splitlines()
    b = (b or b"").decode("utf-8", "replace").splitlines()
    for l in difflib.unified_diff(a, b, f"{old_name}/{label}", f"{new_name}/{label}", lineterm="", n=2):
        print("    " + l)

def head(t):
    print(f"\n## {t}")

print(f"# ROCKNIX {old_name} -> {new_name} ({'source tree' if mode == 'source' else 'images'}), for pocket-ui")

# 1. The UI start the launcher repeats
head("UI start (deploy/run-pocket-ui.sh repeats start_es.sh and es_settings)")
with open(os.path.join(root, "src/os_compat.rs"), encoding="utf-8") as f:
    known = dict((m.group(2), m.group(1)) for m in re.finditer(r'\(\s*"usr/bin/([a-z_.]+)",\s*"([0-9a-f]{64})"', f.read()))
for name in ["start_es.sh", "es_settings"]:
    a, b = read(OLD, f"usr/bin/{name}"), read(NEW, f"usr/bin/{name}")
    sha = hashlib.sha256(b).hexdigest() if b is not None else None
    state = "same" if a == b else ("gone" if b is None else ("new" if a is None else "CHANGED"))
    k = "known to the launcher" if sha in known else ("NOT known to the launcher" if sha else "")
    print(f"- {name}: {state}" + (f", {k} (sha256 {sha})" if sha else ""))
    if a != b:
        diff(a, b, name)
    if sha and sha not in known:
        todo.append(f"{name} {sha[:12]}…: repeat its new steps in deploy/run-pocket-ui.sh (\"ROCKNIX's UI start\"), with launcher tests, then add its sha256 to os_compat::KNOWN_UI_START")
    if b is None and a is not None:
        todo.append(f"{name} is gone: the launcher's fallback to EmulationStation (run_es) needs another way")

# 2. Scripts and files
head("Scripts and files pocket-ui calls, reads or imitates")
for row in table.strip().split("\n"):
    label, ipath, _ = row.split("|")
    if label in ("start_es.sh", "es_settings"):
        continue
    if ipath.endswith("/"):
        names = sorted(set(os.listdir(os.path.join(OLD, "files", ipath)) if os.path.isdir(os.path.join(OLD, "files", ipath)) else []) |
                       set(os.listdir(os.path.join(NEW, "files", ipath)) if os.path.isdir(os.path.join(NEW, "files", ipath)) else []))
        changed = []
        for n in names:
            a, b = read(OLD, ipath + n), read(NEW, ipath + n)
            if a != b:
                changed.append((n, "new" if a is None else ("gone" if b is None else "changed"), a, b))
        print(f"- {label}: " + ("same" if not changed else ", ".join(f"{n} {s}" for n, s, _, _ in changed)))
        for n, s, a, b in changed:
            if verbose:
                diff(a, b, ipath + n)
        if changed:
            todo.append(f"{label} changed ({', '.join(n for n, *_ in changed)}): check what pocket-ui reads of them (DEVICE_* variables, docs/device-baseline)")
        continue
    a, b = read(OLD, ipath), read(NEW, ipath)
    if a == b:
        print(f"- {label}: same" + ("" if a is not None else " (in neither)"))
        continue
    if a is None or b is None:
        print(f"- {label}: {'NEW' if a is None else 'GONE'} (/{ipath})")
        todo.append(f"{label} is {'new' if a is None else 'gone'}: /{ipath}")
        continue
    sm = difflib.SequenceMatcher(None, a.decode('utf-8', 'replace').splitlines(), b.decode('utf-8', 'replace').splitlines())
    plus = sum(j2 - j1 for t, i1, i2, j1, j2 in sm.get_opcodes() if t in ("replace", "insert"))
    minus = sum(i2 - i1 for t, i1, i2, j1, j2 in sm.get_opcodes() if t in ("replace", "delete"))
    print(f"- {label}: changed (+{plus} -{minus})")
    if verbose:
        diff(a, b, label)
    if label not in ("system.cfg", "001-functions"):
        todo.append(f"{label} changed (+{plus} -{minus}): read the diff (-v) for anything pocket-ui calls it with or relies on")

# 3. Shell functions
def functions(data):
    out, name, body = {}, None, []
    for l in (data or b"").decode("utf-8", "replace").splitlines():
        m = re.match(r"^\s*(?:function\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*(?:\(\))?\s*\{?\s*$", l)
        if name is None and m and (l.lstrip().startswith("function") or "()" in l):
            name, body = m.group(1), []
            continue
        if name is not None:
            if l.startswith("}"):
                out[name] = "\n".join(body)
                name = None
            else:
                body.append(l)
    return out
head("Shell functions (/etc/profile.d/001-functions)")
fa, fb = functions(read(OLD, "etc/profile.d/001-functions")), functions(read(NEW, "etc/profile.d/001-functions"))
start_es_calls = set(re.findall(r"^\s*([a-z_][a-z0-9_]*)\b", (read(NEW, "usr/bin/start_es.sh") or b"").decode("utf-8", "replace"), re.M))
watch = {"get_setting", "set_setting", "del_setting", "sort_settings", "wait_lock", "release_lock", "set_kill"} | (start_es_calls & set(fb))
for kind, names in [("added", sorted(set(fb) - set(fa))), ("gone", sorted(set(fa) - set(fb))), ("changed", sorted(n for n in set(fa) & set(fb) if fa[n] != fb[n]))]:
    if names:
        print(f"- {kind}: " + ", ".join(n + (" (pocket-ui relies on it)" if n in watch or used(n) else "") for n in names))
        for n in names:
            if kind != "added" and (n in watch or used(n)):
                todo.append(f"function {n} {kind}: check its users in pocket-ui (src/system_cfg.rs lock and set_setting, fn_keys/emu_choice get_setting, the launcher)")
if fa == fb:
    print("- same")

# 4. system.cfg keys
def cfg(data):
    out = {}
    for l in (data or b"").decode("utf-8", "replace").splitlines():
        if "=" in l and not l.lstrip().startswith("#"):
            k, v = l.split("=", 1)
            out.setdefault(k.strip(), v)
    return out
head("system.cfg keys (/usr/config/system/configs/system.cfg, the defaults)")
ca, cb = cfg(read(OLD, "usr/config/system/configs/system.cfg")), cfg(read(NEW, "usr/config/system/configs/system.cfg"))
any_cfg = False
for k in sorted(set(ca) | set(cb)):
    if ca.get(k) == cb.get(k):
        continue
    any_cfg = True
    mark = " (pocket-ui uses it)" if used(k) else ""
    if k not in ca:
        print(f"- added {k}={cb[k]}{mark}")
    elif k not in cb:
        print(f"- removed {k} (was {ca[k]}){mark}")
    else:
        print(f"- {k}: {ca[k]} -> {cb[k]}{mark}")
    if mark:
        todo.append(f"system.cfg {k} changed: pocket-ui reads or writes it")
if not any_cfg:
    print("- same")

# 5. es_systems
head("es_systems (the systems, their emulators and cores)")
with open(os.path.join(root, "src/hostsys.rs"), encoding="utf-8") as f:
    hs = f.read()
routed = {}
for m in re.finditer(r'\("([a-z0-9-]+)", "([a-z0-9_]+)"\)', hs.split("pub const HANDHELD_CORES", 1)[1].split(";", 1)[0]):
    routed.setdefault(m.group(2), []).append(m.group(1))
for m in re.finditer(r'system\(\s*&\[([^\]]*)\],\s*"([a-z0-9_]+)",\s*[A-Z_]+,\s*(?:true|false),\s*(true|false)\s*\)', hs, re.S):
    if m.group(3) == "true":
        routed.setdefault(m.group(2), []).extend(re.findall(r'"([a-z0-9-]+)"', m.group(1)))
hosted_systems = {s for ss in routed.values() for s in ss}
if mode == "image":
    def systems(data):
        out = {}
        if not data:
            return out
        for s in ET.fromstring(data).iter("system"):
            name = (s.findtext("name") or "").strip()
            emus = {}
            for e in s.iter("emulator"):
                cores = [(c.text or "").strip() for c in e.iter("core")]
                default = [(c.text or "").strip() for c in e.iter("core") if c.get("default") == "true"]
                emus[e.get("name")] = (tuple(cores), tuple(default))
            out[name] = ((s.findtext("path") or "").strip(), (s.findtext("extension") or "").strip(), emus)
        return out
    sa, sb = systems(read(OLD, "usr/config/emulationstation/es_systems.cfg")), systems(read(NEW, "usr/config/emulationstation/es_systems.cfg"))
    for n in sorted(set(sa) | set(sb)):
        if sa.get(n) == sb.get(n):
            continue
        mark = " (pocket-core plays it)" if n in hosted_systems else ""
        if n not in sa:
            print(f"- new system {n}{mark}")
        elif n not in sb:
            print(f"- system {n} gone{mark}")
        else:
            (pa, xa, ea), (pb, xb, eb) = sa[n], sb[n]
            what = []
            if pa != pb:
                what.append(f"path {pa} -> {pb}")
            if xa != xb:
                what.append(f"extensions {xa} -> {xb}")
            for e in sorted(set(ea) | set(eb)):
                if ea.get(e) != eb.get(e):
                    what.append(f"{e}: {ea.get(e)} -> {eb.get(e)}")
            print(f"- {n}{mark}: " + "; ".join(what))
        if mark:
            todo.append(f"es_systems {n} changed: pocket-core plays it (src/hostsys.rs, library.rs reads es_systems.cfg)")
    if sa == sb:
        print("- same")
else:
    ea, eb = set(lines(OLD, "emulators.txt")), set(lines(NEW, "emulators.txt"))
    for l in sorted(eb - ea):
        print(f"+ {l}")
    for l in sorted(ea - eb):
        print(f"- {l}")
    if ea == eb:
        print("- same (add_emu_core / add_es_system in packages/virtual/emulators/package.mk)")
    for l in sorted(ea ^ eb):
        parts = l.split()
        if len(parts) > 1 and parts[1] in hosted_systems:
            todo.append(f"es_systems line for {parts[1]} changed ({l}): pocket-core plays it")

# 6. cores
head("libretro cores" + (" (usr/lib/libretro)" if mode == "image" else " (LIBRETRO_CORES, packages without -lr)"))
ka, kb = set(lines(OLD, "cores.txt")), set(lines(NEW, "cores.txt"))
norm = lambda c: c.replace("-", "_")
for kind, cs in [("added", sorted(kb - ka)), ("gone", sorted(ka - kb))]:
    if cs:
        print(f"- {kind}: " + ", ".join(c + (" (pocket-core uses it)" if norm(c) in routed else "") for c in cs))
for c in sorted(ka - kb):
    if norm(c) in routed:
        todo.append(f"core {c} gone: pocket-core plays {', '.join(routed[norm(c)])} with it (src/hostsys.rs); os_compat warns on the device")
if ka == kb:
    print("- same")
if mode == "source":
    print("  (package names; a core's file name can differ, e.g. mupen64plus-nx -> mupen64plus_next)")

# 7. standalone start scripts
head("Start scripts of standalone emulators (usr/bin/start_*.sh)")
sa, sb = set(lines(OLD, "start-scripts.txt")), set(lines(NEW, "start-scripts.txt"))
for kind, cs in [("added", sorted(sb - sa)), ("gone", sorted(sa - sb))]:
    if cs:
        print(f"- {kind}: " + ", ".join(c + (" (pocket-ui names it)" if used(c) else "") for c in cs))
for c in sorted(sa - sb):
    todo.append(f"{c} gone: games set to that emulator no longer start; \"Runs on\" hides it (src/emu_choice.rs installed()), check src/standalone.rs")
if sa == sb:
    print("- same")

# 8. libraries
head("Libraries pocket-ui loads" if mode == "image" else "Package versions (the libraries and programs pocket-ui loads)")
la, lb = lines(OLD, "libs.txt"), lines(NEW, "libs.txt")
for a, b in zip(la, lb):
    print(f"- {a}" if a == b else f"- {a}  ->  {b}")
    if a != b and ("MISSING" in b or b.startswith("glibc") or b.startswith("sway") or b.startswith("wlroots") or b.startswith("wayland")):
        todo.append(f"{b}: rebuild and run pocket-ui on it (vdev, then the device); pocket-ui --check-os-update on the image")

head("Checklist")
if todo:
    for t in todo:
        print(f"[ ] {t}")
else:
    print("nothing pocket-ui depends on changed")
print("[ ] run pocket-ui --check-os-update on the new image (docs/UPDATES.md), then the device checks of docs/DEVICE-TESTS.md")
PY
python3 -I "$W/report.py" "$W" "$mode" "$VERBOSE" "$ROOT" "$1" "$2" "$TABLE"

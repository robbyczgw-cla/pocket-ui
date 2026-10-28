#!/bin/sh
# Does a ROCKNIX system still keep every promise pocket-ui relies on? One look at one system (rocknix-diff.sh compares
# two): the scripts, functions, files, settings, es_systems entries, cores and services pocket-ui, pocket-core and the
# launcher call, read, imitate or repeat, checked by content where that is cheap (the lock style of set_setting, the
# functions start_es.sh calls, the arguments runemu.sh parses), not only by existence. What is checked and why is
# docs/ROCKNIX-CONTRACT.md; the expectations are scripts/rocknix-contract.txt (cargo test keeps them in step with the
# code, src/os_compat.rs).
#
#   scripts/rocknix-contract.sh [-q] --root DIR    a ROCKNIX root folder: an unpacked SYSTEM squashfs
#                                                  (unsquashfs -d DIR SYSTEM) or a mounted one
#   scripts/rocknix-contract.sh [-q] --host HOST   a running ROCKNIX over one SSH session (read-only): the same checks
#                                                  on / plus the live ones (/storage, the running systemd)
#   scripts/rocknix-contract.sh [-q] --ref REF     ROCKNIX's source tree at REF (github.com/ROCKNIX/distribution: a tag
#                                                  such as 20261001, a commit, origin/next): only the rows whose file
#                                                  the data maps to a source file (`src` rows); the rest print n/a.
#                                                  The clone is ROCKNIX_SRC (default ~/.cache/rocknix-distribution,
#                                                  the one rocknix-diff.sh uses), made and fetched as needed
# -q prints only what is not ok. ROCKNIX_CONTRACT_DATA names another expectations file and ROCKNIX_CONTRACT_SSH another
# ssh command (both for the self-test, scripts/test-rocknix-contract.sh).
#
# One line per promise: "ok", "CHANGED" (there, but not as pocket-ui expects) or "MISSING" (not there), the evidence
# (the line found, the value, the sha256), and for the last two where pocket-ui relies on it. "n/a" is a row this mode
# can't check (a live row on an image folder, a file the source mapping doesn't cover). The last line is
# `CONTRACT: ok` (exit 0) or `CONTRACT: <n> changed` (exit 1, n = CHANGED + MISSING); exit 2 when the system can't be
# read at all.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
DATA=${ROCKNIX_CONTRACT_DATA:-$HERE/rocknix-contract.txt}
QUIET=0
usage() { sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
die() { echo "rocknix-contract: $*" >&2; exit 2; }

# ---8<--- checker (also sent to the device by --host: POSIX sh, BusyBox tools) ---------------------------------------
# contract_check ROOT LIVE MAPPED QUIET < data
#   ROOT: the system's folder ("" for the running system), LIVE: 1 on a running system, MAPPED: a file listing the
#   paths that exist in --ref mode ("" otherwise), QUIET: 1 prints only what is not ok.
contract_check() {
  R=$1 LIVE=$2 MAPPED=$3 CQ=$4
  n_ok=0 n_bad=0 n_na=0

  # PATH (absolute or relative to the root) inside the root with its links followed there, not on this computer.
  rp() {
    if [ -z "$R" ]; then
      case $1 in /*) printf '%s\n' "$1" ;; *) printf '/%s\n' "$1" ;; esac
      return
    fi
    rp_p=${1#/} rp_cur= rp_n=0
    while [ -n "$rp_p" ]; do
      rp_c=${rp_p%%/*}
      case $rp_p in */*) rp_p=${rp_p#*/} ;; *) rp_p= ;; esac
      case $rp_c in '' | .) continue ;; ..) rp_cur=${rp_cur%/*}; continue ;; esac
      if [ -L "$R$rp_cur/$rp_c" ] && [ $rp_n -lt 40 ]; then
        rp_n=$((rp_n + 1))
        rp_t=$(readlink "$R$rp_cur/$rp_c")
        case $rp_t in /*) rp_cur= rp_p=${rp_t#/}${rp_p:+/$rp_p} ;; *) rp_p=$rp_t${rp_p:+/$rp_p} ;; esac
      else
        rp_cur=$rp_cur/$rp_c
      fi
    done
    printf '%s\n' "$R$rp_cur"
  }
  # Whether --ref mode has this path (always yes in the other modes).
  mapped() {
    [ -z "$MAPPED" ] && return 0
    grep -qxF -- "${1#/}" "$MAPPED"
  }
  say() { # status label evidence code
    case $1 in
      ok) n_ok=$((n_ok + 1)) ;;
      n/a) n_na=$((n_na + 1)) ;;
      *) n_bad=$((n_bad + 1)) ;;
    esac
    if [ "$1" = ok ] || [ "$1" = n/a ]; then
      [ "$CQ" = 1 ] || printf '%-7s %s: %s\n' "$1" "$2" "$3"
    else
      printf '%-7s %s: %s\n        pocket-ui: %s\n' "$1" "$2" "$3" "$4"
    fi
  }
  short() { cut -c1-110 | sed 's/^[[:space:]]*//'; }
  sha() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1
    fi
  }
  # The body of shell function $2 in file $1 (from its line to the first line starting with "}").
  body() {
    awk -v n="$2" '
      !inb && ($0 ~ "^[ \t]*(function[ \t]+)?" n "[ \t]*([(][)])?[ \t]*[{]?[ \t]*$") && ($0 ~ /function|[(][)]/) { inb = 1; next }
      inb && /^}/ { exit }
      inb && !/^[ \t]*#/ { print }' "$1"
  }
  defines() {
    grep -qE "^[[:space:]]*(function[[:space:]]+$2([[:space:]]*[(][)])?|$2[[:space:]]*[(][)])[[:space:]]*[{]?[[:space:]]*\$" "$1"
  }
  # es_systems.cfg as "name|path|extensions", one system per line.
  systems() {
    awk '
      function val(tag,   s) { s = $0; sub(".*<" tag ">", "", s); sub("</" tag ">.*", "", s); return s }
      /<system>/ { n = ""; p = ""; e = "" }
      /<name>/ && n == "" { n = val("name") }
      /<path>/ { p = val("path") }
      /<extension>/ { e = val("extension") }
      /<\/system>/ { print n "|" p "|" e }' "$1"
  }
  # The words of $1 that are not in $2, in $1's order.
  setdiff() {
    awk -v a="$1" -v b="$2" 'BEGIN {
      n = split(b, x, " "); for (i = 1; i <= n; i++) h[x[i]] = 1
      m = split(a, y, " "); for (i = 1; i <= m; i++) if (!(y[i] in h) && !(y[i] in s)) { s[y[i]] = 1; printf "%s%s", sep, y[i]; sep = " " }
    }'
  }

  while IFS='|' read -r kind label code path arg; do
    case $kind in '' | '#'* | src) continue ;; esac
    if [ "${kind#live:}" != "$kind" ]; then
      kind=${kind#live:}
      if [ "$LIVE" != 1 ]; then say n/a "$label" "only on a running system (--host)" "$code"; continue; fi
    fi
    case $kind in
      sys | sysroot | core | glibc | systemd | text)
        if [ -n "$MAPPED" ]; then say n/a "$label" "not in the source tree (built into the image)" "$code"; continue; fi ;;
    esac
    case $kind in
      file)
        found= any_mapped=
        old_ifs=$IFS; IFS=';'
        for p in $path; do
          mapped "$p" || continue
          any_mapped=1
          f=$(rp "$p")
          if [ -e "$f" ] || [ -L "$f" ]; then found=$p; break; fi
        done
        IFS=$old_ifs
        if [ -z "$any_mapped" ]; then say n/a "$label" "not in the source mapping" "$code"
        elif [ -n "$found" ]; then say ok "$label" "/${found#/}" "$code"
        else say MISSING "$label" "$(printf '%s' "/${path#/}" | sed 's#;#, /#g')" "$code"
        fi
        ;;
      text)
        # A fixed string in a (binary) file; the path's last part may be a pattern (libsystemd-shared-*.so).
        d=$(rp "${path%/*}") found=
        for f in "$d"/${path##*/}; do
          [ -f "$f" ] || continue
          if LC_ALL=C tr -c ' -~' '\n' <"$f" | grep -qF -- "$arg"; then found=${f#"$d"/}; break; fi
          found=-${f#"$d"/}
        done
        case $found in
          '') say MISSING "$label" "/${path#/}" "$code" ;;
          -*) say CHANGED "$label" "no \"$arg\" in /${path%/*}/${found#-}" "$code" ;;
          *) say ok "$label" "\"$arg\" in /${path%/*}/$found" ;;
        esac
        ;;
      absent)
        f=$(rp "$path")
        if [ -e "$f" ] || [ -L "$f" ]; then say CHANGED "$label" "/${path#/} is there now" "$code"
        else say ok "$label" "no /${path#/}"
        fi
        ;;
      exec)
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path")
        if [ -f "$f" ] && [ -x "$f" ]; then say ok "$label" "/${path#/}"
        elif [ -e "$f" ]; then say CHANGED "$label" "/${path#/} is not an executable file" "$code"
        else say MISSING "$label" "/${path#/}" "$code"
        fi
        ;;
      grep)
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        line=$(grep -n -E -- "$arg" "$f" | head -n 1)
        if [ -n "$line" ]; then say ok "$label" "/${path#/}:$(printf '%s' "$line" | short)"
        else say CHANGED "$label" "/${path#/} has no line matching /$arg/" "$code"
        fi
        ;;
      fn)
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        if defines "$f" "$arg"; then say ok "$label" "$arg() in /${path#/}"
        else say MISSING "$label" "no function $arg in /${path#/}" "$code"
        fi
        ;;
      body)
        file=${path%%:*} fn=${path#*:}
        if ! mapped "$file"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$file")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${file#/}" "$code"; continue; fi
        if ! defines "$f" "$fn"; then say MISSING "$label" "no function $fn in /${file#/}" "$code"; continue; fi
        line=$(body "$f" "$fn" | grep -E -- "$arg" | head -n 1)
        if [ -n "$line" ]; then say ok "$label" "$fn: $(printf '%s' "$line" | short)"
        else say CHANGED "$label" "$fn() in /${file#/} has no line matching /$arg/" "$code"
        fi
        ;;
      cfg)
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        line=$(awk -v k="$arg=" 'index($0, k) == 1 { print; exit }' "$f")
        if [ -n "$line" ]; then say ok "$label" "$line"
        else say CHANGED "$label" "no $arg= in /${path#/}" "$code"
        fi
        ;;
      sha)
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        s=$(sha "$f")
        case " $arg " in
          *" $s "*) say ok "$label" "sha256 $s (a version the launcher knows)" ;;
          *) say CHANGED "$label" "sha256 $s is not a version the launcher knows" "$code" ;;
        esac
        ;;
      keys)
        # Every system.cfg key of arg is read in the script: get_setting, or setsettings.sh's game_setting and add_setting.
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        gone=
        for k in $arg; do
          ke=$(printf '%s' "$k" | sed 's/[.]/[.]/g')
          grep -qE "(get_setting|game_setting|add_setting)[[:space:]]+[\"']?$ke[\"']?([[:space:]]|[)]|\$)" "$f" || gone="$gone $k"
        done
        if [ -z "$gone" ]; then say ok "$label" "reads $(printf '%s ' $arg)"
        else say CHANGED "$label" "/${path#/} no longer reads:$gone" "$code"
        fi
        ;;
      lockstyle)
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        if ! defines "$f" wait_lock; then say MISSING "$label" "no wait_lock in /${path#/}" "$code"; continue; fi
        b=$(body "$f" wait_lock)
        if printf '%s\n' "$b" | grep -qE '(^|[^A-Za-z0-9_])flock([^A-Za-z0-9_]|$)'; then
          if printf '%s\n' "$b" | grep -q 'J_CONF_LOCK'; then say ok "$label" "flock on \${J_CONF_LOCK} (ROCKNIX after 20261001: LockStyle::Flock)"
          else say CHANGED "$label" "wait_lock uses flock, but not on \${J_CONF_LOCK}" "$code"
          fi
        elif printf '%s\n' "$b" | grep -q noclobber && printf '%s\n' "$b" | grep -qF '$$'; then
          say ok "$label" "noclobber with \$\$ (ROCKNIX 20261001: LockStyle::CreateExclusive)"
        else
          say CHANGED "$label" "wait_lock neither takes flock nor creates the file with noclobber and \$\$: $(printf '%s' "$b" | grep . | head -n 2 | tr '\n' ' ' | short)" "$code"
        fi
        ;;
      calls)
        # path: the script; arg: the 001-functions file, then the functions the launcher repeats or knows.
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path") fns=$(rp "${arg%% *}") known=" ${arg#* } "
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        called= new=
        for w in $(sed 's/#.*//' "$f" | grep -oE '(^|[;&|(]|then|do|else)[[:space:]]*[A-Za-z_][A-Za-z0-9_]*' | grep -oE '[A-Za-z_][A-Za-z0-9_]*$' | sort -u); do
          [ -f "$fns" ] && defines "$fns" "$w" || continue
          called="$called $w"
          case $known in *" $w "*) ;; *) new="$new $w" ;; esac
        done
        if [ -z "$new" ]; then say ok "$label" "calls${called:- no ROCKNIX function}"
        else say CHANGED "$label" "calls$new, which the launcher doesn't know (all:$called)" "$code"
        fi
        ;;
      trips)
        if ! mapped "$path"; then say n/a "$label" "not in the source mapping" "$code"; continue; fi
        f=$(rp "$path")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        prof=${arg%% *} want=${arg#* }
        var=TRIPS_$(printf '%s' "$prof" | tr 'a-z' 'A-Z')
        have=$(sed -n "s/^[[:space:]]*$var=\"\([0-9 ]*\)\".*/\1/p" "$f" | head -n 1)
        if [ -z "$have" ]; then say CHANGED "$label" "no $var=\"...\" in /${path#/}" "$code"
        elif [ "$have" = "$want" ]; then say ok "$label" "$var=\"$have\""
        else say CHANGED "$label" "$var=\"$have\", pocket-ui has \"$want\"" "$code"
        fi
        ;;
      sys)
        # path: the system's name; arg: its folder and extensions in the reference, or "-" when it had none.
        f=$(rp usr/config/emulationstation/es_systems.cfg)
        if [ ! -f "$f" ]; then say MISSING "$label" "/usr/config/emulationstation/es_systems.cfg" "$code"; continue; fi
        line=$(systems "$f" | grep -F -- "$path|" | awk -F'|' -v n="$path" '$1 == n' | head -n 1)
        if [ "$arg" = - ]; then
          if [ -z "$line" ]; then say ok "$label" "not in es_systems (as in the reference)"
          else say ok "$label" "new in es_systems: ${line#*|}"
          fi
          continue
        fi
        if [ -z "$line" ]; then say MISSING "$label" "no system $path in es_systems.cfg" "$code"; continue; fi
        hp=$(printf '%s' "$line" | cut -d'|' -f2) he=$(printf '%s' "$line" | cut -d'|' -f3)
        wp=${arg%% *} we=${arg#* }
        [ "$we" = "$arg" ] && we=
        why=
        [ "$hp" = "$wp" ] || why="folder $hp (was $wp)"
        added=$(setdiff "$he" "$we") gone=$(setdiff "$we" "$he")
        [ -n "$added" ] && why="${why:+$why; }new extensions $added"
        [ -n "$gone" ] && why="${why:+$why; }extensions gone $gone"
        if [ -z "$why" ]; then say ok "$label" "$hp, $(printf '%s\n' $he | grep -c .) extensions as expected"
        else say CHANGED "$label" "$why" "$code"
        fi
        ;;
      sysroot)
        f=$(rp usr/config/emulationstation/es_systems.cfg)
        if [ ! -f "$f" ]; then say MISSING "$label" "/usr/config/emulationstation/es_systems.cfg" "$code"; continue; fi
        all=$(systems "$f")
        n=$(printf '%s\n' "$all" | grep -c .)
        # arg: the folder, then the systems known to live elsewhere (Tools, Music...).
        elsewhere=
        case $arg in *' '*) elsewhere=${arg#* } ;; esac
        out=$(printf '%s\n' "$all" | awk -F'|' -v p="${arg%% *}" -v known=" $elsewhere " 'NF && index($2, p) != 1 && index(known, " " $1 " ") == 0 { printf "%s%s=%s", sep, $1, $2; sep = ", " }')
        if [ "$n" -eq 0 ]; then say CHANGED "$label" "no <system> with <name>, <path> and <extension> lines read" "$code"
        elif [ -z "$out" ]; then say ok "$label" "$n systems, every folder under ${arg%% *}${elsewhere:+ but those of $elsewhere}"
        else say CHANGED "$label" "not under ${arg%% *}: $out" "$code"
        fi
        ;;
      core)
        f=$(rp "$path/${arg}_libretro.so")
        if [ -f "$f" ]; then say ok "$label" "/$path/${arg}_libretro.so"
        else say MISSING "$label" "/$path/${arg}_libretro.so" "$code"
        fi
        ;;
      glibc)
        f=$(rp "$path")
        if [ ! -f "$f" ]; then say MISSING "$label" "/${path#/}" "$code"; continue; fi
        v=$(LC_ALL=C tr -c 'A-Za-z0-9 .' '\n' <"$f" | sed -n 's/.*release version \(2\.[0-9]*\).*/\1/p' | head -n 1)
        if [ -z "$v" ]; then say CHANGED "$label" "no version found in /${path#/}" "$code"
        elif [ "${v#2.}" -ge "${arg#2.}" ] 2>/dev/null; then say ok "$label" "glibc $v (pocket-ui is built for $arg)"
        else say CHANGED "$label" "glibc $v is older than $arg" "$code"
        fi
        ;;
      systemd)
        d=$(rp "$path")
        v=$(ls "$d" 2>/dev/null | sed -n 's/^libsystemd-shared-\([0-9]*\)[.a-z0-9-]*\.so$/\1/p' | head -n 1)
        if [ -z "$v" ]; then say MISSING "$label" "no libsystemd-shared-<version>.so in /${path#/}" "$code"
        elif [ "$v" -ge "$arg" ]; then say ok "$label" "systemd $v"
        else say CHANGED "$label" "systemd $v is older than $arg" "$code"
        fi
        ;;
      *) say CHANGED "$label" "unknown kind $kind in the data file" "$code" ;;
    esac
  done
  echo "rocknix-contract: $n_ok ok, $n_bad changed or missing, $n_na n/a"
  if [ $n_bad -eq 0 ]; then echo "CONTRACT: ok"; return 0; fi
  echo "CONTRACT: $n_bad changed"
  return 1
}
# ---8<--- end of checker ----------------------------------------------------------------------------------------------

mode= target=
while [ $# -gt 0 ]; do
  case $1 in
    -q) QUIET=1 ;;
    --root | --host | --ref) [ $# -ge 2 ] || usage; mode=${1#--} target=$2; shift ;;
    -h | --help) usage ;;
    *) usage ;;
  esac
  shift
done
[ -n "$mode" ] || usage
[ -f "$DATA" ] || die "no expectations file $DATA"

case $mode in
  root)
    [ -d "$target" ] || die "--root needs a folder (unpack a SYSTEM squashfs with: unsquashfs -d DIR SYSTEM)"
    root=$(cd "$target" && pwd)
    [ -d "$root/usr" ] || die "$root has no usr/: not a ROCKNIX root folder"
    echo "# ROCKNIX contract of pocket-ui: $root ($(sed -n 's/^OS_VERSION=//p' "$root/etc/os-release" 2>/dev/null | tr -d '"' | head -n 1))"
    contract_check "$root" 0 "" "$QUIET" <"$DATA"
    ;;
  host)
    echo "# ROCKNIX contract of pocket-ui: $target over SSH (read-only)"
    {
      sed -n '/^# ---8<--- checker/,/^# ---8<--- end of checker/p' "$0"
      echo 'echo "# $(sed -n "s/^OS_VERSION=//p" /etc/os-release 2>/dev/null | tr -d "\"")"'
      echo "contract_check '' 1 '' $QUIET <<'__ROCKNIX_CONTRACT_DATA__'"
      grep -v '^__ROCKNIX_CONTRACT_DATA__$' "$DATA"
      echo '__ROCKNIX_CONTRACT_DATA__'
    } | ${ROCKNIX_CONTRACT_SSH:-ssh -o BatchMode=yes} "$target" sh -s
    rc=$?
    [ $rc -le 1 ] || die "ssh $target failed (exit $rc)"
    exit $rc
    ;;
  ref)
    SRC=${ROCKNIX_SRC:-$HOME/.cache/rocknix-distribution}
    if [ ! -d "$SRC/.git" ]; then
      echo "cloning ROCKNIX's distribution into $SRC (once, without file contents)" >&2
      git clone -q --filter=blob:none --no-checkout https://github.com/ROCKNIX/distribution.git "$SRC" || die "clone failed"
    fi
    if ! git -C "$SRC" rev-parse -q --verify "$target^{commit}" >/dev/null; then
      git -C "$SRC" fetch -q --tags origin || die "fetch failed"
      git -C "$SRC" rev-parse -q --verify "$target^{commit}" >/dev/null || die "no ROCKNIX ref $target"
    fi
    W=$(mktemp -d "${TMPDIR:-/tmp}/rocknix-contract.XXXXXX")
    trap 'rm -rf "$W"' EXIT
    mkdir -p "$W/root"
    : >"$W/mapped"
    # src|-|-|<path in the image>|<path in the source tree>[;<where it was or moved to>...]: the first that exists.
    grep '^src|' "$DATA" | while IFS='|' read -r _ _ _ ipath spaths; do
      mkdir -p "$W/root/$(dirname "$ipath")"
      old_ifs=$IFS; IFS=';'
      for spath in $spaths; do
        if git -C "$SRC" show "$target:$spath" >"$W/root/$ipath" 2>/dev/null; then chmod +x "$W/root/$ipath"; break; fi
        rm -f "$W/root/$ipath"
      done
      IFS=$old_ifs
      echo "$ipath" >>"$W/mapped"
    done
    echo "# ROCKNIX contract of pocket-ui: source tree $target ($(git -C "$SRC" rev-parse --short "$target^{commit}"))"
    contract_check "$W/root" 0 "$W/mapped" "$QUIET" <"$DATA"
    ;;
esac

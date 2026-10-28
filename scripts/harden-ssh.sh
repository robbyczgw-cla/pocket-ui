#!/bin/sh
# Key-only SSH for root, rpcbind and LLMNR off, on a ROCKNIX device.
# Background, persistence and risks: docs/SECURITY-HARDENING.md.
#
#   harden-ssh.sh                 dry run (default): read-only checks on the device, prints every change
#   harden-ssh.sh --apply         makes the changes, with the safety checks below
#   harden-ssh.sh --revert        dry run of the revert
#   harden-ssh.sh --revert --apply   undoes everything this script wrote
# Options: --host root@<device-ip> (or POCKETUI_HOST, or scripts/local.env), --pubkey ~/.ssh/id_ed25519.pub (the key we log in with)
#
# Safety: aborts unless our public key is in /storage/.ssh/authorized_keys and a key-only login works
# right now. The new sshd settings are first tried on a second sshd (port 2222) while the running one
# stays untouched; only when a key login to it works (and a password login is refused) is the real
# sshd restarted, with a 3-minute automatic rollback that is disarmed by a second key-based login.
# Only files under /storage are written. Nothing here reboots, touches Wi-Fi or the boot path.
set -eu

. "$(dirname "$0")/host.sh"
PUBKEY=$HOME/.ssh/id_ed25519.pub
APPLY=0
REVERT=0
while [ $# -gt 0 ]; do
  case $1 in
    --apply) APPLY=1 ;;
    --dry-run) APPLY=0 ;;
    --revert) REVERT=1 ;;
    --host) HOST=$2; shift ;;
    --pubkey) PUBKEY=$2; shift ;;
    -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

DROPIN_DIR=/storage/.config/system.d/sshd.service.d
DROPIN=$DROPIN_DIR/10-pocket-ui-keys-only.conf
UNITDIR=/storage/.config/system.d
RESOLVED=/storage/.config/resolved.conf.d/10-pocket-ui-no-llmnr.conf
SSHD_OPTS="-o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o PermitRootLogin=prohibit-password"
TEST_PORT=2222
ROLLBACK_UNIT=pocket-ssh-rollback

# Batch mode, key only: a password prompt would mean the key isn't accepted.
KEYSSH="ssh -o BatchMode=yes -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o ConnectTimeout=10"

say() { printf '%s\n' "$*"; }
step() { if [ $APPLY = 1 ]; then say "==> $1"; else say "[dry-run] would: $1"; fi; }
die() { say "ABORT: $*" >&2; exit 1; }
remote() { $KEYSSH "$HOST" "$@"; }

preflight() {
  say "== preflight (read-only) against $HOST"
  [ -r "$PUBKEY" ] || die "public key $PUBKEY not readable (--pubkey)"
  BLOB=$(awk '{print $2}' "$PUBKEY")
  [ -n "$BLOB" ] || die "no key in $PUBKEY"
  remote true || die "key-only login to $HOST fails right now; fix that first"
  say "ok: key-only login works"
  remote "grep -qF '$BLOB' /storage/.ssh/authorized_keys" || die "our key ($PUBKEY) is not in /storage/.ssh/authorized_keys"
  say "ok: our key is in /storage/.ssh/authorized_keys"
  # sshd ignores keys in group/world-writable files once StrictModes is on; ROCKNIX has it off, check anyway.
  # shellcheck disable=SC2016 # expands on the device
  remote 'p=$(stat -c %a /storage/.ssh/authorized_keys); case $p in *[2367]?|*?[2367]) exit 1;; esac' ||
    die "authorized_keys is group/world writable"
  remote "/usr/sbin/sshd -t $SSHD_OPTS" || die "sshd rejects the new options"
  say "ok: sshd accepts: $SSHD_OPTS"
  say "-- now: $(remote "/usr/sbin/sshd -T 2>/dev/null | grep -E '^(permitrootlogin|passwordauthentication|kbdinteractiveauthentication|pubkeyauthentication) ' | tr '\n' ' '")"
  say "-- listening: $(remote "netstat -tuln | awk '\$4 ~ /:(22|111|5355)\$/ {print \$1\" \"\$4}' | sort -u | tr '\n' ' '")"
  say "-- existing pocket-ui files: $(remote "ls $DROPIN $UNITDIR/rpcbind.socket $UNITDIR/rpcbind.service $RESOLVED 2>/dev/null | tr '\n' ' '")"
}

apply() {
  step "write $DROPIN (ExecStart with $SSHD_OPTS, KillMode=process so restarts keep open sessions)"
  if [ $APPLY = 1 ]; then
    remote "mkdir -p $DROPIN_DIR && cat > $DROPIN.new && mv $DROPIN.new $DROPIN" <<EOF
# Written by pocket-ui scripts/harden-ssh.sh; remove (or run harden-ssh.sh --revert --apply) to undo.
[Service]
KillMode=process
ExecStart=
ExecStart=/usr/sbin/sshd -D $SSHD_OPTS \$SSH_ARGS
EOF
  fi

  step "start a test sshd on port $TEST_PORT with the new options; the running sshd stays as it is"
  step "log in to port $TEST_PORT with our key (must work) and with a password (must be refused); stop the test sshd"
  if [ $APPLY = 1 ]; then
    remote "/usr/sbin/sshd $SSHD_OPTS -o Port=$TEST_PORT -o PidFile=/run/sshd-test.pid"
    sleep 1
    if ! $KEYSSH -p $TEST_PORT "$HOST" true; then
      remote "kill \$(cat /run/sshd-test.pid); rm -f $DROPIN"
      die "key login to the test sshd failed; drop-in removed, nothing else changed"
    fi
    out=$(ssh -p $TEST_PORT -o BatchMode=yes -o PreferredAuthentications=password,keyboard-interactive -o PubkeyAuthentication=no \
      -o ConnectTimeout=10 "$HOST" true 2>&1 || true)
    remote "kill \$(cat /run/sshd-test.pid)"
    case $out in
      *"Permission denied (publickey)"*) say "ok: test sshd offers publickey only" ;;
      *) remote "rm -f $DROPIN"; die "test sshd still offers other methods: $out" ;;
    esac
  fi

  step "arm an automatic rollback in 3 minutes (systemd-run unit $ROLLBACK_UNIT: remove the drop-in, restart sshd)"
  step "systemctl daemon-reload; systemctl restart sshd"
  step "second key-based login on port 22; check the running sshd's arguments; then disarm the rollback"
  if [ $APPLY = 1 ]; then
    remote "systemd-run --unit=$ROLLBACK_UNIT --on-active=180 /bin/sh -c 'rm -f $DROPIN; systemctl daemon-reload; systemctl restart sshd'"
    remote "systemctl daemon-reload && systemctl restart sshd"
    sleep 2
    $KEYSSH "$HOST" "tr '\0' ' ' < /proc/\$(systemctl show -p MainPID --value sshd)/cmdline | grep -q PasswordAuthentication=no" ||
      die "second key login or argument check failed; the rollback runs in under 3 minutes by itself"
    remote "systemctl stop $ROLLBACK_UNIT.timer; systemctl reset-failed $ROLLBACK_UNIT 2>/dev/null; true"
    say "ok: sshd is key-only, rollback disarmed"
  fi

  step "mask rpcbind: $UNITDIR/rpcbind.socket and rpcbind.service -> /dev/null; daemon-reload; stop both"
  if [ $APPLY = 1 ]; then
    remote "ln -sfn /dev/null $UNITDIR/rpcbind.socket && ln -sfn /dev/null $UNITDIR/rpcbind.service &&
      systemctl daemon-reload && systemctl stop rpcbind.socket rpcbind.service"
  fi

  step "write $RESOLVED (LLMNR=no, MulticastDNS=no); systemctl restart systemd-resolved"
  if [ $APPLY = 1 ]; then
    remote "mkdir -p /storage/.config/resolved.conf.d && printf '# Written by pocket-ui scripts/harden-ssh.sh\n[Resolve]\nLLMNR=no\nMulticastDNS=no\n' > $RESOLVED &&
      systemctl restart systemd-resolved"
    say "-- listening now: $(remote "netstat -tuln | awk '\$4 ~ /:(22|111|5355)\$/ {print \$1\" \"\$4}' | sort -u | tr '\n' ' '")"
  fi
}

revert() {
  step "remove $DROPIN, the rpcbind masks and $RESOLVED; daemon-reload"
  step "restart systemd-resolved; start rpcbind.socket rpcbind.service"
  step "restart sshd in 2 s from outside the SSH session (systemd-run), then a key login on port 22"
  if [ $APPLY = 1 ]; then
    remote "rm -f $DROPIN $UNITDIR/rpcbind.socket $UNITDIR/rpcbind.service $RESOLVED; rmdir $DROPIN_DIR 2>/dev/null;
      systemctl daemon-reload; systemctl restart systemd-resolved; systemctl start rpcbind.socket rpcbind.service;
      systemd-run --on-active=2 systemctl restart sshd"
    sleep 5
    remote true || die "key login after the revert failed"
    say "ok: reverted; password login is back to ROCKNIX's default"
  fi
}

preflight
if [ $REVERT = 1 ]; then revert; else apply; fi
[ $APPLY = 1 ] || say "dry run only; nothing was changed. Add --apply to make these changes."

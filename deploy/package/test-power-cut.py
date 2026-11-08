#!/usr/bin/env python3
"""Replay a power cut after every installer filesystem command, without a device.

The observer runs through PATH wrappers, on the real install.sh. At each boundary
it checks the boot chain with all writes retained, and with dirty file data lost
but the latest directory entries retained. sync commits file data by inode, so a
subsequent rename preserves it. This is a fault model, not a physical SD test.
"""
import base64
import json
import os
from pathlib import Path
import shutil
import shlex
import subprocess
import sys
import tempfile


def files(root):
    return [p for p in root.rglob("*") if p.is_file() and not p.is_symlink()]


def observe(tool, args):
    root = Path(os.environ["CUT_ROOT"])
    state_path = root.parent / "observer.json"
    state = json.loads(state_path.read_text())
    if tool == "sync":
        state["durable"] = {
            str(p.stat().st_ino): base64.b64encode(p.read_bytes()).decode()
            for p in files(root)
        }
    state["steps"] += 1
    app = root / "storage/pocket-ui"
    for loss in (False, True):
        def data(p):
            if not p.is_file():
                return b""
            if loss:
                return base64.b64decode(state["durable"].get(str(p.stat().st_ino), ""))
            return p.read_bytes()

        reason = None
        # Existing releases must remain usable even during same-version repair.
        for name, expected in state["protected"].items():
            if data(root / name) != expected.encode():
                reason = f"previous program lost: {name}"
                break
        dropin = root / "storage/.config/system.d/essway.service.d/10-pocket-ui.conf"
        if dropin.exists():
            if data(dropin) != b"[Service]\nEnvironment=POCKETUI_AS_ESSWAY=1\n":
                reason = "incomplete boot entry"
            for name in ("run-pocket-ui.sh", "pocket-ui", "pocket-core", "current/pocket-splash"):
                p = app / name
                if not os.access(p, os.X_OK) or not data(p).startswith(b"#!/bin/sh\n"):
                    reason = f"boot cannot run {name}"
            if state["old"] and (app / "current").is_symlink():
                if os.readlink(app / "current") != state["old"]:
                    prev = app / "previous/pocket-ui"
                    if not data(prev).startswith(b"#!/bin/sh\n"):
                        reason = "no runnable previous version after switch"
        if reason:
            state["errors"].append(
                f"step {state['steps']} ({tool} {' '.join(args)}), "
                f"{'dirty data lost' if loss else 'writes retained'}: {reason}"
            )
    state_path.write_text(json.dumps(state))


def wrapper():
    tool = Path(sys.argv[0]).name
    args = sys.argv[1:]
    if tool == "sync":
        rc = 0  # Commit the modeled disk, never sync the host's unrelated jobs.
    elif tool == "systemctl":
        dropin = Path(os.environ["CUT_ROOT"]) / "storage/.config/system.d/essway.service.d/10-pocket-ui.conf"
        if dropin.exists() and args[:2] == ["show", "-p"]:
            print(str(dropin) if args[2] == "DropInPaths" else "POCKETUI_AS_ESSWAY=1")
        rc = 0
    else:
        rc = subprocess.run([json.loads(os.environ["CUT_TOOLS"])[tool], *args], check=False).returncode
    if rc == 0:
        observe(tool, args)
    sys.exit(rc)


def fixture(root, pkg):
    for directory in ("etc", "usr/lib/systemd/system", "usr/bin", "storage/.config/system.d", "tmp"):
        (root / directory).mkdir(parents=True, exist_ok=True)
    (root / "etc/os-release").write_text('OS_NAME="ROCKNIX"\nOS_VERSION="20261001"\nHW_DEVICE="H700"\nHW_ARCH="aarch64"\n')
    (root / "model").write_text("Anbernic RG35XX Plus")
    (root / "usr/lib/systemd/system/essway.service").touch()
    executable(root / "usr/bin/start_es.sh", "#!/bin/sh\nexit 0\n")
    f = pkg / "files"
    f.mkdir(parents=True)
    (f / "VERSION").write_text("9.8.7\n")
    for name in ("pocket-ui", "pocket-core", "pocket-splash"):
        executable(f / name, f'#!/bin/sh\ncase "$1" in\n--version) echo "{name} 9.8.7" ;;\n--check-os-update) echo "system: would be installed" ;;\nesac\n')
    for name in ("portmaster-setup.sh", "run-pocket-ui.sh"):
        executable(f / name, "#!/bin/sh\nexit 0\n")
    (f / "10-pocket-ui.conf").write_text("[Service]\nEnvironment=POCKETUI_AS_ESSWAY=1\n")
    (f / "pocket-splash.service").write_text("[Service]\n")
    (pkg / "licences").mkdir()
    for name in ("LICENSE", "NOTICE.md", "FONTS-OFL.txt", "THIRD-PARTY-CRATES.md", "THIRD-PARTY-LICENSES.txt"):
        (pkg / "licences" / name).write_text("licence\n")
    executable(pkg / "uninstall.sh", "#!/bin/sh\nexit 0\n")
    sums = subprocess.check_output(["sha256sum", *sorted(p.name for p in f.iterdir())], cwd=f)
    (f / "SHA256SUMS").write_bytes(sums)


def executable(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)
    path.chmod(0o755)


def main():
    installer = Path(os.environ.get("POCKETUI_PACKAGE", Path(__file__).parent)) / "install.sh"
    total = 0
    failures = []
    for shell in (["dash"], ["bash"], ["busybox", "sh"]):
        if not shutil.which(shell[0]):
            continue
        for kind in ("fresh", "upgrade", "same-version"):
            with tempfile.TemporaryDirectory(prefix="pocket-install-cut-") as tmp:
                t = Path(tmp)
                root, pkg = t / "root", t / "pkg"
                fixture(root, pkg)
                shutil.copyfile(installer, pkg / "install.sh")
                app = root / "storage/pocket-ui"
                protected = {}
                old = ""
                if kind != "fresh":
                    old = "releases/" + ("9.8.7" if kind == "same-version" else "1.0.0")
                    for name in ("pocket-ui", "pocket-core", "pocket-splash", "portmaster-setup.sh"):
                        p = app / old / name
                        executable(p, "#!/bin/sh\n# old working build\nexit 0\n")
                        protected[str(p.relative_to(root))] = p.read_text()
                    (app / "current").symlink_to(old)
                    (app / "known-good").symlink_to(old)
                    for name in ("pocket-ui", "pocket-core", "portmaster-setup.sh"):
                        (app / name).symlink_to("current/" + name)
                    executable(app / "run-pocket-ui.sh", "#!/bin/sh\n# previous launcher\nexit 0\n")
                    d = root / "storage/.config/system.d/essway.service.d"
                    d.mkdir()
                    shutil.copyfile(pkg / "files/10-pocket-ui.conf", d / "10-pocket-ui.conf")
                state = {"steps": 0, "errors": [], "old": old, "protected": protected,
                         "durable": {str(p.stat().st_ino): base64.b64encode(p.read_bytes()).decode() for p in files(root)}}
                (t / "observer.json").write_text(json.dumps(state))
                tools = {name: shutil.which(name) for name in ("cp", "mv", "ln", "rm", "mkdir", "chmod")}
                bin_dir = t / "bin"
                bin_dir.mkdir()
                observer = bin_dir / "observer"
                observer.write_text(f"#!{sys.executable}\n" + Path(__file__).read_text())
                observer.chmod(0o755)
                for name in (*tools, "sync", "systemctl"):
                    (bin_dir / name).symlink_to("observer")
                env = dict(os.environ, CUT_ROOT=str(root), CUT_TOOLS=json.dumps(tools),
                           POCKETUI_INSTALL_ROOT=str(root), POCKETUI_MODEL_FILE=str(root / "model"),
                           PATH=f"{bin_dir}:{os.environ['PATH']}")
                # BusyBox standalone ash prefers built-in applets over PATH.
                # Functions force the same observer for all three shells.
                prelude = "\n".join(f'{name}() {{ {shlex.quote(str(bin_dir / name))} "$@"; }}' for name in (*tools, "sync", "systemctl"))
                result = subprocess.run([*shell, "-c", prelude + '\n. "$0"', str(pkg / "install.sh"), "--no-restart"], env=env, capture_output=True, text=True, timeout=90)
                state = json.loads((t / "observer.json").read_text())
                total += state["steps"] * 2
                if result.returncode:
                    failures.append(f"{' '.join(shell)} {kind}: installer failed: {result.stdout}{result.stderr}")
                failures.extend(f"{' '.join(shell)} {kind}: {e}" for e in state["errors"])
                print(f"{' '.join(shell)} {kind}: {state['steps']} cut boundaries, {len(state['errors'])} unsafe outcomes")
    for failure in failures[:20]:
        print(f"FAIL {failure}", file=sys.stderr)
    if failures:
        sys.exit(1)
    print(f"power-cut model: {total} boot outcomes passed")


if __name__ == "__main__":
    if Path(sys.argv[0]).name in ("cp", "mv", "ln", "rm", "mkdir", "chmod", "sync", "systemctl"):
        wrapper()
    else:
        main()

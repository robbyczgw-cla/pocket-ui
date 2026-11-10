#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Use upstream HarbourMaster with card paths, our platform and paced card curl."""
import hashlib
import importlib.util
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import types
import zipfile


def install_archive_policy():
    """Only this TrimUI bridge's HM modules use the portable extractor."""
    spec = importlib.util.spec_from_file_location('portmaster_archive', Path(__file__).with_name('portmaster-archive.py'))
    portable = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(portable)
    local_zip = types.SimpleNamespace(**dict(vars(zipfile), ZipFile=portable.PortableZipFile))
    for name, module in list(sys.modules.items()):
        if not name.startswith('harbourmaster') or module is None:
            continue
        for key, value in list(vars(module).items()):
            if value is zipfile:
                setattr(module, key, local_zip)
            elif value is zipfile.ZipFile:
                setattr(module, key, portable.PortableZipFile)
    return portable.PortableZipFile


def report_progress(stage, size=0, total=0):
    value = os.environ.get("BRICK_PORTMASTER_PROGRESS")
    if not value:
        return
    dest = Path(value)
    temporary = Path(value + ".new")
    temporary.write_text(f"{stage}|{size}|{total or 0}\n")
    temporary.replace(dest)


def response_size(dest):
    try:
        lines = Path(str(dest) + ".headers").read_text().splitlines()
        sizes = [int(line.split(":", 1)[1].strip()) for line in lines if line.lower().startswith("content-length:")]
        return sizes[-1] if sizes and sizes[-1] > 0 else None
    except (OSError, ValueError):
        return None


def transfer(url, dest, callback=None):
    """Keep downloads on the card, with cancellation covering curl and its shell."""
    helper = Path(os.environ["BRICK_APP"]) / "portmaster/portmaster-download.sh"
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen(["sh", str(helper), url, str(dest)], stdout=subprocess.DEVNULL,
                                   stderr=errors, start_new_session=True)
        try:
            while process.poll() is None:
                part = Path(str(dest) + ".part")
                try:
                    size = part.stat().st_size
                except FileNotFoundError:
                    size = 0
                total = response_size(dest)
                report_progress("downloading", size, total)
                if callback is not None:
                    callback.progress("Downloading", size, total, "data")
                time.sleep(0.1)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
        if process.returncode:
            errors.seek(0)
            message = errors.read()[-4096:].decode("utf-8", errors="replace").strip()
            raise OSError(message or "PortMaster download failed")


def download(file_name, file_url, md5_source=None, md5_result=None, callback=None, no_check=False):
    file_name = Path(file_name)
    try:
        transfer(file_url, file_name, callback)
        report_progress("checking", file_name.stat().st_size, file_name.stat().st_size)
        md5 = hashlib.md5()
        with file_name.open("rb") as src:
            for data in iter(lambda: src.read(256 * 1024), b""):
                md5.update(data)
        result = md5.hexdigest()
        if not no_check and md5_source is not None and result != md5_source:
            raise OSError("PortMaster download checksum failed")
    except OSError as error:
        file_name.unlink(missing_ok=True)
        if callback is not None:
            callback.message_box(str(error))
        print(str(error), file=sys.stderr)
        report_progress("error")
        return None
    if md5_result is not None:
        md5_result[0] = result
    if callback is not None:
        callback.progress(None, None, None)
    report_progress("ready", file_name.stat().st_size, file_name.stat().st_size)
    return file_name


def fetch(url):
    import requests
    try:
        with tempfile.TemporaryDirectory() as folder:
            dest = Path(folder) / "response"
            transfer(url, dest)
            response = requests.Response()
            response.status_code = 200
            response.url = url
            response.encoding = "utf-8"
            response._content = dest.read_bytes()
            return response
    except OSError as error:
        print(str(error), file=sys.stderr)
        return None


def configure():
    pm = Path(os.environ["BRICK_PORTMASTER_HOME"])
    card = Path(os.environ.get("BRICK_CARD", "/mnt/SDCARD"))
    ports = card / "Roms/ports"
    ports.mkdir(parents=True, exist_ok=True)
    os.environ.update(HM_TOOLS_DIR=str(pm.parent), HM_PORTS_DIR=str(ports), HM_SCRIPTS_DIR=str(ports))
    # Avoid the upstream /tmp default: port downloads and unpacking may exceed the device RAM.
    scratch = card / "pocket-ui/portmaster/tmp"
    scratch.mkdir(parents=True, exist_ok=True)
    os.environ["TMPDIR"] = str(scratch)
    tempfile.tempdir = str(scratch)
    sys.path[:0] = [str(pm / "pylibs"), str(pm / "exlibs")]
    import harbourmaster
    from harbourmaster.platform import HM_PLATFORMS, PlatformBase
    from harbourmaster.source import HM_SOURCE_APIS, PortMasterV3
    from harbourmaster.util import net
    install_archive_policy()

    class PocketPlatform(PlatformBase):
        WANT_XBOX_FIX = True
        # Base hooks only touch HM paths. The stock TrimUI platform also changes
        # /roms and the System/ frontend pack; none of those belong to this card.

    HM_PLATFORMS["pocket-ui"] = PocketPlatform
    class CardSource(PortMasterV3):
        def _update_images(self):
            # The store has a visible fallback and installed ports retain their own
            # artwork. Opening/installing one port mustn't fetch the whole art pack.
            pass

    HM_SOURCE_APIS["PortMasterV3"] = CardSource
    upstream_info_fetch = harbourmaster.HarbourMaster.load_info_fetch_remote
    def info_fetch(self, local_file, *args, **kwargs):
        if local_file.name == "featured_images.zip":
            return False
        return upstream_info_fetch(self, local_file, *args, **kwargs)

    harbourmaster.HarbourMaster.load_info_fetch_remote = info_fetch
    # Keep optional runtime updates pinned and transactional; installing an upstream
    # PortMaster.zip through the port catalogue must not overwrite the integration.
    def refuse_self_update(self, *args, **kwargs):
        self.callback.message_box("Update PortMaster through pocket-ui.")
        return 255

    harbourmaster.HarbourMaster._install_portmaster = refuse_self_update
    old_fetch, old_download = net.fetch, net.download
    for name, module in list(sys.modules.items()):
        if not name.startswith("harbourmaster") or module is None:
            continue
        for key, value in list(vars(module).items()):
            if value is old_fetch:
                setattr(module, key, fetch)
            elif value is old_download:
                setattr(module, key, download)
    return harbourmaster


def cancellation_guard(operation):
    def interrupted(_number, _frame):
        # Unwind transfer's finally block (which owns the curl process group) and
        # HarbourMaster's rollback, rather than terminating Python around them.
        raise KeyboardInterrupt

    previous = {number: signal.signal(number, interrupted) for number in (signal.SIGTERM, signal.SIGINT)}
    try:
        return operation()
    except KeyboardInterrupt:
        report_progress("cancelled")
        print("Download cancelled.", file=sys.stderr)
        return 130
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)


def main(args):
    def invoke():
        configure()
        from harbourmaster.cli import main as hm_main
        # Fresh cards have no cached source catalogue. Let HarbourMaster populate it;
        # --no-check here would make the very first install unable to find any port.
        result = hm_main(["harbourmaster", "--quiet", "--no-colour", *args])
        # Upstream's @logger.catch returns None after an unexpected exception. Report a
        # failed install to the worker instead of letting sys.exit(None) claim success.
        return 1 if result is None else result

    return cancellation_guard(invoke)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

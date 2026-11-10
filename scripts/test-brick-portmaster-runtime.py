#!/usr/bin/env python3
"""Host checks for persistent-card PortMaster staging, recovery and paced transfers."""
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import shutil
import shlex
import signal
import subprocess
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PM_REVISION = "9ed29285ecbb0610b7c86a0b77d0d9d1c74efd49"


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="brick-pm-test-")
        self.root = Path(self.tmp.name)
        self.app = self.root / "card/pocket-ui/app"
        (self.app / "bin").mkdir(parents=True)
        (self.app / "portmaster").mkdir()
        for src in (ROOT / "deploy/brick").glob("portmaster-*"):
            shutil.copy(src, self.app / "portmaster" / src.name)
        helper = self.app / "portmaster/portmaster-runtime.sh"
        if helper.exists():
            shutil.copy(helper, self.app / "bin/portmaster")
        self.fixtures = self.root / "fixtures"
        self.fixtures.mkdir()
        with zipfile.ZipFile(self.fixtures / "python", "w") as z:
            z.writestr("bin/python3", "#!/bin/sh\nunset PYTHONHOME\nexec " + shutil.which("python3") + ' "$@"\n')
        self.pm_archive("one")
        curl = self.app / "bin/curl"
        curl.write_text('#!/usr/bin/env python3\nimport os,sys,shutil,json\na=sys.argv[1:]\nopen(os.environ["CURL_LOG"],"a").write(json.dumps(a)+"\\n")\nshutil.copy(os.environ["FIXTURES"]+"/"+a[-1].split("/")[-1],a[a.index("--output")+1])\n')
        curl.chmod(0o755)
        sync = self.app / "bin/sync"
        sync.write_text("#!/bin/sh\nexit 0\n")
        sync.chmod(0o755)
        updater = self.app / "bin/pocket-ui"
        updater.write_text('''#!/usr/bin/env python3
import os,sys,json,hashlib,pathlib,zipfile
a=sys.argv[1:]
open(os.environ["CURL_LOG"],"a").write(json.dumps(a)+"\\n")
if a[0]=="--unpack-runtime":
 with zipfile.ZipFile(a[1]) as archive: archive.extractall(a[2])
 sys.exit(0)
assert a[0]=="--download-asset"
file="pm" if "/pm/" in a[1] else a[1].split("/")[-1]
data=pathlib.Path(os.environ["FIXTURES"]+"/"+file).read_bytes()
assert hashlib.sha256(data).hexdigest()==a[2] and len(data)==int(a[3])
pathlib.Path(a[4]).write_bytes(data)
if a[5]: pathlib.Path(a[5]).write_text("download|"+str(len(data))+"|"+a[3]+"\\n")
''')
        updater.chmod(0o755)
        self.env = dict(os.environ, BRICK_APP=str(self.app), BRICK_CARD=str(self.app.parents[1]),
                        BRICK_PORTMASTER_MIN_KB="0", CURL_LOG=str(self.root / "curl.log"),
                        FIXTURES=str(self.fixtures), BRICK_PORTMASTER_MANIFEST=str(self.root / "manifest"),
                        PATH=str(self.app / "bin") + ":" + os.environ["PATH"])
        self.manifest()

    def tearDown(self):
        self.tmp.cleanup()

    def pm_archive(self, version, unsafe=False, unsafe_fonts=False):
        with tarfile.open(self.fixtures / "pm", "w:gz") as t:
            files = {"upstream/PortMaster/harbourmaster": "print('fixture')", "upstream/PortMaster/gptokeyb": version,
                     "upstream/PortMaster/resources/do_init": "",
                     "upstream/PortMaster/pylibs/resources/Latin-Regular.ttf": "latin-font"}
            font_buffer = io.BytesIO()
            with tarfile.open(fileobj=font_buffer, mode="w:xz") as fonts:
                font = tarfile.TarInfo("../outside.ttf" if unsafe_fonts else "NotoSansJP-Regular.ttf")
                font.size = 4
                fonts.addfile(font, io.BytesIO(b"font"))
            files["upstream/PortMaster/pylibs/resources/NotoSans.tar.xz"] = font_buffer.getvalue()
            if unsafe:
                files["upstream/PortMaster/../../escape"] = "bad"
            for name, text in files.items():
                data = text.encode() if isinstance(text, str) else text
                info = tarfile.TarInfo(name)
                info.size = len(data)
                t.addfile(info, io.BytesIO(data))

    def manifest(self):
        (self.root / "manifest").write_text("\n".join(
            f"{kind}|{hashlib.sha256((self.fixtures / file).read_bytes()).hexdigest()}|{(self.fixtures / file).stat().st_size}|https://fixtures/{file + '/' + PM_REVISION if kind == 'portmaster' else file}"
            for kind, file in (("python", "python"), ("portmaster", "pm"))) + "\n")

    def run_helper(self, command, ok=True):
        shell = shlex.split(os.environ.get("BRICK_PM_TEST_SHELL", "sh"))
        r = subprocess.run([*shell, str(self.app / "bin/portmaster"), command], env=self.env, text=True, capture_output=True)
        if ok:
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        else:
            self.assertNotEqual(r.returncode, 0)
        return r

    def current(self):
        return self.app.parent / "portmaster/current"

    def test_install_is_persistent_and_ensure_is_offline(self):
        self.run_helper("ensure")
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "one")
        log = (self.root / "curl.log").read_text()
        self.assertIn('"--download-asset"', log)
        self.run_helper("ensure")
        self.assertEqual((self.root / "curl.log").read_text(), log)
        self.assertFalse(any(p.is_symlink() for p in self.current().rglob("*")))

    def test_bootstrap_generates_honest_source_version_and_extracts_unicode_fonts(self):
        self.run_helper("ensure")
        pm = self.current() / "PortMaster"
        self.assertEqual((pm / "version").read_text(), "git-" + PM_REVISION + "\n")
        self.assertEqual((pm / "pylibs/resources/NotoSansJP-Regular.ttf").read_bytes(), b"font")
        self.assertFalse((pm / "pylibs/resources/NotoSans.tar.xz").exists())
        for name in ("NotoSansJP-Regular.ttf", "Latin-Regular.ttf"):
            self.assertEqual((pm / "resources" / name).read_bytes(),
                             (pm / "pylibs/resources" / name).read_bytes())
        self.assertFalse((pm / "resources/do_init").exists())

    def test_font_initialization_failure_retains_marker_for_retry(self):
        spec = importlib.util.spec_from_file_location("extract_fixture", ROOT / "deploy/brick/portmaster-extract.py")
        helper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(helper)
        helper.extract(self.fixtures / "pm", self.root / "prepare")
        portable = helper.policy()
        actual_materialize = portable.materialize
        def fail_copy(tree, dest, opener, members=None):
            if Path(dest).name == "resources" and Path(dest).parent.name == "PortMaster":
                actual_materialize(tree, dest, opener, ["Latin-Regular.ttf"])
                raise OSError("fixture: card became full during font copy")
            return actual_materialize(tree, dest, opener, members)
        with patch.object(helper, "policy", return_value=portable), patch.object(portable, "materialize", side_effect=fail_copy):
            with self.assertRaisesRegex(OSError, "card became full"):
                helper.prepare(self.root / "prepare", PM_REVISION)
        pm = self.root / "prepare/PortMaster"
        self.assertTrue((pm / "resources/do_init").is_file())
        self.assertEqual((pm / "resources/Latin-Regular.ttf").read_bytes(), b"latin-font")
        self.assertFalse((pm / "resources/NotoSansJP-Regular.ttf").exists())
        self.assertFalse((pm / "version").exists())
        helper.prepare(self.root / "prepare", PM_REVISION)
        self.assertFalse((pm / "resources/do_init").exists())
        self.assertEqual((pm / "resources/Latin-Regular.ttf").read_bytes(), b"latin-font")

    def test_source_version_rejects_non_pinned_revision(self):
        manifest = self.root / "manifest"
        manifest.write_text(manifest.read_text().replace(PM_REVISION, "not-a-revision"))
        result = self.run_helper("ensure", ok=False)
        self.assertIn("pinned Git SHA", result.stderr)
        self.assertFalse(self.current().exists())

    def test_unsafe_nested_font_archive_never_replaces_active_runtime(self):
        self.run_helper("ensure")
        self.pm_archive("two", unsafe_fonts=True)
        self.manifest()
        self.run_helper("update", ok=False)
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "one")
        self.assertFalse((self.current() / "PortMaster/pylibs/outside.ttf").exists())
        self.assertFalse((self.current().parent / ".stage").exists())

    def test_bootstrap_uses_shared_archive_cli_without_firmware_unzip(self):
        marker = self.root / "unzip-was-called"
        self.env["NO_UNZIP_MARKER"] = str(marker)
        poison = self.app / "bin/unzip"
        poison.write_text('#!/bin/sh\nprintf called >"$NO_UNZIP_MARKER"\nexit 127\n')
        poison.chmod(0o755)
        self.run_helper("ensure")
        self.assertFalse(marker.exists(), "bootstrap called the absent firmware unzip tool")
        self.assertIn('"--unpack-runtime"', (self.root / "curl.log").read_text())
        self.assertTrue((self.current() / "runtime/bin/python3").is_file())

    def test_fat_mount_execution_does_not_require_per_file_chmod(self):
        updater = self.app / "bin/pocket-ui"
        # Model a FAT mount supplying executable mode independently of chmod.
        updater.write_text(updater.read_text().replace(
            'with zipfile.ZipFile(a[1]) as archive: archive.extractall(a[2])',
            'with zipfile.ZipFile(a[1]) as archive: archive.extractall(a[2])\n os.chmod(pathlib.Path(a[2])/"bin/python3",0o755)'))
        chmod = self.app / "bin/chmod"
        chmod.write_text('#!/bin/sh\necho "Operation not permitted" >&2\nexit 1\n')
        chmod.chmod(0o755)
        self.run_helper("ensure")
        self.assertIn('ready', self.run_helper("status").stdout)

    def test_runtime_execution_refusal_reports_mount_options_and_keeps_cache(self):
        with zipfile.ZipFile(self.fixtures / "python", "w") as archive:
            archive.writestr("bin/python3", '#!/bin/sh\nexit 126\n')
        self.manifest()
        result = self.run_helper("ensure", ok=False)
        self.assertIn('mount options (noexec)', result.stderr)
        self.assertFalse(self.current().exists())
        self.assertTrue(any((self.current().parent / "cache").iterdir()))

    def test_bad_download_leaves_current_untouched(self):
        self.run_helper("ensure")
        shutil.rmtree(self.current().parent / "cache", ignore_errors=True)
        (self.fixtures / "pm").write_bytes(b"cut off")
        self.run_helper("update", ok=False)
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "one")

    def test_retry_reuses_completed_artifacts_and_reports_progress(self):
        original = (self.fixtures / "pm").read_bytes()
        (self.fixtures / "pm").write_bytes(b"interrupted")
        self.env["BRICK_PORTMASTER_PROGRESS"] = str(self.root / "progress")
        self.run_helper("ensure", ok=False)
        self.assertTrue(list((self.app.parent / "portmaster/cache").iterdir()))
        (self.fixtures / "pm").write_bytes(original)
        self.run_helper("ensure")
        calls = (self.root / "curl.log").read_text()
        self.assertEqual(calls.count("https://fixtures/python"), 1)
        self.assertEqual(calls.count("https://fixtures/pm"), 2)
        self.assertEqual((self.root / "progress").read_text(), "ready|0|0\n")
        self.assertFalse((self.app.parent / "portmaster/cache").exists())

    def test_update_preserves_configuration_and_keeps_rollback(self):
        self.run_helper("ensure")
        (self.current() / "PortMaster/config").mkdir()
        (self.current() / "PortMaster/config/player.json").write_text("save")
        self.pm_archive("two")
        self.manifest()
        self.run_helper("update")
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "two")
        self.assertEqual((self.current() / "PortMaster/config/player.json").read_text(), "save")
        self.assertEqual((self.current().parent / "previous/PortMaster/gptokeyb").read_text(), "one")

    def test_ensure_installs_new_pin_and_refreshes_card_control(self):
        self.run_helper("ensure")
        (self.app / "portmaster/control.txt").write_text("new integration")
        self.run_helper("ensure")
        self.assertEqual((self.current() / "PortMaster/control.txt").read_text(), "new integration")
        self.pm_archive("new pin")
        self.manifest()
        self.run_helper("ensure")
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "new pin")

    def test_unsafe_archive_is_rejected_before_activation(self):
        self.run_helper("ensure")
        self.pm_archive("two", unsafe=True)
        self.manifest()
        self.run_helper("update", ok=False)
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "one")

    def test_interrupted_activation_restores_previous(self):
        self.run_helper("ensure")
        self.current().rename(self.current().parent / "previous")
        self.run_helper("ensure")
        self.assertTrue((self.current() / "PortMaster/harbourmaster").is_file())

    def test_disk_space_failure_does_not_start_download(self):
        self.env["BRICK_PORTMASTER_MIN_KB"] = "999999999999"
        self.run_helper("ensure", ok=False)
        self.assertFalse((self.root / "curl.log").exists())

    def fake_space(self, available):
        df = self.app / "bin/df"
        df.write_text(f'#!/bin/sh\nprintf "Filesystem 1024-blocks Used Available Capacity Mounted on\\ncard 1000000 500000 {available} 50%% /fixture\\n"\n')
        df.chmod(0o755)
        # BusyBox ash can prefer its compiled applets over PATH; use an explicit
        # tool path so both shells exercise the same fixed free-space response.
        self.env["BRICK_PORTMASTER_DF"] = str(df)
        self.env["BRICK_PORTMASTER_MIN_KB"] = "716800"

    def test_space_reports_actual_kib_without_creating_runtime(self):
        self.fake_space(123456)
        self.assertEqual(self.run_helper("space").stdout, "needed_kib=716800 available_kib=123456\n")
        self.assertFalse(self.current().parent.exists())

    def test_low_space_reports_typed_values_before_network(self):
        self.fake_space(42)
        result = self.run_helper("ensure", ok=False)
        self.assertIn("PORTMASTER_SPACE|716800|42", result.stderr)
        self.assertFalse((self.root / "curl.log").exists())

    def create_player_data(self):
        for relative, value in {"PortMaster/config/player.json": "player", "PortMaster/config/.hidden": "hidden",
                                "PortMaster/libs/engine.squashfs": "engine"}.items():
            target = self.current() / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(value)
        outside = [self.app.parents[1] / "Roms/ports/game/save.dat", self.app.parent / "storage" / "home" / ".config" / "game.sav",
                   self.current().parent / "unrelated/user.txt"]
        for path in outside:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("untouched")
        return outside

    def test_remove_preserves_ports_saves_configuration_and_engine_runtimes(self):
        self.run_helper("ensure")
        outside = self.create_player_data()
        self.run_helper("remove-runtime")
        self.assertFalse(self.current().exists())
        self.assertEqual((self.current().parent / "preserved/config/player.json").read_text(), "player")
        self.assertEqual((self.current().parent / "preserved/config/.hidden").read_text(), "hidden")
        self.assertEqual((self.current().parent / "preserved/libs/engine.squashfs").read_text(), "engine")
        self.assertIn("missing", self.run_helper("status", ok=False).stdout)
        archive = (self.fixtures / "pm").read_bytes()
        (self.fixtures / "pm").write_bytes(b"failed reinstall")
        self.run_helper("ensure", ok=False)
        self.assertEqual((self.current().parent / "preserved/config/player.json").read_text(), "player")
        self.assertEqual((self.current().parent / "preserved/libs/engine.squashfs").read_text(), "engine")
        (self.fixtures / "pm").write_bytes(archive)
        self.run_helper("ensure")
        self.assertEqual((self.current() / "PortMaster/config/player.json").read_text(), "player")
        self.assertEqual((self.current() / "PortMaster/libs/engine.squashfs").read_text(), "engine")
        for path in outside:
            self.assertEqual(path.read_text(), "untouched")

    def test_interrupted_preservation_merges_without_discarding_old_entries(self):
        self.run_helper("ensure")
        self.create_player_data()
        preserved = self.current().parent / "preserved/config"
        preserved.mkdir(parents=True)
        (preserved / "earlier.json").write_text("earlier")
        (preserved / "player.json").write_text("stale")
        self.run_helper("remove-runtime")
        self.assertEqual((preserved / "earlier.json").read_text(), "earlier")
        self.assertEqual((preserved / "player.json").read_text(), "player")

    def test_removal_retry_keeps_latest_preserved_data_over_previous_snapshot(self):
        for current_still_exists in (True, False):
            with self.subTest(current=current_still_exists):
                self.run_helper("ensure")
                self.create_player_data()
                data = self.current().parent
                old = data / "previous/PortMaster/config"
                old.mkdir(parents=True)
                (old / "player.json").write_text("older snapshot")
                saved = data / "preserved"
                saved.mkdir()
                (self.current() / "PortMaster/config").rename(saved / "config")
                if not current_still_exists:
                    (self.current() / "PortMaster/libs").rename(saved / "libs")
                    shutil.rmtree(self.current())
                self.run_helper("remove-runtime")
                self.assertEqual((saved / "config/player.json").read_text(), "player")
                self.assertEqual((saved / "libs/engine.squashfs").read_text(), "engine")

    def test_remove_recovers_previous_player_data_and_cleans_only_known_folders(self):
        self.run_helper("ensure")
        outside = self.create_player_data()
        data = self.current().parent
        self.current().rename(data / "previous")
        for folder in (".stage", "cache"):
            (data / folder).mkdir()
            (data / folder / "partial").write_text("temporary")
        self.run_helper("remove-runtime")
        for folder in ("current", "previous", ".stage", "cache"):
            self.assertFalse((data / folder).exists())
        self.assertEqual((data / "preserved/config/player.json").read_text(), "player")
        self.assertEqual((data / "preserved/libs/engine.squashfs").read_text(), "engine")
        for path in outside:
            self.assertEqual(path.read_text(), "untouched")

    def test_remove_refuses_active_installer_lock(self):
        self.run_helper("ensure")
        lock = self.current().parent / ".lock"
        lock.mkdir()
        (lock / "pid").write_text(str(os.getpid()))
        self.assertIn("another installation", self.run_helper("remove-runtime", ok=False).stderr)
        self.assertTrue((self.current() / ".ready").exists())

    def test_sigterm_checkpoints_download_and_keeps_active_generation(self):
        self.run_helper("ensure")
        self.pm_archive("two")
        self.manifest()
        updater = self.app / "bin/pocket-ui"
        original = updater.read_text()
        started = self.root / "downloader-started"
        self.env["DOWNLOADER_STARTED"] = str(started)
        updater.write_text('#!/usr/bin/env python3\nimport os,sys,signal,time,pathlib\na=sys.argv[1:]\ndef stopped(*args):\n pathlib.Path(a[4]).write_bytes(b"partial checkpoint")\n sys.exit(1)\nsignal.signal(signal.SIGTERM,stopped)\npathlib.Path(os.environ["DOWNLOADER_STARTED"]).write_text(str(os.getpid()))\nwhile True: time.sleep(.02)\n')
        shell = shlex.split(os.environ.get("BRICK_PM_TEST_SHELL", "sh"))
        worker = subprocess.Popen([*shell, str(self.app / "bin/portmaster"), "update"], env=self.env,
                                  text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            until = time.monotonic() + 3
            while not started.exists() and time.monotonic() < until:
                time.sleep(.02)
            self.assertTrue(started.exists(), "fake downloader did not start")
            worker.send_signal(signal.SIGTERM)
            worker.communicate(timeout=5)
            self.assertNotEqual(worker.returncode, 0)
            self.assertFalse(Path(f"/proc/{started.read_text()}").exists(), "cancel left its downloader running")
            self.assertTrue(any(p.read_bytes() == b"partial checkpoint" for p in (self.current().parent / "cache").iterdir()))
            self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "one")
            self.assertFalse((self.current().parent / ".stage").exists())
            self.assertFalse((self.current().parent / ".lock").exists())
        finally:
            if worker.poll() is None:
                worker.terminate()
                worker.wait(timeout=5)
        updater.write_text(original)
        self.run_helper("ensure")
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "two")

    def test_harbourmaster_sigterm_stops_held_curl_cleanly(self):
        started = self.root / "curl-started"
        dest = self.root / "held-download"
        self.env.update(DOWNLOADER_STARTED=str(started), CANCEL_DEST=str(dest),
                        BRICK_PORTMASTER_PROGRESS=str(self.root / "progress"))
        (self.app / "bin/curl").write_text('#!/usr/bin/env python3\nimport os,sys,time,pathlib\na=sys.argv[1:]\npathlib.Path(a[a.index("--output")+1]).write_bytes(b"unfinished port")\npathlib.Path(os.environ["DOWNLOADER_STARTED"]).write_text(str(os.getpid()))\nwhile True: time.sleep(.02)\n')
        harness = self.root / "cancel-worker.py"
        harness.write_text('import importlib.util,sys,os,pathlib\n'
                           f'spec=importlib.util.spec_from_file_location("bridge",{str(ROOT / "deploy/brick/portmaster-bridge.py")!r})\n'
                           'bridge=importlib.util.module_from_spec(spec);spec.loader.exec_module(bridge)\n'
                           'sys.exit(bridge.cancellation_guard(lambda: bridge.transfer("https://fixtures/held",pathlib.Path(os.environ["CANCEL_DEST"]))))\n')
        worker = subprocess.Popen([shutil.which("python3"), str(harness)], env=self.env,
                                  text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            until = time.monotonic() + 3
            while not started.exists() and worker.poll() is None and time.monotonic() < until:
                time.sleep(.02)
            self.assertTrue(started.exists(), "held curl did not start")
            worker.send_signal(signal.SIGTERM)
            output, errors = worker.communicate(timeout=5)
            self.assertEqual(worker.returncode, 130, output + errors)
            self.assertIn("Download cancelled.", errors)
            self.assertNotIn("Traceback", errors)
            self.assertFalse(Path(f"/proc/{started.read_text()}").exists(), "cancel left curl running")
            self.assertFalse(Path(str(dest) + ".part").exists())
            self.assertEqual((self.root / "progress").read_text(), "cancelled|0|0\n")
        finally:
            if worker.poll() is None:
                worker.terminate()
                worker.wait(timeout=5)

    def test_sigterm_stops_unpack_and_validation_before_staging_cleanup(self):
        self.run_helper("ensure")
        host_python = shutil.which("python3")
        delayed = self.root / "delayed-runtime-python.py"
        delayed.write_text('import os,sys,time,pathlib\n'
                           'stage="validate" if sys.argv[1]=="-c" else "fonts" if "--prepare" in sys.argv else "portmaster"\n'
                           'if os.environ.get("HOLD_INSTALL_STAGE")==stage:\n'
                           ' pathlib.Path(os.environ["EXTRACTOR_STARTED"]).write_text(str(os.getpid()))\n'
                           ' time.sleep(1.2)\n'
                           ' pathlib.Path(os.environ["LATE_EXTRACT_WRITE"]).write_text("orphan write")\n'
                           f'os.execv({host_python!r},[{host_python!r},*sys.argv[1:]])\n')
        with zipfile.ZipFile(self.fixtures / "python", "w") as archive:
            archive.writestr("bin/python3", '#!/bin/sh\nunset PYTHONHOME\nexec '
                             + shlex.quote(host_python) + ' ' + shlex.quote(str(delayed)) + ' "$@"\n')
        self.pm_archive("two")
        self.manifest()
        updater = self.app / "bin/pocket-ui"
        updater.write_text(updater.read_text().replace('import os,sys,json,hashlib,pathlib,zipfile',
                           'import os,sys,json,hashlib,pathlib,zipfile,time').replace(
                           'if a[0]=="--unpack-runtime":\n',
                           'if a[0]=="--unpack-runtime":\n'
                           ' if os.environ.get("HOLD_INSTALL_STAGE")=="python":\n'
                           '  pathlib.Path(os.environ["EXTRACTOR_STARTED"]).write_text(str(os.getpid()))\n'
                           '  time.sleep(1.2)\n'
                           '  pathlib.Path(os.environ["LATE_EXTRACT_WRITE"]).write_text("orphan write")\n'))
        shell = shlex.split(os.environ.get("BRICK_PM_TEST_SHELL", "sh"))
        for stage in ("python", "portmaster", "fonts", "validate"):
            with self.subTest(stage=stage):
                started, late = self.root / (stage + "-started"), self.root / (stage + "-late")
                self.env.update(HOLD_INSTALL_STAGE=stage, EXTRACTOR_STARTED=str(started),
                                LATE_EXTRACT_WRITE=str(late))
                worker = subprocess.Popen([*shell, str(self.app / "bin/portmaster"), "update"],
                                          env=self.env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                child_pid = None
                try:
                    until = time.monotonic() + 3
                    while not started.exists() and worker.poll() is None and time.monotonic() < until:
                        time.sleep(.02)
                    self.assertTrue(started.exists(), "held extractor did not start")
                    child_pid = int(started.read_text())
                    worker.send_signal(signal.SIGTERM)
                    worker.communicate(timeout=5)
                    self.assertNotEqual(worker.returncode, 0)
                    self.assertFalse(Path(f"/proc/{child_pid}").exists(), "cancel left its extractor running")
                    self.assertFalse(late.exists(), "extractor wrote after cancellation")
                    self.assertFalse((self.current().parent / ".stage").exists())
                    self.assertFalse((self.current().parent / ".lock").exists())
                    self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "one")
                    checksum = hashlib.sha256((self.fixtures / "python").read_bytes()).hexdigest()
                    self.assertEqual((self.current().parent / "cache" / checksum).read_bytes(),
                                     (self.fixtures / "python").read_bytes())
                finally:
                    if worker.poll() is None:
                        worker.terminate()
                        worker.wait(timeout=5)
                    if child_pid is not None:
                        try:
                            os.kill(child_pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
        calls = (self.root / "curl.log").read_text().count('"--download-asset"')
        self.env["HOLD_INSTALL_STAGE"] = ""
        self.run_helper("ensure")
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "two")
        self.assertEqual((self.root / "curl.log").read_text().count('"--download-asset"'), calls)

    def test_ready_run_exports_runtime(self):
        (self.app / "bin/portmaster-run").write_text('#!/bin/sh\nprintf "%s\\n" "$BRICK_PORTMASTER_HOME" "$BRICK_PORTMASTER_RUNTIME" "$HOME"\n')
        self.run_helper("ensure")
        r = self.run_helper("run")
        self.assertIn(str(self.current() / "PortMaster"), r.stdout)
        self.assertIn(str(self.current() / "runtime"), r.stdout)
        self.assertIn(str(self.app.parent / "storage/home"), r.stdout)

    def test_status_is_read_only_and_missing_run_does_not_download(self):
        self.assertIn("missing", self.run_helper("status", ok=False).stdout)
        self.assertFalse((self.app.parent / "portmaster").exists())
        self.run_helper("run", ok=False)
        self.assertFalse((self.root / "curl.log").exists())
        self.run_helper("ensure")
        self.assertIn("ready", self.run_helper("status").stdout)

    def test_tar_links_are_rejected_for_fat_cards(self):
        self.run_helper("ensure")
        with tarfile.open(self.fixtures / "pm", "w:gz") as t:
            info = tarfile.TarInfo("upstream/PortMaster/link")
            info.type = tarfile.SYMTYPE
            info.linkname = "/etc/passwd"
            t.addfile(info)
        self.manifest()
        self.run_helper("update", ok=False)
        self.assertEqual((self.current() / "PortMaster/gptokeyb").read_text(), "one")

    def test_store_download_checks_md5_and_cleans_failed_result(self):
        spec = importlib.util.spec_from_file_location("pm_bridge", ROOT / "deploy/brick/portmaster-bridge.py")
        bridge = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bridge)
        payload = b"small free game"
        (self.fixtures / "game").write_bytes(payload)
        dest = self.root / "a folder/game.zip"
        result = [None]
        with patch.dict(os.environ, self.env):
            self.assertEqual(bridge.download(dest, "https://fixtures/game", hashlib.md5(payload).hexdigest(), result), dest)
            self.assertEqual(result[0], hashlib.md5(payload).hexdigest())
            self.assertIsNone(bridge.download(dest, "https://fixtures/game", "0" * 32))
        self.assertFalse(dest.exists())
        self.assertFalse(Path(str(dest) + ".part").exists())
        self.assertIn('"--limit-rate", "512k"', (self.root / "curl.log").read_text())

    def test_redirect_zero_content_length_is_unknown_to_upstream_progress(self):
        spec = importlib.util.spec_from_file_location("pm_bridge", ROOT / "deploy/brick/portmaster-bridge.py")
        bridge = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bridge)
        dest = self.root / "download"
        Path(str(dest) + ".headers").write_text("HTTP/2 302\ncontent-length: 0\n")
        self.assertIsNone(bridge.response_size(dest))
        Path(str(dest) + ".headers").write_text("HTTP/2 302\ncontent-length: 0\nHTTP/2 200\ncontent-length: 2048\n")
        self.assertEqual(bridge.response_size(dest), 2048)

    def test_download_rejects_plain_http(self):
        r = subprocess.run(["sh", str(self.app / "portmaster/portmaster-download.sh"),
                            "http://fixtures/game", str(self.root / "game")], env=self.env, capture_output=True)
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse((self.root / "curl.log").exists())


if __name__ == "__main__":
    unittest.main()

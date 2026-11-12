#!/usr/bin/env python3
"""Offline Android channel regressions; requires STORE_TOOL (the existing store-tool binary).

Uses throwaway signing keys, no network, SDK or real signing key.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from urllib.parse import urlunsplit

HERE = Path(__file__).resolve().parent
TOOL = os.environ.get("STORE_TOOL") or shutil.which("store-tool")


class AndroidChannel(unittest.TestCase):
    def setUp(self):
        self.assertTrue(TOOL, "set STORE_TOOL to the built store-tool binary")
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.apk = self.root / "pocket-ui-android.apk"
        self.apk.write_bytes(b"APK fixture bytes")
        self.key = self.root / "throwaway.key"
        subprocess.run([TOOL, "keygen", str(self.key)], check=True, capture_output=True)
        self.out = self.root / "out"
        self.manifest = self.out / "updates/android/update.json"
        self.version = "0.4.0-android.2"

    def run_channel(self, *args):
        return subprocess.run([sys.executable, "-I", str(HERE / "release-channel.py"), *map(str, args)],
                              capture_output=True, text=True, env={**os.environ, "STORE_TOOL": TOOL})

    def generate(self, *args, foreign=True, version=None, url=None):
        return self.run_channel("--apk", self.apk, "--version", version or self.version,
                                "--url", url or "https://example.invalid/releases/0.4.0-android.2/pocket-ui-android.apk",
                                "--out", self.out, "--key", self.key,
                                *(["--allow-foreign-key"] if foreign else []), *args)

    def check(self, *args):
        return self.run_channel("--check", self.manifest, "--apk", self.apk, "--version", self.version,
                                "--key", self.key, "--allow-foreign-key", *args)

    def test_signed_offer_matches_apk_and_android_version(self):
        run = self.generate("--notes", "Update over Wi-Fi")
        self.assertEqual(run.returncode, 0, run.stderr)
        data = json.loads(self.manifest.read_bytes())
        self.assertEqual(data["format"], 1)
        self.assertEqual(data["app"], {"version": self.version,
                         "url": "https://example.invalid/releases/0.4.0-android.2/pocket-ui-android.apk",
                         "sha256": hashlib.sha256(self.apk.read_bytes()).hexdigest(),
                         "size": self.apk.stat().st_size, "notes": "Update over Wi-Fi"})
        subprocess.run([TOOL, "verify", str(self.manifest), str(self.key)], check=True, capture_output=True)
        self.assertEqual(self.check().returncode, 0)
        self.assertEqual((self.manifest.parent / self.version / self.apk.name).read_bytes(), self.apk.read_bytes())
        self.assertEqual(len(base64.b64decode(self.manifest.with_suffix(".json.sig").read_bytes())), 64)
        secret = self.key.read_bytes()  # Test-generated key only.
        for path in self.out.rglob("*"):
            if path.is_file():
                self.assertNotIn(secret, path.read_bytes())

    def test_unknown_key_cannot_prepare_a_release(self):
        run = self.generate(foreign=False)
        self.assertNotEqual(run.returncode, 0)
        self.assertIn("PUBLIC_KEY", run.stderr)
        self.assertFalse(self.out.exists())

    def test_check_rejects_changed_manifest_signature_and_apk(self):
        self.assertEqual(self.generate().returncode, 0)
        original = self.manifest.read_bytes()
        self.manifest.write_bytes(original + b" ")
        self.assertNotEqual(self.check().returncode, 0, "changed signed bytes must be refused")
        self.manifest.write_bytes(original)
        sig = self.manifest.with_suffix(".json.sig")
        original_sig = sig.read_bytes()
        sig.write_text(base64.b64encode(bytes(64)).decode())
        self.assertNotEqual(self.check().returncode, 0, "wrong signer must be refused")
        sig.write_bytes(original_sig)
        self.apk.write_bytes(b"tampered APK bytes")
        run = self.check()
        self.assertNotEqual(run.returncode, 0)
        self.assertIn("APK", run.stderr)

    def test_versions_and_insecure_urls_are_refused(self):
        for version in ["0.4.0", "0.4.0-android.100", "0.4.0-android.2/escape", "0.4.0-android.0"]:
            with self.subTest(version=version):
                self.assertNotEqual(self.generate(version=version).returncode, 0)
        for url in ["http://example.invalid/app.apk", "https://", urlunsplit(("https", "user:fixture@example.invalid", "/app.apk", "", "")), "latest.apk"]:
            with self.subTest(url=url):
                self.assertNotEqual(self.generate(url=url).returncode, 0)
        self.assertFalse(self.out.exists())

    def test_local_channel_keeps_old_offers_and_refuses_changed_version(self):
        self.assertEqual(self.generate(url=self.apk.as_uri()).returncode, 0)
        old_apk = self.manifest.parent / self.version / self.apk.name
        self.version = "0.4.0-android.3"
        self.assertEqual(self.generate().returncode, 0)
        self.assertTrue(old_apk.is_file(), "players may still have the previous offer open")
        manifest = self.manifest.read_bytes()
        self.apk.write_bytes(b"different bytes for already prepared version")
        self.assertNotEqual(self.generate().returncode, 0)
        self.assertEqual(self.manifest.read_bytes(), manifest)

    def test_missing_and_malformed_keys_leave_output_untouched(self):
        self.key.write_bytes(b"not-a-32-byte-key")
        run = self.generate()
        self.assertNotEqual(run.returncode, 0)
        self.assertNotIn("not-a-32-byte-key", run.stdout + run.stderr)
        self.key.unlink()
        self.assertNotEqual(self.generate().returncode, 0)
        self.assertFalse(self.out.exists())


if __name__ == "__main__":
    unittest.main()

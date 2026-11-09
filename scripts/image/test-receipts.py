#!/usr/bin/env python3
"""Release fixtures: no images built, Docker, hosts or real GitHub writes."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
CLI = HERE / 'receipt.py'
UPLOAD = HERE.parent / 'upload-images.sh'
VERSION = '9.8.7'


class Receipts(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='image-receipts-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.image = self.root / f'pocket-ui-{VERSION}-rp-mini-v2.img.xz'
        self.image.write_bytes(b'fixture image bytes')
        self.notes(self.image).write_text('fixture notes\n')
        self.minimum = int(re.search(r'^P2_MIN_HEADROOM_BLOCKS=(\d+)$',
                                    (HERE/'pins.sh').read_text(), re.M)[1])
        self.calls = self.root/'gh-calls'
        tools = self.root/'bin'
        tools.mkdir()
        gh = tools/'gh'
        gh.write_text('#!/bin/sh\nprintf "%s\\n" "$@" >>"$RECEIPT_GH_CALLS"\n')
        gh.chmod(0o755)
        self.env = dict(os.environ, PATH=str(tools)+':'+os.environ['PATH'],
                        RECEIPT_GH_CALLS=str(self.calls))
        self.sums()

    def notes(self, image):
        return image.with_name(image.name.removesuffix('.img.xz')+'.txt')

    def sidecar(self, image=None):
        return Path(str(image or self.image)+'.receipt.json')

    def digest(self, path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def sums(self, images=None):
        images = images or [self.image]
        lines = ''.join(f'{self.digest(p)}  {p.name}\n' for i in images for p in (i,self.notes(i)))
        (self.root/'SHA256SUMS').write_text(lines)
        (self.root/'SHA256SUMS-images').write_text(lines)
        return lines

    def block(self, image=None, free=None):
        image = image or self.image
        free = self.minimum+140 if free is None else free
        return (f'== {image.name} (version {VERSION})\n'
                'ok   SHA256SUMS matches\n'
                "ok   STORAGE survives the first boot's resize on any card, 8 GB to 2 TB (grow-check.sh)\n"
                f'STORAGE before the first boot: 27M used of 32 MB, {free} of 7027 blocks (4 KiB) free\n'
                f'ok   STORAGE keeps at least {self.minimum} blocks ({self.minimum*4} KiB) free for the programs to grow\n'
                'all checks passed (0 warnings)\n'
                f'IMAGE OK: {image.name}\n')

    def log(self, image=None, content=None):
        image = image or self.image
        path = self.root/(image.stem+'.log')
        path.write_text(content if content is not None else self.sums([image])+self.block(image)+'rc=0\n')
        return path

    def cli(self, *args, ok=True):
        p = subprocess.run([sys.executable,'-I',str(CLI),*map(str,args)],env=self.env,
                           capture_output=True,text=True,timeout=15)
        if ok:
            self.assertEqual(p.returncode,0,p.stderr+p.stdout)
        else:
            self.assertNotEqual(p.returncode,0,p.stdout)
        return p

    def checked(self, image=None, content=None):
        image = image or self.image
        self.cli('import-log',image,'--version',VERSION,'--log',self.log(image,content),
                 '--sums',self.root/'SHA256SUMS')

    def release(self):
        self.cli('release',self.root,'--tag','v'+VERSION)

    def upload(self, ok, *args):
        p = subprocess.run(['sh',str(UPLOAD),'v'+VERSION,str(self.root),'--repo','example/test',*args],
                           env=self.env,capture_output=True,text=True,timeout=15)
        self.assertEqual(p.returncode == 0,ok,p.stderr+p.stdout)
        if not ok:
            self.assertFalse(self.calls.exists(),'upload started before every artifact was verified')
        return p

    def test_assembled_hashes_are_not_a_checked_release(self):
        self.cli('assemble',self.image,'--version',VERSION)
        self.assertEqual(json.loads(self.sidecar().read_text())['state'],'assembled')
        self.cli('release',self.root,'--tag','v'+VERSION,ok=False)
        self.upload(False)

    def test_checked_release_uploads_only_the_explicit_assets(self):
        self.checked()
        check = json.loads(self.sidecar().read_text())
        self.assertEqual(check['state'],'checked')
        self.assertEqual(check['check']['storage']['minimum_blocks'],self.minimum)
        self.release()
        self.cli('verify',self.root,'--tag','v'+VERSION)
        dry = self.upload(True,'--dry-run')
        self.assertIn(self.image.name,dry.stdout)
        self.assertFalse(self.calls.exists())
        self.upload(True)
        calls = self.calls.read_text().splitlines()
        self.assertIn(str(self.image),calls)
        self.assertIn(str(self.notes(self.image)),calls)
        self.assertIn(str(self.root/'SHA256SUMS-images'),calls)
        self.assertNotIn(str(self.root/'SHA256SUMS'),calls)
        self.assertNotIn(str(self.sidecar()),calls)

    def test_changed_bytes_with_the_same_size_and_mtime_are_refused(self):
        self.checked()
        self.release()
        old = self.image.stat()
        self.image.write_bytes(b'X'*old.st_size)
        os.utime(self.image,ns=(old.st_atime_ns,old.st_mtime_ns))
        self.upload(False)

    def test_changed_notes_or_receipt_or_manifest_are_refused(self):
        for which in ('notes','receipt','manifest'):
            with self.subTest(which=which):
                self.notes(self.image).write_text('fixture notes\n')
                self.sums()
                self.checked()
                self.release()
                target = {'notes':self.notes(self.image),'receipt':self.sidecar(),
                          'manifest':self.root/'SHA256SUMS-images'}[which]
                target.write_bytes(target.read_bytes()+b'\nchanged')
                self.upload(False)

    def test_old_success_with_new_checksum_files_does_not_certify_new_bytes(self):
        log = self.log()
        self.image.write_bytes(b'new image')
        self.sums()
        self.cli('import-log',self.image,'--version',VERSION,'--log',log,
                 '--sums',self.root/'SHA256SUMS',ok=False)
        self.upload(False)

    def test_unbound_truncated_wrong_or_failed_logs_are_refused(self):
        good = self.sums()+self.block()
        cases = [self.block(),good.replace('IMAGE OK:','IMAGE NOT OK:'),
                 good.replace(self.image.name+' (version','wrong.img.xz (version'),
                 good.replace('IMAGE OK: '+self.image.name,'IMAGE OK: wrong.img.xz'),
                 good.split('IMAGE OK:')[0],good+'FAIL late failure\n',
                 good.replace('all checks passed (0 warnings)','all checks passed (1 warnings)')]
        for content in cases:
            with self.subTest(content=content[-80:]):
                self.cli('import-log',self.image,'--version',VERSION,'--log',self.log(content=content),
                         '--sums',self.root/'SHA256SUMS',ok=False)

    def test_a_later_failed_attempt_cannot_reuse_earlier_green(self):
        good = self.sums()+self.block()
        failed = f'== {self.image.name} (version {VERSION})\nFAIL interrupted\n'
        self.cli('import-log',self.image,'--version',VERSION,'--log',self.log(content=good+failed),
                 '--sums',self.root/'SHA256SUMS',ok=False)
        self.upload(False)

    def test_landed_storage_budget_and_grow_check_are_mandatory(self):
        for block in (self.block(free=self.minimum-1),self.block().replace('ok   STORAGE survives','WARN STORAGE survives'),
                      self.block().replace('ok   STORAGE keeps','WARN STORAGE keeps')):
            self.cli('import-log',self.image,'--version',VERSION,'--log',self.log(content=self.sums()+block),
                     '--sums',self.root/'SHA256SUMS',ok=False)

    def test_known_missing_binary_runtime_check_is_preserved_as_a_limitation(self):
        block = self.block().replace('IMAGE OK:',
            'WARN pocket-vdev-base:20261001 missing (scripts/vdev.sh image, needs an aarch64-capable Docker host): --version not checked\nIMAGE OK:')
        self.checked(content=self.sums()+block)
        self.assertFalse(json.loads(self.sidecar().read_text())['check']['binary_version_checked'])
        self.release()

    def test_failed_or_mutating_checker_leaves_no_checked_receipt(self):
        for mode in ('fail','mutate'):
            self.image.write_bytes(b'fixture image bytes')
            self.sums()
            self.checked()
            code = 'import pathlib,sys; '
            if mode == 'mutate':
                code += f'pathlib.Path({str(self.image)!r}).write_bytes(b"different"); '
            code += f'print({self.block()!r}); sys.exit({7 if mode == "fail" else 0})'
            self.cli('run-check',self.image,'--version',VERSION,'--',sys.executable,'-c',code,ok=False)
            self.assertEqual(json.loads(self.sidecar().read_text())['state'],'assembled')

    def test_successful_fresh_checker_records_the_exact_checked_bytes(self):
        self.cli('run-check',self.image,'--version',VERSION,'--',sys.executable,'-c',f'print({self.block()!r})')
        self.assertEqual(json.loads(self.sidecar().read_text())['state'],'checked')
        self.release()

    def test_real_check_entrypoints_record_success_and_retire_stale_green(self):
        work = self.root/'work'
        (work/'releases'/VERSION).mkdir(parents=True)
        body = '\n'.join(self.block().splitlines()[1:-1])
        docker = self.root/'bin/docker'
        docker.write_text(f'''#!{sys.executable}
import os,sys
if sys.argv[1:3] == ['image','inspect']:
    sys.exit(1 if 'pocket-vdev-base:' in sys.argv[-1] else 0)
if any(a in ('/repo/scripts/image/check.sh','/repo/scripts/image/check-h700.sh') for a in sys.argv):
    print({body!r})
    sys.exit(int(os.environ.get('FIXTURE_CHECK_FAIL','0')))
sys.exit(0)
''')
        docker.chmod(0o755)
        for script,model in (('check-image.sh','rp-mini-v2'),('check-image-family.sh','rg28xx')):
            image = self.root/f'pocket-ui-{VERSION}-{model}.img.xz'
            image.write_bytes(b'fixture image bytes')
            self.notes(image).write_text('fixture notes\n')
            self.sums([image])
            env = dict(self.env,CHECK_WORK=str(work))
            env.pop('POCKET_IMAGE_RECEIPT_CHILD',None)
            cmd = ['sh',str(HERE.parent/script),str(image),'--version',VERSION]
            good = subprocess.run(cmd,env=env,capture_output=True,text=True,timeout=20)
            self.assertEqual(good.returncode,0,good.stderr+good.stdout)
            self.assertEqual(json.loads(self.sidecar(image).read_text())['state'],'checked')
            bad = subprocess.run(cmd,env=dict(env,FIXTURE_CHECK_FAIL='1'),capture_output=True,text=True,timeout=20)
            self.assertNotEqual(bad.returncode,0,bad.stdout)
            self.assertEqual(json.loads(self.sidecar(image).read_text())['state'],'assembled')

    def test_import_release_handles_a_shared_family_log_without_rebuilding(self):
        other = self.root/f'pocket-ui-{VERSION}-rg28xx.img.xz'
        other.write_bytes(b'other image')
        self.notes(other).write_text('other notes')
        log = self.log(content=self.sums([self.image,other])+self.block()+self.block(other))
        self.cli('import-release',self.root,'--tag','v'+VERSION,'--log',log)
        self.cli('verify',self.root,'--tag','v'+VERSION)
        self.assertEqual(json.loads(self.sidecar(other).read_text())['state'],'checked')

    def test_ambiguous_import_logs_cannot_create_a_release(self):
        first = self.log()
        second = self.root/'another.log'
        second.write_bytes(first.read_bytes())
        self.cli('import-release',self.root,'--tag','v'+VERSION,'--log',first,'--log',second,ok=False)
        self.assertFalse((self.root/'image-release.receipt.json').exists())

    def test_all_images_are_verified_before_the_first_upload_call(self):
        other = self.root/f'pocket-ui-{VERSION}-rg28xx.img.xz'
        other.write_bytes(b'other image')
        self.notes(other).write_text('other notes')
        shared = self.sums([self.image,other])+self.block()+self.block(other)
        for image in (self.image,other):
            self.checked(image,shared)
        self.sums([self.image,other])
        self.release()
        other.write_bytes(b'damaged')
        self.upload(False)

    def test_missing_unlisted_and_duplicate_assets_are_refused(self):
        self.checked()
        self.notes(self.image).unlink()
        self.cli('release',self.root,'--tag','v'+VERSION,ok=False)
        self.notes(self.image).write_text('fixture notes\n')
        self.sums()
        self.checked()
        extra = self.root/f'pocket-ui-{VERSION}-unknown.img.xz'
        extra.write_bytes(b'unchecked extra image')
        self.cli('release',self.root,'--tag','v'+VERSION,ok=False)
        extra.unlink()
        manifest = self.root/'SHA256SUMS-images'
        manifest.write_text(manifest.read_text()*2)
        self.cli('release',self.root,'--tag','v'+VERSION,ok=False)


if __name__ == '__main__':
    unittest.main()

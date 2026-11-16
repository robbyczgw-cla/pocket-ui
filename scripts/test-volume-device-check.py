#!/usr/bin/env python3
"""Regression for serial gain, hardware-topology refusal and restoration after abort."""
import copy
import fcntl
import hashlib
import importlib.util
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('volume_check',HERE/'volume-device-check.py')
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)
CAPTURE = (HERE.parent/'testdata/audio/h616-speaker-topology.txt').read_text()
SPEAKER = CAPTURE.split('# /sys/kernel/debug/asoc/H616 Audio Codec/dapm/Speaker\n')[1].split('# /sys/')[0]
LINEOUT = CAPTURE.split('# /sys/kernel/debug/asoc/H616 Audio Codec/5096000.codec/dapm/LINEOUT\n')[1].split('# Device tree')[0]
TOPOLOGY = dict(speaker=SPEAKER,lineout=LINEOUT,mixer=CAPTURE)
ON = {'Speaker Switch':'on','DAC Playback Volume':'44','DAC Playback Switch':'on,on',
      'Line Out Playback Volume':'19','Line Out Playback Switch':'on,on'}


class FakeDevice:
    def __init__(self, abort_at=None, old_curve=False):
        self.volume = 75
        self.abort_at = abort_at
        self.old_curve = old_curve
        self.restored = False

    def idle(self): pass

    def snapshot(self):
        return dict(volume=self.volume,executable_sha256='a'*64,topology=TOPOLOGY)

    def set_volume(self,percent): self.volume=percent

    def sample(self,percent):
        if percent == self.abort_at: raise RuntimeError('physical input')
        # Hardware steps captured on the Plus, not inferred from the production DAC helper.
        line = {10:2,20:4,30:11,40:16,50:19,60:23,70:25,80:28,90:30,100:31}[percent]
        controls = dict(ON,**{'Line Out Playback Volume':str(line)})
        if self.old_curve and percent==50: controls['DAC Playback Volume']='31'
        soft = min(1,10**((60*math.log10(percent/100)-(-46.5+line*1.5))/20))
        row=check.gain_row(percent,controls,dict(softVolumes=[soft,soft]))
        row['pcm']='closed'
        return row

    def restore(self,before):
        self.volume=before['volume']
        self.restored=True
        return dict(exact=True,errors=[])


class VolumeCheckTests(unittest.TestCase):
    def test_old_double_attenuation_fails_and_fixed_maximum_passes(self):
        old = dict(ON,**{'DAC Playback Volume':'31'})
        row=check.gain_row(50,old,dict(softVolumes=[1,1]))
        self.assertAlmostEqual(row['measured_db'][0],-55.12)
        self.assertFalse(row['passed'])
        fixed=check.gain_row(50,ON,dict(softVolumes=[1,1]))
        self.assertAlmostEqual(fixed['measured_db'][0],-40.04)
        self.assertTrue(fixed['passed'])
        maximum=check.gain_row(100,dict(ON,**{'Line Out Playback Volume':'31'}),dict(softVolumes=[1,1]))
        self.assertAlmostEqual(maximum['measured_db'][0],-22.04)
        self.assertTrue(maximum['passed'])

    def test_unknown_topology_or_tlv_is_not_treated_as_h700(self):
        check.validate_topology(**TOPOLOGY)
        with self.assertRaises(AssertionError):
            check.validate_topology(**dict(TOPOLOGY,speaker=SPEAKER.replace('LINEOUT','DAC')))
        with self.assertRaises(AssertionError):
            check.validate_topology(**dict(TOPOLOGY,mixer=CAPTURE.replace('step=1.16dB','step=1.00dB')))

    def test_pass_checks_every_stage_and_restores_original_volume(self):
        device=FakeDevice()
        result=check.probe(device,wait=lambda _:None)
        self.assertEqual(result['status'],'pass')
        self.assertEqual(device.volume,75)
        self.assertTrue(device.restored)
        check.verify(result,'a'*64)

    def test_curve_failure_and_interruption_both_restore(self):
        for device in (FakeDevice(old_curve=True),FakeDevice(abort_at=40)):
            result=check.probe(device,wait=lambda _:None)
            self.assertEqual(result['status'],'fail')
            self.assertTrue(device.restored)
            self.assertEqual(device.volume,75)

    def test_release_refuses_stale_wrong_binary_or_forged_pass_evidence(self):
        good=check.probe(FakeDevice(),wait=lambda _:None)
        with self.assertRaises(AssertionError): check.verify(good,'b'*64)
        stale=copy.deepcopy(good)
        stale['timestamp']=time.time()-86401
        with self.assertRaises(AssertionError): check.verify(stale,'a'*64)
        forged=copy.deepcopy(good)
        forged['rows'][4]['controls']['DAC Playback Volume']='31'
        with self.assertRaises(AssertionError): check.verify(forged,'a'*64)

    def test_other_families_are_explicit_todos_without_device_access(self):
        for family in ('brick','retroid'):
            result=subprocess.run([sys.executable,str(HERE/'volume-device-check.py'),'--family',family],capture_output=True,text=True)
            self.assertEqual(result.returncode,3)
            self.assertIn('"status": "todo"',result.stdout)

    def test_the_lock_is_taken_on_the_device_not_on_the_host(self):
        command=check.remote_command('/tmp/plus-device.lock')
        with tempfile.TemporaryDirectory() as directory:
            lock=str(Path(directory)/'device.lock')
            fake=Path(directory)/'python3'
            fake.write_text('#!/bin/sh\necho measured "$@"\n'); fake.chmod(0o755)
            env=dict(os.environ,PATH=directory+os.pathsep+os.environ['PATH'])
            free=subprocess.run(['sh','-c',check.remote_command(lock)],env=env,capture_output=True,text=True)
            self.assertEqual(free.returncode,0,free.stderr)
            self.assertIn('measured - --on-device --family h700',free.stdout)
            with open(lock,'a') as held:
                fcntl.flock(held,fcntl.LOCK_EX)
                busy=subprocess.run(['sh','-c',check.remote_command(lock)],env=env,capture_output=True,text=True)
            self.assertEqual(busy.returncode,75)
            self.assertIn('device lock busy',busy.stderr)
            self.assertNotIn('measured',busy.stdout)
        self.assertIn('/tmp/plus-device.lock',command)

    def test_release_entry_point_requires_proof_before_running_cargo(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'scripts/image').mkdir(parents=True)
            (root/'bin').mkdir()
            for name in ('release-check.sh','volume-device-check.py'):
                shutil.copyfile(HERE/name,root/'scripts'/name)
            (root/'scripts/image/storage-budget.sh').write_text('exit 0\n')
            (root/'scripts/doc-missing.py').write_text('')
            (root/'scripts/bookkeeping.sh').write_text('echo "BOOKKEEPING: 0 warnings"\n')
            binary=root/'bin/pocket-ui'
            binary.write_bytes(b'offline fixture: exact release binary')
            cargo=root/'bin/cargo'
            cargo.write_text('#!/bin/sh\ntouch "$RELEASE_CARGO_MARK"\nexit 0\n')
            cargo.chmod(0o755)
            marker=root/'cargo-ran'
            env=dict(os.environ,PATH=str(root/'bin')+os.pathsep+os.environ['PATH'],
                     POCKET_STORAGE_BIN_DIR=str(root/'bin'),RELEASE_CHECK_FULL_GATE='0',RELEASE_CARGO_MARK=str(marker))
            env.pop('POCKET_VOLUME_CHECK_EVIDENCE',None)
            missing=subprocess.run(['sh',str(root/'scripts/release-check.sh')],env=env,capture_output=True,text=True)
            self.assertNotEqual(missing.returncode,0)
            self.assertFalse(marker.exists(), 'missing evidence must fail before the expensive test gate')
            evidence=root/'proof.json'
            result=check.probe(FakeDevice(),wait=lambda _:None)
            evidence.write_text(check.json.dumps(result))
            env['POCKET_VOLUME_CHECK_EVIDENCE']=str(evidence)
            wrong=subprocess.run(['sh',str(root/'scripts/release-check.sh')],env=env,capture_output=True,text=True)
            self.assertNotEqual(wrong.returncode,0)
            self.assertFalse(marker.exists(), 'an earlier/different binary is not a release proof')
            result['executable_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
            evidence.write_text(check.json.dumps(result))
            good=subprocess.run(['sh',str(root/'scripts/release-check.sh')],env=env,capture_output=True,text=True)
            self.assertEqual(good.returncode,0,good.stderr+good.stdout)
            self.assertTrue(marker.exists())


if __name__=='__main__': unittest.main()

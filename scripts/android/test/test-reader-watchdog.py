#!/usr/bin/env python3
"""Deadline regressions use a virtual pressure clock, never a host stress run."""
import importlib.util
import ast
import contextlib
import io
from pathlib import Path
import subprocess
import signal
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

spec=importlib.util.spec_from_file_location('reader_watchdog',Path(__file__).with_name('reader_watchdog.py'))
w=importlib.util.module_from_spec(spec);sys.modules[spec.name]=w;spec.loader.exec_module(w)


class Deadlines(unittest.TestCase):
    def budget(self):
        return w.Budget(w.Limits(),0.0,(0.0,0.0))

    def test_slow_jvm_boot_does_not_consume_reader_budget(self):
        b=self.budget()
        self.assertIsNone(b.observe(6.0,False,0.1,(0,0)))
        self.assertIsNone(b.observe(8.0,True,0.2,(0,0)))
        self.assertIsNone(b.observe(12.0,True,0.3,(0,0)))
        self.assertEqual(b.observe(13.1,True,0.3,(0,0)), 'reader unpressured deadline')

    def test_jvm_boot_hang_is_bounded(self):
        self.assertEqual(self.budget().observe(30.1,False,0.1,(30,0)), 'JVM startup deadline')

    def test_measured_io_stall_can_finish_after_old_five_second_deadline(self):
        b=self.budget();b.observe(0.1,True,0.1,(0,0))
        self.assertIsNone(b.observe(6.1,True,0.2,(0,5.5)))
        self.assertAlmostEqual(b.grace,5.5)

    def test_cpu_and_io_pressure_cannot_double_count_wall_time(self):
        b=self.budget();b.observe(0.1,True,0.1,(0,0))
        self.assertIsNone(b.observe(6.1,True,0.2,(6,6)))
        self.assertAlmostEqual(b.grace,6)

    def test_permanent_blocked_reader_fails_even_under_continuous_load(self):
        b=self.budget();b.observe(0.1,True,0.1,(0,0))
        self.assertEqual(b.observe(30.2,True,0.2,(30.1,30.1)), 'reader wall cap')

    def test_busy_reader_cannot_hide_behind_system_pressure(self):
        b=self.budget();b.observe(0.1,True,0.1,(0,0))
        self.assertEqual(b.observe(6.0,True,5.1,(6,6)), 'CPU budget exhausted')

    def test_quiet_host_keeps_five_second_reader_limit(self):
        b=self.budget();b.observe(0.1,True,0.1,(0,0))
        self.assertEqual(b.observe(5.2,True,0.2,(0,0)), 'reader unpressured deadline')

    def test_actual_blocked_child_is_diagnosed_killed_and_reaped(self):
        code="import signal,time;signal.signal(signal.SIGQUIT,lambda *_:print('DIAGNOSTIC',flush=True));print('READER_READY',flush=True);time.sleep(3600)"
        children=[];popen=subprocess.Popen
        def spawn(*args,**kwargs):
            child=popen(*args,**kwargs);child.send_signal=Mock(wraps=child.send_signal);children.append(child);return child
        with tempfile.TemporaryDirectory() as root, patch.object(w.subprocess,'Popen',spawn):
            with self.assertRaises(w.ProbeTimeout) as error:
                w.run_probe([sys.executable,'-c',code],root,
                            limits=w.Limits(reader=0.1,wall=0.1,diagnostic=0.1),pressure=lambda:(0,0))
        self.assertIn('phase=reader',str(error.exception))
        children[0].send_signal.assert_any_call(signal.SIGQUIT)
        self.assertIsNotNone(children[0].poll())

    def test_successful_child_has_kernel_cpu_limit(self):
        code="import resource;print('cpu='+str(resource.getrlimit(resource.RLIMIT_CPU)),flush=True);print('READER_READY',flush=True)"
        with tempfile.TemporaryDirectory() as root:
            result=w.run_probe([sys.executable,'-c',code],root)
        self.assertEqual(result.returncode,0)
        soft,hard=ast.literal_eval(next(line[4:] for line in result.stdout.splitlines() if line.startswith('cpu=')))
        self.assertLessEqual(soft,5);self.assertLessEqual(hard,6);self.assertLessEqual(soft,hard)

    def test_missing_ready_marker_cannot_pass(self):
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaisesRegex(RuntimeError,'without READER_READY'):
                w.run_probe([sys.executable,'-c','pass'],root)

    def test_kernel_wait_after_sigkill_cannot_hang_the_watchdog(self):
        child=Mock(pid=12345,returncode=None)
        child.poll.return_value=None
        def stalled_communicate(*,timeout):
            raise subprocess.TimeoutExpired('blocked',timeout,output=b'READER_READY\n')
        child.communicate.side_effect=stalled_communicate
        log=io.StringIO()
        with patch.object(w.subprocess,'Popen',return_value=child), patch.object(w.time,'monotonic',side_effect=[0,1,2]), contextlib.redirect_stderr(log):
            with self.assertRaises(w.ProbeTimeout):
                w.run_probe(['blocked'],'.',limits=w.Limits(wall=0.1),pressure=lambda:(0,0),cpu=lambda _:0)
        child.kill.assert_called_once()
        child.stdout.close.assert_called_once()
        self.assertIn('not reaped',log.getvalue())
        self.assertTrue(all(call.kwargs['timeout']>0 for call in child.communicate.call_args_list))


if __name__=='__main__':unittest.main()

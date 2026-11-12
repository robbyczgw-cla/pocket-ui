#!/usr/bin/env python3
"""Avoid false latency conclusions when presentation timestamps are old or still starting."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('latency', Path(__file__).with_name('analyze-aaudio-latency.py'))
latency = importlib.util.module_from_spec(spec)
spec.loader.exec_module(latency)
HEADER = 'elapsed_ms,now_ns,written_before,written_after,read,presented,presented_ns,status,xruns\n'


class PresentationClock(unittest.TestCase):
    def test_an_old_timestamp_does_not_add_its_age_to_a_twenty_ms_output(self):
        # The server clock is 250 ms old. Output is only 960 frames ahead at both observations.
        text = HEADER + '2000,10000000000,48960,48960,48576,36000,9750000000,0,0\n'
        text += '3000,11000000000,96960,96960,96576,84000,10750000000,0,0\n'
        report = latency.analyse(text)
        self.assertAlmostEqual(report['timestamp_delay_ms']['median'], 20)
        self.assertAlmostEqual(report['presented_rate_hz'], 48000)
        self.assertNotEqual(report['written_minus_read_ms_median'], 20)

    def test_startup_server_clock_and_unavailable_timestamp_do_not_hide_later_backlog(self):
        text = HEADER + '100,1000000000,4800,4800,4800,4800,1000000000,0,0\n'
        text += '2000,2000000000,48000,48000,47000,-1,0,-895,0\n'
        text += '3000,3000000000,96000,96000,95500,70608,3000000000,0,0\n'
        text += '4000,4000000000,144000,144000,143500,118608,4000000000,0,0\n'
        report = latency.analyse(text)
        self.assertEqual(report['samples'], 2)
        self.assertAlmostEqual(report['timestamp_delay_ms']['median'], 529)

    def test_without_a_running_presentation_clock_there_is_no_latency_result(self):
        with self.assertRaisesRegex(ValueError, 'presentation clock did not advance'):
            latency.analyse(HEADER + '2000,2000000000,48000,48000,48000,48000,2000000000,0,0\n'
                            + '3000,3000000000,48000,48000,48000,48000,2000000000,0,0\n')


if __name__ == '__main__':
    unittest.main()

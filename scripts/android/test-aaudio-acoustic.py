#!/usr/bin/env python3
"""Prove the analysis distinguishes a false driver offset from a real half-second delay."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import numpy as np


class AcousticComparison(unittest.TestCase):
    def measure(self, raw_extra_ms, signal=True):
        rate, base = 48000, 100000000000
        pcm = np.random.default_rng(4).normal(0, 2, 21 * rate).astype('<i2')
        signature = []
        seed = 0x13579bdf
        for i in range(480):
            seed ^= (seed << 13) & 0xffffffff
            seed ^= seed >> 17
            seed ^= (seed << 5) & 0xffffffff
            seed &= 0xffffffff
            signature.append(int((1024 if seed & 1 else -1024) * np.sin(np.pi * i / 479)))
        text = 'frame,read_end_ns,frames\n'
        # A constant 20 ms capture delay must disappear from the A/B comparison.
        for end in range(192, len(pcm) + 1, 192):
            text += f'{end},{base + round(end / rate * 1e9) + 20000000},192\n'
        for click in range(20):
            block = click % 2
            at = click + 1
            delay = 25 + (raw_extra_ms if block == 0 else 0)
            start = at * rate + round(delay * rate / 1000)
            if signal:
                pcm[start:start + 480] += signature
            text += f'# click={click} block={block} generated_ns={base + at * 1000000000} output_frame=48000\n'
        with tempfile.TemporaryDirectory() as tmp:
            log, audio = Path(tmp) / 'probe.csv', Path(tmp) / 'mic.s16le'
            log.write_text(text)
            pcm.tofile(audio)
            result = subprocess.check_output([sys.executable, str(Path(__file__).with_name('analyze-aaudio-acoustic.py')), str(log), str(audio)])
        return json.loads(result)['summary']

    def test_noise_without_recognisable_clicks_has_no_latency_verdict(self):
        result = self.measure(0, signal=False)
        self.assertEqual(result['status'], 'insufficient-signal')
        self.assertIsNone(result['raw_minus_fast_median_ms'])

    def test_equal_physical_paths_do_not_invent_a_half_second(self):
        result = self.measure(0)
        self.assertAlmostEqual(result['raw_minus_fast_median_ms'], 0, places=3)
        self.assertGreater(result['matched_filter_peak_to_null_rms_min'], 10)

    def test_a_real_half_second_is_detected_despite_the_input_delay(self):
        result = self.measure(500)
        self.assertAlmostEqual(result['raw_minus_fast_median_ms'], 500, places=3)
        self.assertGreater(result['matched_filter_peak_to_null_rms_min'], 10)


if __name__ == '__main__':
    unittest.main()

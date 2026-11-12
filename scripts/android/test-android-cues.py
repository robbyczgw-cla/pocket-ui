#!/usr/bin/env python3
"""Independent synthetic microphone/frame timelines for the acoustic cue analyser."""
import csv
import json
import pathlib
import subprocess
import tempfile
import unittest
import wave
import numpy as np

ROOT = pathlib.Path(__file__).resolve().parents[2]


class CueMeasurement(unittest.TestCase):
    def measure(self, offset_ms, signal=True, frames=True):
        with tempfile.TemporaryDirectory() as directory:
            folder = pathlib.Path(directory)
            with wave.open(str(ROOT / 'assets/ui-sounds/cart-in.wav')) as wav:
                reference = np.frombuffer(wav.readframes(wav.getnframes()), dtype='<i2')
            hit = int(np.argmax(abs(reference.astype(float))))
            pcm = np.random.default_rng(26).normal(0, 3, 48000 * 7)
            lines = []
            for launch in [100_000, 102_200, 104_400]:
                lines.append(f'insert: snes start cozy=true at {launch}')
                if frames:
                    lines.append(f'insert-frame {launch + 1200} 1200')
                if signal:
                    at = round((launch - 100_000 + 1200 + offset_ms) * 48) - hit
                    pcm[at:at + len(reference)] += reference
            (folder / 'app-log.txt').write_text('\n'.join(lines))
            pcm.astype('<i2').tofile(folder / 'microphone.s16le')
            with (folder / 'microphone.csv').open('w') as out:
                writer = csv.writer(out)
                writer.writerow(['frame', 'read_end_ns', 'frames'])
                for frame in range(192, len(pcm) + 1, 192):
                    writer.writerow([frame, round((100_000 + frame / 48) * 1e6), 192])
            return json.loads(subprocess.check_output([
                'python3', str(ROOT / 'scripts/android/analyze-android-cues.py'), str(folder)]))

    def test_real_half_second_early_cues_are_measured_without_output_timestamps(self):
        result = self.measure(-500)
        self.assertEqual(result['status'], 'measured')
        self.assertAlmostEqual(result['median_offset_ms'], -500, delta=0.03)

    def test_corrected_cues_preserve_the_small_input_delivery_offset(self):
        result = self.measure(25)
        self.assertEqual(result['status'], 'measured')
        self.assertAlmostEqual(result['median_offset_ms'], 25, delta=0.03)

    def test_noise_is_not_an_acoustic_verdict(self):
        result = self.measure(-500, signal=False)
        self.assertEqual(result['status'], 'insufficient-signal')
        self.assertIsNone(result['median_offset_ms'])

    def test_menu_only_recording_without_animation_is_not_a_verdict(self):
        result = self.measure(-500, frames=False)
        self.assertEqual(result['status'], 'insufficient-signal')
        self.assertIsNone(result['median_offset_ms'])


if __name__ == '__main__':
    unittest.main()

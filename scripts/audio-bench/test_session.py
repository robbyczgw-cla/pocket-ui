#!/usr/bin/env python3
"""Regression fixtures: incomplete evidence must never become a zero-error audio result."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("audio_bench_session", Path(__file__).with_name("session.py"))
session = importlib.util.module_from_spec(spec)
spec.loader.exec_module(session)


def final_session():
    measurement = {"samples": 10, "failures": 0, "min": 0, "max": 480, "latest": 240}
    alsa = {key: 0 for key in ("recover_calls", "recover_failures", "xrun_errors", "query_xrun_errors",
                              "write_errors", "zero_writes", "short_writes", "config_query_failures")}
    alsa.update(pcm_rate=48000, buffer_frames=1536, period_frames=384, avail_supported=True,
                delay_frames=copy.deepcopy(measurement), avail_frames=copy.deepcopy(measurement))
    return {"schema": 1, "writer_finished": True,
            "backend": {"backend": "alsa", "opens": 1, "alsa_opens": 1, "pulse_opens": 0, "android_opens": 0, "write_chunks": 1000, "write_frames": 480000,
                        "write_failures": 0, "last_write_ago_us": 10000, "alsa": alsa},
            "queue": {"state": "stopped", "input_frames": 655360, "prefill_wait_polls": 40,
                      "prefill_silence_chunks": 3, "paused_silence_chunks": 100, "refill_silence_chunks": 20,
                      "playing_underruns": 0, "playing_missing_frames": 0, "dropped_frames": 0},
            "pipewire": {"status": "partial", "reason": "final playback tail unobserved", "client_xruns": None,
                         "observed_xruns": 0, "samples": 5, "final_sample_age_ms": 400}}


class EvidenceTests(unittest.TestCase):
    def test_bench_requires_prebuilt_before_any_build_or_container_command(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            spy = root / "unexpected-command"
            for command in ("cargo", "docker", "ssh"):
                stub = root / command
                stub.write_text(f'#!/bin/sh\necho called > "{spy}"\nexit 97\n')
                stub.chmod(0o755)
            env = {**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"],
                   "AUDIO_BENCH_HOST": "local", "AUDIO_BENCH_BIN": "", "AUDIO_BENCH_BUILD": "0"}
            result = subprocess.run(["sh", str(Path(__file__).with_name("bench.sh")), "6", str(root / "out")],
                                    env=env, capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 2)
            self.assertIn("AUDIO_BENCH_BIN", result.stderr)
            self.assertFalse(spy.exists(), "a missing benchmark binary must never trigger a build or host command")

    def test_old_zero_underrun_log_overwrites_stale_green_json_with_unavailable(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "speaker.tone.log"
            log.write_text("audio_tone: underruns 0 queue 10 ms server 20 ms\n")
            log.with_name("speaker.session.json").write_text(json.dumps(final_session()))
            report, failed = session.analyze(log)
            self.assertEqual(failed, 1)
            self.assertEqual(report["status"], "unavailable")
            self.assertIsNone(json.loads(log.with_name("speaker.session.json").read_text()))

    def test_unopened_or_stalled_writer_is_not_valid_zero_error_evidence(self):
        for change in ({"opens": 0, "backend": "unopened"}, {"write_frames": 0}, {"last_write_ago_us": 2000000}):
            with self.subTest(change=change):
                value = final_session()
                value["backend"].update(change)
                report, failed = session.summarize(value, "case")
                self.assertEqual(failed, 1)
                self.assertEqual(report["status"], "incomplete")

    def test_prefill_pause_and_refill_silence_are_not_playing_underruns(self):
        value = final_session()
        report, failed = session.summarize(value, "case")
        self.assertEqual(failed, 0)
        self.assertEqual(report["status"], "partial")
        self.assertEqual(report["observed_errors"], [])
        self.assertEqual(report["queue"]["refill_silence_chunks"], 20)
        value["queue"]["playing_missing_frames"] = 256
        self.assertEqual(session.summarize(value, "case")[1], 1)

    def test_pipewire_observed_zero_does_not_fill_in_missing_session_tail(self):
        report, failed = session.summarize(final_session(), "case")
        self.assertEqual(failed, 0)
        self.assertEqual(report["pipewire"]["observed_xruns"], 0)
        self.assertIsNone(report["pipewire"]["client_xruns"])
        self.assertEqual(report["pipewire"]["final_sample_age_ms"], 400)
        value = final_session()
        value["pipewire"].update(samples=0, status="unavailable")
        report, _ = session.summarize(value, "case")
        self.assertIsNone(report["pipewire"]["observed_xruns"])

    def test_pulse_unsupported_alsa_and_absent_pipewire_remain_unknown(self):
        value = final_session()
        value["backend"].update(backend="pulse", alsa=None)
        value["pipewire"] = None
        report, _ = session.summarize(value, "case")
        self.assertIsNone(report["backend"]["alsa"])
        self.assertIsNone(report["pipewire"]["client_xruns"])
        self.assertIsNone(report["pipewire"]["observed_xruns"])
        self.assertEqual(report["status"], "partial")

    def test_silent_recovery_is_an_observed_error_even_with_zero_queue_underruns(self):
        value = final_session()
        value["backend"]["alsa"].update(recover_calls=1, xrun_errors=1, write_errors=1)
        report, failed = session.summarize(value, "case")
        self.assertEqual(failed, 1)
        self.assertEqual(report["status"], "errors_observed")
        self.assertIn("alsa.recover_calls=1", report["observed_errors"])
        self.assertEqual(report["queue"]["playing_underruns"], 0)

    def test_historical_alsa_zeroes_do_not_cover_pulse_fallback_or_intervening_pulse(self):
        for backend in ("pulse", "alsa"):
            with self.subTest(backend=backend):
                value = final_session()
                value["backend"].update(backend=backend, opens=3, pulse_opens=1)
                value["pipewire"].update(status="available", client_xruns=0)
                report, failed = session.summarize(value, "case")
                self.assertEqual(failed, 0)
                self.assertEqual(report["status"], "partial")
                self.assertEqual(report["alsa_scope"], "partial_backend_coverage")
                self.assertEqual(report["backend"]["alsa"]["recover_calls"], 0)

    def test_preserves_original_session_and_time_series_without_promoting_raw_pwtop(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "speaker.tone.log"
            value = final_session()
            sample = copy.deepcopy(value)
            sample["writer_finished"] = False
            log.write_text("audio sample " + json.dumps(sample) + "\naudio session " + json.dumps(value) + "\n")
            log.with_name("speaker.pwtop.end").write_text("R 42 256 48000 0.0 0.0 0.0 0.0 999 S16LE audio_tone\n")
            report, failed = session.analyze(log)
            self.assertEqual(failed, 0)
            self.assertEqual(json.loads(log.with_name("speaker.session.json").read_text()), value)
            self.assertEqual(json.loads(log.with_name("speaker.samples.jsonl").read_text()), sample)
            self.assertEqual(report["pipewire"]["observed_xruns"], 0)
            self.assertTrue(report["pwtop_context"][1]["present"])

    def test_truncated_final_json_cannot_reuse_previous_valid_record_as_green(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "case.tone.log"
            log.write_text("audio session " + json.dumps(final_session()) + '\naudio session {"schema":\n')
            report, failed = session.analyze(log)
            self.assertEqual(failed, 1)
            self.assertEqual(report["status"], "incomplete")


if __name__ == "__main__":
    unittest.main()

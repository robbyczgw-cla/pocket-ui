#!/usr/bin/env python3
"""Extract `audio session {JSON}` without turning missing measurements into zeroes.

Usage: session.py CASE.tone.log [...]. Writes CASE.session.json (original session) and
CASE.diagnostics.json (evidence summary), and prints that summary as JSON. Exit 1 means
missing/incomplete writer evidence or observed errors. Partial PipeWire coverage remains
explicit, even when all observed counters are zero; this is not a waveform-quality verdict.
"""
import argparse
import json
from pathlib import Path
import sys

PREFIX = "audio session "
HEARTBEAT_STALE_US = 1_000_000


def counter(value):
    """A measured zero is valid; null, booleans and malformed values are unknown."""
    return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else None


def summarize(session, source, parse_errors=()):
    issues = list(parse_errors)
    incomplete = bool(parse_errors)
    errors = []
    partial = False
    if not isinstance(session, dict) or session.get("available") is False:
        reason = session.get("reason") if isinstance(session, dict) else "no valid audio session JSON"
        return {"schema": 1, "source": str(source), "status": "unavailable", "issues": issues + [reason or "audio unavailable"],
                "backend": None, "queue": None, "pipewire": None}, 1
    backend = session.get("backend")
    if session.get("schema") != 1:
        issues.append("unsupported or missing audio session schema")
        incomplete = True
    queue = session.get("queue")
    pw = session.get("pipewire")
    backend = backend if isinstance(backend, dict) else {}
    queue = queue if isinstance(queue, dict) else {}
    name = backend.get("backend")
    opens = counter(backend.get("opens"))
    chunks = counter(backend.get("write_chunks"))
    frames = counter(backend.get("write_frames"))
    age = counter(backend.get("last_write_ago_us"))
    if name in (None, "unopened") or opens in (None, 0):
        issues.append("backend was not observed open")
        incomplete = True
    if chunks in (None, 0) or frames in (None, 0):
        issues.append("no accepted-frame writer heartbeat")
        incomplete = True
    if age is None:
        issues.append("writer heartbeat age unavailable")
        incomplete = True
    elif age > HEARTBEAT_STALE_US:
        issues.append("writer heartbeat stale (>1 second)")
        incomplete = True
    if session.get("writer_finished") is not True:
        issues.append("writer completion unverified")
        incomplete = True
    if counter(queue.get("input_frames")) in (None, 0):
        issues.append("no producer input frames observed")
        incomplete = True
    for key in ("playing_underruns", "playing_missing_frames", "dropped_frames"):
        value = counter(queue.get(key))
        if value is None:
            issues.append(f"queue.{key} unavailable")
            incomplete = True
        elif value > 0:
            errors.append(f"queue.{key}={value}")
    failures = counter(backend.get("write_failures"))
    if failures is None:
        issues.append("backend.write_failures unavailable")
        incomplete = True
    elif failures > 0:
        errors.append(f"backend.write_failures={failures}")
    alsa = backend.get("alsa")
    alsa_scope = "unsupported"
    if isinstance(alsa, dict):
        alsa_scope = "alsa_streams_only"
        if name != "alsa" or any(counter(backend.get(key)) not in (None, 0) for key in ("pulse_opens", "android_opens")):
            alsa_scope = "partial_backend_coverage"
            issues.append("historical ALSA counters do not cover Pulse/Android streams in this session")
            partial = True
        for key in ("recover_calls", "recover_failures", "xrun_errors", "query_xrun_errors", "write_errors", "zero_writes"):
            value = counter(alsa.get(key))
            if value is None:
                issues.append(f"alsa.{key} unavailable")
                incomplete = True
            elif value > 0:
                errors.append(f"alsa.{key}={value}")
        for key in ("pcm_rate", "buffer_frames", "period_frames"):
            if counter(alsa.get(key)) in (None, 0):
                issues.append(f"negotiated ALSA {key} unavailable")
                partial = True
        if counter(alsa.get("config_query_failures")) not in (None, 0):
            issues.append("some ALSA configuration queries failed")
            partial = True
        for key in ("delay_frames", "avail_frames"):
            measurement = alsa.get(key)
            if not isinstance(measurement, dict) or counter(measurement.get("samples")) in (None, 0):
                issues.append(f"ALSA {key} has no successful samples")
                partial = True
            elif counter(measurement.get("failures")) not in (None, 0):
                issues.append(f"some ALSA {key} queries failed")
                partial = True
    else:
        issues.append("ALSA observations unsupported or unavailable on this backend")
        partial = True
    if not isinstance(pw, dict):
        pw_report = {"status": "unavailable", "client_xruns": None, "observed_xruns": None,
                     "final_sample_age_ms": None, "reason": "no PipeWire collector snapshot"}
        partial = True
    else:
        # Do not substitute observed_xruns for the session-wide client_xruns. In particular,
        # a final partial snapshot retains observed zero while the unobserved tail stays unknown.
        pw_report = {key: pw.get(key) for key in (
            "status", "reason", "client_xruns", "observed_xruns", "samples", "sampling_gaps",
            "age_ms", "max_sample_gap_ms", "final_sample_age_ms", "stop_timed_out", "provenance")}
        if pw.get("status") != "available" or counter(pw.get("client_xruns")) is None:
            pw_report["client_xruns"] = None
            partial = True
        if counter(pw.get("samples")) in (None, 0):
            # Collector initialization has an observed accumulator of zero before any measurement exists.
            pw_report["observed_xruns"] = None
            pw_report["client_xruns"] = None
            partial = True
        for key in ("observed_xruns", "client_xruns"):
            value = counter(pw.get(key))
            if value is not None and value > 0:
                errors.append(f"pipewire.{key}={value}")
    if partial:
        issues.append("partial measurement coverage; no session-wide zero-error claim")
    status = "errors_observed" if errors else "incomplete" if incomplete else "partial" if partial else "observed"
    return {"schema": 1, "source": str(source), "status": status, "issues": issues, "observed_errors": errors,
            "backend": backend, "alsa_scope": alsa_scope, "queue": queue, "writer_finished": session.get("writer_finished"),
            "pipewire": pw_report}, int(bool(errors) or incomplete)


def analyze(path):
    sessions, samples, parse_errors = [], [], []
    try:
        with path.open(errors="replace") as log:
            for number, line in enumerate(log, 1):
                prefix = PREFIX if line.startswith(PREFIX) else "audio sample " if line.startswith("audio sample ") else None
                if prefix is None:
                    continue
                try:
                    value = json.loads(line[len(prefix):])
                    if not isinstance(value, dict):
                        raise ValueError("session is not an object")
                    (sessions if prefix == PREFIX else samples).append(value)
                except (ValueError, json.JSONDecodeError) as error:
                    parse_errors.append(f"invalid {prefix.strip()} on line {number}: {error}")
    except OSError as error:
        parse_errors.append(f"cannot read tone log: {error}")
    session = sessions[-1] if sessions else None
    report, result = summarize(session, path, parse_errors)
    report["session_records"] = len(sessions)
    report["pwtop_context"] = []
    stem = path.name.removesuffix(".tone.log")
    sample_path = path.with_name(f"{stem}.samples.jsonl")
    sample_path.write_text("".join(json.dumps(sample) + "\n" for sample in samples))
    report["time_series"] = {"path": str(sample_path), "samples": len(samples)}
    for suffix in ("pwtop", "pwtop.end", "pwtop.short"):
        context = path.with_name(f"{stem}.{suffix}")
        report["pwtop_context"].append({"path": str(context), "present": context.is_file(),
            "attribution": "raw context only; generic pw-top ERR is not a verified per-client xrun count"})
    # Write null explicitly when the source log lacks a session. Never leave a previous run's green JSON behind.
    path.with_name(f"{stem}.session.json").write_text(json.dumps(session, indent=2) + "\n")
    path.with_name(f"{stem}.diagnostics.json").write_text(json.dumps(report, indent=2) + "\n")
    return report, result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", nargs="+", type=Path)
    args = parser.parse_args()
    result = 0
    for path in args.logs:
        report, failed = analyze(path)
        print(json.dumps(report, sort_keys=True))
        result |= failed
    return result


if __name__ == "__main__":
    sys.exit(main())

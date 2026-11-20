"""Bound probe CPU and unpressured time without confusing JVM startup with IO.

Linux PSI reports time tasks were delayed by CPU, IO or memory contention. Only that
measured delay buys extra reader time; the 30-second wall cap never moves.
"""
from dataclasses import dataclass
import math
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys
import time

READY = "READER_READY"


@dataclass(frozen=True)
class Limits:
    startup: float = 30.0
    reader: float = 5.0
    wall: float = 30.0
    cpu: float = 5.0
    poll: float = 0.05
    diagnostic: float = 0.5
    reap: float = 2.0


def pressure_totals():
    totals = []
    for kind in ("cpu", "io", "memory"):
        try:
            fields = Path(f"/proc/pressure/{kind}").read_text().splitlines()[0].split()
            totals.append(int(next(f[6:] for f in fields if f.startswith("total="))) / 1e6)
        except (OSError, StopIteration, ValueError, IndexError):
            totals.append(0.0)
    return tuple(totals)


def cpu_seconds(pid):
    try:
        # The command name may contain spaces or parentheses; fields start at state (3).
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
    except (OSError, ValueError, IndexError):
        return None


def text(output):
    return output.decode(errors="replace") if isinstance(output, bytes) else output or ""


class ProbeTimeout(RuntimeError):
    pass


class Budget:
    """Pure deadline policy; tests advance its clock without loading the host."""
    def __init__(self, limits, now, pressure):
        self.limits, self.started = limits, now
        self.previous_time, self.previous_pressure = now, pressure
        self.ready = None
        self.grace = 0.0

    def observe(self, now, ready, cpu, pressure):
        if self.ready is None and ready:
            self.ready = now
        elif self.ready is not None:
            delayed = sum(max(0.0, b-a) for a,b in zip(self.previous_pressure, pressure))
            self.grace += min(now-self.previous_time, delayed)
        self.previous_time, self.previous_pressure = now, pressure
        if cpu >= self.limits.cpu:
            return "CPU budget exhausted"
        if self.ready is None:
            if now-self.started >= self.limits.startup:
                return "JVM startup deadline"
        elif now-self.ready >= self.limits.wall:
            return "reader wall cap"
        elif now-self.ready-self.grace >= self.limits.reader:
            return "reader unpressured deadline"
        return None


def run_probe(command, cwd, limits=Limits(), pressure=pressure_totals, cpu=cpu_seconds):
    def bound_cpu():
        seconds = max(1, math.ceil(limits.cpu))
        soft, hard = resource.getrlimit(resource.RLIMIT_CPU)
        soft = seconds if soft == resource.RLIM_INFINITY else min(soft, seconds)
        hard = seconds+1 if hard == resource.RLIM_INFINITY else min(hard, seconds+1)
        resource.setrlimit(resource.RLIMIT_CPU, (min(soft, hard), hard))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

    started = time.monotonic()
    budget = Budget(limits, started, pressure())
    used_cpu = 0.0
    child = subprocess.Popen(command, cwd=cwd, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, text=True, preexec_fn=bound_cpu)
    try:
        while True:
            output = ""
            completed = False
            try:
                output, _ = child.communicate(timeout=limits.poll)
                completed = True
            except subprocess.TimeoutExpired as error:
                output = text(error.output)
            now = time.monotonic()
            current_pressure = pressure()
            sampled_cpu = cpu(child.pid)
            if sampled_cpu is not None:
                used_cpu = sampled_cpu
            reason = budget.observe(now, READY in output.splitlines(), used_cpu, current_pressure)
            if child.returncode == -signal.SIGXCPU:
                reason = "CPU budget exhausted"
            if reason:
                if child.poll() is None and budget.ready is not None:
                    # HotSpot has installed its SIGQUIT handler by READY. Never signal
                    # an uninitialised VM (SIGQUIT could otherwise create a core dump).
                    child.send_signal(signal.SIGQUIT)
                    try:
                        output, _ = child.communicate(timeout=limits.diagnostic)
                    except subprocess.TimeoutExpired as error:
                        output = text(error.output)
                phase = "reader" if budget.ready is not None else "startup"
                raise ProbeTimeout(f"{reason}; phase={phase}, wall={now-started:.3f}s, "
                                   f"cpu={used_cpu:.3f}s, pressure_grace={budget.grace:.3f}s, "
                                   f"pid={child.pid}\n{text(output)}")
            if completed:
                if child.returncode == 0 and budget.ready is None:
                    raise RuntimeError("Probe exited successfully without READER_READY")
                result = subprocess.CompletedProcess(command, child.returncode, text(output))
                result.startup_seconds = (budget.ready or now) - started
                result.reader_seconds = now - (budget.ready or now)
                result.pressure_grace = budget.grace
                return result
    finally:
        if child.poll() is None:
            child.kill()
        try:
            child.communicate(timeout=limits.reap)
        except subprocess.TimeoutExpired:
            # SIGKILL can remain pending during uninterruptible kernel IO. Report
            # that failure instead of making the watchdog itself wait forever.
            print(f'watchdog: SIGKILL sent; child/pipes not reaped within {limits.reap}s, pid={child.pid}',file=sys.stderr)
            child.stdout.close()

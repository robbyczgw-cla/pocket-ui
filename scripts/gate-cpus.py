#!/usr/bin/env python3
"""CPU-only scheduling; no global reservations or state survive a gate."""
import os
from pathlib import Path
import re
import shutil
import sys


def cpus(value):
    if not re.fullmatch(r"\d+(?:-\d+)?(?:,\d+(?:-\d+)?)*", value):
        raise ValueError("GATE_CPUSET must be a taskset CPU list")
    result = set()
    for part in value.split(","):
        ends = list(map(int, part.split("-")))
        first, last = ends[0], ends[-1]
        if last < first or last > 65535:
            raise ValueError("invalid GATE_CPUSET range")
        result.update(range(first, last + 1))
    return result


def plan(allowed, siblings, exclusive, enabled, taskset):
    allowed = set(allowed)
    if not allowed:
        raise ValueError("GATE_CPUSET has no CPU in the current affinity")
    graph = {cpu: {cpu} for cpu in allowed}
    for cpu in allowed:
        # Unknown topology connects everything: there is no proven separate core.
        for sibling in set(siblings.get(cpu, allowed)) & allowed:
            graph[cpu].add(sibling)
            graph[sibling].add(cpu)
    groups = []
    left = set(allowed)
    while left:
        group, todo = set(), {min(left)}
        while todo:
            cpu = todo.pop()
            group.add(cpu)
            todo |= graph[cpu] - group
        groups.append(group)
        left -= group
    normal, launcher, small = allowed, set(), set()
    parallel = exclusive and enabled and taskset and len(allowed) >= 8 and len(groups) >= 3
    if parallel:
        launcher, small = groups[-1], groups[-2]
        normal = allowed - launcher - small
        if len(normal) < 6:
            parallel = False
            normal, launcher, small = allowed, set(), set()
    return {"allowed": allowed, "normal": normal, "launcher": launcher, "small": small, "parallel": bool(parallel)}


def main():
    affinity = set(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else set(range(os.cpu_count() or 1))
    cap = os.environ.get("GATE_CPUSET")
    if cap is not None:
        affinity &= cpus(cap)
        if not shutil.which("taskset"):
            raise ValueError("GATE_CPUSET needs taskset to enforce its limit")
    siblings = {}
    for cpu in affinity:
        try:
            siblings[cpu] = cpus(Path(f"/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list").read_text().strip())
        except (OSError, ValueError):
            pass
    result = plan(affinity, siblings, os.environ.get("GATE_EXCLUSIVE") == "1", os.environ.get("GATE_PARALLEL") != "0", bool(shutil.which("taskset")))
    for key, value in result.items():
        print(f"gate_{key}='{int(value) if isinstance(value, bool) else ','.join(map(str, sorted(value)))}'")
    print(f"gate_cores='{len(result['normal'])}'")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError) as error:
        sys.exit(f"gate: CPU allocation FAILED: {error}")

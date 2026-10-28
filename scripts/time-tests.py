#!/usr/bin/env python3
"""Time stable libtest executables from cargo's --message-format=json output.

Build first: CARGO_BUILD_JOBS=4 nice -n 10 cargo test --profile gate --no-run \
    --message-format=json > /tmp/test-build.json
Run: nice -n 10 python3 scripts/time-tests.py /tmp/test-build.json /tmp/test-times.json
--individual additionally measures each test in its own process (includes process startup).
No device or network access. Failures are retained and cause a nonzero exit.
"""
import argparse
import concurrent.futures
import json
import subprocess
import time


def measure(job):
    label, command = job
    start = time.monotonic()
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    return dict(name=label, seconds=round(time.monotonic() - start, 3), status=result.returncode,
                output=result.stdout.decode(errors='replace') if result.returncode else '')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build_json')
    parser.add_argument('output')
    parser.add_argument('--individual', action='store_true')
    parser.add_argument('--individual-only', action='store_true', help='skip whole-binary runs')
    parser.add_argument('--jobs', type=int, default=4)
    args = parser.parse_args()
    binaries = {}
    with open(args.build_json) as stream:
        for line in stream:
            item = json.loads(line)
            if item.get('reason') == 'compiler-artifact' and item.get('profile', {}).get('test') and item.get('executable'):
                binaries[item['executable']] = item['target']['name']
    results = []
    for binary, name in ([] if args.individual_only else binaries.items()):
        result = measure((name, [binary, '--test-threads', str(args.jobs)]))
        results.append(result)
        print(f"{result['seconds']:8.3f}s {name} status={result['status']}", flush=True)
    individual = []
    if args.individual or args.individual_only:
        jobs = []
        for binary, name in binaries.items():
            listing = subprocess.check_output([binary, '--list', '--format', 'terse'], text=True)
            jobs.extend((name + '::' + line[:-6], [binary, '--exact', line[:-6]])
                        for line in listing.splitlines() if line.endswith(': test'))
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            individual = list(pool.map(measure, jobs))
    report = dict(binaries=results, tests=sorted(individual, key=lambda r: -r['seconds']))
    with open(args.output, 'w') as stream:
        json.dump(report, stream, indent=2)
    for result in report['tests'][:40]:
        print(f"{result['seconds']:8.3f}s {result['name']} status={result['status']}")
    return int(any(r['status'] for r in results + individual))


if __name__ == '__main__':
    raise SystemExit(main())

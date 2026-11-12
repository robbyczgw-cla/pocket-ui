#!/usr/bin/env python3
"""Read aaudio-latency.c CSV. Reports driver estimates, never measured acoustic latency."""
import csv
import json
import math
from pathlib import Path
import statistics
import sys


def analyse(text):
    lines = text.splitlines()
    metadata = [line[2:] for line in lines if line.startswith('# ')]
    rows = [{key: float(value) for key, value in row.items()}
            for row in csv.DictReader(line for line in lines if not line.startswith('#'))]
    # Startup can switch from the server clock to the hardware clock after ~1 second.
    usable = [row for row in rows if row['status'] == 0 and row['elapsed_ms'] >= 2000]
    if len(usable) < 2:
        raise ValueError('need at least two successful timestamps after the first two seconds')
    delays, read_gaps, uncertainty = [], [], []
    for row in usable:
        if row['written_after'] < row['written_before']:
            raise ValueError('written frame counter went backwards')
        heard = row['presented'] + (row['now_ns'] - row['presented_ns']) * 48000 / 1e9
        # Callback-generated silence counts as written too; a client PCM capture index does not.
        delays.append((row['written_after'] - heard) / 48)
        read_gaps.append((row['written_after'] - row['read']) / 48)
        uncertainty.append((row['written_after'] - row['written_before']) / 48)
    first, last = usable[0], usable[-1]
    duration = (last['presented_ns'] - first['presented_ns']) / 1e9
    if duration <= 0:
        raise ValueError('presentation clock did not advance')
    ordered = sorted(delays)
    return {
        'scope': 'AAudio driver presentation estimate; no acoustic output measurement',
        'metadata': metadata,
        'samples': len(usable),
        'timestamp_delay_ms': {'median': statistics.median(delays), 'min': min(delays),
                               'max': max(delays), 'p95': ordered[math.ceil(.95 * len(ordered)) - 1]},
        'written_minus_read_ms_median': statistics.median(read_gaps),
        'counter_sampling_uncertainty_ms_max': max(uncertainty),
        'presented_rate_hz': (last['presented'] - first['presented']) / duration,
        'xruns_start': rows[0]['xruns'],
        'xruns_end': rows[-1]['xruns'],
    }


if __name__ == '__main__':
    if len(sys.argv) < 2:
        sys.exit('usage: analyze-aaudio-latency.py CSV [CSV ...]')
    print(json.dumps([dict(file=str(path), **analyse(Path(path).read_text()))
                      for path in sys.argv[1:]], indent=2))

#!/usr/bin/env python3
"""Report Brick gain steps from mixer readbacks; no device writes or acoustic claims.

Input JSON: {"codec_step_db": 1.16, "samples": [
  {"percent": 5, "codec_raw": 31, "dac_raw": "160,160"}, ...]}.
The codec's TLV direction is reversed by the vendor driver. Report attenuation
relative to raw zero, never its advertised absolute gain or speaker SPL.
"""
import argparse
import csv
import json
from pathlib import Path


def levels(samples, step_db):
    if not 0 < step_db < 10:
        raise ValueError('invalid measured codec tick size')
    result = []
    previous = None
    for sample in sorted(samples, key=lambda s: s['percent']):
        pct, raw = sample['percent'], sample['codec_raw']
        if not 0 <= pct <= 100 or not isinstance(raw, int) or not 0 <= raw <= 63:
            raise ValueError('invalid mixer readback')
        gain = None if pct == 0 else round(-raw * step_db, 3)
        delta = None if previous is None or gain is None else round(gain - previous, 3)
        if delta is not None and delta <= 0:
            raise ValueError(f'{pct}% does not raise measured gain')
        result.append({'percent': pct, 'codec_raw': raw, 'relative_db': gain, 'step_db': delta})
        if gain is not None:
            previous = gain
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('readbacks', type=Path)
    p.add_argument('--csv', type=Path, required=True)
    a = p.parse_args()
    measurement = json.loads(a.readbacks.read_text())
    rows = levels(measurement['samples'], measurement['codec_step_db'])
    with a.csv.open('w', newline='') as out:
        writer = csv.DictWriter(out, fieldnames=['percent', 'codec_raw', 'relative_db', 'step_db'])
        writer.writeheader()
        writer.writerows(rows)
    deltas = [r['step_db'] for r in rows if r['step_db'] is not None]
    print(json.dumps({'steps': len(deltas), 'min_step_db': min(deltas, default=None),
                      'max_step_db': max(deltas, default=None),
                      'basis': 'mixer readback; relative control gain, no acoustic calibration'}))


if __name__ == '__main__':
    main()

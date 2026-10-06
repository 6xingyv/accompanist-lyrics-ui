#!/usr/bin/env python3
"""Check visible row displacement against spring acceleration in an LSPR playback audit.

This reports candidates, not automatic proof of a bug. Geometry changes, recycling,
and rows outside the viewport are excluded from the acceleration comparison.
"""

import argparse
import csv
import json
import math
import struct
import zlib
from pathlib import Path


def records(path):
    with path.open('rb') as stream:
        if stream.read(8) != b'LSPR\x01\x00\x00\x00':
            raise ValueError(f'Unsupported trace: {path}')
        while header := stream.read(8):
            if len(header) != 8:
                raise ValueError(f'Truncated header: {path}')
            raw_size, compressed_size = struct.unpack('<II', header)
            raw = zlib.decompress(stream.read(compressed_size), wbits=-15)
            if len(raw) != raw_size:
                raise ValueError(f'Truncated record: {path}')
            yield raw


def inspect(path):
    previous = older = None
    checks = frames = 0
    candidates = []
    max_step = 0
    for raw in records(path):
        if struct.unpack_from('<ii', raw, 21) != (202, 30):
            continue
        clock = struct.unpack_from('<d', raw, 37)[0]
        music_time = struct.unpack_from('<d', raw, 29)[0]
        base = struct.unpack_from('<d', raw, 49)[0]
        count = struct.unpack_from('<i', raw, 94)[0]
        rows = {}
        for offset in range(98, 98 + count * 48, 48):
            index, top, position, velocity, displacement, target, acceleration, stiffness, damping = struct.unpack_from('<iddffdfff', raw, offset)
            y = top - base + displacement
            if math.isfinite(y):
                rows[index] = (y, top, acceleration)
        frames += 1
        if previous and older and clock - previous[0] == 16 and previous[0] - older[0] == 16:
            base_speed = (base - previous[1]) / .016
            previous_base_speed = (previous[1] - older[1]) / .016
            for index, row in rows.items():
                p = previous[2].get(index)
                o = older[2].get(index)
                if p is None or o is None or row[1] != p[1] or p[1] != o[1]:
                    continue
                if not all(-500 <= r[0] <= 900 for r in (row, p, o)):
                    continue
                speed = (row[0] - p[0]) / .016
                previous_speed = (p[0] - o[0]) / .016
                step = abs(speed - previous_speed)
                # Include the measured scroll acceleration and an integration/rounding allowance.
                allowed = .016 * 1.25 * max(abs(r[2]) for r in (row, p, o)) + abs(base_speed - previous_base_speed) + 50
                checks += 1
                max_step = max(max_step, step)
                if step > allowed and len(candidates) < 30:
                    candidates.append(dict(timeMs=int(music_time), row=index,
                        previousSpeed=round(previous_speed, 3), speed=round(speed, 3),
                        step=round(step, 3), allowed=round(allowed, 3)))
        older, previous = previous, (clock, base, rows)
    return dict(frames=frames, rowChecks=checks, maxScreenVelocityStep=round(max_step, 3), candidates=candidates)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    args = parser.parse_args()
    entries = list(csv.DictReader((args.report / 'results.tsv').open(encoding='utf-8'), delimiter='\t'))
    reports = []
    for entry in entries:
        if entry['status'] != 'PASS':
            continue
        report = inspect(args.report / f"song-{entry['index']}.lstr")
        report.update(index=int(entry['index']), source=entry['source'])
        reports.append(report)
        if report['candidates']:
            print(entry['index'], entry['source'], json.dumps(report['candidates'][:3]), flush=True)
    result = dict(songs=len(reports), frames=sum(r['frames'] for r in reports),
        rowChecks=sum(r['rowChecks'] for r in reports),
        songsWithCandidates=sum(bool(r['candidates']) for r in reports), reports=reports)
    (args.report / 'screen-velocity-analysis.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print({k: v for k, v in result.items() if k != 'reports'}, flush=True)


if __name__ == '__main__':
    main()

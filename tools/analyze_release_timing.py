"""Convert the multi-result CSV from lyrics-release-timing.sql into a compact JSON report."""
import argparse
import csv
import io
import json
from pathlib import Path


def read_sections(path):
    section = None
    result = {}
    for block in path.read_text(encoding="utf-8-sig").strip().split("\n\n"):
        rows = list(csv.reader(io.StringIO(block)))
        if not rows:
            continue
        if rows[0] == ["section"]:
            section = rows[1][0]
            continue
        if section:
            def number(value):
                if value == "[NULL]":
                    return None
                try:
                    return float(value) if "." in value else int(value)
                except ValueError:
                    return value
            result[section] = [dict(zip(rows[0], map(number, row))) for row in rows[1:]]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("summaries", type=Path, nargs="+")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    sections = ["clock counters", "lyrics CPU slices",
                "app frame completion and screen presentation",
                "lyrics draw to presentation", "trace losses and errors", "perf sample count"]
    report = {
        "device": "Xiaomi 13 / 2211133C / Android API 36 / 60 Hz",
        "build": "2.0.0 release; R8/resource shrink enabled; non-debuggable; profileable",
        "track": "Sabrina Carpenter - Go Go Juice",
        "method": "Android Trace counters + Perfetto FrameTimeline; surface tokens join app frames to SurfaceFlinger presentation",
        "limitations": [
            "No microphone/audio loopback measurement: speaker acoustic latency is unknown.",
            "ExoPlayer currentPosition is a cached app-visible position, not a per-frame acoustic ground truth.",
            "Counters add roughly 0.05-0.07 ms per read during active tracing.",
            "First 45 s capture retained about 25 s of ftrace/counters because its 64 MiB ring buffer wrapped.",
            "Second 30 s capture has no ring buffer loss and includes a track repeat (discontinuity 1 to 3).",
            "Device reports 37 ftrace setup errors; recorded app slices, counters and FrameTimeline are present."
        ],
        "captures": []
    }
    for path in args.summaries:
        data = read_sections(path)
        report["captures"].append({"summary": str(path), **{s: data.get(s, []) for s in sections}})
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(args.output)


if __name__ == "__main__":
    main()

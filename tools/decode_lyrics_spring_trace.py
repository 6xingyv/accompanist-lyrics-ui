#!/usr/bin/env python3
"""Decode the Android LSPR v1 trace into JSON Lines for offline inspection."""

from __future__ import annotations

import argparse
import json
import struct
import sys
import zlib
from pathlib import Path


EVENT_NAMES = {
    100: "configure",
    101: "retain",
    102: "layout",
    103: "content_shift",
    104: "rebase",
    105: "focus",
    106: "reset",
    107: "follow_scroll",
    108: "follow_request",
    109: "click_target_cleared",
    110: "auto_follow_suspended",
    111: "follow_target",
    112: "animation_start_or_retarget",
    113: "click_handoff",
    114: "geometry_retarget",
    115: "direct_scroll_target",
    120: "scroll_frame",
}


def decode_record(raw: bytes) -> dict:
    cursor = 0

    def take(kind: str):
        nonlocal cursor
        fmt = "<" + kind
        value = struct.unpack_from(fmt, raw, cursor)[0]
        cursor += struct.calcsize(fmt)
        return value

    record_type = take("B")
    sequence = take("I")
    elapsed_ns = take("Q")
    requested_seconds = take("f")
    integrated_seconds = take("f")
    event_code = take("i")
    event_index = take("i")
    event_value = take("d")
    event_value2 = take("d")
    event_float = take("f")
    base = take("d")
    limit = take("f")
    focus = take("i")
    first = take("i")
    end = take("i")
    active = bool(take("B"))
    stiffness = take("f")
    damping = take("f")
    coupling = take("f")
    distance_falloff = take("f")
    min_response = take("f")
    row_count = take("i")

    rows = []
    for _ in range(row_count):
        rows.append(
            {
                "index": take("i"),
                "layout_top_px": take("d"),
                "position_from_base_px": take("d"),
                "velocity_px_s": take("f"),
                "offset_px": take("f"),
                "target_from_base_px": take("d"),
                "acceleration_px_s2": take("f"),
                "stiffness": take("f"),
                "damping": take("f"),
            }
        )

    if cursor != len(raw):
        raise ValueError(f"record {sequence}: schema left {len(raw) - cursor} unread bytes")

    return {
        "type": {1: "frame", 2: "state_event", 3: "scroll_actor"}.get(record_type, record_type),
        "sequence": sequence,
        "elapsed_seconds": elapsed_ns / 1_000_000_000,
        "requested_dt_seconds": requested_seconds,
        "integrated_dt_seconds": integrated_seconds,
        "event_code": event_code,
        "event": EVENT_NAMES.get(event_code, f"unknown_{event_code}"),
        "event_index": event_index,
        "event_value": event_value,
        "event_value2": event_value2,
        "event_float": event_float,
        "base_scroll_px": base,
        "viewport_limit_px": limit,
        "focus_index": focus,
        "retained_first": first,
        "retained_end_exclusive": end,
        "active": active,
        "chain": {
            "stiffness": stiffness,
            "damping": damping,
            "coupling": coupling,
            "distance_falloff": distance_falloff,
            "min_response": min_response,
        },
        "rows": rows,
    }


def read_trace(path: Path):
    data = path.read_bytes()
    if len(data) < 8 or data[:4] != b"LSPR":
        raise ValueError("not an LSPR trace")
    version = struct.unpack_from("<I", data, 4)[0]
    if version != 1:
        raise ValueError(f"unsupported LSPR version: {version}")

    offset = 8
    while offset < len(data):
        if len(data) - offset < 8:
            print(f"warning: incomplete final block header at byte {offset}", file=sys.stderr)
            return
        raw_size, compressed_size = struct.unpack_from("<II", data, offset)
        offset += 8
        if len(data) - offset < compressed_size:
            print(f"warning: incomplete final compressed block at byte {offset}", file=sys.stderr)
            return
        compressed = data[offset : offset + compressed_size]
        offset += compressed_size
        raw = zlib.decompress(compressed, wbits=-15)
        if len(raw) != raw_size:
            raise ValueError(f"block size mismatch: expected {raw_size}, decoded {len(raw)}")
        yield decode_record(raw)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path, help="lyrics-spring.lstr copied from the device")
    parser.add_argument("output", type=Path, nargs="?", help="JSONL output (defaults to TRACE.jsonl)")
    args = parser.parse_args()

    output = args.output or args.trace.with_suffix(args.trace.suffix + ".jsonl")
    count = 0
    with output.open("w", encoding="utf-8", newline="\n") as stream:
        for record in read_trace(args.trace):
            stream.write(json.dumps(record, separators=(",", ":"), allow_nan=True) + "\n")
            count += 1
    print(f"decoded {count} records to {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

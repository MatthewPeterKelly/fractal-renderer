#!/usr/bin/env python3
"""One-time mass edit migrating render_options blocks from the legacy
two-axis schema to the unified `sampling_level` schema introduced in
Phase 2.2 of the GUI unification roadmap.

Mapping:
- subpixel_antialiasing: u (>0), downsample_stride: 1
    -> sampling_level: u
- subpixel_antialiasing: 0,        downsample_stride: m (>1)
    -> sampling_level: -(m - 1)
- subpixel_antialiasing: 0,        downsample_stride: 1
    -> sampling_level: 0
- Anything else (e.g. both axes engaged simultaneously) is reported and
  skipped.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def find_render_options(node: Any):
    """Yield every render_options dict reachable in the JSON tree."""
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "render_options" and isinstance(v, dict):
                yield v
            else:
                yield from find_render_options(v)
    elif isinstance(node, list):
        for item in node:
            yield from find_render_options(item)


def migrate_render_options(opts: dict) -> tuple[bool, str]:
    """Mutate `opts` in place. Returns (changed, message)."""
    if "sampling_level" in opts:
        return False, "already migrated"

    aa = opts.get("subpixel_antialiasing")
    ds = opts.get("downsample_stride")
    if aa is None and ds is None:
        return False, "no legacy keys to migrate"

    aa = int(aa) if aa is not None else 0
    ds = int(ds) if ds is not None else 1

    if aa < 0 or ds < 1:
        return False, f"unexpected values aa={aa} ds={ds}"

    if aa > 0 and ds > 1:
        return False, f"both axes engaged (aa={aa}, ds={ds}); cannot collapse"

    if aa > 0:
        sampling = aa
    elif ds > 1:
        sampling = -(ds - 1)
    else:
        sampling = 0

    opts.pop("subpixel_antialiasing", None)
    opts.pop("downsample_stride", None)
    opts["sampling_level"] = sampling
    return True, f"sampling_level: {sampling}"


def migrate_file(path: Path) -> tuple[bool, list[str]]:
    text = path.read_text()
    data = json.loads(text)
    messages: list[str] = []
    changed = False
    for opts in find_render_options(data):
        c, msg = migrate_render_options(opts)
        if c:
            changed = True
        messages.append(msg)
    if changed:
        path.write_text(json.dumps(data, indent=2) + "\n")
    return changed, messages


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "roots",
        nargs="*",
        default=["examples", "tests/param_files", "benches"],
        help="Directories (or files) to scan for *.json (default: examples, tests/param_files, benches)",
    )
    args = parser.parse_args()

    files: list[Path] = []
    for root in args.roots:
        p = Path(root)
        if p.is_file() and p.suffix == ".json":
            files.append(p)
        elif p.is_dir():
            files.extend(sorted(p.rglob("*.json")))
        else:
            print(f"WARN: {root} not found, skipping", file=sys.stderr)

    total_changed = 0
    for path in files:
        try:
            changed, messages = migrate_file(path)
        except json.JSONDecodeError as e:
            print(f"SKIP {path}: invalid JSON ({e})")
            continue
        if changed:
            total_changed += 1
            print(f"MIGRATED {path}: {', '.join(messages)}")
        elif messages:
            relevant = [m for m in messages if m != "no legacy keys to migrate"]
            if relevant:
                print(f"      {path}: {', '.join(relevant)}")
    print(f"\nDone. {total_changed} file(s) migrated.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Migrate every fractal-renderer params JSON to the unified Phase-3 ColorMap shape.

The Phase-3 architecture collapses the three previous color-map variants
(`ForegroundBackground`, `BackgroundWithColorMap`, `MultiColorMap`) onto a
single `ColorMap { flat_color, gradients }` struct embedded in each
fractal's params.

This script rewrites every JSON file under examples/, benches/, and
tests/param_files/ that targets a fractal whose schema changed:

- Mandelbrot / Julia: `color_map.color = { background, color_map }` ->
  `color_map.color = { flat_color: background, gradients: [color_map] }`.
  The legacy `histogram_sample_count` field is dropped.

- Driven-damped pendulum: `color = { foreground, background }` ->
  `color = { flat_color: background,
              gradients: [[ {query: 0.0, rgb_raw: foreground},
                            {query: 1.0, rgb_raw: foreground} ]] }`.

- Newton's method: `color = { cyclic_attractor, color_maps }` ->
  `color = { flat_color: cyclic_attractor, gradients: color_maps }`.
  The legacy `histogram_sample_count` field is dropped.

Other fractal types (Barnsley fern, Sierpinski, color swatch) are
untouched. Files that are already in the new shape are left as-is so the
script is idempotent.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET_DIRS = ("examples", "benches", "tests/param_files")


def migrate_legacy_quadratic_color(color: dict) -> dict | None:
    """Convert {background, color_map} -> {flat_color, gradients}."""
    if not isinstance(color, dict):
        return None
    if "flat_color" in color and "gradients" in color:
        return None  # already migrated
    if "background" in color and "color_map" in color:
        return {
            "flat_color": color["background"],
            "gradients": [color["color_map"]],
        }
    return None


def migrate_quadratic_params_block(params: dict) -> bool:
    """Migrate the `color_map` sub-block of a Mandelbrot/Julia params dict.
    Returns True iff a change was made."""
    cmp = params.get("color_map")
    if not isinstance(cmp, dict):
        return False
    changed = False
    new_color = migrate_legacy_quadratic_color(cmp.get("color"))
    if new_color is not None:
        cmp["color"] = new_color
        changed = True
    if "histogram_sample_count" in cmp:
        del cmp["histogram_sample_count"]
        changed = True
    return changed


def migrate_ddp(params: dict) -> bool:
    """Migrate DDP `color: {foreground, background}` -> unified `ColorMap`.
    Returns True iff a change was made."""
    color = params.get("color")
    if not isinstance(color, dict):
        return False
    if "flat_color" in color and "gradients" in color:
        return False  # already migrated
    if "foreground" in color and "background" in color:
        params["color"] = {
            "flat_color": color["background"],
            "gradients": [
                [
                    {"query": 0.0, "rgb_raw": color["foreground"]},
                    {"query": 1.0, "rgb_raw": color["foreground"]},
                ]
            ],
        }
        return True
    return False


def migrate_newton(params: dict) -> bool:
    """Migrate Newton `color: {cyclic_attractor, color_maps}` -> unified
    `ColorMap`. Returns True iff a change was made."""
    color = params.get("color")
    changed = False
    if isinstance(color, dict):
        if "flat_color" not in color or "gradients" not in color:
            if "cyclic_attractor" in color and "color_maps" in color:
                params["color"] = {
                    "flat_color": color["cyclic_attractor"],
                    "gradients": color["color_maps"],
                }
                changed = True
    if "histogram_sample_count" in params:
        del params["histogram_sample_count"]
        changed = True
    return changed


def migrate_top_level(doc) -> bool:
    """Mutate `doc` in place. Returns True iff anything changed."""
    if not isinstance(doc, dict):
        return False
    changed = False

    # Tagged enum form: {"Mandelbrot": {...}}.
    for key in ("Mandelbrot", "Julia"):
        inner = doc.get(key)
        if isinstance(inner, dict):
            changed |= migrate_quadratic_params_block(inner)

    # Untagged form: file is itself the params dict (used by benches).
    if "color_map" in doc and "convergence_params" in doc:
        changed |= migrate_quadratic_params_block(doc)

    inner = doc.get("DrivenDampedPendulum")
    if isinstance(inner, dict):
        changed |= migrate_ddp(inner)
    if "n_max_period" in doc and "color" in doc:
        changed |= migrate_ddp(doc)

    inner = doc.get("NewtonsMethod")
    if isinstance(inner, dict):
        params = inner.get("params")
        if isinstance(params, dict):
            changed |= migrate_newton(params)

    return changed


def migrate_file(path: Path) -> bool:
    """Migrate a single JSON file. Returns True iff the file was rewritten."""
    raw = path.read_text()
    try:
        doc = json.loads(raw)
    except json.JSONDecodeError as e:
        print(f"  skipped (invalid JSON): {path} - {e}", file=sys.stderr)
        return False
    if not migrate_top_level(doc):
        return False
    path.write_text(json.dumps(doc, indent=2) + "\n")
    return True


def main() -> int:
    rewritten = 0
    visited = 0
    for d in TARGET_DIRS:
        for path in sorted((ROOT / d).rglob("*.json")):
            visited += 1
            if migrate_file(path):
                rewritten += 1
                print(f"  rewrote: {path.relative_to(ROOT)}")
    print(f"Visited {visited} JSON file(s); rewrote {rewritten}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

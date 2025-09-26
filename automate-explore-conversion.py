#!/usr/bin/env python3
from pathlib import Path
import argparse
import shutil
import sys
import re

ALLOWED_FRACTALS = {"mandelbrot", "julia", "driven-damped-pendulum"}

# Use token replacement to avoid brace-escaping headaches
MAIN_RS_TEMPLATE = (
    "#[path = \"../common/mod.rs\"]\n"
    "mod common;\n\n"
    "fn main() {\n"
    "    common::explore_example_from_string(\"explore-__FRACTAL__-__NAME__\")\n"
    "}\n"
)

RENDER_DIR_RE = re.compile(r"^render-(?P<fractal>mandelbrot|julia|driven-damped-pendulum)-(?P<name>.+)$")

def make_main_rs_text(fractal: str, name: str) -> str:
    return (MAIN_RS_TEMPLATE
            .replace("__FRACTAL__", fractal)
            .replace("__NAME__", name))

def convert_one(src_dir: Path, dst_dir: Path, fractal: str, name: str, force: bool, mode: str) -> None:
    src_main = src_dir / "main.rs"
    src_params = src_dir / "params.json"

    if not src_main.exists():
        print(f"WARNING: missing main.rs in {src_dir}", file=sys.stderr)
    if not src_params.exists():
        print(f"WARNING: missing params.json in {src_dir}", file=sys.stderr)

    dst_dir.mkdir(parents=True, exist_ok=True)

    # Copy or move params.json
    if src_params.exists():
        dst_params = dst_dir / "params.json"
        if dst_params.exists() and not force:
            print(f"SKIP (exists): {dst_params}")
        else:
            if mode == "move":
                if dst_params.exists():
                    dst_params.unlink()
                shutil.move(str(src_params), str(dst_params))
                print(f"MOVED: {src_params} -> {dst_params}")
            else:
                shutil.copy2(src_params, dst_params)
                print(f"COPIED: {src_params} -> {dst_params}")

    # Write fresh main.rs
    dst_main = dst_dir / "main.rs"
    if dst_main.exists() and not force:
        print(f"SKIP (exists): {dst_main}")
    else:
        dst_main.write_text(make_main_rs_text(fractal, name), encoding="utf-8")
        print(f"WROTE: {dst_main}")

    # Optional cleanup of empty source directory in move mode
    if mode == "move":
        try:
            remaining = list(src_dir.iterdir())
            if not remaining:
                src_dir.rmdir()
                print(f"REMOVED EMPTY DIR: {src_dir}")
        except Exception:
            pass

def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Create explore-<fractal>-<name> examples from render-<fractal>-<name>."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path("examples"),
        help="Root directory containing render-<fractal>-<name> folders (default: examples)",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite files in destination if they already exist.",
    )
    parser.add_argument(
        "--mode",
        choices=["copy", "move"],
        default="copy",
        help="Copy to new explore-* dirs (default) or move/rename.",
    )
    args = parser.parse_args(argv)

    if not args.root.is_dir():
        print(f"ERROR: Root directory not found: {args.root}", file=sys.stderr)
        sys.exit(1)

    candidates = sorted(d for d in args.root.iterdir() if d.is_dir() and d.name.startswith("render-"))
    if not candidates:
        print(f"No render-<fractal>-<name> directories found under {args.root}")
        return

    any_converted = False
    for d in candidates:
        m = RENDER_DIR_RE.match(d.name)
        if not m:
            # Not one of the supported fractal types or not matching pattern
            continue
        fractal = m.group("fractal")
        name = m.group("name")
        if fractal not in ALLOWED_FRACTALS:
            # Redundant due to regex, but keep for clarity/extensibility
            continue
        dst = args.root / f"explore-{fractal}-{name}"
        convert_one(d, dst, fractal, name, args.force, args.mode)
        any_converted = True

    if not any_converted:
        print("No matching render-<fractal>-<name> directories to convert (supported: mandelbrot, julia, driven-damped-pendulum).")
    else:
        print("\nDone.")

if __name__ == "__main__":
    main()

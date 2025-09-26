#!/usr/bin/env python3
from pathlib import Path
import argparse
import sys

MAIN_RS_TEMPLATE = """#[path = "../common/mod.rs"]
mod common;

fn main() {{
    common::render_example_from_string("render-mandelbrot-{name}")
}}
"""

def generate_for_json(src_json: Path, out_root: Path, force: bool) -> None:
    example_name = src_json.stem  # EXAMPLE (without .json)
    target_dir = out_root / f"render-mandelbrot-{example_name}"
    target_dir.mkdir(parents=True, exist_ok=True)

    params_rs = target_dir / "params.json"
    main_rs = target_dir / "main.rs"

    # Write params.rs exactly matching the JSON contents
    json_text = src_json.read_text(encoding="utf-8")

    if params_rs.exists() and not force:
        print(f"SKIP (exists): {params_rs}")
    else:
        params_rs.write_text(json_text, encoding="utf-8")
        print(f"WROTE: {params_rs}")

    # Write main.rs from template
    main_text = MAIN_RS_TEMPLATE.format(name=example_name)
    if main_rs.exists() and not force:
        print(f"SKIP (exists): {main_rs}")
    else:
        main_rs.write_text(main_text, encoding="utf-8")
        print(f"WROTE: {main_rs}")

def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Generate render-mandelbrot-* example crates from JSON params."
    )
    parser.add_argument(
        "--src",
        type=Path,
        default=Path("examples") / "mandelbrot",
        help="Directory containing *.json files (default: examples/mandelbrot)",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("examples"),
        help="Output directory where render-mandelbrot-* folders are created (default: examples)",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite existing files if present.",
    )
    args = parser.parse_args(argv)

    if not args.src.is_dir():
        print(f"ERROR: Source directory not found: {args.src}", file=sys.stderr)
        sys.exit(1)
    if not args.out.exists():
        try:
            args.out.mkdir(parents=True, exist_ok=True)
        except Exception as e:
            print(f"ERROR: Could not create output directory {args.out}: {e}", file=sys.stderr)
            sys.exit(1)

    json_files = sorted(p for p in args.src.glob("*.json") if p.is_file())
    if not json_files:
        print(f"No JSON files found in {args.src}")
        return

    for jf in json_files:
        generate_for_json(jf, args.out, args.force)

    print("\nDone.")

if __name__ == "__main__":
    main()

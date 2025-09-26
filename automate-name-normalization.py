#!/usr/bin/env python3
from pathlib import Path
import argparse
import re
import sys
import shutil

# Replace the string literal inside:
#   common::render_example_from_string("render-...-...")
#   common::explore_example_from_string("explore-...-...")
CALL_RE = re.compile(
    r'(common::(?:render|explore)_example_from_string\(")([^"]+)("\))'
)

def hyphenate(name: str) -> str:
    return name.replace("_", "-")

def update_main_rs_text(src: str, correct_name: str) -> str:
    """Force the argument to correct_name inside the common::{render,explore}_... call."""
    def repl(m: re.Match) -> str:
        prefix, _old_arg, suffix = m.groups()
        return f'{prefix}{correct_name}{suffix}'
    return CALL_RE.sub(repl, src)

def normalize_dir(dir_path: Path, dry_run: bool, force: bool) -> None:
    # Only process render-* or explore-* dirs
    name = dir_path.name
    if not (name.startswith("render-") or name.startswith("explore-")):
        return

    target_name = hyphenate(name)
    target_dir = dir_path.with_name(target_name)
    will_rename_dir = (target_name != name)

    # 1) Directory rename (or plan)
    if will_rename_dir:
        if target_dir.exists() and not force:
            print(f"[WARN] Destination already exists: {target_dir} (use --force to overwrite).")
            # If not forcing, we *do not* rename; we still try to update contents in the original dir.
        else:
            if dry_run:
                print(f"[DRY] Would rename: {dir_path} -> {target_dir}")
            else:
                if target_dir.exists() and force:
                    if target_dir.is_dir():
                        shutil.rmtree(target_dir)
                    else:
                        target_dir.unlink()
                dir_path.rename(target_dir)
                print(f"[OK ] Renamed: {name} -> {target_name}")
                # After actual rename, operate on the new path
                dir_path = target_dir

    # After possible real rename, recompute current facts
    current_dir = dir_path
    current_dir_name = current_dir.name
    final_name_for_literal = hyphenate(current_dir_name)  # the directory name is already hyphenated if we renamed

    # 2) Update main.rs string literal to match the *final* directory name
    main_rs = current_dir / "main.rs"
    if main_rs.exists():
        if dry_run:
            # Peek to decide whether there would be a change
            try:
                text = main_rs.read_text(encoding="utf-8")
            except Exception as e:
                print(f"[ERR] Could not read {main_rs}: {e}")
                return
            updated = update_main_rs_text(text, final_name_for_literal)
            if updated != text:
                print(f"[DRY] Would update call target in: {main_rs} -> \"{final_name_for_literal}\"")
        else:
            try:
                text = main_rs.read_text(encoding="utf-8")
                updated = update_main_rs_text(text, final_name_for_literal)
                if updated != text:
                    main_rs.write_text(updated, encoding="utf-8")
                    print(f"[OK ] Updated call target in: {main_rs}")
            except Exception as e:
                print(f"[ERR] Could not update {main_rs}: {e}")
    else:
        print(f"[INFO] No main.rs in {current_dir} (skipped content update).")

    # 3) Rename any files inside the directory that contain underscores
    #    (e.g., params_like_this.json -> params-like-this.json)
    try:
        children = list(current_dir.iterdir())
    except FileNotFoundError:
        # This only happens if user did dry-run with a planned rename and the
        # target path doesn't exist (we didn't change current_dir in dry-run).
        # Nothing to do.
        return
    except Exception as e:
        print(f"[ERR] Could not list {current_dir}: {e}")
        return

    for child in children:
        if child.is_file() and "_" in child.name:
            new_name = hyphenate(child.name)
            new_path = child.with_name(new_name)
            if dry_run:
                if new_path.exists() and new_path != child:
                    print(f"[DRY] Would rename file (replace existing): {child} -> {new_path}")
                else:
                    print(f"[DRY] Would rename file: {child} -> {new_path}")
                continue

            # Real rename:
            if new_path.exists() and new_path != child:
                if not force:
                    print(f"[WARN] File rename collision: {new_path} exists (skipping {child}). Use --force to replace.")
                    continue
                else:
                    try:
                        new_path.unlink()
                    except Exception as e:
                        print(f"[ERR] Could not remove existing {new_path}: {e}")
                        continue
            try:
                child.rename(new_path)
                print(f"[OK ] Renamed file: {child.name} -> {new_name}")
            except Exception as e:
                print(f"[ERR] Failed to rename file {child} -> {new_path}: {e}")

def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Normalize example directory and file names to hyphens and fix main.rs string literal for render/explore variants."
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path("examples"),
        help="Root directory to scan (default: examples)"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show actions without making changes."
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite/replace when destination already exists."
    )
    args = parser.parse_args(argv)

    if not args.root.is_dir():
        print(f"[ERR] Root directory not found: {args.root}", file=sys.stderr)
        sys.exit(1)

    candidates = [d for d in args.root.iterdir()
                  if d.is_dir() and (d.name.startswith("render-") or d.name.startswith("explore-"))]
    if not candidates:
        print("[INFO] No render-* or explore-* directories found.")
        return

    for d in sorted(candidates, key=lambda p: p.name):
        try:
            normalize_dir(d, args.dry_run, args.force)
        except Exception as e:
            print(f"[ERR] Failed to process {d}: {e}")

    if args.dry_run:
        print("\n[DRY] No changes were made. Re-run without --dry-run to apply.")

if __name__ == "__main__":
    main()

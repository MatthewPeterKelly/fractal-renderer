#!/usr/bin/env python3
from pathlib import Path
import argparse
import re
import sys
import shutil

# Matches the specific call we need to fix:
#   common::render_example_from_string("render-...-...")
#   common::explore_example_from_string("explore-...-...")
CALL_RE = re.compile(
    r'(common::(render|explore)_example_from_string\(")([^"]+)("\))'
)

def hyphenate(name: str) -> str:
    return name.replace("_", "-")

def update_main_rs_text(src: str, correct_name: str) -> str:
    """
    Replace the string literal inside common::{render,explore}_example_from_string("...").
    If not found, return text unchanged.
    """
    def repl(m: re.Match) -> str:
        prefix, variant, _old_arg, suffix = m.groups()
        # Force the argument to the correct directory name (variant included inside correct_name)
        return f'{prefix}{correct_name}{suffix}'
    return CALL_RE.sub(repl, src)

def normalize_dir(dir_path: Path, dry_run: bool, force: bool) -> None:
    # Only process dirs that look like render-* or explore-*
    name = dir_path.name
    if not (name.startswith("render-") or name.startswith("explore-")):
        return

    target_name = hyphenate(name)
    target_dir = dir_path.with_name(target_name)

    # 1) Rename the directory if needed (to enforce hyphens)
    if target_name != name:
        if target_dir.exists():
            print(f"[WARN] Destination already exists: {target_dir}")
            if not force:
                print("       Skipping rename (use --force to overwrite).")
                # Continue to still attempt content updates in existing target
            else:
                # Be conservative: if force, remove dest then rename
                if dry_run:
                    print(f"[DRY] Would remove existing: {target_dir}")
                    print(f"[DRY] Would rename: {dir_path} -> {target_dir}")
                else:
                    if target_dir.is_dir():
                        shutil.rmtree(target_dir)
                    else:
                        target_dir.unlink()
                    dir_path.rename(target_dir)
                # Update handle for subsequent steps
                dir_path = target_dir
        else:
            if dry_run:
                print(f"[DRY] Would rename: {dir_path} -> {target_dir}")
                # Virtually treat as renamed
                dir_path = target_dir
            else:
                dir_path.rename(target_dir)
                dir_path = target_dir
            print(f"[OK ] Renamed: {name} -> {target_name}")
    else:
        # No rename necessary
        target_dir = dir_path

    # 2) Update main.rs string literal to match the (possibly new) dir name
    main_rs = target_dir / "main.rs"
    if main_rs.exists():
        try:
            text = main_rs.read_text(encoding="utf-8")
        except Exception as e:
            print(f"[ERR] Could not read {main_rs}: {e}")
            return

        updated = update_main_rs_text(text, target_dir.name)
        if updated != text:
            if dry_run:
                print(f"[DRY] Would update string literal in: {main_rs}")
            else:
                main_rs.write_text(updated, encoding="utf-8")
                print(f"[OK ] Updated call target in: {main_rs}")
        else:
            # If we didn’t find/replace, optionally warn (maybe custom main.rs)
            if CALL_RE.search(text) is None:
                print(f"[INFO] No call to common::{{render,explore}}_example_from_string found in {main_rs} (skipped).")
    else:
        print(f"[INFO] No main.rs in {target_dir} (skipped content update).")

    # 3) (Optional) Normalize any files immediately inside the directory that contain underscores
    #     – Typically not needed (main.rs, params.json are already hyphenated),
    #     – but we’ll rename any *_* -> *-*
    for child in list(target_dir.iterdir()):
        if child.is_file() and "_" in child.name:
            new_name = hyphenate(child.name)
            new_path = child.with_name(new_name)
            if new_path.exists() and new_path != child:
                if not force:
                    print(f"[WARN] File rename collision: {new_path} already exists (skipping {child}).")
                    continue
                else:
                    if dry_run:
                        print(f"[DRY] Would remove existing file: {new_path}")
                    else:
                        new_path.unlink()
            if dry_run:
                print(f"[DRY] Would rename file: {child} -> {new_path}")
            else:
                child.rename(new_path)
                print(f"[OK ] Renamed file: {child.name} -> {new_name}")

def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Normalize example names to hyphens in directory + main.rs content for render/explore variants."
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

    # Only consider direct children of root that are directories starting with render-/explore-
    candidates = [d for d in args.root.iterdir() if d.is_dir() and (d.name.startswith("render-") or d.name.startswith("explore-"))]
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

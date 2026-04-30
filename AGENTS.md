# Fractal Renderer — Agent Instructions

## Project Overview

High-performance fractal renderer in Rust. Renders Mandelbrot sets, Julia sets, Newton's method fractals, Barnsley fern, driven-damped pendulum phase portraits, quadratic maps, and Sierpinski variants. Supports interactive exploration (`winit`/`pixels` window) and offline rendering to image files.

Key dependencies: `rayon` (parallel iteration), `criterion` (benchmarks), `clap` (CLI parsing), `winit` + `pixels` (render window), `nalgebra` (linear algebra), `serde`/`serde_json` (parameter file serialization).

## Project Structure

```
src/
  lib.rs              # Crate root — lint config (forbid unsafe, deny clippy::all)
  main.rs             # Binary entry point, CLI dispatch
  cli/                # clap argument structs, render/explore/color-swatch subcommands
  core/               # Shared infrastructure: histogram, color maps, render window,
                      # image utilities, ODE solvers, view control, render FSM, etc.
  fractals/           # One module per fractal family: mandelbrot, julia,
                      # newtons_method, barnsley_fern, quadratic_map, etc.
benches/
  benchmark.rs        # criterion benchmarks
  *.json              # Parameter files used by benchmarks
tests/
  full_cli_integration_and_regression_tests.rs  # Pixel-hash regression tests
  example_parameter_validation_tests.rs         # Validates all param JSON files parse
  param_files/        # JSON fixtures for tests
examples/             # JSON parameter files for each fractal variant
```

## Code Standards

### Safety

- No unsafe code. `lib.rs` enforces `#![forbid(unsafe_code)]` — the compiler rejects it. Never suggest `unsafe` blocks, even for performance.
- `#![deny(clippy::all)]` is active on the library root. Clippy must be clean with `-D warnings` before committing.

### Style

- Prefer iterator/functional style over manual loops: `map`, `filter`, `fold`, `flat_map`, `zip`, `enumerate`, `windows`, `chunks`.
- Use `rayon` parallel iterators (`par_iter`, `par_iter_mut`, `into_par_iter`) for computationally intensive loops — `populate_histogram` in `src/fractals/utilities.rs` is the reference pattern.
- Avoid introducing new `unwrap()` or `expect()` in library code except tests and benchmarks. Prefer `?` propagation or explicit error handling; treat existing uses in `src/` as legacy exceptions to clean up opportunistically rather than patterns to copy.
- Add unit test for any new function with non-trivial logic. This is especially true for the `core/` directory.
- Match existing style. Keep the code concise and correct. Avoid unnecessary comments - code should be self-documenting.

### Documentation

- Every `pub` item (structs, enums, traits, functions, type aliases, and their `pub` fields/methods) must have a `///` doc comment.
- This applies to new code only — don't add docs to untouched code in the same PR.

### Performance Changes

- Any new or modified function on the render-critical path (called per-pixel or per-sample) must have a criterion benchmark in `benches/benchmark.rs`.
- Profile before optimizing — use `cargo bench` to measure before and after. Don't guess at bottlenecks.
- Reference benchmark pattern: `run_quadratic_map_histogram_benchmark` in `benches/benchmark.rs`.
- Prefer generics for render-critical polymorphism. Avoid `dyn` on per-pixel, per-sample, and other core rendering hot paths; however, non-hot-path uses such as error handling and API-boundary types (for example `Box<dyn std::error::Error>`) are acceptable.

## CI Requirements

Every commit must pass all of these — run them locally in this order:

```bash
cargo fmt                    # Fix Rust formatting (CI checks with --check)
cargo clippy -- -D warnings  # Zero warnings
cargo test                   # All unit and integration tests
cargo bench --no-run         # Benchmarks must compile
npm run fmt:check            # Prettier formatting for JSON and Markdown
```

JSON and Markdown are formatted with [Prettier](https://prettier.io/). Run `npm install` once to set it up, then `npm run fmt` to auto-format and `npm run fmt:check` to verify. Requires Node ≥14.

## Workflow

### Before Committing

1. `cargo fmt` — format all Rust files.
2. `cargo clippy -- -D warnings` — fix all warnings before committing.
3. `cargo test` — verify correctness.
4. `cargo bench --no-run` — verify benchmarks compile.
5. `npm run fmt:check` — verify JSON/Markdown formatting (use `npm run fmt` to fix).
6. If you modified a hot path: `cargo bench` to check for regressions.

When running inside Claude Code, steps 1–5 are enforced automatically by pre-commit hooks in `.claude/settings.json` before any `git commit` executes.

### Adding a New Fractal

1. Create `src/fractals/<name>.rs` with the fractal implementation.
2. Add `pub mod <name>;` to `src/fractals/mod.rs`.
3. Implement the relevant shared trait from `src/fractals/common.rs`.
4. Add `///` doc comments to all `pub` items.
5. Add parameter JSON files under `examples/` following the existing naming pattern.
6. Add at least one regression test with a fixture in `tests/param_files/`.
7. If the render path is computationally intensive, add a criterion benchmark.

### Parameter Files

Parameters are serialized as JSON via serde. The test suite validates all JSON files under `examples/` and `benches/` automatically. Ensure any new parameter file is covered by the glob pattern in `tests/example_parameter_validation_tests.rs`.

## Branch and Commit Conventions

Branch names: `feature/description`, `fix/description`, `perf/description`.

Commit messages: conventional commits (`feat:`, `fix:`, `perf:`, `refactor:`, `test:`, `docs:`, `chore:`) for structured changes; imperative short titles for small focused fixes. One logical change per commit.

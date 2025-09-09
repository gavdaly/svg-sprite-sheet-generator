# Repository Guidelines

## Project Structure & Module Organization
- `src/` — Rust sources: `main.rs` (entry), `cli.rs` (CLI parsing with clap), `svg.rs` (SVG parsing/transform).
- `svgs/` — Input SVG assets; each file becomes a `<pattern>` in the sprite.
- `sprite.svg` — Default generated output (configurable via flags).
- `target/` — Build artifacts (ignored in VCS).

## Build, Test, and Development Commands
- `cargo build` — Compile the project in debug mode.
- `cargo run` — Generate `sprite.svg` from `svgs/` using defaults.
- `cargo run -- -d svgs -f sprite2.svg build` — Explicit dirs/files; `build` subcommand runs generation.
- `cargo test` — Run unit tests in `src/svg.rs`.
- `cargo fmt` — Format code with `rustfmt`.
- `cargo clippy -- -D warnings` — Lint and treat warnings as errors.

## Coding Style & Naming Conventions
- Rust 2021 edition; use `rustfmt` defaults (4‑space indent).
- Names: modules/files `snake_case`, functions `snake_case`, types/enums `CamelCase`.
- Keep CLI concerns in `cli.rs` and parsing/IO in `svg.rs`.
- Prefer `Result<T, E>` returns; avoid new `unwrap()` on IO paths.

## Testing Guidelines
- Use built-in Rust tests (`#[cfg(test)]`), colocated with code.
- Name tests descriptively (e.g., `parse_attribute_in_kebab_case_test`).
- Run with `cargo test`; keep tests deterministic and file-system isolated.

## Commit & Pull Request Guidelines
- Use clear, imperative messages; prefer Conventional Commits (`feat:`, `fix:`, `refactor:`).
- PRs include summary, rationale, before/after notes, and CLI examples if behavior changes.
- Link issues; include screenshots only when output examples are relevant.

## Security & Configuration Tips
- Tool reads from `svgs/` and writes the sprite file; avoid committing generated outputs.
- Validate SVG inputs if sourced from untrusted locations.
- No network usage; paths are relative to repo root unless flags override.

## Agent-Specific Instructions
- Keep changes minimal and scoped; do not rename files or public APIs without discussion.
- Follow this guide’s style; run `cargo fmt`, `cargo clippy`, and add/adjust tests for parsing changes.

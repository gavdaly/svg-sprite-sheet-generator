# svg_sheet

A small Rust CLI that combines individual SVG files into a single sprite. Each SVG in a directory becomes a `<pattern>` definition inside one output SVG, enabling easy referencing by id.

## Quick Start
- Build: `cargo build --release`
- Run with defaults (reads `svgs/`, writes `sprite.svg`):
  - `cargo run`
- Example with explicit paths and subcommand:
  - `cargo run -- -d svgs -f sprite2.svg build`

## Usage
```
svg_sheet [OPTIONS] [COMMAND]

Options:
  -f, --file <FILE>       Output sprite file (default: sprite.svg)
  -d, --directory <DIR>   Input directory of SVGs (default: svgs)
  -h, --help              Print help
  -V, --version           Print version

Commands:
  build   Generate the sprite (same as default)
  watch   Watch for changes (coming soon)
```

## How It Works
- Reads all `*.svg` files in the specified directory.
- Parses the root `<svg>` element’s attributes and inner content.
- Emits a single file: `<svg><defs><pattern id="{name}" ...>{children}</pattern>...</defs></svg>`
  - `{name}` is the source filename without `.svg`.

## Examples
- Default directories/files: `cargo run`
- Custom output file: `cargo run -- -f sprite2.svg`
- Custom input directory: `cargo run -- -d assets/icons`

## Development
- Format: `cargo fmt`
- Lint: `cargo clippy -- -D warnings`
- Tests: `cargo test`

## Releases
- Follow the steps in `RELEASE.md` to version, build, package, and publish.
- Quick build: `cargo build --release` (artifacts in `target/release/`).
- Tag format: `vMAJOR.MINOR.PATCH` (e.g., `v1.2.3`).

## Notes
- Generated files and `svgs/` are ignored by VCS (see `.gitignore`).
- Avoid placing non-SVG files in the input directory.
- Error handling is improving; avoid write-protected output locations.

## Roadmap
- See `ROADMAP.md` for detailed backlog and priorities.

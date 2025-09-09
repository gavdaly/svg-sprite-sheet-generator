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

## Using <use> With Generated Ids

This tool emits one `<pattern>` per input file, with the pattern’s `id` set to the filename (without `.svg`). Patterns are great for paint servers (e.g., `fill="url(#dots)"`), but `<use>` does not render a `<pattern>` directly. To use `<use>`, reference renderable elements that you define inside your source SVGs.

Guidelines and examples:

1) Reference inner elements (not the root)

- Do not rely on the root `<svg id>`; root ids are moved to `data-id` and are not referenceable. Instead, give an inner group a stable id.

Source file `svgs/arrow.svg`:

```svg
<svg width="24" height="24" viewBox="0 0 24 24">
  <g id="icon-arrow">
    <path d="M4 12h12M10 6l6 6-6 6" stroke="currentColor" fill="none" stroke-width="2"/>
  </g>
  <!-- other shapes are fine too -->
  <!-- avoid using root id; it will be moved to data-id -->
  <!-- <svg id="arrow"> ... </svg> -->
  <!-- becomes data-id="arrow" in the sprite -->
  
</svg>
```

Generated sprite will contain:

```svg
<svg xmlns="http://www.w3.org/2000/svg"><defs>
  <pattern id="arrow" width="24" height="24">
    <g id="icon-arrow">...</g>
  </pattern>
</defs></svg>
```

You can reference `#icon-arrow` with `<use>` because it’s a renderable `<g>`:

```html
<!-- Inline the sprite in your HTML (recommended for portability) -->
<div style="display:none">{{ sprite.svg content here }}</div>

<!-- Later in the document -->
<svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true">
  <use href="#icon-arrow" />
</svg>
```

2) Using the pattern id for fills

- The filename-based id lives on a `<pattern>`. Use it as a paint server:

```html
<!-- Apply pattern fill via url(#id) -->
<svg width="100" height="24" viewBox="0 0 100 24" role="img" aria-label="Decorative pattern">
  <rect x="0" y="0" width="100" height="24" fill="url(#arrow)" />
  <!-- If your source SVG content is a seamless tile, this paints a repeated fill -->
  <!-- For icons, prefer the <use href="#inner-id"> pattern above -->
  
</svg>
```

3) Best practices for ids

- Root `<svg id>`: Avoid — it is moved to `data-id`. If present, it must not be referenced inside the same file.
- Inner ids: Allowed and unchanged; ensure uniqueness across files. Collisions fail the build.
- Stable naming: Prefix inner ids, e.g., `id="icon-<file>"` to reduce risk of clashes.
- Sanitization: Only the root id is sanitized and moved to `data-id`; inner ids are preserved verbatim.

4) Inline vs external `<use>`

- Inline the sprite (server include, build-time HTML injection) for the most reliable `<use href="#...">` support.
- External references like `<use href="/sprite.svg#icon-id">` can work on same-origin, but may be blocked by some browsers or CORS settings. Inlining avoids these issues.

5) `href` vs `xlink:href`

- Use `href` (SVG2). `xlink:href` still works in many browsers but is deprecated.

6) Sizing and viewBox

- Set `viewBox` and `width`/`height` on your `<svg>` where you place `<use>`; the referenced content inherits that viewport.
- The tool normalizes root `viewBox` and `width`/`height` for each source, but does not modify nested elements.

If you prefer `<symbol>`-based sprites for `<use>`, consider wrapping your icon content in a `<symbol>` in each source file. The element will still be emitted inside `<pattern>`, but the inner `<symbol id="...">` remains referenceable via `<use href="#...">` just like `<g>`.

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

# Release and Packaging

## Versioning
- Follow Semantic Versioning: MAJOR.MINOR.PATCH (e.g., 1.2.3).
- Update `version` in `Cargo.toml` and ensure `-V/--version` reflects it.

## Pre‑flight Checklist
- Clean tree: `git status` shows no changes; `git pull` up to date.
- Lint, format, test:
  - `cargo fmt --all`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test --all`

## Build Binaries (manual)
- Optimize build: `cargo build --release` (artifacts in `target/release/`).
- Optional strip: `strip target/release/svg_sheet` (platform permitting).

## Package Artifacts (manual)
- Linux/macOS tarball:
  - `tar -C target/release -czf svg_sheet-$VERSION-$TARGET.tar.gz svg_sheet`
- Windows zip:
  - `cd target\\release && powershell -Command "Compress-Archive -Path svg_sheet.exe -DestinationPath ..\\svg_sheet-$VERSION-windows.zip"`
- Checksums:
  - `shasum -a 256 svg_sheet-* > SHA256SUMS.txt` (use `sha256sum` on Linux).

## Tag and Release (automated)
There is a GitHub Actions workflow that builds and uploads release artifacts and a checksum file when a SemVer tag is pushed (e.g., `v1.2.3`).

- Bump version and tag (using cargo-release recommended):
  - `cargo install cargo-release` (first time only)
  - `cargo release <level-or-version> --execute` (e.g., `cargo release minor --execute`)
    - Creates commit, tag `vX.Y.Z`, and pushes.
- CI builds artifacts for Linux, macOS, and Windows, computes `SHA256SUMS.txt`, and attaches them to the GitHub Release.

Manual alternative:
- Commit version bump: `git add Cargo.toml Cargo.lock && git commit -m "chore(release): v$VERSION"`.
- Create tag: `git tag -a v$VERSION -m "Release v$VERSION"` and `git push --follow-tags`.
- CI will detect the tag and produce the Release with artifacts.

## crates.io (optional)
- Ensure `Cargo.toml` has complete package metadata (description, repository, license, keywords, categories, readme).
- Dry run: `cargo publish --dry-run`.
- Publish: `cargo publish`.

## Cross‑Compilation (optional)
- For reproducible cross-builds, consider `cross`: https://github.com/cross-rs/cross
  - Install: `cargo install cross`
  - Build: `cross build --release --target x86_64-unknown-linux-gnu`

## Homebrew/Scoop (optional)
- Homebrew: maintain a tap with a formula pointing to the GitHub Release tarball and checksum.
- Scoop: add a bucket manifest referencing the Windows zip and checksum.

## Verify
- Run `svg_sheet -V` from each artifact to confirm version.
- Smoke test: run against a small `svgs/` folder and confirm output.

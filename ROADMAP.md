# Project Roadmap

## Features

- [ ] Event-based `watch` using `notify` with debounce; `--poll` fallback.
- [ ] Verbosity flags: `--quiet`, `--verbose`; `--dry-run` and `--fail-on-warn`.
- [ ] Shell completions and man page via `clap_complete` and `clap_mangen`.

## Parsing & Robustness

- [x] Attribute values: allow single quotes, digits, underscores, colons (e.g., `xmlns:*`), and boolean attributes.
- [x] Normalize/validate `viewBox`, `width`/`height`; handle BOM, XML prolog, comments.
- [ ] Optional `quick-xml` parser for full XML compliance; keep lightweight fast-path.
- [x] Stable id generation, collision detection, and id sanitization.

## Performance

- [ ] Parallelize file read/parse with `rayon` (bounded concurrency).
- [x] Stream sprite writing to reduce memory usage for large inputs.
- [x] Cache by file mtime/hash for incremental rebuilds during `watch`.

## UX & Error Reporting

- [ ] Structured logging via `tracing` with `--log-level` and env filter.
- [ ] Pretty diagnostics using `miette` with hints for common parse failures.
- [ ] Progress bar for large directories; concise rebuild summaries.

## Tooling & CI

- [x] GitHub Actions workflow: fmt, clippy, test (matrix: macOS, Linux, Windows; include MSRV).
- [ ] Coverage reporting (`cargo-llvm-cov` or `tarpaulin`).
- [ ] Security and policy checks: `cargo-audit`, `cargo-deny` (licenses/vulns).
- [ ] Pin toolchain in `rust-toolchain.toml`.

## Testing

- [x] Integration tests with `assert_cmd` + `predicates` covering CLI behaviors.
- [ ] Snapshot tests of output sprite with `insta`.
- [x] Property tests for attribute parser with `proptest`.
- [ ] Fuzzing harness via `cargo-fuzz` targeting parsing.

## Packaging & Release

- [ ] Automate releases with `cargo-release` or `cargo-dist` (artifacts + checksums).
- [ ] Publish completions and man page with releases; attach SBOM.
- [ ] Add formulas/manifests: Homebrew tap, Scoop bucket, Nix flake, winget.

## Documentation

- [ ] CHANGELOG (Keep a Changelog) and CONTRIBUTING guide.
- [ ] Installation instructions (brew/scoop/cargo) and troubleshooting.
- [x] Examples of using `<use>` with generated ids and best practices.

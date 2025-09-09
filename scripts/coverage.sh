#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found; please install Rust (https://rustup.rs)" >&2
  exit 1
fi

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "cargo-llvm-cov not found." >&2
  echo "Install with: cargo install cargo-llvm-cov" >&2
  echo "See https://github.com/taiki-e/cargo-llvm-cov for requirements (llvm-tools)." >&2
  exit 1
fi

# Clean previous coverage data
cargo llvm-cov clean --workspace

# Text summary
echo "==> Running coverage (text summary)"
cargo llvm-cov --workspace --all-features --summary-only

# Generate HTML report
echo "==> Generating HTML report at target/llvm-cov/html"
cargo llvm-cov --workspace --all-features --html

# Generate lcov info
echo "==> Writing LCOV at ./lcov.info"
cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info

echo "Done. Open target/llvm-cov/html/index.html in your browser."


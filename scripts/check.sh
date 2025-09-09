#!/usr/bin/env bash
set -euo pipefail

echo "Running cargo fmt --check"
cargo fmt --all -- --check

echo "Running cargo clippy (-D warnings)"
cargo clippy --all-targets --all-features -- -D warnings

echo "Running cargo test"
cargo test --all-features --no-fail-fast

echo "All checks passed."


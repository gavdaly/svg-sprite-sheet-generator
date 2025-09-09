#!/usr/bin/env bash
set -euo pipefail

echo "[dev_checks] Running cargo fmt..."
cargo fmt

echo "[dev_checks] Running cargo clippy (deny warnings)..."
cargo clippy -- -D warnings

echo "[dev_checks] Running cargo test..."
cargo test --all-features

echo "[dev_checks] All checks passed."


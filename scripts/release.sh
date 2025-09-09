#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Release helper: bump version and tag.

Usage:
  scripts/release.sh [options] <major|minor|patch|X.Y.Z>

Options:
  --dry-run     Print actions without changing anything
  --no-push     Do not push commits/tags to origin

Notes:
  - If 'cargo-release' is installed, this script will delegate to it.
  - Otherwise it performs a minimal, manual bump + tag flow.
USAGE
}

is_semver() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

get_current_version() {
  # Extract first occurrence of version = "X.Y.Z" from Cargo.toml
  awk -F '"' '/^version\s*=\s*"[0-9]+\.[0-9]+\.[0-9]+"/ {print $2; exit}' Cargo.toml
}

compute_bump() {
  local cur="$1" level="$2"
  IFS='.' read -r major minor patch <<<"$cur"
  case "$level" in
    major) echo "$((major + 1)).0.0" ;;
    minor) echo "$major.$((minor + 1)).0" ;;
    patch) echo "$major.$minor.$((patch + 1))" ;;
    *) echo "error: unknown bump level '$level'" >&2; return 1 ;;
  esac
}

sed_in_place() {
  # Portable in-place sed (BSD/macOS and GNU)
  local expr="$1" file="$2"
  if sed --version >/dev/null 2>&1; then
    sed -i "$expr" "$file"
  else
    sed -i '' "$expr" "$file"
  fi
}

ensure_clean_tree() {
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "error: not a git repository" >&2
    exit 1
  fi
  local branch
  branch=$(git rev-parse --abbrev-ref HEAD)
  if [[ "$branch" != "main" && "$branch" != "master" ]]; then
    echo "error: releases must be on 'main' or 'master' (current: $branch)" >&2
    exit 1
  fi
  if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "error: working tree not clean; commit or stash changes first" >&2
    exit 1
  fi
  git fetch -q origin "$branch" || true
  local ahead
  ahead=$(git rev-list --left-right --count "$branch"..."origin/$branch" | awk '{print $1}')
  if [[ "${ahead:-0}" != "0" ]]; then
    echo "error: local branch is ahead of origin; push or sync first" >&2
    exit 1
  fi
}

run_checks() {
  if [[ -x scripts/dev_checks.sh ]]; then
    bash scripts/dev_checks.sh
  else
    echo "[release] dev_checks.sh not found or not executable; skipping"
  fi
}

delegate_to_cargo_release() {
  local spec="$1" dry_run="$2" no_push="$3"
  local args=("release" "$spec")
  if [[ "$dry_run" == "1" ]]; then args+=("--dry-run"); else args+=("--execute"); fi
  if [[ "$no_push" == "1" ]]; then args+=("--no-push"); fi
  echo "[release] Using cargo-release: cargo ${args[*]}"
  cargo "${args[@]}"
}

manual_release() {
  local new_version="$1" dry_run="$2" no_push="$3"
  local tag="v${new_version}"

  echo "[release] New version: $new_version (tag: $tag)"

  if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "error: tag $tag already exists" >&2
    exit 1
  fi

  if [[ "$dry_run" == "1" ]]; then
    echo "[dry-run] Would update Cargo.toml version -> $new_version"
    echo "[dry-run] Would run dev checks"
    echo "[dry-run] Would commit and tag $tag"
    if [[ "$no_push" == "0" ]]; then
      echo "[dry-run] Would push commits and tag to origin"
    fi
    return 0
  fi

  # Update Cargo.toml version
  sed_in_place "s/^version\\s*=\\s*\"[0-9]*\\.[0-9]*\\.[0-9]*\"/version = \"$new_version\"/" Cargo.toml

  # Run checks after bump
  run_checks

  # Commit, tag, push
  git add Cargo.toml Cargo.lock 2>/dev/null || true
  git commit -m "chore(release): v$new_version"
  git tag -a "$tag" -m "Release $tag"
  if [[ "$no_push" == "0" ]]; then
    git push
    git push origin "$tag"
  fi
}

main() {
  local dry_run=0 no_push=0
  local spec=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run) dry_run=1; shift ;;
      --no-push) no_push=1; shift ;;
      -h|--help) usage; exit 0 ;;
      *) spec="$1"; shift ;;
    esac
  done

  if [[ -z "$spec" ]]; then
    usage; exit 1
  fi

  ensure_clean_tree

  # Determine desired new version
  local current new_version
  current=$(get_current_version)
  if [[ -z "$current" ]]; then
    echo "error: could not read current version from Cargo.toml" >&2
    exit 1
  fi

  if [[ "$spec" == "major" || "$spec" == "minor" || "$spec" == "patch" ]]; then
    new_version=$(compute_bump "$current" "$spec")
  elif is_semver "$spec"; then
    new_version="$spec"
  else
    echo "error: spec must be one of major|minor|patch or X.Y.Z" >&2
    exit 1
  fi

  # Prefer cargo-release if available
  if cargo release -V >/dev/null 2>&1 || command -v cargo-release >/dev/null 2>&1; then
    # Ensure code passes checks before delegating
    if [[ "$dry_run" == "0" ]]; then run_checks; fi
    delegate_to_cargo_release "$spec" "$dry_run" "$no_push"
  else
    manual_release "$new_version" "$dry_run" "$no_push"
  fi
}

main "$@"


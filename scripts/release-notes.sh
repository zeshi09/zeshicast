#!/usr/bin/env bash
# Release helpers for .github/workflows/release.yml (Q4.2), runnable locally.
#
#   scripts/release-notes.sh v0.2.0                  print the CHANGELOG section
#   scripts/release-notes.sh --check-version v0.2.0  fail unless Cargo.toml agrees
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo_version() {
  sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1
}

if [[ "${1:-}" == "--check-version" ]]; then
  tag="${2:?usage: --check-version vX.Y.Z}"
  version="$(cargo_version)"
  if [[ "$tag" != "v$version" ]]; then
    echo "tag $tag does not match Cargo.toml version $version" >&2
    exit 1
  fi
  exit 0
fi

tag="${1:?usage: release-notes.sh vX.Y.Z}"
version="${tag#v}"

# The lines after "## [X.Y.Z]" up to the next "## " heading.
notes="$(awk -v heading="## [$version]" '
  index($0, heading) == 1 { found = 1; next }
  found && /^## / { exit }
  found { print }
' "$ROOT_DIR/CHANGELOG.md")"

if [[ -z "${notes//[[:space:]]/}" ]]; then
  echo "CHANGELOG.md has no section for [$version]" >&2
  exit 1
fi
printf '%s\n' "$notes"

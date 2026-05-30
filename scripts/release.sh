#!/usr/bin/env bash
#
# Cut a new plz release.
#
#   scripts/release.sh patch          # 0.1.1 -> 0.1.2
#   scripts/release.sh minor          # 0.1.1 -> 0.2.0
#   scripts/release.sh major          # 0.1.1 -> 1.0.0
#   scripts/release.sh 0.3.0          # explicit version
#   scripts/release.sh patch --rc     # 0.1.2-rc1 (prerelease: npm `next`, Homebrew skipped)
#
# Bumps Cargo.toml + Cargo.lock + npm package.json, commits "Release vX.Y.Z",
# tags vX.Y.Z, and (after confirmation) pushes. The tag push triggers
# .github/workflows/release.yml, which builds binaries and publishes to GitHub
# Releases, npm, and the Homebrew tap. See docs/release.md.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

usage() { echo "usage: $0 <patch|minor|major|X.Y.Z> [--rc]" >&2; exit 1; }

bump="${1:-}"
rc=""
[[ "${2:-}" == "--rc" ]] && rc=1
[[ -n "$bump" ]] || usage

# Preconditions: on main, clean tree, in sync with origin.
branch="$(git rev-parse --abbrev-ref HEAD)"
[[ "$branch" == "main" ]] || { echo "error: not on main (on '$branch')" >&2; exit 1; }
[[ -z "$(git status --porcelain)" ]] || { echo "error: working tree is dirty — commit or stash first" >&2; exit 1; }
git fetch -q origin
[[ "$(git rev-parse @)" == "$(git rev-parse '@{u}')" ]] || { echo "error: local main is not in sync with origin/main" >&2; exit 1; }

current="$(grep -m1 '^version = ' Cargo.toml | sed -E 's/^version = "(.*)"/\1/')"

case "$bump" in
  patch|minor|major)
    IFS='.' read -r major minor patch <<<"${current%%-*}"
    case "$bump" in
      patch) patch=$((patch + 1)) ;;
      minor) minor=$((minor + 1)); patch=0 ;;
      major) major=$((major + 1)); minor=0; patch=0 ;;
    esac
    new="${major}.${minor}.${patch}"
    ;;
  [0-9]*.[0-9]*.[0-9]*) new="$bump" ;;
  *) usage ;;
esac

[[ -n "$rc" ]] && new="${new}-rc1"

echo "Releasing ${current} -> ${new}"

# Bump versions. The first `version = "..."` line in Cargo.toml is the [package]
# one (awk, first-match-only — portable across BSD/GNU; `sed 0,/re/` is GNU-only).
awk -v v="$new" '!done && /^version = "/ { sub(/=.*/, "= \"" v "\""); done=1 } { print }' \
  Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml
( cd npm/pretty-plz && npm pkg set version="${new}" >/dev/null )
# Sync Cargo.lock's own-package entry (and sanity-check it still compiles).
cargo check --quiet

git add Cargo.toml Cargo.lock npm/pretty-plz/package.json
git commit -q -m "Release v${new}"
git tag "v${new}"
echo "Committed and tagged v${new}."

printf 'Push main + tag now to trigger the release? [y/N] '
read -r reply
if [[ "$reply" == [yY] ]]; then
  git push origin main "v${new}"
  echo "Pushed. Find the run:   gh run list --workflow=release.yml"
  echo "       Then watch it:   gh run watch <run-id>"
else
  echo "Not pushed. When ready:  git push origin main v${new}"
  echo "To undo the local release: git tag -d v${new} && git reset --hard HEAD~1"
fi

#!/usr/bin/env bash
# next-version.sh <current-version> <current|patch|minor|major>
#
# Print the version to release. `current` keeps the version as it is (for
# releasing a version that was never tagged, such as the very first one).
#   next-version.sh 0.1.0 patch   ->  0.1.1
#   next-version.sh 0.1.9 minor   ->  0.2.0
#   next-version.sh 0.4.2 major   ->  1.0.0
set -euo pipefail

current=${1:?usage: next-version.sh <version> <current|patch|minor|major>}
bump=${2:?usage: next-version.sh <version> <current|patch|minor|major>}

if [[ ! $current =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "error: '$current' is not a MAJOR.MINOR.PATCH version" >&2
  exit 1
fi
major=${BASH_REMATCH[1]} minor=${BASH_REMATCH[2]} patch=${BASH_REMATCH[3]}

case $bump in
  current) ;;
  patch) patch=$((patch + 1)) ;;
  minor) minor=$((minor + 1)) patch=0 ;;
  major) major=$((major + 1)) minor=0 patch=0 ;;
  *)
    echo "error: unknown bump '$bump' (expected current, patch, minor or major)" >&2
    exit 1
    ;;
esac

echo "$major.$minor.$patch"

#!/usr/bin/env bash
# Release logic for .github/workflows/build.yml (the header there explains the
# flow). The release tag is the version in .github/VERSION.
#
#   release.sh version        Print the version from .github/VERSION.
#   release.sh sync           Write that version into Cargo.toml and Cargo.lock.
#   release.sh plan           Decide what this commit needs and write
#                             version / tag / action to $GITHUB_OUTPUT:
#                               create   no release for the tag yet
#                               rebuild  release exists, app code changed
#                               retag    release exists, only docs changed
#                               none     release exists at this very commit
#   release.sh publish DIR    Apply the action from $RELEASE_ACTION using the
#                             packages in DIR.
#
# Needs: git, gh (preinstalled on GitHub runners), GH_TOKEN with
# contents:write for publish.
set -euo pipefail

SHA="${GITHUB_SHA:-$(git rev-parse HEAD)}"

die() { echo "::error::$*" >&2; exit 1; }

# Paths that never reach the packages; a push touching only these moves the
# tag (so the source archive is current) without rebuilding.
only_unpackaged() {
  local path
  while IFS= read -r path; do
    case "$path" in
      *.md|LICENSE|.gitignore|.gitattributes|Run.bat) ;;
      *) return 1 ;;
    esac
  done
}

read_version() {
  local version
  version="$(tr -d '[:space:]' < .github/VERSION 2>/dev/null || true)"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] \
    || die ".github/VERSION is missing or not a version like 1.2.3"
  echo "$version"
}

load_version() {
  VERSION="$(read_version)"
  TAG="$VERSION"
}

# Cargo.toml and Cargo.lock take their version from .github/VERSION, so it is
# never edited anywhere else. Only the package's own entry changes.
sync() {
  load_version
  sed -i.bak -E '0,/^version = ".*"/s//version = "'"$VERSION"'"/' Cargo.toml
  awk -v version="$VERSION" '
    /^name = "betterwriter"$/ { mine = 1; print; next }
    mine && /^version = / { print "version = \"" version "\""; mine = 0; next }
    { print }
  ' Cargo.lock > Cargo.lock.new
  mv Cargo.lock.new Cargo.lock
  rm -f Cargo.toml.bak
  echo "Cargo.toml and Cargo.lock are at $VERSION."
}

emit() {
  echo "$1=$2"
  if [ -n "${GITHUB_OUTPUT:-}" ]; then echo "$1=$2" >> "$GITHUB_OUTPUT"; fi
}

plan() {
  load_version
  emit version "$VERSION"
  emit tag "$TAG"

  local action changed
  changed="$(mktemp)"
  if ! gh release view "$TAG" >/dev/null 2>&1; then
    echo "No release for $TAG yet."
    action=create
  elif [ "${FORCE_REBUILD:-false}" = "true" ]; then
    echo "Release $TAG exists, rebuild forced."
    action=rebuild
  else
    local built
    built="$(git rev-parse "refs/tags/$TAG^{commit}" 2>/dev/null || true)"
    if [ "$built" = "$SHA" ]; then
      echo "Release $TAG already points at this commit."
      action=none
    elif [ -n "$built" ] && git diff --name-only "$built" "$SHA" > "$changed" 2>/dev/null; then
      if only_unpackaged < "$changed"; then
        echo "Release $TAG exists, only unpackaged files changed since ${built:0:7}."
        action=retag
      else
        echo "Release $TAG exists, packaged files changed since ${built:0:7}:"
        sed 's/^/  /' "$changed"
        action=rebuild
      fi
    else
      echo "Release $TAG exists but its tag is missing or not in history."
      action=rebuild
    fi
  fi
  rm -f "$changed"
  emit action "$action"
}

# GitHub stores spaces in asset names as dots. Uploading under that name
# up front lets --clobber find and replace the existing asset on a rebuild.
stage_assets() {
  local dir="$1" file base count=0
  STAGE="$(mktemp -d)"
  trap 'rm -rf "$STAGE"' EXIT
  for file in "$dir"/*; do
    [ -f "$file" ] || continue
    base="$(basename "$file")"
    cp "$file" "$STAGE/${base// /.}"
    count=$((count + 1))
  done
  [ "$count" -gt 0 ] || die "no packages found in $dir"
}

move_tag() {
  if [ "$(git rev-parse "refs/tags/$TAG^{commit}" 2>/dev/null || true)" != "$SHA" ]; then
    git tag -f "$TAG" "$SHA"
    git push -f origin "refs/tags/$TAG"
    echo "Moved tag $TAG to ${SHA:0:7}."
  fi
}

publish() {
  local dir="${1:?package directory}"
  load_version
  case "${RELEASE_ACTION:?RELEASE_ACTION not set}" in
    create)
      stage_assets "$dir"
      local flags=()
      case "$VERSION" in *-*) flags+=(--prerelease) ;; esac
      gh release create "$TAG" "$STAGE"/* ${flags[@]+"${flags[@]}"} \
        --title "Release $VERSION" --target "$SHA" --generate-notes
      echo "Created release $TAG."
      ;;
    rebuild)
      stage_assets "$dir"
      gh release upload "$TAG" "$STAGE"/* --clobber
      echo "Replaced packages on release $TAG."
      move_tag
      ;;
    retag)
      move_tag
      ;;
    *)
      die "unknown RELEASE_ACTION '$RELEASE_ACTION'"
      ;;
  esac
}

case "${1:-}" in
  version) read_version ;;
  sync) sync ;;
  plan) plan ;;
  publish) shift; publish "$@" ;;
  *) die "usage: release.sh version | sync | plan | publish DIR" ;;
esac

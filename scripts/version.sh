#!/usr/bin/env bash
# A release has one version: the program's, which is also the IDL's and both
# clients' (docs/releases.md).
#
#   scripts/version.sh check [--tag <tag>]
#   scripts/version.sh set <version>
#
#   check   The program, the IDL of record and the two clients say the same
#           version. If that version is released, the IDL is still the
#           release's: a changed interface needs the next version. With
#           --tag, that the tag is that version with a `v`.
#   set     Writes <version> into the program's and the clients' manifests and
#           lockfiles. The IDL takes it from the next build: see what it
#           prints.
set -euo pipefail

cd "$(dirname "$0")/.."

program=programs/raffles/Cargo.toml
rust_client=clients/rust/Cargo.toml
ts_client=clients/ts/package.json
ts_lock=clients/ts/package-lock.json
idl=idls/raffles.json

die() {
  echo "error: $*" >&2
  exit 1
}

usage() {
  awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
}

command -v jq >/dev/null || die "jq is not on PATH"

# The `version` of a Cargo package: the first one in its manifest.
cargo_version() {
  sed -n 's/^version = "\(.*\)"$/\1/p' "$1" | head -n 1
}

check() {
  local tag=""
  if [[ ${1:-} == --tag ]]; then
    tag=${2:-}
    [[ -n $tag ]] || die "--tag needs a tag"
    shift 2
  fi
  (($# == 0)) || die "unknown argument: $1 (see --help)"

  local version where found wrong=false
  version=$(cargo_version "$program")
  [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "$program: '$version' is not a version such as 0.1.0"

  while IFS=$'\t' read -r where found; do
    if [[ $found != "$version" ]]; then
      echo "$where is at '$found', but $program is at $version" >&2
      wrong=true
    fi
  done <<EOF_VERSIONS
$rust_client	$(cargo_version "$rust_client")
$ts_client	$(jq -r '.version' "$ts_client")
$ts_lock	$(jq -r '.version' "$ts_lock")
$ts_lock, its root package	$(jq -r '.packages[""].version' "$ts_lock")
$idl	$(jq -r '.metadata.version' "$idl")
EOF_VERSIONS
  $wrong && die "one release, one version: scripts/version.sh set $version, then rebuild the IDL"

  # A released version's IDL is frozen: consumers pinned to it hold that IDL.
  # A clone without the tag can't tell, so CI fetches the tags first.
  if git rev-parse -q --verify "refs/tags/v$version^{commit}" >/dev/null 2>&1 &&
    [[ $(git show "refs/tags/v$version:$idl" | sha256sum) != "$(sha256sum <"$idl")" ]]; then
    die "release $version is out, and $idl is no longer its IDL. The interface changed, so this is the next version: scripts/version.sh set <version>"
  fi

  if [[ -n $tag && $tag != "v$version" ]]; then
    die "the tag is $tag, but the version is $version: the tag would be v$version"
  fi
  echo "version $version"
}

set_version() {
  local version=${1:-}
  [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "set takes a version such as 0.2.0"
  (($# == 1)) || die "set takes one version"

  local manifest
  # Not `local`: the trap runs when the script exits, after this function.
  updated=$(mktemp)
  trap 'rm -f "$updated"' EXIT
  for manifest in "$program" "$rust_client"; do
    # Only the package's own version: the first `version = ` line.
    awk -v version="$version" '
      !done && /^version = "/ { print "version = \"" version "\""; done = 1; next }
      { print }
    ' "$manifest" >"$updated"
    cat "$updated" >"$manifest"
  done
  jq --arg version "$version" '.version = $version' "$ts_client" >"$updated"
  cat "$updated" >"$ts_client"
  jq --arg version "$version" '.version = $version | .packages[""].version = $version' "$ts_lock" >"$updated"
  cat "$updated" >"$ts_lock"

  # The lockfiles name the two crates with their version.
  if command -v cargo >/dev/null; then
    cargo update --workspace --offline --quiet
    cargo update --workspace --offline --quiet --manifest-path "$rust_client"
  else
    echo "cargo is not on PATH: Cargo.lock and clients/rust/Cargo.lock still name the old version" >&2
  fi

  cat <<EOF_NEXT
The program and both clients are at $version. The IDL carries the version too,
so build it and take the new one:

  anchor build --ignore-keys
  cp target/idl/raffles.json idls/raffles.json
EOF_NEXT
}

case "${1:-}" in
  check) shift; check "$@" ;;
  set) shift; set_version "$@" ;;
  -h | --help) usage ;;
  *) usage >&2; exit 2 ;;
esac

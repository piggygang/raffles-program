#!/usr/bin/env bash
# The drift check: fails when a copy of the raffles IDL isn't byte for byte
# the IDL of a release (docs/releases.md).
#
#   scripts/idl-drift.sh <idl> [--release <version>]
#                              [--cluster <cluster> [--strict]]
#
#   <idl>                 The copy to check: the IDL a consumer vendored.
#   --release <version>   Compare with that release's raffles.json, fetched
#                         from GitHub. Without it, compare with
#                         idls/raffles.json next to this script: the release
#                         this checkout is at. The action runs it that way
#                         (.github/actions/idl-drift), pinned to a tag.
#   --cluster <cluster>   Also read deployments.json on the default branch,
#                         and warn when the cluster's last deployment has
#                         another IDL than the release compared with.
#   --strict              Fail on that instead of warning.
#   --deployments <file>  Read this deployments.json instead of fetching it.
#
# Exit status: 0 the copy is the release's IDL, 1 it isn't (or, with
# --strict, the cluster runs another), 2 the check couldn't run.
set -euo pipefail

repository=https://github.com/piggygang/raffles-program
raw=https://raw.githubusercontent.com/piggygang/raffles-program
here=$(cd "$(dirname "$0")" && pwd)

fail() {
  echo "error: $*" >&2
  exit 2
}

usage() {
  awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
}

# An annotation on the pull request when GitHub Actions runs this.
annotate() {
  local level=$1 message=$2
  if [[ ${GITHUB_ACTIONS:-} == true ]]; then
    echo "::$level title=Raffles IDL::$message"
  fi
}

sha256() {
  sha256sum "$1" | cut -d' ' -f1
}

idl="" release="" cluster="" strict=false deployments=""
while (($# > 0)); do
  case "$1" in
    --release) release=${2:-}; shift 2 || fail "--release needs a version" ;;
    --cluster) cluster=${2:-}; shift 2 || fail "--cluster needs a cluster" ;;
    --deployments) deployments=${2:-}; shift 2 || fail "--deployments needs a file" ;;
    --strict) strict=true; shift ;;
    -h | --help) usage; exit 0 ;;
    -*) fail "unknown argument: $1 (see --help)" ;;
    *)
      [[ -z $idl ]] || fail "one IDL at a time: $idl, $1"
      idl=$1
      shift
      ;;
  esac
done
[[ -n $idl ]] || { usage >&2; exit 2; }
[[ -f $idl ]] || fail "no IDL at $idl"
for tool in jq sha256sum cmp diff; do
  command -v "$tool" >/dev/null || fail "$tool is not on PATH"
done

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

if [[ -n $release ]]; then
  release=${release#v}
  [[ $release =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "--release is a version such as 0.1.0"
  command -v curl >/dev/null || fail "curl is not on PATH"
  expected=$scratch/raffles.json
  curl -sSfL -o "$expected" "$repository/releases/download/v$release/raffles.json" ||
    fail "could not fetch the IDL of release $release"
else
  expected=$here/../idls/raffles.json
  [[ -f $expected ]] || fail "no idls/raffles.json next to this script: pass --release"
fi
version=$(jq -er '.metadata.version' "$expected" 2>/dev/null) ||
  fail "the release's IDL has no metadata.version"
[[ -z $release || $release == "$version" ]] ||
  fail "release $release's raffles.json says it is $version"
expected_sha256=$(sha256 "$expected")

if [[ -n ${GITHUB_OUTPUT:-} ]]; then
  {
    echo "version=$version"
    echo "sha256=$expected_sha256"
  } >>"$GITHUB_OUTPUT"
fi

if ! cmp -s "$idl" "$expected"; then
  {
    echo "error: $idl is not the raffles IDL of release $version"
    echo
    echo "  $(sha256 "$idl")  $idl"
    echo "  $expected_sha256  release $version"
    echo
    if [[ $(jq -cS . "$idl" 2>/dev/null) == "$(jq -cS . "$expected")" ]]; then
      echo "The two are the same JSON in another layout: a formatter or an editor rewrote the"
      echo "copy. The check compares bytes, because a release's IDL hash is of its bytes."
    else
      # The first differences are enough to tell a stale copy from an edit.
      diff -u --label "release $version" --label "$idl" "$expected" "$idl" | head -n 40 || true
    fi
    echo
    echo "Take raffles.json from the release again, or pin the release this copy is from:"
    echo "  $repository/releases/download/v$version/raffles.json"
  } >&2
  annotate error "$idl is not the IDL of release $version: it drifted, or the pin did."
  exit 1
fi
echo "$idl is the raffles IDL of release $version ($expected_sha256)"

[[ -n $cluster ]] || exit 0

# What the cluster runs, by the record the program repository keeps.
if [[ -z $deployments ]]; then
  command -v curl >/dev/null || fail "curl is not on PATH"
  deployments=$scratch/deployments.json
  if ! curl -sSfL -o "$deployments" "$raw/main/deployments.json"; then
    $strict && fail "could not fetch deployments.json"
    annotate warning "Could not fetch deployments.json, so $cluster wasn't compared."
    echo "warning: could not fetch deployments.json: $cluster wasn't compared" >&2
    exit 0
  fi
fi
jq -e --arg cluster "$cluster" '.clusters | has($cluster)' "$deployments" >/dev/null 2>&1 ||
  fail "deployments.json has no cluster '$cluster'"

# One field at a time: a build without a version has an empty one, which a
# tab-separated line would lose.
last() {
  jq -r --arg cluster "$cluster" --arg field "$1" \
    '.clusters[$cluster].deployments[-1][$field] // empty' "$deployments"
}
deployed_sha256=$(last idl_sha256)
deployed_version=$(last version)
deployed_slot=$(last slot)
if [[ -z $deployed_sha256 ]]; then
  echo "The program isn't deployed on $cluster yet."
  exit 0
fi
if [[ $deployed_sha256 == "$expected_sha256" ]]; then
  echo "$cluster has the interface of release $version, since slot $deployed_slot."
  exit 0
fi

if [[ -n $deployed_version ]]; then
  running="release $deployed_version"
else
  running="a build no release describes"
fi
message="$cluster runs $running since slot $deployed_slot, not release $version: its interface is another."
if $strict; then
  echo "error: $message" >&2
  annotate error "$message"
  exit 1
fi
echo "warning: $message" >&2
annotate warning "$message"

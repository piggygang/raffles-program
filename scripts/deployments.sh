#!/usr/bin/env bash
# deployments.json: which release of the program each cluster runs, and since
# which slot (docs/releases.md has the format).
#
#   scripts/deployments.sh check [--base <ref>]
#   scripts/deployments.sh plan --cluster <cluster> [--untagged] [--base <ref>]
#   scripts/deployments.sh record --cluster <cluster> --slot <slot> \
#                                 --executable-hash <hash> [--untagged] \
#                                 [--base <ref>]
#
#   check    The file is well-formed, devnet's program is `declare_id!`, and
#            each release it names is that tag's commit and IDL. Needs the
#            tags: `git fetch --tags`. With --base, the file also still has
#            every deployment of that commit's file, each in its place and
#            under the same program.
#   plan     Says what `record` would write for HEAD, or why it couldn't.
#            scripts/deploy-devnet.sh asks before it deploys, so that a
#            deployment it can't record never happens.
#   record   Appends the deployment of HEAD to a cluster's list.
#            scripts/deploy-devnet.sh calls it after a deploy.
#
# The file only grows. So `plan` and `record` start from a file that passes
# `check` and has every deployment main has recorded: a checkout of an older
# tag has the file as it was then, and appending to that would drop the
# deployments since. They fetch main from origin to compare.
#
# None of the three sends a transaction, and only `record` writes a file.
#
#   --untagged    HEAD isn't a release: record the deployment without a
#                 version, and without a commit if the tree is dirty. Consumers
#                 then know the cluster runs an interface no release describes.
#   --base <ref>  The commit whose deployments the file has to keep. `plan` and
#                 `record` take it in place of main as origin has it.
set -euo pipefail

cd "$(dirname "$0")/.."

file=deployments.json
idl=idls/raffles.json
# What a deployment is built from: a change to any of these is another build.
sources=(programs Cargo.toml Cargo.lock "$idl")

die() {
  echo "error: $*" >&2
  exit 1
}

usage() {
  awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
}

for tool in jq git sha256sum; do
  command -v "$tool" >/dev/null || die "$tool is not on PATH"
done

sha256() {
  sha256sum | cut -d' ' -f1
}

# The shape of the deployments file at `path`, as a list of what is wrong
# with it.
shape_errors() {
  jq -r '
    def hex($n): type == "string" and test("^[0-9a-f]{\($n)}$");
    def address: type == "string" and test("^[1-9A-HJ-NP-Za-km-z]{32,44}$");

    [
      (select(type != "object") | "the file is not an object"),
      (select(type == "object") |
        (select((keys - ["clusters", "schema"]) != [] or (["clusters", "schema"] - keys) != [])
          | "the keys are `schema` and `clusters`"),
        (select(.schema != 1) | "`schema` is 1"),
        (select((.clusters | type) != "object" or (.clusters | keys) != ["devnet", "mainnet-beta"])
          | "`clusters` has `devnet` and `mainnet-beta`"),
        (.clusters // {} | to_entries[] | .key as $cluster | .value |
          (select(type != "object" or keys != ["deployments", "program_id"])
            | "\($cluster): the keys are `program_id` and `deployments`"),
          (select(type == "object") |
            (select(.program_id != null and (.program_id | address | not))
              | "\($cluster): `program_id` is an address, or null before the program has one"),
            (select((.deployments | type) != "array") | "\($cluster): `deployments` is a list"),
            (select(.program_id == null and (.deployments | type) == "array" and (.deployments | length) > 0)
              | "\($cluster): deployments, but no `program_id`"),
            ((.deployments | arrays) as $list | $list | to_entries[] | .key as $index | .value |
              "\($cluster), deployment \($index + 1)" as $where |
              (select(type != "object" or keys != ["commit", "executable_hash", "idl_sha256", "slot", "version"])
                | "\($where): the keys are `version`, `commit`, `slot`, `executable_hash` and `idl_sha256`"),
              (select(type == "object") |
                (select(.version != null and ((.version | type) != "string" or (.version | test("^[0-9]+\\.[0-9]+\\.[0-9]+$") | not)))
                  | "\($where): `version` is a release such as 0.1.0, or null for an untagged build"),
                (select(.commit != null and (.commit | hex(40) | not))
                  | "\($where): `commit` is a full commit hash, or null for a dirty tree"),
                (select(.version != null and .commit == null)
                  | "\($where): a release has a commit"),
                (select((.slot | type) != "number" or .slot < 1 or .slot != (.slot | floor))
                  | "\($where): `slot` is the slot the deployment landed in"),
                (select(.executable_hash | hex(64) | not)
                  | "\($where): `executable_hash` is 64 hex digits"),
                (select(.idl_sha256 | hex(64) | not)
                  | "\($where): `idl_sha256` is 64 hex digits"),
                (select($index > 0 and (.slot | type) == "number" and (($list[$index - 1].slot // 0) >= .slot))
                  | "\($where): its slot is not after the deployment before it")
              )
            )
          )
        )
      )
    ] | .[]
  ' "$1"
}

# Stops unless the deployments file at `path` is one `check` accepts: the
# file itself, or what `record` is about to write over it.
validate() {
  local path=$1
  [[ -f $path ]] || die "no $file"
  jq empty "$path" 2>/dev/null || die "$file is not JSON"

  local errors
  errors=$(shape_errors "$path")
  if [[ -n $errors ]]; then
    echo "$file:" >&2
    sed 's/^/  /' <<<"$errors" >&2
    exit 1
  fi

  local program_id devnet_id
  program_id=$(sed -n 's/^declare_id!("\(.*\)");$/\1/p' programs/raffles/src/lib.rs)
  devnet_id=$(jq -r '.clusters.devnet.program_id' "$path")
  [[ $devnet_id == "$program_id" ]] ||
    die "$file: devnet's program_id is $devnet_id, but declare_id! is $program_id"

  # A release in the file is the tag: the same commit, and the same IDL.
  local cluster version commit idl_sha256 tag
  while IFS=$'\t' read -r cluster version commit idl_sha256; do
    tag="v$version"
    git rev-parse -q --verify "refs/tags/$tag^{commit}" >/dev/null ||
      die "$file: $cluster names release $version, but this clone has no tag $tag (git fetch --tags)"
    [[ $(git rev-parse "refs/tags/$tag^{commit}") == "$commit" ]] ||
      die "$file: $cluster's $version is commit $commit, but $tag is $(git rev-parse "refs/tags/$tag^{commit}")"
    [[ $(git show "refs/tags/$tag:$idl" | sha256) == "$idl_sha256" ]] ||
      die "$file: $cluster's $version has IDL hash $idl_sha256, which isn't $idl at $tag"
  done < <(jq -r '
    .clusters | to_entries[] | .key as $cluster | .value.deployments[]
    | select(.version != null) | [$cluster, .version, .commit, .idl_sha256] | @tsv
  ' "$path")
}

# What the file at `path` has lost of the deployments `commit` recorded, a
# line each: a cluster's list no longer starts with every one of them, or its
# program is another, which would make them that program's deployments. A
# cluster without a deployment can still be given its program.
history_errors() {
  local commit=$1 path=$2
  # A commit from before the file existed recorded nothing.
  git cat-file -e "$commit:$file" 2>/dev/null || return 0
  jq -rn --slurpfile here "$path" --slurpfile there <(git show "$commit:$file") '
    ($there[0].clusters // {}) | to_entries[] | .key as $cluster
    | (.value.deployments // []) as $recorded
    | .value.program_id as $program
    | ($here[0].clusters[$cluster] // {}) as $mine
    | ($mine.deployments // []) as $kept
    | select(($recorded | length) > 0)
    | (
        (select($mine.program_id != $program)
          | "\($cluster): its program_id is no longer \($program), the program of its recorded deployments"),
        ([range($recorded | length) | select($kept[.] != $recorded[.])][0]
          | select(. != null)
          | "\($cluster): deployment \(. + 1) of \($recorded | length) is missing or changed")
      )
  ' || die "$file at $commit is not JSON"
}

# Stops unless the file at `path` still has what `commit` recorded. `advice`
# is what to do about it, if there is something.
keeps_history() {
  local commit=$1 path=$2 advice=$3 errors
  errors=$(history_errors "$commit" "$path")
  [[ -n $errors ]] || return 0
  {
    echo "error: $file doesn't keep what ${commit:0:7} recorded:"
    sed 's/^/  /' <<<"$errors"
    [[ -z $advice ]] || echo "$advice"
  } >&2
  exit 1
}

# The file as `plan` and `record` need it before a deployment is added: one
# `check` accepts, with every deployment `base` recorded. `base` is main as
# origin has it now, unless one was given.
ready() {
  local base=$1 commit
  validate "$file"

  if [[ -z $base ]]; then
    git fetch --quiet origin main ||
      die "could not fetch main from origin, to see which deployments it has recorded. Pass --base <ref> to compare with a commit of this clone"
    base=FETCH_HEAD
  fi
  commit=$(git rev-parse -q --verify "$base^{commit}") || die "no commit '$base' to compare $file with"
  keeps_history "$commit" "$file" "A checkout of an older tag has the file as it was then. Take the newer one first:
  git checkout $commit -- $file
Then record again any deployment this file had and that one doesn't."
}

check() {
  local base="" commit
  while (($# > 0)); do
    case "$1" in
      --base) base=${2:-}; shift 2 || die "--base needs a ref" ;;
      *) die "unknown argument: $1 (see --help)" ;;
    esac
  done

  validate "$file"
  if [[ -n $base ]]; then
    commit=$(git rev-parse -q --verify "$base^{commit}") || die "no commit '$base' to compare $file with"
    keeps_history "$commit" "$file" "The file only grows: a recorded deployment keeps its place and its program. Add below."
  fi
  echo "$file: ok"
}

# What a deployment of HEAD to `cluster` is, into `version`, `commit` and
# `idl_sha256`: a release, or with `untagged` a build without a version.
describe_head() {
  local cluster=$1 untagged=$2 dirty=false

  jq -e --arg cluster "$cluster" '.clusters[$cluster].program_id != null' "$file" >/dev/null 2>&1 ||
    die "$file has no program_id for '$cluster': set it first"

  version=$(sed -n 's/^version = "\(.*\)"$/\1/p' programs/raffles/Cargo.toml | head -n 1)
  [[ -n $version ]] || die "no version in programs/raffles/Cargo.toml"
  commit=$(git rev-parse HEAD)
  [[ -z $(git status --porcelain -- "${sources[@]}") ]] || dirty=true

  if $untagged; then
    version=
    if $dirty; then
      commit=
      idl_sha256=$(sha256 <"$idl")
    else
      idl_sha256=$(git show "HEAD:$idl" | sha256)
    fi
    return
  fi

  $dirty && die "uncommitted changes under ${sources[*]}: a release is a commit. Pass --untagged to deploy this build without a version"
  git tag --points-at HEAD | grep -qxF "v$version" ||
    die "HEAD isn't tagged v$version: release it first (docs/releases.md), or pass --untagged to deploy a build no release describes"
  idl_sha256=$(git show "HEAD:$idl" | sha256)
}

plan() {
  local cluster="" untagged=false base="" version commit idl_sha256
  while (($# > 0)); do
    case "$1" in
      --cluster) cluster=${2:-}; shift 2 || die "--cluster needs a cluster" ;;
      --untagged) untagged=true; shift ;;
      --base) base=${2:-}; shift 2 || die "--base needs a ref" ;;
      *) die "unknown argument: $1 (see --help)" ;;
    esac
  done
  [[ -n $cluster ]] || die "plan needs --cluster (see --help)"

  # Before anything is deployed: what would stop `record` afterwards.
  ready "$base"
  describe_head "$cluster" "$untagged"
  if [[ -n $version ]]; then
    echo "release $version"
  elif [[ -n $commit ]]; then
    echo "an untagged build of ${commit:0:7}"
  else
    echo "an untagged build of a dirty tree"
  fi
}

record() {
  local cluster="" slot="" executable_hash="" untagged=false base="" version commit idl_sha256
  while (($# > 0)); do
    case "$1" in
      --cluster) cluster=${2:-}; shift 2 || die "--cluster needs a cluster" ;;
      --slot) slot=${2:-}; shift 2 || die "--slot needs a slot" ;;
      --executable-hash) executable_hash=${2:-}; shift 2 || die "--executable-hash needs a hash" ;;
      --untagged) untagged=true; shift ;;
      --base) base=${2:-}; shift 2 || die "--base needs a ref" ;;
      *) die "unknown argument: $1 (see --help)" ;;
    esac
  done
  [[ -n $cluster && -n $slot && -n $executable_hash ]] ||
    die "record needs --cluster, --slot and --executable-hash (see --help)"
  [[ $slot =~ ^[1-9][0-9]*$ ]] || die "--slot is a slot number, not '$slot'"
  [[ $executable_hash =~ ^[0-9a-f]{64}$ ]] || die "--executable-hash is 64 hex digits"

  ready "$base"
  describe_head "$cluster" "$untagged"

  local last
  last=$(jq -r --arg cluster "$cluster" '.clusters[$cluster].deployments[-1].slot // 0' "$file")
  ((slot > last)) || die "slot $slot is not after the last recorded deployment's, $last"

  # Not `local`: the trap runs when the script exits, after this function.
  updated=$(mktemp)
  trap 'rm -f "$updated"' EXIT
  jq --arg cluster "$cluster" --arg version "$version" --arg commit "$commit" \
    --argjson slot "$slot" --arg executable_hash "$executable_hash" --arg idl_sha256 "$idl_sha256" '
      .clusters[$cluster].deployments += [{
        version: (if $version == "" then null else $version end),
        commit: (if $commit == "" then null else $commit end),
        slot: $slot,
        executable_hash: $executable_hash,
        idl_sha256: $idl_sha256
      }]
    ' "$file" >"$updated"
  # The file is replaced only by one that passes.
  validate "$updated"
  cat "$updated" >"$file"

  echo "$file: recorded ${version:+release }${version:-an untagged build} on $cluster at slot $slot"
}

case "${1:-}" in
  check) shift; check "$@" ;;
  plan) shift; plan "$@" ;;
  record) shift; record "$@" ;;
  -h | --help) usage ;;
  *) usage >&2; exit 2 ;;
esac

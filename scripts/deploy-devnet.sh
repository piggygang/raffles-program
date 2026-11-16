#!/usr/bin/env bash
# Deploys or upgrades the raffles program on devnet from the verifiable Docker
# build, so every team member ships the same bytes for the same commit. Then
# records the deployment in deployments.json, which is what tells the indexer,
# the services and the app which interface devnet speaks (docs/releases.md).
#
#   scripts/deploy-devnet.sh [--dry-run] [--untagged] [--allow-dirty]
#
#   --dry-run      build and run every check, deploy nothing
#   --untagged     deploy a commit that isn't a release. Without it, HEAD has
#                  to be the release's tag, v<version of programs/raffles>.
#                  deployments.json then records a build without a version.
#   --allow-dirty  deploy with uncommitted changes under programs/ or Cargo.*.
#                  Such a build is never a release: it needs --untagged too.
#
# Keys stay outside the repo (README, "Keys"). Override the defaults with:
#   RAFFLES_DEVNET_AUTHORITY  upgrade authority and fee payer
#                             (~/.config/solana/piggygang/raffles-devnet-authority.json)
#   RAFFLES_PROGRAM_KEYPAIR   program keypair, read only by the first deploy
#                             (~/.config/solana/piggygang/raffles-program-devnet.json)
#   RAFFLES_DEVNET_RPC_URL    devnet RPC (https://api.devnet.solana.com, which
#                             rate-limits deploys; prefer a Helius devnet URL)
set -euo pipefail

cd "$(dirname "$0")/.."

dry_run=false
allow_dirty=false
untagged=false
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=true ;;
    --allow-dirty) allow_dirty=true ;;
    --untagged) untagged=true ;;
    -h | --help)
      awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $arg (see --help)" >&2
      exit 2
      ;;
  esac
done

keys="$HOME/.config/solana/piggygang"
authority=${RAFFLES_DEVNET_AUTHORITY:-$keys/raffles-devnet-authority.json}
program_keypair=${RAFFLES_PROGRAM_KEYPAIR:-$keys/raffles-program-devnet.json}
rpc_url=${RAFFLES_DEVNET_RPC_URL:-https://api.devnet.solana.com}
binary=target/deploy/raffles.so
devnet_genesis_hash=EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG

die() {
  echo "error: $*" >&2
  exit 1
}

# For tool output headed to the terminal: drops control characters and every
# URL, since an RPC URL can carry an API key.
redact() {
  tr -d '\000-\010\013-\037\177' | sed -E 's#[[:alpha:]]+://[^[:space:])"]+#<url>#g'
}

for tool in solana solana-keygen solana-verify docker git jq; do
  command -v "$tool" >/dev/null || die "$tool is not on PATH (README, \"Toolchain\")"
done
docker info >/dev/null 2>&1 || die "Docker is not running"

pinned_solana=$(sed -n 's/^solana_version = "\(.*\)"$/\1/p' Anchor.toml)
installed_solana=$(solana --version | awk '{print $2}')
[[ $installed_solana == "$pinned_solana" ]] ||
  die "solana $installed_solana is on PATH; Anchor.toml pins $pinned_solana"

program_id=$(sed -n 's/^declare_id!("\(.*\)");$/\1/p' programs/raffles/src/lib.rs)
[[ -n $program_id ]] || die "no declare_id! in programs/raffles/src/lib.rs"
sed -n '/^\[programs\.devnet\]$/,/^\[/p' Anchor.toml | grep -qxF "raffles = \"$program_id\"" ||
  die "[programs.devnet] in Anchor.toml does not match declare_id! ($program_id)"

[[ $(solana genesis-hash --url "$rpc_url") == "$devnet_genesis_hash" ]] ||
  die "$rpc_url is not a devnet RPC"

[[ -f $authority ]] || die "no upgrade authority keypair at $authority (README, \"Keys\")"
authority_pubkey=$(solana-keygen pubkey "$authority")

commit=$(git rev-parse --short HEAD 2>/dev/null) || die "no commit to deploy yet"
# The build and the deploy take minutes, and the record describes HEAD: it has
# to be this commit still, both when the build is deployed and when it is
# recorded.
head=$(git rev-parse HEAD)
if [[ -n $(git status --porcelain -- programs Cargo.toml Cargo.lock) ]]; then
  $allow_dirty || die "uncommitted changes under programs/ or Cargo.*; commit first or pass --allow-dirty"
  commit="$commit+dirty"
fi

# What deployments.json will say devnet runs. Asked before anything is built:
# a deployment that can't be recorded must not happen. So this also stops when
# the file doesn't pass its check, or lacks a deployment main has recorded.
record=(--cluster devnet)
if $untagged; then
  record+=(--untagged)
fi
release=$(scripts/deployments.sh plan "${record[@]}")

# `solana program` wants a keypair even to read, and the default one may not
# exist on this machine.
if show=$(solana program show "$program_id" --url "$rpc_url" --keypair "$authority" 2>&1); then
  current_authority=$(awk '/^Authority:/ { print $2 }' <<<"$show")
  [[ $current_authority == "$authority_pubkey" ]] ||
    die "the devnet upgrade authority is $current_authority, not $authority_pubkey"
  mode=upgrade
  program_arg=$program_id
elif [[ $show == *"Unable to find the account"* ]]; then
  [[ -f $program_keypair ]] ||
    die "the first deploy needs the program keypair at $program_keypair (README, \"Keys\")"
  [[ $(solana-keygen pubkey "$program_keypair") == "$program_id" ]] ||
    die "$program_keypair is not the keypair for $program_id"
  mode="first deploy"
  program_arg=$program_keypair
else
  die "could not read $program_id from $rpc_url: $show"
fi

echo "Building $commit in Docker with solana-verify $(solana-verify --version | awk '{print $2}')..."
solana-verify build --library-name raffles --arch v3
hash=$(solana-verify get-executable-hash "$binary")
[[ $(git rev-parse HEAD) == "$head" ]] ||
  die "HEAD moved during the build, so the build may not be of ${head:0:7}: nothing was deployed"

cat <<EOF

  program    $program_id ($mode)
  commit     $commit
  records    $release
  hash       $hash
  size       $(wc -c <"$binary" | tr -d ' ') bytes
  authority  $authority_pubkey, $(solana balance "$authority_pubkey" --url "$rpc_url")
  rpc        $rpc_url

EOF

if $dry_run; then
  echo "Dry run: nothing deployed."
  exit 0
fi

solana program deploy "$binary" \
  --program-id "$program_arg" \
  --upgrade-authority "$authority" \
  --keypair "$authority" \
  --url "$rpc_url" \
  --use-rpc \
  --max-sign-attempts 10

# solana-verify reads at finalized commitment, a few seconds behind the deploy:
# until then a first deploy isn't found and an upgrade still has the old hash.
lookup_errors=$(mktemp)
trap 'rm -f "$lookup_errors"' EXIT
for _ in $(seq 30); do
  if output=$(solana-verify --url "$rpc_url" get-program-hash "$program_id" 2>"$lookup_errors"); then
    # solana-verify prints its RPC retry notices to stdout too; the hash is the
    # last line.
    deployed_hash=${output##*$'\n'}
    [[ $deployed_hash == "$hash" ]] && break
  else
    deployed_hash=
  fi
  sleep 2
done
[[ -n $deployed_hash ]] ||
  die "could not read the program hash from devnet: $(redact <"$lookup_errors")"
[[ $deployed_hash == "$hash" ]] ||
  die "devnet runs $deployed_hash, but the build hashed to $hash"
echo "Deployed $commit to devnet: $program_id ($hash)"

# The slot this deployment landed in: the first one's is where the program's
# history starts, and each later one's is where its interface changed.
slot=$(solana program show "$program_id" --url "$rpc_url" --keypair "$authority" --output json |
  jq -r '.lastDeploySlot') || slot=
[[ $slot =~ ^[1-9][0-9]*$ ]] ||
  die "deployed, but could not read the deploy slot. Record it by hand: scripts/deployments.sh record ${record[*]} --slot <slot> --executable-hash $hash"
by_hand="scripts/deployments.sh record ${record[*]} --slot $slot --executable-hash $hash"
# `record` describes HEAD, so it has to be the commit that was built.
[[ $(git rev-parse HEAD) == "$head" ]] ||
  die "deployed, but HEAD moved meanwhile, and the record describes HEAD. Check out $head, then record it by hand: $by_hand"
scripts/deployments.sh record "${record[@]}" --slot "$slot" --executable-hash "$hash" ||
  die "deployed, but not recorded. Once what it reports is put right, record it by hand: $by_hand"
echo "Commit deployments.json: until it is on main, nothing tells the consumers what devnet runs."

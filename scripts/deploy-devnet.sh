#!/usr/bin/env bash
# Deploys or upgrades the raffles program on devnet from the verifiable Docker
# build, so every team member ships the same bytes for the same commit.
#
#   scripts/deploy-devnet.sh [--dry-run] [--allow-dirty]
#
#   --dry-run      build and run every check, deploy nothing
#   --allow-dirty  deploy with uncommitted changes under programs/ or Cargo.*
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
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=true ;;
    --allow-dirty) allow_dirty=true ;;
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

for tool in solana solana-keygen solana-verify docker git; do
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
if [[ -n $(git status --porcelain -- programs Cargo.toml Cargo.lock) ]]; then
  $allow_dirty || die "uncommitted changes under programs/ or Cargo.*; commit first or pass --allow-dirty"
  commit="$commit+dirty"
fi

if show=$(solana program show "$program_id" --url "$rpc_url" 2>&1); then
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

cat <<EOF

  program    $program_id ($mode)
  commit     $commit
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
  if deployed_hash=$(solana-verify --url "$rpc_url" get-program-hash "$program_id" 2>"$lookup_errors"); then
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

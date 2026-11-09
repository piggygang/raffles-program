#!/usr/bin/env bash
# Dumps one live pig of each membership rule from mainnet into
# programs/claim-spike/tests/fixtures: the bytes claim_entries decodes. Goes
# away with the spike (PIG-133).
#
#   scripts/dump-claim-spike-fixtures.sh
#
#   MAINNET_RPC_URL  mainnet RPC (https://api.mainnet-beta.solana.com)
set -euo pipefail

cd "$(dirname "$0")/.."

rpc_url=${MAINNET_RPC_URL:-https://api.mainnet-beta.solana.com}
dir=programs/claim-spike/tests/fixtures
token_metadata=metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s
# From the indexer's closed mint lists. SOL Gang #6545 has `token_standard`
# set; Girl Gang #1310, like every Girl Gang pig, doesn't.
sol_gang_mint=CVqmUYDPX4FgVjoTCLASdt4Nz3TUgKEV8KCYoxrwcUuv
girl_gang_mint=Dk1cJAAYPMa6Y39Qs31Jo8EofTJqJFFE62FXDdtyRQmN
# Piggy Gang #2369, a Core asset in Collection(J3nH…), 166 bytes: the largest
# of the 745.
piggy_gang_asset=12ruPvhouMCmAQRTMXhxmeu4NsTd3BXv6ZjCqgXvXvvA

metadata_of() {
  local address
  address=$(solana find-program-derived-address "$token_metadata" string:metadata \
    "pubkey:$token_metadata" "pubkey:$1" | awk '{ print $1 }')
  [[ -n $address ]] || { echo "could not derive the metadata address of $1" >&2; exit 1; }
  echo "$address"
}

mkdir -p "$dir"
slot=$(solana slot --url "$rpc_url")
sol_gang_metadata=$(metadata_of "$sol_gang_mint")
girl_gang_metadata=$(metadata_of "$girl_gang_mint")
for pair in "sol_gang_metadata:$sol_gang_metadata" "girl_gang_metadata:$girl_gang_metadata" \
  "piggy_gang_asset:$piggy_gang_asset"; do
  solana account --url "$rpc_url" "${pair#*:}" --output json \
    --output-file "$dir/${pair%%:*}.json" >/dev/null
done

cat >"$dir/README.md" <<EOF
# Pig fixtures

Mainnet accounts the claim-spike LiteSVM tests decode, written by \`scripts/dump-claim-spike-fixtures.sh\` at slot $slot, in the \`solana account --output json\` format. Don't edit by hand.

- \`sol_gang_metadata.json\`: the Token Metadata account \`$sol_gang_metadata\` of SOL Gang mint \`$sol_gang_mint\`.
- \`girl_gang_metadata.json\`: the Token Metadata account \`$girl_gang_metadata\` of Girl Gang mint \`$girl_gang_mint\`.
- \`piggy_gang_asset.json\`: the Piggy Gang Core asset \`$piggy_gang_asset\`.

The tests clone them: each metadata copy gets its own mint, and each asset copy its own address and owner.
EOF
echo "Wrote $dir:"
ls -l "$dir"

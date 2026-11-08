#!/usr/bin/env bash
# Dumps the devnet ORAO VRF program, and the accounts the vrf-spike LiteSVM
# tests run it against, into programs/vrf-spike/tests/fixtures. Goes away with
# the spike (PIG-114).
#
#   scripts/dump-vrf-spike-fixtures.sh
#
#   DEVNET_RPC_URL  devnet RPC (https://api.devnet.solana.com)
set -euo pipefail

cd "$(dirname "$0")/.."

rpc_url=${DEVNET_RPC_URL:-https://api.devnet.solana.com}
dir=programs/vrf-spike/tests/fixtures
vrf=VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y
network_state=$(solana find-program-derived-address "$vrf" string:orao-vrf-network-configuration |
  awk '{ print $1 }')
[[ -n $network_state ]] || { echo "could not derive the network state address for $vrf" >&2; exit 1; }

mkdir -p "$dir"
slot=$(solana slot --url "$rpc_url")
solana program dump --url "$rpc_url" "$vrf" "$dir/orao_vrf.so" >/dev/null
deployed=$(solana program show --url "$rpc_url" "$vrf" | awk '/^Last Deployed In Slot:/ { print $5 }')
[[ -n $deployed ]] || { echo "could not parse program show output for $vrf" >&2; exit 1; }
solana account --url "$rpc_url" "$network_state" --output json \
  --output-file "$dir/network_state.json" >/dev/null
# The fee goes to the treasury, which the network state names; the tests check
# that it still does. A transfer smaller than the rent-exempt minimum fails on
# an account that doesn't exist, so the tests need the account.
treasury=9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR
solana account --url "$rpc_url" "$treasury" --output json --output-file "$dir/treasury.json" >/dev/null

cat >"$dir/README.md" <<EOF
# ORAO fixtures

Devnet state the vrf-spike LiteSVM tests load, written by \`scripts/dump-vrf-spike-fixtures.sh\` at slot $slot. Don't edit by hand.

- \`orao_vrf.so\`: the ORAO VRF program \`$vrf\`, last deployed in slot $deployed.
- \`network_state.json\`: its configuration PDA \`$network_state\`, in the \`solana account --output json\` format. The tests replace its fulfillment authorities with their own keys.
- \`treasury.json\`: the treasury \`$treasury\` the request fee goes to.
EOF
echo "Wrote $dir:"
ls -l "$dir"

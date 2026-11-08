#!/usr/bin/env bash
# Dumps the mainnet programs the LiteSVM tests load into
# programs/raffles/tests/fixtures and records which deploy each came from.
# Token Metadata is immutable; rerun this when mpl-core ships an upgrade.
#
#   scripts/dump-fixtures.sh
#
#   MAINNET_RPC_URL  mainnet RPC (https://api.mainnet-beta.solana.com)
set -euo pipefail

cd "$(dirname "$0")/.."

rpc_url=${MAINNET_RPC_URL:-https://api.mainnet-beta.solana.com}
dir=programs/raffles/tests/fixtures
programs=(
  "mpl_token_metadata metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
  "mpl_core CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d"
)

mkdir -p "$dir"
{
  echo "# Program fixtures"
  echo
  echo "Mainnet programs the LiteSVM tests load, written by \`scripts/dump-fixtures.sh\`. Don't edit by hand."
  echo
  echo "| File | Program | Last deployed slot | Upgradeable |"
  echo "| --- | --- | --- | --- |"
  for entry in "${programs[@]}"; do
    read -r name id <<<"$entry"
    solana program dump --url "$rpc_url" "$id" "$dir/$name.so" >/dev/null
    show=$(solana program show --url "$rpc_url" "$id")
    slot=$(awk '/^Last Deployed In Slot:/ { print $5 }' <<<"$show")
    authority=$(awk '/^Authority:/ { print $2 }' <<<"$show")
    upgradeable=yes
    [[ $authority == none ]] && upgradeable=no
    echo "| \`$name.so\` | \`$id\` | $slot | $upgradeable |"
  done
} >"$dir/README.md.tmp"
mv "$dir/README.md.tmp" "$dir/README.md"
echo "Wrote $dir:"
ls -l "$dir"

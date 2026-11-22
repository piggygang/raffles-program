#!/usr/bin/env bash
# Dumps the mainnet programs the LiteSVM tests load, and the accounts of
# theirs that the tests need, into programs/raffles/tests/fixtures, and
# records which deploy each program came from. Token Metadata and Squads are
# immutable; rerun this when mpl-core ships an upgrade.
#
#   scripts/dump-fixtures.sh
#
#   MAINNET_RPC_URL  mainnet RPC (https://api.mainnet-beta.solana.com)
set -euo pipefail

cd "$(dirname "$0")/.."

rpc_url=${MAINNET_RPC_URL:-https://api.mainnet-beta.solana.com}
dir=programs/raffles/tests/fixtures
squads=SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf
programs=(
  "mpl_token_metadata metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
  "mpl_core CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d"
  "squads_multisig $squads"
)
# Squads' one global account. `multisig_create_v2` reads the creation fee and
# the treasury it goes to from it, so a test can't create a multisig without.
squads_program_config=$(solana find-program-derived-address "$squads" string:multisig string:program_config |
  awk '{ print $1 }')
[[ -n $squads_program_config ]] || { echo "could not derive the program config address of $squads" >&2; exit 1; }

mkdir -p "$dir"
# A failed dump or parse exits mid-write; don't leave the partial file behind.
trap 'rm -f "$dir/README.md.tmp"' EXIT
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
    [[ -n $slot && -n $authority ]] || { echo "could not parse program show output for $id" >&2; exit 1; }
    upgradeable=yes
    [[ $authority == none ]] && upgradeable=no
    echo "| \`$name.so\` | \`$id\` | $slot | $upgradeable |"
  done

  slot=$(solana slot --url "$rpc_url")
  solana account --url "$rpc_url" "$squads_program_config" --output json \
    --output-file "$dir/squads_program_config.json" >/dev/null
  echo
  echo "Accounts of those programs, in the \`solana account --output json\` format, read at slot $slot:"
  echo
  echo "| File | Account | Is |"
  echo "| --- | --- | --- |"
  echo "| \`squads_program_config.json\` | \`$squads_program_config\` | Squads' program config: the multisig creation fee, and the treasury it goes to |"
} >"$dir/README.md.tmp"
mv "$dir/README.md.tmp" "$dir/README.md"
echo "Wrote $dir:"
ls -l "$dir"

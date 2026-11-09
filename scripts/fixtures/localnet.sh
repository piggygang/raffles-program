#!/usr/bin/env bash
# The fixture set on a fresh local validator. Token Metadata and mpl-core load
# from the committed program fixtures, the bytes the LiteSVM tests run, so this
# needs no network. Writes scripts/fixtures/localnet/ and the claim spike's
# stand-in dumps, then stops the validator.
#
#   scripts/fixtures/localnet.sh          # build, dump, stop
#   scripts/fixtures/localnet.sh --keep   # leave the validator on :8899
set -euo pipefail

cd "$(dirname "$0")/../.."

keep=false
[[ ${1:-} == --keep ]] && keep=true
programs=programs/raffles/tests/fixtures
work=$(mktemp -d)
solana-test-validator --reset --quiet --ledger "$work/ledger" \
  --bpf-program metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s "$programs/mpl_token_metadata.so" \
  --bpf-program CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d "$programs/mpl_core.so" \
  >"$work/validator.log" 2>&1 &
validator=$!
cleanup() {
  if ! $keep; then
    kill "$validator" 2>/dev/null || true
    wait "$validator" 2>/dev/null || true
    rm -rf "$work"
  fi
}
trap cleanup EXIT

url=http://127.0.0.1:8899
for _ in $(seq 60); do
  solana cluster-version --url "$url" >/dev/null 2>&1 && break
  sleep 1
done
solana cluster-version --url "$url" >/dev/null || { cat "$work/validator.log" >&2; exit 1; }

# A throwaway payer: localnet SOL is free.
solana-keygen new --no-bip39-passphrase --silent -o "$work/payer.json" >/dev/null
solana airdrop 50 "$(solana-keygen pubkey "$work/payer.json")" --url "$url" >/dev/null

fixtures=(cargo run --quiet --manifest-path scripts/fixtures/Cargo.toml --
  --cluster localnet --url "$url" --keypair "$work/payer.json")
"${fixtures[@]}" apply
"${fixtures[@]}" dump --out programs/claim-spike/tests/fixtures/stand-ins

if $keep; then
  echo "The validator (pid $validator) keeps running on $url; its ledger is $work/ledger."
fi

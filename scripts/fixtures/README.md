# Fixtures

Devnet stand-ins for the three Piggy collections, plus prize assets and test wallets (PIG-115). The collections exist only on mainnet. The program tests, the devnet indexer, the crank and the E2E run need copies with the same on-chain structure ([holder-proof design](https://github.com/piggygang/raffles/blob/main/docs/piggygang-raffles-holder-proof.md) §3 and §7).

`fixtures` is a Rust CLI with its own workspace, like `spikes/vrf-driver`: `cargo build-sbf` builds every member of the program workspace for SBF, and an RPC client doesn't compile there.

## The fixture set

Every key derives from one seed. A rerun therefore finds the accounts it made before, and `plan` can write the manifest without a network.

### Collections

The pigs are made the way mainnet's were:

- **Token Metadata pigs:** `CreateV1` and `MintV1`, then `Verify` signed by the verified creator.
- **Core pigs:** `CreateCollectionV2` with Piggy Gang's one plugin (Royalties without a rule set), then `CreateV2`.

A Token Metadata stand-in comes out byte-compatible with mainnet's 607-byte pigs: padded strings, and `creators[0]` at offset 326.

| Collection | Standard | Count | Structure |
| --- | --- | ---: | --- |
| Test SOL Gang | Token Metadata | 60 | `collection: None`; symbol `PSG`; creators `[creator verified 0, update authority 100]`; update authority = the update authority |
| Test Girl Gang | Token Metadata | 40 | The same, symbol `PGG` |
| Test Piggy Gang | Core | 60 | `update_authority = Collection(…)`, under the update authority |
| Test Pig Mud | Token Metadata | 10 | Not eligible, as on mainnet: the same update authority with its own verified creator, symbol `PMUD` |

### Authorities

These keys sign but hold nothing. `apply`'s payer pays every rent, so they need no SOL.

| Key | Stands in for |
| --- | --- |
| `creator` | `Pigv3g…`, the verified creator of both legacy collections |
| `update-authority` | `7Nsng…`, the update authority of the legacy pigs, of Pig Mud and of the Core collection |
| `pig-mud-creator` | `5uJo…`, Pig Mud's verified creator |
| `stranger` | Nobody the rules name: the wrong update authority, and another Core collection's authority |
| `prize-authority` | The prize collections' update authority and the prize token's mint authority |

### Wallets

| Wallet | SOL Gang · Girl Gang · Piggy Gang · Pig Mud | SOL | For |
| --- | --- | ---: | --- |
| `single-holder` | 1 · 0 · 0 · 0 | 0.1 | The plainest claim |
| `full-gang-holder` | 2 · 1 · 1 · 0 | 0.1 | The Full Gang bonus (E-E7) |
| `whale` | 12 · 8 · 10 · 2 | 0.25 | 30 eligible pigs, three times the cap (E-E1, E-E14, E-E16) |
| `seller` | 2 · 2 · 2 · 0 | 0.1 | Transfer-and-reclaim with the buyer (E-E9) |
| `buyer` | 1 · 0 · 0 · 0 | 0.1 | Buys an entered pig from the seller |
| `creator` | 1 · 0 · 0 · 0 | 1 | An approved partner with prizes, and a pig it can't enter in its own raffle (E-E12) |
| `gm-bot` | — | 1 | GM prizes and prize tokens |
| `crank` | — | 0.5 | The draw and the closes |
| `admin` | — | 1 | The devnet config authority and DAO admin: Core prizes, DressMe vouchers, prize tokens |
| `impostor` | 1 · 0 · 0 · 1 | 0.1 | Every look-alike, next to one real pig |
| `reserve` | the rest | 0.05 | Ad-hoc tests |

`apply` tops a wallet up to its amount once it falls below half of it.

### Look-alikes

They are held by `impostor`, and the rule must reject each one. Each is claimed under the rule of the collection it imitates (SOL Gang, or Piggy Gang for the Core ones).

| Look-alike | Why the rule rejects it |
| --- | --- |
| `unverified-creator` | The creator is present but unverified, like mainnet's #293 |
| `wrong-symbol` | The right creator and update authority, but symbol `PSGX` |
| `wrong-update-authority` | The creator verified and symbol `PSG`, but another update authority |
| `honorary` | The creator verified, but as its own update authority and with symbol `HPSG`, like Honorary Piggy 1 |
| `pig-mud` | The impostor's Test Pig Mud pig: the same update authority, another verified creator |
| `other-collection` | A Core asset in another collection |
| `address-update-authority` | A Core asset whose update authority is an address |

Holder-proof §7 lists two more rejections: metadata for one mint presented with another mint's token account, and a copy of a metadata account that Token Metadata doesn't own. Neither can be minted. The claim spike builds both in LiteSVM (`the_rules_reject_lookalikes`).

### Prizes

| Prize | Allowlist kind | Holders |
| --- | --- | --- |
| Test GM Prizes: a certified collection of 10 NFTs | `Collection` (0); `Mint` (1) for one of them | `gm-bot` 6, `creator` 4 |
| Test DressMe Vouchers: a certified collection of 3 voucher NFTs (spec D8) | `Collection` or `Mint` | `admin` 3 |
| Test Core Prizes: a Core collection of 4 assets (spec D22) | `CoreCollection` (2) | `admin` 2, `creator` 2 |
| Test Prize Token `TPRZ`: 6 decimals, with Token Metadata (spec D11) | `Mint` (1) | 1,000 each to `creator`, `gm-bot`, `admin` |

SOL prizes are wSOL, whose mint exists on every cluster, so there is nothing to create.

## Commands

```sh
cd scripts/fixtures
cargo run -- --cluster <devnet|localnet> <command>
```

| Command | Does | Network |
| --- | --- | --- |
| `plan` | Writes the outputs below | none |
| `apply` | Creates what's missing, verifies what's unverified, and returns items that moved to another fixture key. Then checks the result and writes the outputs. | **sends transactions** |
| `check` | Compares the cluster with the spec | reads |
| `dump --out <dir>` | Copies the first pig of each eligible collection and every look-alike, with `cases.json` | reads |
| `keys --out <dir>` | Writes the wallets' and authorities' keypair files for the Solana CLI and the E2E run. Refuses a path inside the repo. | none |

- **`--cluster`:** defaults to `localnet`.
- **`--url`:** defaults to `$RAFFLES_DEVNET_RPC_URL` or the public endpoint on devnet, and to `http://127.0.0.1:8899` on localnet.
- **`--keypair`:** the payer, `~/.config/solana/id.json` by default.
- **`--seed-file`:** the devnet seed, `~/.config/solana/piggygang/devnet-fixtures-seed.json` by default.
- **The genesis check:** every command that talks to a cluster first checks its genesis hash. `--cluster localnet` therefore refuses devnet, testnet and mainnet.

## Devnet

1. **The seed.** Create it once, and keep a copy in the team password manager. Everyone who runs the fixtures uses the same file.

   ```sh
   solana-keygen new --no-bip39-passphrase -o ~/.config/solana/piggygang/devnet-fixtures-seed.json
   ```

   The manifest records the seed's root address. `plan` and `apply` refuse to overwrite a manifest that another seed wrote, unless you pass `--new-seed`.

2. **The payer.** It needs about 6.7 SOL for a fresh run, measured on a local validator at devnet's rent:
   - 4.3 SOL for the wallets
   - 2.2 SOL for 130 Token Metadata creates, the 129 NFTs and the prize token: Token Metadata's protocol fee of 0.01 SOL each, plus rent
   - 0.2 SOL for the 66 Core assets and their 3 collections: mpl-core's fee of 0.0015 SOL per asset, plus rent

   A rerun costs only what it fixes. Set `RAFFLES_DEVNET_RPC_URL` to a Helius devnet URL, because the public endpoint rate-limits.

3. **Run it, then commit `scripts/fixtures/devnet/`:**

   ```sh
   cargo run -- --cluster devnet apply
   ```

   `apply` prints each transaction. If anything differs from the spec it ends with an error, after writing the outputs. Items it can't bring back are reported as `lost`: an escrowed prize, or a pig sold outside the fixture keys. A winning fixture wallet's prize comes back to its holder on the next run.

## Localnet

```sh
scripts/fixtures/localnet.sh          # build on a fresh validator, dump, stop
scripts/fixtures/localnet.sh --keep   # leave the validator on :8899
```

- **The validator:** Token Metadata and mpl-core load from `programs/raffles/tests/fixtures`, the bytes the LiteSVM tests run, so no network is needed. The localnet seed is public, and a throwaway payer is airdropped 50 SOL, since `solana-test-validator`'s rent is higher than devnet's. The script refuses to start while another validator answers on :8899, such as one left by `--keep`.
- **What the script writes:**
  - `scripts/fixtures/localnet/`
  - `programs/claim-spike/tests/fixtures/stand-ins/`, which `cargo test` runs through the spike's `claim_entries`. Each stand-in must pass and each look-alike must fail with `RuleMismatch` (`stand_ins_pass_the_rule_and_lookalikes_fail_it`).
- **`anchor test` stays on LiteSVM;** this validator is for runs that need a real RPC.

## Outputs

`scripts/fixtures/<cluster>/`:

- **`manifest.json`**, for the tests and the E2E run:
  - the seed's root and the membership rules (the `Config` values for that cluster)
  - every authority and wallet
  - every pig, with its mint, metadata, token account and holder (or its asset)
  - the look-alikes and the prizes
- **`collections.toml` and `seeds/*.mints.json`:** the indexer's seed format (its `config/collections.toml`), for the cluster registry (PIG-134, PIG-139). They list the four collections. The look-alikes aren't in any list, as on mainnet.

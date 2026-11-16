# PiggyGang Raffles program

The Anchor program behind PiggyGang Raffles: escrowed prizes, free per-NFT entries for Piggy holders, a Switchboard On-Demand draw, and settlement with rent refunds. It is the only source of truth for raffle state; the indexer, the crank and the frontend all read from it. The product spec and the system architecture live in `docs/` of [piggygang/raffles](https://github.com/piggygang/raffles).

So far the program has its account model, its events and a single scaffold instruction, `hello`. The repo is Rust: the program, its tests and their harness. The one exception is the TypeScript client in `clients/ts`, which CI generates from the IDL.

[docs/account-model.md](docs/account-model.md) has every account's layout, size and rent, the addresses they live at, and the raffle state machine (PIG-86).

[docs/events.md](docs/events.md) has every event's layout, and how the program emits them: as inner instructions, not log lines (PIG-117).

[docs/releases.md](docs/releases.md) has how the program's interface is released and consumed (PIG-117): the IDL, a TypeScript client and a Rust client per tag, `deployments.json`, which says what each cluster runs, and the one line with which the indexer, raffles-services and the app pin a release.

`programs/vrf-spike` and `spikes/vrf-driver` are the PIG-114 spike: ORAO VRF on devnet. Its end-to-end devnet measurement is still to run. They go once PIG-83 lands the real draw. The findings and how to rerun them are in [docs/spikes/pig-114-vrf-on-devnet.md](docs/spikes/pig-114-vrf-on-devnet.md).

`scripts/fixtures` builds devnet stand-ins for the Piggy collections, prize assets and test wallets, with mainnet's on-chain structure (PIG-115). See [scripts/fixtures/README.md](scripts/fixtures/README.md).

`programs/claim-spike` is the PIG-133 spike: `claim_entries`' holder proofs and receipts, measured on LiteSVM for transaction size and compute units. It goes once PIG-84 lands the real claim. The findings are in [docs/spikes/pig-133-claim-entries-budget.md](docs/spikes/pig-133-claim-entries-budget.md).

| Cluster | Program ID |
| --- | --- |
| devnet | `RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM` |
| mainnet | gets its own program ID at launch |

The VRF spike program is devnet-only, at `VRFsDurdc8NMVFAS5G8XVGzGjnBkRm1Rv7ZiEckYUCw`. The claim spike runs only in LiteSVM, at `CLMsQPHKpKDBUnKGEYeKR3AtRq3HKtU5BrdSfDrEGSz1`, and no keypair exists for it.

## Toolchain

| Tool | Version | Pinned in |
| --- | --- | --- |
| Anchor CLI and crates | 1.2.2 | `Anchor.toml`, `programs/raffles/Cargo.toml` |
| Agave (Solana CLI) | 4.1.2 | `Anchor.toml`, `Cargo.toml` (for solana-verify) |
| platform-tools | v1.57 (Rust 1.95.0) | `Cargo.toml` |
| Rust (host) | 1.95.0 | `rust-toolchain.toml` |
| solana-verify (deploys only; needs Docker) | 0.5.2 | `.github/workflows/ci.yml` |
| Node (the TypeScript client only) | 24.19.0 | `.github/workflows/ci.yml` |

The scripts for versions, deployments and the drift check also need `jq`.

CI checks that the Anchor and Solana pins agree. To install:

```sh
# AVM must be v1-capable to install Anchor 1.x (avm self-update, once it is).
cargo install avm --git https://github.com/otter-sec/anchor --tag v1.2.2 --locked --force
avm install 1.2.2 && avm use 1.2.2

sh -c "$(curl -sSfL https://release.anza.xyz/v4.1.2/install)"

# Only needed to deploy.
cargo install solana-verify --version 0.5.2 --locked
```

If the active Solana CLI isn't 4.1.2, Anchor switches to it for each command and back afterwards, so make 4.1.2 the active release. Rustup installs Rust 1.95.0 on the first `cargo` command, and `cargo build-sbf` downloads platform-tools v1.57 on the first build.

## Build

```sh
anchor build --ignore-keys
```

This builds `target/deploy/raffles.so` and the IDL at `target/idl/raffles.json`, and the same for the spike programs.

The IDL is also committed, as `idls/raffles.json`: the IDL of record, which a release publishes and both clients are generated from. A change to the program's interface changes the built IDL, and the tests then fail until the committed one is the same:

```sh
cp target/idl/raffles.json idls/raffles.json
```

Without the Anchor CLI, take the file from the `idl` artifact of the pull request's CI run.

`--ignore-keys` is deliberate. Anchor compares `declare_id!` against `target/deploy/raffles-keypair.json`, a random key that the build generates when none exists. The real program keypair never enters the repo, so that check can only ever fail. **Never run `anchor keys sync` or `anchor deploy`**: the first rewrites the program ID to the throwaway key, and the second deploys to it.

## Test

```sh
anchor test    # build, then cargo test
cargo test     # rerun after a build
```

The tests run in-process on [LiteSVM](https://github.com/LiteSVM/litesvm): no validator, no network, about 0.1 s for the suite. Each test gets a fresh VM with `raffles` from `target/deploy` at its declared address, so build before running `cargo test` alone.

The VM also runs Token Metadata and mpl-core: their mainnet bytecode, dumped into `programs/raffles/tests/fixtures` along with the deploy each came from. Token Metadata is immutable on mainnet; when mpl-core ships an upgrade, refresh the dump with `scripts/dump-fixtures.sh` and commit it. Smoke tests create an NFT and a Core asset through them, which is also how raffle tests will mint their prizes and Piggies.

The tests are one binary, `programs/raffles/tests/integration`, with a module per area and a shared `harness`. Add new areas as modules there, not as new files in `tests/`, so LiteSVM links once.

The account model has unit tests next to its types, in `programs/raffles/src/state`. They check every layout, size, rent figure, enum value and state transition against [docs/account-model.md](docs/account-model.md), so a change to an account fails until the document says the same. The events have the same in `programs/raffles/src/events.rs`, against [docs/events.md](docs/events.md).

The `interface` module of the integration tests holds the released interface to the program: the committed IDL is the one the build wrote, the events document lists the IDL's events, and the Rust client in `clients/rust` has the program's ID, version and addresses and reads the events it emits.

The VRF spike's tests run the ORAO VRF program dumped from devnet, with test keys in place of ORAO's fulfillment authorities, so request, fulfill and consume all run for real. `scripts/dump-vrf-spike-fixtures.sh` refreshes the dump.

The claim spike's tests decode real pigs: a SOL Gang and a Girl Gang metadata account and a Piggy Gang Core asset, copied from mainnet into `programs/claim-spike/tests/fixtures` by `scripts/dump-claim-spike-fixtures.sh`. Its `budget` module runs each batch at 8 address seeds, which takes the suite to about 15 s, and prints the measurement tables:

```sh
cargo test -p claim-spike --test integration budget -- --nocapture --test-threads 1
```

Its `stand_ins` test runs the fixture set (PIG-115) through `claim_entries`: each stand-in passes and each look-alike fails. The accounts come from `scripts/fixtures/localnet.sh`, which builds the set on a local validator.

CI runs the blocks below, each from the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
anchor build --ignore-keys
cargo test --workspace
```

Clippy flags `unwrap`, `expect`, `panic`, lossy `as` casts and float arithmetic in program code; test code may unwrap. The lint set lives in `[workspace.lints]` in `Cargo.toml`.

`spikes/vrf-driver` is a workspace of its own, because `cargo build-sbf` builds every workspace member for SBF and an RPC client doesn't compile there. CI lints it in a separate job:

```sh
cd spikes/vrf-driver
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

`scripts/fixtures` is one too. CI runs the same two commands there, plus `cargo test --locked`.

The two clients have a job each. The Rust client is its own workspace, and its consumers build on Rust 1.90:

```sh
cd clients/rust
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo +1.90 build --locked
```

Nothing the TypeScript client generates is committed:

```sh
cd clients/ts
npm ci --ignore-scripts
npm run build
npm test
```

CI also checks that the program, the IDL and both clients are at one version (`scripts/version.sh check`), that `deployments.json` is well-formed (`scripts/deployments.sh check`), and that the drift check fails on a deliberate mismatch.

## Deploy to devnet

```sh
scripts/deploy-devnet.sh --dry-run    # build and every check, no SOL spent
scripts/deploy-devnet.sh
```

The script builds with `solana-verify build` in Docker, which pins the whole build environment. Everyone who deploys the same commit ships the same bytes, and the CI `verifiable` job's `raffles-verifiable` artifact carries that commit's hash. To check what devnet runs:

```sh
solana-verify -u devnet get-program-hash RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM
```

The script refuses to deploy when:

- the RPC isn't devnet (it compares the genesis hash)
- the Solana CLI isn't the pinned version
- `programs/` or `Cargo.*` has uncommitted changes (override with `--allow-dirty`)
- `HEAD` isn't a release: the tag `v<version>` of the program's version (override with `--untagged`)
- `deployments.json` doesn't pass its check, or lacks a deployment that `main` has recorded, as a checkout of an older tag does
- the authority keypair doesn't match the program's upgrade authority on devnet
- `HEAD` moves while it builds

It upgrades when the program exists and does the first deploy otherwise. After deploying, it checks that the on-chain hash matches the build. Then it records the deployment in `deployments.json`: the release, the commit, the slot and the hashes. Commit that file. It is what tells the indexer, raffles-services and the app which interface devnet speaks, and an `--untagged` deployment is recorded without a version, so they can tell that no release describes it ([docs/releases.md](docs/releases.md), section 3).

The public devnet RPC rate-limits the write transactions of a deploy, so set `RAFFLES_DEVNET_RPC_URL` to a Helius devnet URL. The build image is `linux/amd64`, so it runs under emulation and slower on Apple Silicon. An interrupted deploy leaves a buffer account holding the SOL; reclaim it with `solana program close --buffers -k <authority keypair> -u devnet`.

## Keys

No key ever enters the repo. `*-keypair.json` is gitignored as a backstop.

| Key | Default path | Needed for |
| --- | --- | --- |
| Program keypair (`RAFA…`) | `~/.config/solana/piggygang/raffles-program-devnet.json` | The first devnet deploy only |
| Devnet upgrade authority | `~/.config/solana/piggygang/raffles-devnet-authority.json` | Every devnet deploy; it also pays, so keep a few SOL on it |
| Spike program keypair (`VRFs…`) | `~/.config/solana/piggygang/vrf-spike-program-devnet.json` | The spike's first devnet deploy (PIG-114) |
| Devnet fixtures seed | `~/.config/solana/piggygang/devnet-fixtures-seed.json` | `scripts/fixtures` on devnet: every fixture key derives from it (PIG-115) |

The raffles keys and the fixtures seed are team secrets. Their canonical copies live in the team password manager: copy them to the paths above. You can also point `RAFFLES_PROGRAM_KEYPAIR` and `RAFFLES_DEVNET_AUTHORITY` at the raffles keys, and `--seed-file` at the seed. After the first deploy, the program keypair has no further use, because the address is permanent and only the upgrade authority can change the program. If the authority doesn't exist yet, create it once and store it in the password manager:

```sh
solana-keygen new -o ~/.config/solana/piggygang/raffles-devnet-authority.json
solana airdrop 2 -u devnet -k ~/.config/solana/piggygang/raffles-devnet-authority.json
```

The spike keypair is disposable: it lives only on the machine that made it. Whoever runs the spike deploys from there, or grinds a new keypair and updates `declare_id!` in `programs/vrf-spike` and its `Anchor.toml` entries.

The program ID changes only through a reviewed edit of `declare_id!` and of both `[programs.*]` entries in `Anchor.toml`. On mainnet the upgrade authority will be the Squads v4 vault, as the architecture lays out; the mainnet program and its verified-build submission are out of scope here.

## Dependencies

Anchor 1.2.2 is built on the Solana 3 crates, and every dependency is aligned to that one SDK. `anchor-lang` has its `event-cpi` feature on, because the program emits events as inner instructions ([docs/events.md](docs/events.md)).

- **Token Metadata** comes through `anchor-spl`'s `metadata` feature (`anchor_spl::metadata::mpl_token_metadata`). It pins the `mpl-token-metadata` release built on Solana 3; the latest stable, 5.1.1, is still Solana 2.
- **mpl-core** is used without its `anchor` features, which target Anchor 0.31/0.32.
- **Switchboard On-Demand** uses `default-features = false, features = ["solana-v3"]`. It asks for `solana-program >=3`, which `cargo update` resolves to the newest major, so `Cargo.lock` pins it to the 3.0.0 the Metaplex crates share. CI fails if a second `solana-program` appears. To restore the pin, run `cargo update -p solana-program@<new version> --precise 3.0.0`. It also pulls in `getrandom` 0.2, which has no Solana backend, so the program enables its `custom` feature on the Solana target.
- **LiteSVM** is 0.16, which runs the Agave 4.2 runtime. That's the newest that builds on Rust 1.95: 0.15 breaks against the 4.2 crates, and 0.17 and later need the 4.3 runtime, which needs Rust 1.97. The workspace uses resolver 3, so `cargo update` stays on versions that build with `rust-version`.

## Layout

```text
programs/raffles/src/                 the program
programs/raffles/tests/integration/   LiteSVM tests: harness and one module per area
programs/raffles/tests/fixtures/      Token Metadata and mpl-core dumped from mainnet
programs/vrf-spike/                   PIG-114 spike program, its tests and ORAO fixtures
programs/claim-spike/                 PIG-133 spike program, its tests and mainnet pig fixtures
spikes/vrf-driver/                    PIG-114 devnet driver (its own workspace)
scripts/fixtures/                     PIG-115 devnet fixtures (its own workspace) and their outputs
idls/raffles.json                     the IDL of record: what a release publishes
clients/rust/                         the Rust client, from the IDL (its own workspace)
clients/ts/                           the TypeScript client, generated from the IDL by Codama
deployments.json                      which release each cluster runs, and since which slot
docs/account-model.md                 accounts, sizes, rent and the state machine
docs/events.md                        events, their layouts and how they are emitted
docs/releases.md                      releases, pinning one, deployments.json and the drift check
docs/spikes/                          spike write-ups
scripts/                              deploy-devnet.sh, the fixture dumps (dump-*.sh), and
                                      version.sh, deployments.sh and idl-drift.sh for releases
.github/actions/idl-drift/            the drift check, as an action for the consumers
```

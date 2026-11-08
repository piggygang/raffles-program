# PiggyGang Raffles program

The Anchor program behind PiggyGang Raffles: escrowed prizes, free per-NFT entries for Piggy holders, a Switchboard On-Demand draw, and settlement with rent refunds. It is the only source of truth for raffle state; the indexer, the crank and the frontend all read from it. The product spec and the system architecture live in `docs/` of [piggygang/raffles](https://github.com/piggygang/raffles).

So far the program has a single scaffold instruction, `hello`. The repo is pure Rust: the program, its tests and their harness.

`programs/vrf-spike` and `spikes/vrf-driver` are the PIG-114 spike: ORAO VRF on devnet, measured end to end. They go once PIG-83 lands the real draw. The findings and how to rerun them are in [docs/spikes/pig-114-vrf-on-devnet.md](docs/spikes/pig-114-vrf-on-devnet.md).

| Cluster | Program ID |
| --- | --- |
| devnet | `RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM` |
| mainnet | gets its own program ID at launch |

The spike program is devnet-only, at `VRFsDurdc8NMVFAS5G8XVGzGjnBkRm1Rv7ZiEckYUCw`.

## Toolchain

| Tool | Version | Pinned in |
| --- | --- | --- |
| Anchor CLI and crates | 1.2.2 | `Anchor.toml`, `programs/raffles/Cargo.toml` |
| Agave (Solana CLI) | 4.1.2 | `Anchor.toml`, `Cargo.toml` (for solana-verify) |
| platform-tools | v1.57 (Rust 1.95.0) | `Cargo.toml` |
| Rust (host) | 1.95.0 | `rust-toolchain.toml` |
| solana-verify (deploys only; needs Docker) | 0.5.2 | `.github/workflows/ci.yml` |

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

This builds `target/deploy/raffles.so` and the IDL at `target/idl/raffles.json`, and the same for the spike program.

`--ignore-keys` is deliberate. Anchor compares `declare_id!` against `target/deploy/raffles-keypair.json`, a random key that the build generates when none exists. The real program keypair never enters the repo, so that check can only ever fail. **Never run `anchor keys sync` or `anchor deploy`**: the first rewrites the program ID to the throwaway key, and the second deploys to it.

## Test

```sh
anchor test    # build, then cargo test
cargo test     # rerun after a build
```

The tests run in-process on [LiteSVM](https://github.com/LiteSVM/litesvm): no validator, no network, about 0.1 s for the suite. Each test gets a fresh VM with `raffles` from `target/deploy` at its declared address, so build before running `cargo test` alone.

The VM also runs Token Metadata and mpl-core: their mainnet bytecode, dumped into `programs/raffles/tests/fixtures` along with the deploy each came from. Token Metadata is immutable on mainnet; when mpl-core ships an upgrade, refresh the dump with `scripts/dump-fixtures.sh` and commit it. Smoke tests create an NFT and a Core asset through them, which is also how raffle tests will mint their prizes and Piggies.

The tests are one binary, `programs/raffles/tests/integration`, with a module per area and a shared `harness`. Add new areas as modules there, not as new files in `tests/`, so LiteSVM links once.

The spike's tests run the ORAO VRF program dumped from devnet, with test keys in place of ORAO's fulfillment authorities, so request, fulfill and consume all run for real. `scripts/dump-vrf-spike-fixtures.sh` refreshes the dump.

CI runs:

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
- the authority keypair doesn't match the program's upgrade authority on devnet

It upgrades when the program exists and does the first deploy otherwise. After deploying, it checks that the on-chain hash matches the build.

The public devnet RPC rate-limits the write transactions of a deploy, so set `RAFFLES_DEVNET_RPC_URL` to a Helius devnet URL. The build image is `linux/amd64`, so it runs under emulation and slower on Apple Silicon. An interrupted deploy leaves a buffer account holding the SOL; reclaim it with `solana program close --buffers -k <authority keypair> -u devnet`.

## Keys

No key ever enters the repo. `*-keypair.json` is gitignored as a backstop.

| Key | Default path | Needed for |
| --- | --- | --- |
| Program keypair (`RAFA…`) | `~/.config/solana/piggygang/raffles-program-devnet.json` | The first devnet deploy only |
| Devnet upgrade authority | `~/.config/solana/piggygang/raffles-devnet-authority.json` | Every devnet deploy; it also pays, so keep a few SOL on it |
| Spike program keypair (`VRFs…`) | `~/.config/solana/piggygang/vrf-spike-program-devnet.json` | The spike's first devnet deploy (PIG-114) |

The raffles keys are team secrets. Their canonical copies live in the team password manager: copy them to the paths above, or point `RAFFLES_PROGRAM_KEYPAIR` and `RAFFLES_DEVNET_AUTHORITY` at them. After the first deploy, the program keypair has no further use, because the address is permanent and only the upgrade authority can change the program. If the authority doesn't exist yet, create it once and store it in the password manager:

```sh
solana-keygen new -o ~/.config/solana/piggygang/raffles-devnet-authority.json
solana airdrop 2 -u devnet -k ~/.config/solana/piggygang/raffles-devnet-authority.json
```

The spike keypair is disposable: it lives only on the machine that made it. Whoever runs the spike deploys from there, or grinds a new keypair and updates `declare_id!` in `programs/vrf-spike` and its `Anchor.toml` entries.

The program ID changes only through a reviewed edit of `declare_id!` and of both `[programs.*]` entries in `Anchor.toml`. On mainnet the upgrade authority will be the Squads v4 vault, as the architecture lays out; the mainnet program and its verified-build submission are out of scope here.

## Dependencies

Anchor 1.2.2 is built on the Solana 3 crates, and every dependency is aligned to that one SDK:

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
spikes/vrf-driver/                    PIG-114 devnet driver (its own workspace)
docs/spikes/                          spike write-ups
scripts/                              deploy-devnet.sh, dump-fixtures.sh, dump-vrf-spike-fixtures.sh
```

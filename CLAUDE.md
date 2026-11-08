# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Git rules

- **Never commit, push, tag, open PRs or deploy.** That means no `git commit`, `git push`, `git tag`, `gh pr create`, `scripts/deploy-devnet.sh`, `solana program deploy`, `vrf-driver run` or `withdraw`, or anything else that sends a transaction to devnet or mainnet, not even when asked to "finish up". Reading chain state is fine (`vrf-driver status` and `observe` only read). Creating a branch is fine: use the Linear issue's suggested name (`feature/pig-NNN-<slug>`). When the work is done, run the gate CI runs (below) for any code change, then output a suggested commit message plus the exact paths to stage. Never suggest `git add -A` or `git add .`.
- **Suggested commit messages** are a single line in Conventional Commits form: lowercase imperative subject, optional scope, no trailing period, no body. Types are `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `chore`, `build` and `ci`. Scopes are `program`, `idl`, `tests`, `deploy` and `deps`. Example: `feat(program): add create_raffle with prize escrow`. Never append ` (#N)`.
- **No AI attribution anywhere.** Never write `Co-Authored-By: Claude …`, "🤖 Generated with Claude Code" or anything similar, whether in commit messages, PR text or code comments.
- **No key ever enters the repo**, not even a devnet one. Keys live under `~/.config/solana/piggygang/` (README, "Keys").

## Commands

- `anchor build --ignore-keys`: builds `target/deploy/raffles.so` and `target/idl/raffles.json`. Always pass `--ignore-keys` (see Program ID below).
- `anchor test`: builds, then runs `cargo test`. No validator: `skip_local_validator = true` in `Anchor.toml`.
- `cargo test`: the LiteSVM suite against the last build. Run a build first; the harness loads `target/deploy/raffles.so`.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: the lint gates. Clippy doesn't need a build.
- `cargo test __anchor_private_print_idl --features idl-build -- --show-output` in `programs/raffles` prints the IDL without the Anchor CLI.
- `scripts/dump-fixtures.sh` refreshes the mainnet program fixtures, and `scripts/dump-vrf-spike-fixtures.sh` the devnet ORAO ones. They need network access, so run them only when asked.
- In `spikes/vrf-driver`: `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings`. It is its own workspace, so the commands above don't reach it.

The gate is fmt, clippy, `anchor build --ignore-keys` and `cargo test --workspace`, plus the two driver commands when `spikes/vrf-driver` or `programs/vrf-spike` changed.

## Architecture

The repo is pure Rust: no Node, no TypeScript, no package manager besides Cargo. One Anchor 1.2.2 program, `programs/raffles` (lib `raffles`). `lib.rs` only dispatches; each instruction lives in `src/instructions/<name>.rs` with its `#[derive(Accounts)]` struct and a `handle_<name>` function, and `instructions.rs` re-exports them (the `#[program]` macro needs the generated client modules at the crate root). `hello` is the scaffold's smoke test and goes once a real instruction exists.

- **The PIG-114 spike.** `programs/vrf-spike` (lib `vrf_spike`, `VRFsDurdc8NMVFAS5G8XVGzGjnBkRm1Rv7ZiEckYUCw`, devnet only) has the shape of `request_draw`/`fulfill_draw` on ORAO VRF: a data-less vault PDA pays ORAO by CPI, and `request` and `consume` are permissionless. Its `client` module builds every instruction for both the tests and `spikes/vrf-driver`, the driver that measures it on devnet. The driver is its own workspace, because `cargo build-sbf` (and so solana-verify) builds every workspace member for SBF, and an RPC client doesn't compile there. Both go once PIG-83 lands. Findings: `docs/spikes/pig-114-vrf-on-devnet.md`. Switchboard On-Demand shut down in September 2026, so ADR-1's fallback applies.
- **Program ID.** `RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM` in `declare_id!` and in both `[programs.localnet]` and `[programs.devnet]`. The tests load the binary at `raffles::ID`, so they never need the program keypair. `target/deploy/raffles-keypair.json` is a throwaway the build generates. Never run `anchor keys sync` (it rewrites the ID to that throwaway) or `anchor deploy` (it deploys to it).
- **One Solana SDK.** Anchor 1.2.2 is on the Solana 3 crates, and every dependency must resolve onto them. Token Metadata comes only through `anchor-spl`'s `metadata` feature (`anchor_spl::metadata::mpl_token_metadata`), since mpl-token-metadata 5.1.1 is Solana 2. mpl-core runs without its `anchor` features, which target Anchor 0.31/0.32. switchboard-on-demand uses `default-features = false, features = ["solana-v3"]`, and `Cargo.lock` pins its unbounded `solana-program >=3` to 3.0.0. CI fails on a second `solana-program`; after a `cargo update`, restore the pin with `cargo update -p solana-program@<new> --precise 3.0.0`. Switchboard also pulls `getrandom` 0.2 into the SBF build; the target-specific `custom` feature in `programs/raffles/Cargo.toml` is what lets it compile, so keep it.
- **Lints.** `[workspace.lints]` in `Cargo.toml` warns on `unwrap`/`expect`/`panic`, lossy `as` casts and float arithmetic, and CI denies warnings. Return an `#[error_code]` error instead. `clippy.toml` lets `#[test]` functions unwrap. The integration test crate allows unwrap, expect and panic at its root, because its harness helpers aren't `#[test]` functions. rustfmt uses stable options only.
- **Tests.** LiteSVM 0.16 (Agave 4.2 runtime), in-process. `tests/integration` is one binary: `main.rs` declares a module per area, and `harness.rs` provides `Test::new()` (a fresh VM with `raffles`, Token Metadata and mpl-core loaded and a funded payer) and `Test::send()` (signs, sends, panics with the program logs on failure). Add areas as modules there, never as new top-level files in `tests/`, which would each link LiteSVM again. For deadlines, move time with `svm.warp_to_slot` and `svm.set_sysvar(&Clock { .. })`. VRF needs no mock: the vrf-spike tests load ORAO's devnet program from `programs/vrf-spike/tests/fixtures` and install test keys as its fulfillment authorities, so ORAO's own `fulfill_v2` runs (LiteSVM's `precompiles` feature provides the Ed25519 precompile it needs). Live fulfillment is exercised on devnet (architecture §8).
- **Fixtures.** `tests/fixtures/*.so` are Token Metadata (immutable on mainnet) and mpl-core, dumped from mainnet by `scripts/dump-fixtures.sh`, which also rewrites `tests/fixtures/README.md` with each program's deploy slot. Load them through the harness by crate ID (`mpl_token_metadata::ID`, `mpl_core::ID`), and build their instructions with the crates' builders (see `metaplex.rs`).
- **Dependency versions.** The workspace uses resolver 3, so Cargo picks versions that build with `rust-version` (1.95). LiteSVM is held at 0.16 for that reason: 0.17 and later need the Agave 4.3 runtime, which needs Rust 1.97.
- **Verifiable builds.** `scripts/deploy-devnet.sh` deploys the `solana-verify build --library-name raffles --arch v3` output, and the CI `verifiable` job builds the same thing. solana-verify picks its image from `[workspace.metadata.cli] solana`; `cargo build-sbf` reads `[workspace.metadata.solana] tools-version` on every build path. SBPF v3 is active on devnet and mainnet.

## Pins

CI checks that the Anchor and Solana pins agree; keeping platform-tools and Rust in step is on you. When bumping Anchor, change all of these together:

- `anchor_version` in `Anchor.toml`, the exact `anchor-lang`/`anchor-spl` versions in `programs/raffles/Cargo.toml` (and `anchor-lang` in `programs/vrf-spike` and `spikes/vrf-driver`, while they exist) and `ANCHOR_VERSION` in `ci.yml`.
- If the release notes recommend a new Solana version: `solana_version` in `Anchor.toml`, `[workspace.metadata.cli] solana` in `Cargo.toml` and `SOLANA_VERSION` in `ci.yml`.
- If Anchor's default platform-tools change: `[workspace.metadata.solana] tools-version` in `Cargo.toml`, plus `rust-toolchain.toml` to that platform-tools release's Rust, and LiteSVM to the newest release that builds on it.

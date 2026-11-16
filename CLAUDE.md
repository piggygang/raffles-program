# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Git rules

- **Never commit, push, tag, open PRs or deploy.** That means no `git commit`, `git push`, `git tag`, `gh pr create`, `gh release`, `npm publish`, `scripts/deploy-devnet.sh`, `solana program deploy`, `vrf-driver run` or `withdraw`, `fixtures --cluster devnet apply`, or anything else that sends a transaction to devnet or mainnet, not even when asked to "finish up". A `v*` tag publishes a release. Reading chain state is fine (`vrf-driver status` and `observe`, and `fixtures check` and `dump`, only read), and so is a local validator (`scripts/fixtures/localnet.sh`). Creating a branch is fine: use the Linear issue's suggested name (`feature/pig-NNN-<slug>`). When the work is done, run the gate CI runs (below) for any code change, then output a suggested commit message plus the exact paths to stage. Never suggest `git add -A` or `git add .`.
- **Suggested commit messages** are a single line in Conventional Commits form: lowercase imperative subject, optional scope, no trailing period, no body. Types are `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `chore`, `build` and `ci`. Scopes are `program`, `idl`, `tests`, `deploy` and `deps`. Example: `feat(program): add create_raffle with prize escrow`. Never append ` (#N)`.
- **No AI attribution anywhere.** Never write `Co-Authored-By: Claude …`, "🤖 Generated with Claude Code" or anything similar, whether in commit messages, PR text or code comments.
- **No key ever enters the repo**, not even a devnet one. Keys live under `~/.config/solana/piggygang/` (README, "Keys").

## PR reviews

- **Review comments get codebase fixes and replies.** Check each open thread against the current code and fix what still holds. Then write a reply for every thread: short, brief and straight to the point, saying what changed or why nothing did.
- **Replies to CodeRabbit start with `@coderabbitai`.**

## Commands

- `anchor build --ignore-keys`: builds `target/deploy/raffles.so` and `target/idl/raffles.json`. Always pass `--ignore-keys` (see Program ID below).
- `cp target/idl/raffles.json idls/raffles.json`: after a build that changed the program's interface. `cargo test` fails until the committed IDL is the built one.
- `anchor test`: builds, then runs `cargo test`. No validator: `skip_local_validator = true` in `Anchor.toml`.
- `cargo test`: the LiteSVM suite against the last build. Run a build first; the harness loads `target/deploy/raffles.so`.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: the lint gates. Clippy doesn't need a build.
- `cargo test __anchor_private_print_idl --features idl-build -- --show-output` in `programs/raffles` prints the IDL without the Anchor CLI.
- `scripts/dump-fixtures.sh` refreshes the mainnet program fixtures, `scripts/dump-vrf-spike-fixtures.sh` the devnet ORAO ones, and `scripts/dump-claim-spike-fixtures.sh` the mainnet pig accounts. They need network access, so run them only when asked.
- `cargo test -p claim-spike --test integration budget -- --nocapture --test-threads 1` prints the PIG-133 measurement tables.
- In `spikes/vrf-driver`: `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings`. It is its own workspace, so the commands above don't reach it.
- In `scripts/fixtures`: the same two commands plus `cargo test --locked`; it is its own workspace too. `scripts/fixtures/localnet.sh` rebuilds the localnet fixture set and the claim spike's stand-in dumps on a local validator, no network needed.
- In `clients/rust`: the same two commands, `cargo test --locked`, and `cargo +1.90 build --locked`; its own workspace as well.
- In `clients/ts`: `npm ci --ignore-scripts`, `npm run build` and `npm test`, on Node 24. Nothing they generate is committed.
- `scripts/version.sh check` and `scripts/deployments.sh check`: one version everywhere, and a well-formed `deployments.json`. Both need `jq`. `scripts/version.sh set <version>` writes a new version into every manifest.

The gate is fmt, clippy, `anchor build --ignore-keys`, `cargo test --workspace` and the two script checks, plus:

- the two driver commands when `spikes/vrf-driver` or `programs/vrf-spike` changed
- the three fixtures commands when `scripts/fixtures` changed
- the `clients/rust` commands and the `clients/ts` commands when either client or `idls/raffles.json` changed

## Architecture

The repo is Rust, with one exception: `clients/ts`, the TypeScript client, which is generated from the IDL and built with npm. Nothing else uses Node. One Anchor 1.2.2 program, `programs/raffles` (lib `raffles`). `lib.rs` only dispatches; each instruction lives in `src/instructions/<name>.rs` with its `#[derive(Accounts)]` struct and a `handle_<name>` function, and `instructions.rs` re-exports them (the `#[program]` macro needs the generated client modules at the crate root). `hello` is the scaffold's smoke test and goes once a real instruction exists.

- **The account model (PIG-86).** `src/state/<account>.rs` holds each account next to its enums: `Config`, `CreatorEntry`, `PrizeEntry`, `Raffle`, `EntryPosition` and `EntryReceipt`. `constants.rs` has the seed prefixes, the timers and the limits (`#[constant]` publishes a value in the IDL), and `error.rs` has `RaffleError`.
  - `bump` comes first, and fixed-size fields come before variable-length ones.
  - A kind, state or reason is a `u8` field with an enum of the same values, declared through `byte_enum!`. Never renumber a value.
  - `_reserved` bytes stay zero.
  - Instructions change a raffle's state only through `Raffle::transition`.
  - `docs/account-model.md` has every layout, size, rent figure, enum value and transition. The unit tests in `src/state` compare it with the code, so change an account and the document together.
  - Anchor puts an account into the IDL only once an instruction's accounts struct names it.
- **The events (PIG-117).** `src/events.rs` holds the seventeen events, and `docs/events.md` every layout and what emits it. Its unit tests compare the two, so change an event and the document together.
  - An instruction emits with `emit_cpi!`, once, after its last change, and its accounts struct has `#[event_cpi]`. Never `emit!`: readers take events from inner instructions, not logs.
  - A raffle's event starts with `raffle`, and fixed-size fields come before variable-length ones.
  - A field named like an account field carries that field's value after the instruction.
  - An event's name and fields never change. A new need gets a new event.
  - An event is in the IDL from the moment it is declared.
- **The released interface (PIG-117).** `docs/releases.md` has the whole of it.
  - `idls/raffles.json` is the IDL of record: a committed copy of `target/idl/raffles.json`. A release publishes it, and both clients are generated from it.
  - `clients/rust` (crate `raffles-client`, its own workspace, Rust 1.90) is `declare_program!` over that IDL, plus `pda` and `read_event`, written by hand. Consumers take it by git tag. `clients/ts` (`@piggygang/raffles-client`) is Codama's client of the IDL.
  - `programs/raffles/tests/integration/interface.rs` holds the IDL of record, the events document and the Rust client to the program. Tests in the `raffles` package must not name what the client generates from the IDL, such as an event's type: `anchor build` compiles them to write the IDL, so a test that needed the new IDL could never get it.
  - The program, the IDL and both clients share one version (`scripts/version.sh`). A tag `vX.Y.Z` runs `.github/workflows/release.yml`: the whole gate, then the release.
  - `deployments.json` says which release each cluster runs and since which slot. `scripts/deploy-devnet.sh` writes it through `scripts/deployments.sh`. It only grows: a recorded deployment keeps its place, and CI fails a pull request that drops or changes one.
  - `.github/actions/idl-drift` and `scripts/idl-drift.sh` are the drift check the consumers run.
- **The PIG-114 spike.** `programs/vrf-spike` (lib `vrf_spike`, `VRFsDurdc8NMVFAS5G8XVGzGjnBkRm1Rv7ZiEckYUCw`, devnet only) has the shape of `request_draw`/`fulfill_draw` on ORAO VRF: a data-less vault PDA pays ORAO by CPI, and `request` and `consume` are permissionless. Its `client` module builds every instruction for both the tests and `spikes/vrf-driver`, the driver that measures it on devnet. The driver is its own workspace, because `cargo build-sbf` (and so solana-verify) builds every workspace member for SBF, and an RPC client doesn't compile there. Both go once PIG-83 lands. Findings: `docs/spikes/pig-114-vrf-on-devnet.md`. Switchboard On-Demand shut down in September 2026, so ADR-1's fallback applies.
- **The PIG-133 spike.** `programs/claim-spike` (lib `claim_spike`, `CLMsQPHKpKDBUnKGEYeKR3AtRq3HKtU5BrdSfDrEGSz1`, LiteSVM only, no keypair) prototypes `claim_entries`: holder-proof §4's checks per NFT, an `EntryReceipt` per NFT created with `create_account_allow_prefund`, and the `EntryPosition`'s cap and Full Gang bonus. Its tests clone real mainnet pigs from `programs/claim-spike/tests/fixtures`, and its `budget` module measures bytes and compute units and pins the findings in assertions. It goes once PIG-84 lands. Findings: `docs/spikes/pig-133-claim-entries-budget.md`.
- **Fixtures (PIG-115).** `scripts/fixtures` (its own workspace, like the driver) builds devnet stand-ins with mainnet's structure:
  - two legacy Token Metadata collections without a certified collection, one Core collection and a Pig Mud stand-in
  - a look-alike for each way the membership rule rejects an NFT
  - a prize of each allowlist kind, and the test wallets
  - Every key derives from one seed: on devnet `~/.config/solana/piggygang/devnet-fixtures-seed.json`, a team secret; on localnet a public one. So `apply` is idempotent, and `plan` writes `scripts/fixtures/<cluster>/` (manifest, indexer `collections.toml`, mint lists) without a network.
  - The claim spike's `stand_ins` test runs `localnet.sh`'s dumps through `claim_entries`: the stand-ins pass, the look-alikes fail. Spec: `scripts/fixtures/src/spec.rs`; README: `scripts/fixtures/README.md`.
- **Program ID.** `RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM` in `declare_id!` and in both `[programs.localnet]` and `[programs.devnet]`. The tests load the binary at `raffles::ID`, so they never need the program keypair. `target/deploy/raffles-keypair.json` is a throwaway the build generates. Never run `anchor keys sync` (it rewrites the ID to that throwaway) or `anchor deploy` (it deploys to it).
- **One Solana SDK.** Anchor 1.2.2 is on the Solana 3 crates, and every dependency must resolve onto them. `anchor-lang` has `event-cpi` on, for the events. Token Metadata comes only through `anchor-spl`'s `metadata` feature (`anchor_spl::metadata::mpl_token_metadata`), since mpl-token-metadata 5.1.1 is Solana 2. mpl-core runs without its `anchor` features, which target Anchor 0.31/0.32. switchboard-on-demand uses `default-features = false, features = ["solana-v3"]`, and `Cargo.lock` pins its unbounded `solana-program >=3` to 3.0.0. CI fails on a second `solana-program`; after a `cargo update`, restore the pin with `cargo update -p solana-program@<new> --precise 3.0.0`. Switchboard also pulls `getrandom` 0.2 into the SBF build; the target-specific `custom` feature in `programs/raffles/Cargo.toml` is what lets it compile, so keep it.
- **Lints.** `[workspace.lints]` in `Cargo.toml` warns on `unwrap`/`expect`/`panic`, lossy `as` casts and float arithmetic, and CI denies warnings. Return an `#[error_code]` error instead. `clippy.toml` lets `#[test]` functions unwrap. The integration test crate allows unwrap, expect and panic at its root, because its harness helpers aren't `#[test]` functions. rustfmt uses stable options only.
- **Tests.** LiteSVM 0.16 (Agave 4.2 runtime), in-process. `tests/integration` is one binary: `main.rs` declares a module per area, and `harness.rs` provides `Test::new()` (a fresh VM with `raffles`, Token Metadata and mpl-core loaded and a funded payer) and `Test::send()` (signs, sends, panics with the program logs on failure), with `Test::try_send()` for a transaction that should fail. Add areas as modules there, never as new top-level files in `tests/`, which would each link LiteSVM again. For deadlines, move time with `svm.warp_to_slot` and `svm.set_sysvar(&Clock { .. })`. LiteSVM's Rent sysvar still has the old rate, 6,960 lamports per byte; devnet and mainnet charge 5,080. VRF needs no mock: the vrf-spike tests load ORAO's devnet program from `programs/vrf-spike/tests/fixtures` and install test keys as its fulfillment authorities, so ORAO's own `fulfill_v2` runs (LiteSVM's `precompiles` feature provides the Ed25519 precompile it needs). Live fulfillment is exercised on devnet (architecture §8).
- **Fixtures.** `tests/fixtures/*.so` are Token Metadata (immutable on mainnet) and mpl-core, dumped from mainnet by `scripts/dump-fixtures.sh`, which also rewrites `tests/fixtures/README.md` with each program's deploy slot. Load them through the harness by crate ID (`mpl_token_metadata::ID`, `mpl_core::ID`), and build their instructions with the crates' builders (see `metaplex.rs`).
- **Dependency versions.** The workspace uses resolver 3, so Cargo picks versions that build with `rust-version` (1.95). LiteSVM is held at 0.16 for that reason: 0.17 and later need the Agave 4.3 runtime, which needs Rust 1.97.
- **Verifiable builds.** `scripts/deploy-devnet.sh` deploys the `solana-verify build --library-name raffles --arch v3` output, and the CI `verifiable` job builds the same thing. solana-verify picks its image from `[workspace.metadata.cli] solana`; `cargo build-sbf` reads `[workspace.metadata.solana] tools-version` on every build path. SBPF v3 is active on devnet and mainnet.

## Pins

CI checks that the Anchor and Solana pins agree; keeping platform-tools and Rust in step is on you. When bumping Anchor, change all of these together:

- `anchor_version` in `Anchor.toml`, the exact `anchor-lang`/`anchor-spl` versions in `programs/raffles/Cargo.toml`, `anchor-lang` in `clients/rust` (and in `programs/vrf-spike` and `spikes/vrf-driver`, and both in `programs/claim-spike`, while they exist) and `ANCHOR_VERSION` in `ci.yml`.
- If the release notes recommend a new Solana version: `solana_version` in `Anchor.toml`, `[workspace.metadata.cli] solana` in `Cargo.toml` and `SOLANA_VERSION` in `ci.yml`.
- If Anchor's default platform-tools change: `[workspace.metadata.solana] tools-version` in `Cargo.toml`, plus `rust-toolchain.toml` to that platform-tools release's Rust, and LiteSVM to the newest release that builds on it.

The TypeScript client has its own pins: `NODE_VERSION` in `ci.yml` and `release.yml`, and Codama, `@solana/kit` and TypeScript in `clients/ts/package.json`. The Rust client's `rust-version` follows its consumers, the indexer and raffles-services, not the program.

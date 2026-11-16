# Releases

|  |  |
| --- | --- |
| Issue | PIG-117 |
| Date | 2026-11-15 |
| Code | `idls/raffles.json`, `clients/`, `deployments.json`, `.github/workflows/release.yml`, `.github/actions/idl-drift`, `scripts/version.sh`, `scripts/deployments.sh`, `scripts/idl-drift.sh` |
| Status | The pipeline, with no release yet. `v0.1.0` belongs to PIG-155, once the instructions exist. |
| Built on | Architecture 1.2 §1, the [account model](account-model.md) and the [events](events.md) |

## Summary

- **A release is a tag, `vX.Y.Z`.** The program, its IDL and both clients carry that one version.
- **It publishes the program's interface:** the IDL, the verifiable build, a TypeScript client and a Rust client (section 1).
- **A consumer pins a release in one line,** and moves to another by changing that line (section 2).
- **`deployments.json` says which release each cluster runs,** and since which slot (section 3).
- **The drift check** fails a consumer's CI when its copy of the IDL isn't the pinned release's (section 2).
- **An interface can't change quietly.** CI fails when the committed IDL isn't the one the program builds, and when a released version's IDL changes without a new version (section 5).

## 1. What a release is

| Artifact | What it is | Where |
| --- | --- | --- |
| `raffles.json` | The Anchor IDL: `idls/raffles.json` at the tag, byte for byte | The GitHub release, and the tag's tree |
| `raffles.so` | The verifiable build, the one `scripts/deploy-devnet.sh` deploys | The GitHub release |
| `raffles-client.tgz` | The TypeScript client, `@piggygang/raffles-client`: Codama's reading of the IDL, for `@solana/kit` | The GitHub release, and npm once it is switched on (section 6) |
| `raffles-client`, the crate | The Rust client: the IDL, and what Anchor's `declare_program!` generates from it | The tag itself: a git dependency |
| `SHA256SUMS` | sha256 of the three files | The GitHub release |

- **The IDL of record is `idls/raffles.json`.** It is committed, so a pull request that changes the interface shows it. Everything else derives from that file: both clients are generated from it and neither is written by hand, apart from the Rust client's addresses.
- **The IDL hash** is the sha256 of `raffles.json`'s bytes. `deployments.json` records it for every deployment.
- **The executable hash** is what `solana-verify get-executable-hash` prints for `raffles.so`. A cluster that runs the release reports the same from `solana-verify get-program-hash`.
- **A release never changes.** A fix is the next version.
- **Versions, while they start with 0:**

  | Change | Next version |
  | --- | --- |
  | The interface changes: an instruction, an account, an event, a type, an error or a constant | `0.Y+1.0` |
  | Anything else: the program's code, or only a doc comment the IDL carries | `0.Y.Z+1` |

  The IDL carries the version and the doc comments, so every release has its own IDL hash, and a consumer bumps for each. Once a version is released, any change to the IDL needs the next one, a doc comment included: `scripts/version.sh check` enforces that much, and the table is how to choose it.

## 2. Pinning a release

| Consumer | Pins | To move to another release | What fails on drift |
| --- | --- | --- | --- |
| The indexer, `crates/raffles` | The Rust client's tag | Change the tag in `Cargo.toml` | `cargo build --locked`: `Cargo.lock` names the tag's commit |
| raffles-services, `program` | The Rust client's tag | Change the tag in `Cargo.toml` | The same |
| The app | The TypeScript client's version | Change it in `package.json` | `pnpm install --frozen-lockfile`: the lockfile pins the tarball |
| Anything that vendors `raffles.json` | The drift check's tag | Change the tag, and take that release's file | The drift check |

### The Rust client

```toml
# The typed client.
raffles-client = { git = "https://github.com/piggygang/raffles-program", tag = "v0.1.0" }

# Or the IDL alone, without a single dependency.
raffles-client = { git = "https://github.com/piggygang/raffles-program", tag = "v0.1.0", default-features = false }
```

- **Without a feature:** `raffles_client::IDL`, the release's `raffles.json` as a string, and `VERSION`. A decoder that reads the IDL itself needs nothing more, and has no copy to drift.
- **With `anchor`, the default:**
  - `accounts`, `events` and `types`, with their Borsh layouts
  - `constants` and `error`
  - `client::args` and `client::accounts`, which build every instruction
  - `parsers::Account`, `parsers::Event` and `parsers::Instruction`, which read one from bytes
  - `pda`: every address of [account model](account-model.md) section 2, and the event authority
  - `read_event`: the event in an inner instruction's data ([events](events.md), section 1)
  - `anchor_lang`: the Anchor these types are built with, for `Pubkey` and its traits, so a consumer needs none of its own
- **It builds on Rust 1.90,** which CI checks. The indexer and raffles-services are there.
- **The typed client costs 131 crates:** `anchor-lang` 1.2.2 and the Solana 3 crates under it. The indexer has none of them today. The IDL alone costs none.

### The TypeScript client

```json
"@piggygang/raffles-client": "https://github.com/piggygang/raffles-program/releases/download/v0.1.0/raffles-client.tgz"
```

Once the package is on npm (section 6), the version alone does: `"@piggygang/raffles-client": "0.1.0"`.

- **What it has:** an instruction builder and parser for each instruction, a codec for each account, event and type, the errors with their messages, the constants, `RAFFLES_PROGRAM_ADDRESS`, `VERSION`, and `EVENT_TAG` with `getEventData` for event instructions. The IDL itself is `@piggygang/raffles-client/idl.json`.
- **It is built on `@solana/kit`,** a peer dependency, version 8.5 or later. It needs neither `@solana/web3.js` nor a `Buffer` polyfill.
- **ES modules only.**

### A vendored IDL, and the drift check

A repository that keeps its own copy of `raffles.json` checks it in CI:

```yaml
- uses: piggygang/raffles-program/.github/actions/idl-drift@v0.1.0
  with:
    idl: idls/raffles.json
```

- **The tag is the pin.** The action is this repository at that tag, so it compares the copy with the tag's `idls/raffles.json`. It fetches nothing.
- **It compares bytes.** A copy that a formatter rewrote fails, and the check says that this is all that differs: the IDL hash is of the bytes.
- **`cluster: devnet`** also reads `deployments.json` on `main`, and warns when devnet's last deployment has another IDL than the pinned release. `strict: true` makes that a failure. This is the other direction of drift: the copy is right, and the cluster has moved on.
- **Outside GitHub Actions:** `scripts/idl-drift.sh <copy> --release 0.1.0` fetches the release's `raffles.json` and compares.

### What the clients don't have yet

- **Accounts.** Anchor puts an account in the IDL once an instruction names it, so the clients gain each account with its instruction. `EntryReceipt` never arrives: receipts are passed as remaining accounts. Its layout is in the [account model](account-model.md), section 4.
- **Addresses in TypeScript.** Codama generates an address function for each account an instruction derives from seeds. It can't for a receipt, nor for a `PrizeEntry`, whose seeds are fields of the account. Those go into `clients/ts/scripts/generate.mjs` by hand, with Codama's `addPdasVisitor`, when `claim_entries` and the registry instructions land (PIG-84, PIG-140). The Rust client has them all in `pda`.

## 3. deployments.json

Which release each cluster runs, and every deployment before it.

```json
{
  "schema": 1,
  "clusters": {
    "devnet": {
      "program_id": "RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM",
      "deployments": [
        {
          "version": "0.1.0",
          "commit": "8d547cb2a1f0c3e4d5b6a798897a6b5c4d3e2f10",
          "slot": 412345678,
          "executable_hash": "5f0c…",
          "idl_sha256": "e423…"
        }
      ]
    },
    "mainnet-beta": { "program_id": null, "deployments": [] }
  }
}
```

| Field | Is |
| --- | --- |
| `program_id` | The program's address on the cluster, or `null` before it has one |
| `deployments` | Every deployment, oldest first. The last one is what the cluster runs. |
| `version` | The release, without the `v`. `null` for a build that isn't a release. |
| `commit` | The commit that was built. `null` when the tree had uncommitted changes. |
| `slot` | The slot the deployment landed in |
| `executable_hash` | What `solana-verify get-program-hash` printed after the deployment |
| `idl_sha256` | The IDL hash of what was deployed |

- **The first deployment's `slot` is where the program's history starts:** the `deploy_slot` of the indexer's `raffles.toml`, and the floor of its backfill. A later deployment's slot is where the interface may have changed, so a replay decodes each transaction with the release that was live at its slot.
- **`scripts/deploy-devnet.sh` writes it,** after it has checked the hash on-chain. It refuses to deploy what it couldn't record: without `--untagged`, `HEAD` has to be the release's tag, and the file has to pass its check as it is.
- **A `null` version says the cluster runs an interface no release describes.** A consumer can still compare `idl_sha256` with its pinned release's.
- **The file is committed to `main` after the deployment,** in a pull request. So the tag of a release never contains its own deployment: read the file from `main`.
- **The file only grows.** A deployment, once recorded, keeps its place. A checkout of an older tag has the file as it was at the tag, and appending to that would drop every deployment since, the first one's slot with them. So before it builds, the deploy script compares the file with `main`'s, stops when `main` has recorded more, and says how to take `main`'s file first.
- **CI checks it.** The file has the format above, devnet's `program_id` is `declare_id!`, slots only grow, each release it names is that tag's commit and IDL, and a pull request keeps every deployment its base has.

## 4. Cutting a release

1. **Set the version,** in the pull request that completes the release:

   ```sh
   scripts/version.sh set 0.2.0
   anchor build --ignore-keys
   cp target/idl/raffles.json idls/raffles.json
   ```

   The first command writes the version into the program's and both clients' manifests and lockfiles. The IDL takes it from the build.
2. **Tag the merged commit** on `main`, and push the tag: `v0.2.0`. A person does this.
3. **The release workflow** runs the whole gate on the tag, then publishes the artifacts of section 1 from what the gate built. A tag that doesn't match the version, or a commit that fails the gate, publishes nothing. A run that stopped halfway can be run again: it publishes the release, or finds it published and checks that its files are the same.
4. **Deploy it,** from the tag: `scripts/deploy-devnet.sh`. Commit the `deployments.json` it wrote, in a pull request to `main`.
5. **Each consumer bumps its pin.**

A release that is only deployed later, or never, is still a release: the tag and the deployment are separate steps.

## 5. What keeps it true

- **The committed IDL is the one the program builds.** `cargo test` compares `idls/raffles.json` with `target/idl/raffles.json`, which the build wrote. CI uploads the built IDL as the `idl` artifact before it runs the tests, so a failed run hands over the file to commit.
- **A released version's IDL is frozen.** `scripts/version.sh check` fails when the version's tag exists and `idls/raffles.json` no longer matches it.
- **One version.** The same script fails when the program, the IDL and the two clients disagree, and the release workflow when the tag isn't that version.
- **The Rust client is the program.** The program's `interface` tests check its program ID, its version, every address in `pda` against the program's own, and that it reads the events the program emits.
- **The TypeScript client reads the IDL as Borsh does.** Its tests make a value of every type in the IDL, encode it with a second implementation of Borsh, and require every generated codec to agree in both directions. They name nothing from the interface, so they hold as it grows.
- **The events document lists the IDL's events** ([events](events.md), section 8).
- **The drift check works.** CI runs the action on a copy of the IDL, which passes, and the script on a copy from another release, which has to fail with the status for drift.

```sh
cargo test -p raffles --test integration interface      # after anchor build
(cd clients/rust && cargo test --locked)
(cd clients/ts && npm ci --ignore-scripts && npm run build && npm test)
scripts/version.sh check && scripts/deployments.sh check
```

## 6. Switching npm on

The release workflow always attaches `raffles-client.tgz`, which installs by its URL. Publishing to the npm registry is off until someone with the accounts does this once:

1. Create the `piggygang` organization on npmjs.com: it owns the `@piggygang` scope. No package exists under it today.
2. Publish the first version by hand, from the release's tarball: `npm publish raffles-client.tgz --access public`. A trusted publisher can only be added to a package that exists.
3. In the package's settings, add a trusted publisher: GitHub Actions, organization `piggygang`, repository `raffles-program`, workflow `release.yml`, with `npm publish` among the allowed actions.
4. Set the repository variable `NPM_PUBLISH` to `true`.

From then on a tag publishes with a short-lived token from GitHub, and no npm token is stored anywhere.

Two settings on GitHub are worth making at the same time:

- A tag ruleset for `v*`, so that only maintainers create release tags and nobody moves one.
- `client-rust`, `client-ts` and `idl-drift` as required checks, next to `check` and `verifiable`.

## 7. What the later issues have to keep true

- **Every issue that changes the interface.** Copy the built IDL over `idls/raffles.json` in the same pull request. The clients need nothing: they follow the IDL.
- **The program's tests name nothing the Rust client generates,** such as an event's type. `anchor build` compiles the tests in order to write the IDL, so a test that needed the new IDL to compile could never get it. `interface.rs` uses what the client writes by hand, and what any IDL gives it.
- **`claim_entries` and the registry instructions (PIG-84, PIG-140).** Add the receipt's and the prize entry's address to the TypeScript generator (section 2).
- **A new kind of type in the IDL,** such as a struct with unnamed fields, fails the TypeScript tests until `clients/ts/tests/reference.mjs` covers it. Enums and aliases are covered: the draw brings ORAO's.
- **The first release and deployment (PIG-155).** Tag `v0.1.0`, deploy it, and commit `deployments.json`. The indexer's `config/devnet/raffles.toml` takes its `deploy_slot` from the first deployment.
- **The indexer (PIG-119), raffles-services (PIG-157) and the app (PIG-162).** Section 2. The indexer chooses between the typed client and reading `IDL` itself: the first costs 131 crates, the second a decoder.
- **Mainnet (PIG-67).** The README gives mainnet its own program ID. A release has one IDL, and its `address` is `declare_id!`, devnet's. The clients take another program address per call, but the Rust client's `pda` derives under `ID`. So PIG-67 decides between one ID for both clusters and a second build of each release. `deployments.json` already keeps a `program_id` per cluster, and `scripts/deployments.sh record --cluster mainnet-beta` records a mainnet deployment once it has one.

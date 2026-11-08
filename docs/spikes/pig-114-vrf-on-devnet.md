# PIG-114: VRF on devnet

|  |  |
| --- | --- |
| Issue | PIG-114, the Switchboard On-Demand half. The compute and account budget moved to PIG-133. |
| Date | 2026-11-08 |
| Switchboard On-Demand | **No-go.** The service shut down in September 2026. |
| ORAO VRF | **Go for devnet and the E2E milestone.** Mainnet needs the team to accept ORAO's trust model (section 2.5). |
| Still to run | The active measurement on devnet (section 6.3). It sends transactions, so a team member runs it and pastes the table into section 6.4. |

## Summary

- Switchboard On-Demand no longer works. Switchboard announced its shutdown on 18–19 September 2026, and support ended on 25 September. Every oracle on the devnet and mainnet randomness queues stopped heartbeating in early October. That is ADR-1's revisit trigger ("the on-demand service is deprecated"), so the draw moves to ORAO, ADR-1's named fallback, behind the provider seam.
- ORAO is healthy on devnet. All 247 requests in an 11-hour sample were fulfilled, at a median of 7 slots (about 3 s) after the request landed. The slowest took 131 slots (section 2.2).
- A request costs 0.00165 SOL on devnet and 0.00185 SOL on mainnet: ORAO's fee plus the rent the fulfilled request account keeps for good. The crank adds two transaction fees.
- The flow is simpler than commit–reveal. A request creates an account from a seed, ORAO's authorities fulfill it on their own, and anyone can read the result. There is no reveal step and no reveal window to miss.
- The catch is trust. ORAO's randomness is the XOR of its authorities' Ed25519 signatures of the seed, and on mainnet ORAO holds all three authority keys. An authority that wanted a particular result could produce a different valid signature, and the program can't tell.

## 1. Switchboard On-Demand: no-go

What we found on 2026-11-08:

- **The announcement.** Switchboard Technology Labs said it was shutting down, deprecated every implementation with immediate effect, ended support on 25 September 2026 and pointed price-feed users to Pyth and RedStone ([Solana Compass](https://solanacompass.com/news/switchboard-oracle-protocol-shuts-down-giving-solana-defi-six-days-to-migrate), [Crypto Briefing](https://cryptobriefing.com/switchboard-oracle-shuts-down-migration/), [KuCoin](https://www.kucoin.com/news/flash/oracle-protocol-switchboard-announces-shutdown-services-to-end-on-september-25)).
- **The chain.** We read the accounts directly.
  - The devnet randomness queue (`EYiAmGSdsQTuCw413V5BzaruWuCCSDgTPtBGvLkXHbe7`) lists 9 oracles. The most recent heartbeat among them was about 30 days old, around 9 October, and every oracle's enclave quote had expired.
  - The mainnet queue (`A43DyUGA7s8eXPxqEjJY6EBu1KKbNgfxF8h17VAHn13w`) looks the same: 12 oracles, the newest heartbeat about 33 days old.
  - The latest SDK (`@switchboard-xyz/on-demand` 3.10.6) commits only to an oracle with a fresh heartbeat and a valid quote, so it would find none.
- **The infrastructure.** `crossbar.switchboard.xyz` and `docs.switchboard.xyz` no longer resolve.

Two things we learned on the way matter only if another commit–reveal provider comes up:

- Switchboard's reveal needs the randomness account's authority to sign. To keep reveals permissionless, the authority has to be a PDA of our program, with reveal done by CPI.
- The oracle's reveal endpoint takes the caller's RPC URL, so a keyed URL must never go there.

## 2. ORAO VRF

### 2.1 How it works

ORAO VRF is the program `VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y`, at the same address on devnet and mainnet. We use its Rust crate `orao-solana-vrf` 0.8.1 with only the `cpi` feature, which supports Anchor 1.2 on the Solana 3 crates.

1. **Request.** `request_v2(seed)` creates the request account, a PDA of the seed, and charges ORAO's fee. Whoever pays becomes the request's *client*. A zero seed or a seed already used fails.
2. **Fulfill.** ORAO's fulfillment authorities each send a transaction with an Ed25519 signature of the seed, which the Ed25519 precompile checks. Once a quorum of them has signed, the request holds the XOR of their signatures as its 64-byte randomness. The quorum is a strict two-thirds majority:

   | Cluster | Authorities | Quorum |
   | --- | --- | --- |
   | devnet | 4 | 3 |
   | mainnet | 3 | 3 |

   Fulfilling shrinks the account to its final size and refunds the difference to the client.
3. **Consume.** Our program reads the request account, checks that the address derives from its seed and that ORAO owns it, and uses the randomness. Nothing else has to be sent.

The LiteSVM tests run the real devnet ORAO binary, with test keys installed as fulfillment authorities. They confirm the whole path: the quorum, the XOR, the refund to the client, and that a late fulfillment of a superseded request settles nothing.

### 2.2 Health on devnet

`vrf-driver observe` reads ORAO's recent devnet transactions, anyone's, and matches each request to its fulfillments. It sends nothing. It counts latency from the slot the request landed in to the slot of the fulfillment that completed the quorum.

| Sample | Requests | Fulfilled | Request → quorum, slots |
| --- | --- | --- | --- |
| 1,000 transactions, about 11 h, 2026-11-08 | 247 | 247 | p50 7, p90 9, p99 113, max 131 |

Every request completed with exactly three successful fulfillments, one per responding authority.

### 2.3 Cost

These figures use today's rent of 5,080 lamports per byte, including the 128-byte account overhead.

| Per request | devnet | mainnet |
| --- | --- | --- |
| ORAO fee | 300,000 | 500,000 |
| Rent the fulfilled request keeps (137 bytes, no instruction closes it) | 1,346,200 | 1,346,200 |
| **Total** | **1,646,200 (0.00165 SOL)** | **1,846,200 (0.00185 SOL)** |
| Also needed up front, refunded at fulfillment: rent of the pending request (749 bytes) beyond the kept 137 | 3,108,960 | 3,108,960 |

The crank pays two transaction fees per draw (request and fulfill), 5,000 lamports each at the base fee, plus any priority fee. ADR-1 estimated ORAO at "~0.001 SOL base + request-account rent". Today the base is half that on mainnet, and the rent dominates.

### 2.4 Compute

Measured on LiteSVM (Agave 4.2 runtime) for the spike's instructions. The real ones will do more.

| Instruction | Compute units |
| --- | --- |
| `request`: hash the seed, CPI `request_v2` paid by the vault | 22,669 |
| `request` adopting a request someone else made for the seed | 9,534 |
| `consume`: read the fulfilled request, pick the winner | 7,043 |

### 2.5 Failure modes and trust

- **Authority outage.** A request waits until a quorum of authorities answers. On mainnet that means all three, so one authority offline stalls every draw until it comes back. Re-requesting doesn't help, because a new request needs the same authorities. That is what spec E-D5's break-glass is for.
- **Bias.** Ed25519 doesn't force a signer to use deterministic nonces, so an authority can produce many valid signatures of the same seed, and the program can't tell which is canonical.
  - The authority that completes the quorum can see the earlier responses on chain. It could pick a signature that makes the XOR land on the winner it wants.
  - On mainnet, ORAO runs all three authorities, so a draw is fair only if ORAO and its keys are honest.
  - This is sharper than ADR-1's "trusting a smaller operator set". The team should accept it explicitly in the ADR-1 revision, or look for a source no operator can steer. A threshold beacon such as drand is one option; whether Solana can verify its signatures on-chain at a reasonable cost hasn't been checked.
- **Precomputation.** If the seed is known before entries close, ORAO's authorities could compute the outcome while entries are still open. Section 3 covers the mitigation.
- **Seed squatting.** If the seed is predictable, anyone can create the request account for it first. Without handling, our request would then fail, again and again. The spike adopts such a request instead: its randomness depends only on the seed, so the squatter gains nothing and only pays the fee.
- **Late fulfillment.** After a re-request, ORAO may still fulfill the old request. `consume` only reads the draw's latest seed, so the old result is ignored.

## 3. What this means for the program (PIG-83)

The spike program, `programs/vrf-spike`, already has the shape of the two draw instructions.

- **Who pays.** Each raffle gets a vault: a PDA without data, funded with the creator's draw fee at `create_raffle`. The program signs for the vault in the `request_v2` CPI, so `request_draw` is permissionless and the crank pays only its transaction fee, as spec §8 wants. Fulfillment refunds come back to the vault, and `close_raffle` returns what is left.
- **The draw fee.** Budget for the worst case, every attempt pending at once. A re-request happens because the previous request hasn't been fulfilled, so that request still holds its full pending rent.
  - Per attempt: ORAO's fee plus 4,455,160 lamports of pending rent.
  - Once: the vault's own rent-exempt minimum, 650,240 lamports today.

  At most three attempts on mainnet need 3 × (500,000 + 4,455,160) + 650,240 = 15,515,720 lamports (0.0155 SOL). Each fulfilled request refunds 3,108,960 to the vault, and `close_raffle` returns what is left. Rent per byte has already dropped once, from 6,960 to 5,080, so compute the fee from the Rent sysvar at create time.
- **The seed.** It has to be unique per raffle and attempt, and never zero. It also must not repeat for a later raffle at the same address, or that raffle would adopt the old one's fulfilled request. The spike uses `sha256("vrf-spike" ‖ draw ‖ attempt)` and never closes a draw, so the attempt count never restarts.
- **Entropy from after the close.** Mix in something that doesn't exist before entries close, so nobody can compute the outcome while entries are open. SlotHashes holds no timestamps, so the program pins the slot itself rather than taking one from the crank:
  1. The first `request_draw` at or after `end_ts` only records the current slot on the raffle. That slot's hash exists only once the slot ends, after entries closed.
  2. The next `request_draw`, at least a slot later, reads exactly that slot's hash from SlotHashes and mixes it into the seed. The crank never picks a slot, so it can't choose between hashes.
  3. SlotHashes keeps the last 512 slots, about 3.5 minutes. If the hash has aged out, `request_draw` records a new slot and starts over. Each such restart is visible on chain.
- **Adopting a squatted request.** If the request account already exists, check its address and owner and adopt it. The spike does this, and a test covers it.
- **`fulfill_draw`.**
  - Read the request as Anchor's `Account<RandomnessV2>` with `seeds::program` set to ORAO. That checks the owner, discriminator and address, so the program only has to require `fulfilled()`.
  - Then `winner_index = u64::from_le_bytes(randomness[..8]) % total_entries`, as spec §8 already says. The randomness is now 64 bytes, of which the first 8 are used.
- **Re-request (E-D4).** After the retry timeout, `request_draw` moves to the next attempt's seed, but only while the current request is still unfulfilled. Once it is fulfilled, the draw has to settle with that result, or a crank could discard a winner it doesn't like by re-requesting. `fulfill_draw` accepts only the latest seed. The slowest observed fulfillment took 131 slots, under a minute, so a retry timeout of a few minutes leaves ample margin. The spike skips the fulfillment check and re-requests whenever asked.
- **Compute limits.** About 23k CU for the spike's request and 7k for consume, before the real instructions' own work. Set the limits from measurements in PIG-83.
- **Dependencies.** `programs/raffles` still depends on `switchboard-on-demand`. Replace it with `orao-solana-vrf` (`default-features = false, features = ["cpi"]`). That also removes the `getrandom` workaround, which only Switchboard needed.

## 4. What this means for the crank (PIG-160)

- **The loop.** There's nothing to commit or reveal:
  1. Send `request_draw`.
  2. Wait for the request account to show fulfilled. Poll it, or subscribe to ORAO's `Fulfill` event.
  3. Send `fulfill_draw`.

  On devnet, step 2 took a median of 7 slots.
- **The missed reveal window is gone** (architecture R7, §3c step 4). In its place:
  - an authority outage, which needs a timeout and an alert
  - re-requests, which spend part of the draw fee each time
- **Cost to the crank.** Two transaction fees per draw. The fee and the rent come from the raffle's vault.
- **`spikes/vrf-driver` is the seed** of the VRF driver. It already sizes the compute budget by simulating, re-signs when a blockhash expires, and treats a request someone else made as success.

## 5. Docs and issues to update

These belong to piggygang/raffles and Linear, so they're proposals, not edits:

- **ADR-1** flips to ORAO. Its table needs today's numbers: cost 0.00185 SOL per request on mainnet, model Ed25519 multi-authority with a 3-of-3 quorum, trust in ORAO's three keys.
- **Spec D3, §8 and R14:** provider, randomness size and draw-fee sizing.
- **Architecture §3c step 4 and R7:** no reveal window. **R5:** resolved, with Switchboard unavailable and ORAO healthy on devnet.
- **PIG-83:** "request_draw + Switchboard commit" becomes the ORAO request, with section 3 as the design notes.
- **PIG-160:** the Switchboard commit/reveal driver becomes the ORAO request/fulfill driver (section 4).
- **PIG-84 and the eligible-entries story:** their numbers come from PIG-133, not from this spike.

## 6. Rerun it

### 6.1 Tests

```sh
anchor build --ignore-keys
cargo test -p vrf-spike
```

They run against `programs/vrf-spike/tests/fixtures`, the devnet ORAO program and its network state, which `scripts/dump-vrf-spike-fixtures.sh` refreshes.

### 6.2 Passive observation

This only reads, from any cluster, and needs no wallet or deployment:

```sh
cargo run --release --manifest-path spikes/vrf-driver/Cargo.toml -- observe --txs 1000
```

The public devnet RPC rate-limits it. With `RAFFLES_DEVNET_RPC_URL` set to a Helius devnet URL, it is much faster. Pass `--url` with a mainnet URL to see mainnet.

### 6.3 Active measurement

This sends devnet transactions. It deploys the spike program once, then runs 50 request → fulfill → consume cycles through it and records each:

```sh
# The wallet for every step. The driver doesn't read `solana config`, so pass
# it explicitly: the draw belongs to the wallet that runs it.
wallet=~/.config/solana/id.json

# 1. Deploy. Needs about 1.8 SOL while deploying; about 0.89 SOL stays as program rent.
anchor build --ignore-keys
solana program deploy target/deploy/vrf_spike.so \
  --program-id ~/.config/solana/piggygang/vrf-spike-program-devnet.json \
  --keypair "$wallet" -u devnet

# 2. Measure. Funds the draw's vault with about 0.09 SOL and writes a JSON line per cycle.
export RAFFLES_DEVNET_RPC_URL=<Helius devnet URL>
cargo run --release --manifest-path spikes/vrf-driver/Cargo.toml -- \
  run --runs 50 --out docs/spikes/pig-114-runs.jsonl --keypair "$wallet"

# 3. Return the vault's remainder.
cargo run --release --manifest-path spikes/vrf-driver/Cargo.toml -- \
  withdraw --keypair "$wallet"
```

- The program keypair lives outside the repo (README, "Keys"). `$wallet` pays and becomes the upgrade authority.
- The draw stays after `withdraw`, so a later `run` with the same wallet carries on with new seeds.
- `status` shows the draw and the vault at any point.
- `solana program close VRFsDurdc8NMVFAS5G8XVGzGjnBkRm1Rv7ZiEckYUCw -u devnet --bypass-warning` returns the program rent once the spike is over.

### 6.4 Results of the active run

Not run yet. Paste the table `vrf-driver run` prints here, and commit `pig-114-runs.jsonl` next to it.

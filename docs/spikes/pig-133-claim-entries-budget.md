# PIG-133: claim_entries compute and transaction budget

|  |  |
| --- | --- |
| Issue | PIG-133, split from PIG-114 |
| Date | 2026-11-09 |
| Proofs per transaction | **8 Token Metadata pigs or 12 Core pigs.** A mix fits while 100 bytes per Token Metadata pig plus 67 per Core pig stays within 866 bytes. Transaction size decides. Compute stays under 110,000 units. |
| Compute unit limit | **Simulate, and request the result plus 10%.** Without a simulation, 150,000 covers every batch that fits. |
| Lookup table | **No.** Only the config and the system program can move into a table, plus the raffle with a table per raffle. Neither table saves a transaction on any claim the cap allows. |
| Mint account | **Drop it.** The rule never reads the mint. Without it a Token Metadata pig needs 100 bytes instead of 133, so 8 fit instead of 6. |

## Summary

- **Transaction size decides the batch, not compute.**
  - A claim transaction needs 366 bytes before its first pig. Each Token Metadata pig adds 100 bytes and each Core pig 67, so 8 Token Metadata or 12 Core pigs fit in 1,232 bytes.
  - The largest batch that fits needs at most 108,599 compute units, under 8% of the 1.4 million a transaction can have. Account locks are no limit either: 31 accounts at most, of 64.
- **R9's ~6 per transaction came from passing each mint.** The membership rule doesn't read the mint (holder-proof §4), and leaving it out takes a Token Metadata pig from 133 bytes to 100.
- **A lookup table doesn't pay.**
  - Only two of a claim's accounts can come from a table that every claim shares: the config and the system program. That saves 26 bytes, enough for a 13th Core pig, which the cap of 10 never needs.
  - A table per raffle also holds the raffle and saves 57 bytes, enough for a 9th Token Metadata pig. That saves a transaction only on a claim of exactly 9 Token Metadata pigs, and it costs a table to create, fund and close for every raffle.
- **The receipt is the cost, not the proof.**
  - Of a Token Metadata pig's ~7,300 units, the receipt takes ~5,700: finding its bump, creating it and writing it.
  - The Borsh decode that holder-proof §7 asked about, together with the token account, takes ~1,600. A Core pig's proof takes ~200.
- **Two design fixes for PIG-84 and PIG-86:**
  - Give receipts and positions their own seed prefixes.
  - Create receipts with `create_account_allow_prefund`, so lamports sent to a receipt address can't block a pig.
- **Rent:** 0.0012 SOL per pig plus 0.0013 SOL for the wallet's position in the raffle, fronted at claim time and refunded after settlement.

## 1. What was measured

`programs/claim-spike` is `claim_entries` shaped as PIG-84 will build it, with nothing left out that costs compute or bytes:

- **Per Token Metadata pig:**
  - The token account: classic SPL Token, owned by the signer, amount 1.
  - The metadata account: owned by Token Metadata, key `MetadataV1`, and its `mint` equals the token account's.
  - The rule: update authority, symbol with the NUL padding stripped, and a verified creator. This follows holder-proof §4 exactly. The metadata is decoded through `creators` only, with its strings kept as bytes.
- **Per Core pig:** owned by mpl-core, key `AssetV1`, the signer as owner, and update authority `Collection(rule address)`, all at fixed offsets.
- **Per pig:** a new `EntryReceipt` at `[b"receipt", raffle, nft]` (105 bytes), paid by the wallet.
- **Per claim:**
  - The pause flag from `Config`, and the raffle's state, `end_ts`, creator and snapshotted rules from the `Raffle`.
  - The wallet's `EntryPosition` (132 bytes), opened by its first claim in the raffle.
  - The cap, with spec E-E16's exception at the cap.
  - The Full Gang bonus, and one entry range per claim.
- **Instruction data:** one `rule_index` byte per pig (holder-proof §5).

`Config` and `Raffle` carry every field spec §7 lists, so their decode costs what PIG-86's will. The tests check what the prototype accepts and rejects, including every negative fixture in holder-proof §7. The numbers below therefore come from a claim that does the real work.

The transactions are what a client sends:

- one signature, the wallet's, which also pays the fee
- `SetComputeUnitLimit` and `SetComputeUnitPrice`
- the claim

They run on LiteSVM 0.16: the Agave 4.2 runtime with mainnet's feature set as of 2026-08-24. The program is built as SBPF v3, as the deploys are.

The pigs are real. The fixtures are a live SOL Gang metadata account, a Girl Gang one (without `token_standard`) and a Piggy Gang Core asset (166 bytes, the largest of the 745), copied from mainnet. Each test pig is a copy with its own mint, or its own asset address and owner.

Addresses come from fixed seeds, so every number repeats exactly. The compute tables run each batch at 8 seeds, because a pig's cost depends on where its receipt falls (section 4.3).

## 2. Transaction size: what decides the batch

Bytes and accounts per legacy claim transaction (✗: over 1,232 bytes or 64 accounts). Token Metadata alternates SOL Gang and Girl Gang; mixed takes SOL Gang, Girl Gang and Piggy Gang in turn.

| Pigs | Token Metadata | Core | Mixed | Token Metadata, each with its mint |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 466 · 10 | 433 · 9 | 466 · 10 | 499 · 11 |
| 2 | 566 · 13 | 500 · 11 | 566 · 13 | 632 · 15 |
| 3 | 666 · 16 | 567 · 13 | 633 · 15 | 765 · 19 |
| 4 | 766 · 19 | 634 · 15 | 733 · 18 | 898 · 23 |
| 5 | 866 · 22 | 701 · 17 | 833 · 21 | 1,031 · 27 |
| 6 | 966 · 25 | 768 · 19 | 900 · 23 | 1,164 · 31 |
| 7 | 1,066 · 28 | 835 · 21 | 1,000 · 26 | 1,297 · 35 ✗ |
| 8 | 1,166 · 31 | 902 · 23 | 1,100 · 29 | 1,430 · 39 ✗ |
| 9 | 1,266 · 34 ✗ | 969 · 25 | 1,167 · 31 | 1,563 · 43 ✗ |
| 10 | 1,366 · 37 ✗ | 1,036 · 27 | 1,267 · 34 ✗ | 1,696 · 47 ✗ |
| 11 | 1,466 · 40 ✗ | 1,103 · 29 | 1,367 · 37 ✗ | 1,829 · 51 ✗ |
| 12 | 1,566 · 43 ✗ | 1,170 · 31 | 1,434 · 39 ✗ | 1,962 · 55 ✗ |
| 13 | 1,666 · 46 ✗ | 1,237 · 33 ✗ | 1,534 · 42 ✗ | 2,095 · 59 ✗ |

**The rule:** `366 + 100 × Token Metadata pigs + 67 × Core pigs ≤ 1,232`.

- **The 366 bytes:**
  - the signature (65)
  - the header (3)
  - the seven accounts every claim names, each 32 bytes plus its count (225): wallet, config, raffle, position, system program, the spike program and the compute budget program
  - the blockhash (32)
  - the instruction count and the two compute budget instructions (21)
  - the claim's own overhead (20)
- **A Token Metadata pig:** the token account, the metadata and the receipt (96), their indexes (3) and the rule index (1).
- **A Core pig:** the asset and the receipt (64), their indexes (2) and the rule index (1).
- **The mint, if passed:** 33 more bytes per Token Metadata pig.

Account locks never bind: 8 Token Metadata pigs lock 31 accounts, of 64.

## 3. Lookup tables: no

A lookup table can't hold the signer, or the programs the transaction invokes (the spike program and the compute budget program). The claim names neither Token Metadata nor mpl-core: it only checks who owns each account. The membership rules live inside `Config` and the `Raffle`, so there are no separate rule accounts either. The per-pig accounts belong to each wallet and change with every claim. What's left:

| Table | Holds | Saves | Token Metadata | Core | Mixed |
| --- | --- | ---: | ---: | ---: | ---: |
| None (legacy) | | | 8 | 12 | 9 |
| Shared by every claim | config, system program | 26 B | 8 | 13 | 9 |
| One per raffle | config, system program, raffle | 57 B | 9 | 13 | 10 |

A v0 transaction through the table costs exactly the same compute units (section 5).

**Why not:**

- **The cap already keeps every claim within two transactions.** A wallet enters at most 12 pigs per raffle: 10 under the cap plus up to two completing the Full Gang set. A legacy transaction holds 8, so no claim needs a third, with or without a table.
- **A shared table gains nothing the cap allows.** Its only gain is a 13th Core pig.
- **A per-raffle table saves a transaction only on a claim of exactly 9 Token Metadata pigs** (or 10 mixed), and it costs work every raffle:
  - Create and extend it before the raffle opens. It's usable from the next slot.
  - Fund its rent, 1,422,400 lamports for three addresses.
  - Deactivate and close it after settlement. Closing has to wait 512 slots, until the deactivation slot leaves SlotHashes.
  - Every client has to read the table and send v0.

Revisit this if the per-pig account list grows (for example, if the listing policy, E-E4, needs an account per pig), if a second signer joins (96 bytes more), or if the cap rises.

## 4. Compute units

### 4.1 Per batch

A wallet's first claim in a raffle (it opens the position, the dearest case). Median and maximum over 8 address seeds; ✗ marks batches too large to send.

| Pigs | Token Metadata | Core | Mixed |
| ---: | ---: | ---: | ---: |
| 1 | 25,129 · 32,629 | 23,500 · 31,000 | 25,129 · 32,629 |
| 2 | 31,385 · 40,385 | 28,127 · 37,127 | 31,385 · 40,385 |
| 3 | 39,116 · 46,616 | 34,229 · 41,729 | 37,495 · 44,995 |
| 4 | 45,347 · 55,847 | 38,831 · 49,331 | 43,726 · 54,226 |
| 5 | 59,078 · 63,578 | 50,933 · 55,433 | 57,457 · 61,957 |
| 6 | 66,809 · 77,309 | 57,035 · 67,535 | 63,559 · 74,059 |
| 7 | 73,040 · 86,540 | 61,637 · 75,137 | 69,790 · 83,290 |
| 8 | 79,271 · 98,771 | 66,239 · 85,739 | 76,021 · 95,521 |
| 9 | 86,954 · 104,954 ✗ | 72,293 · 90,293 | 82,075 · 100,075 |
| 10 | 94,685 · 111,185 ✗ | 78,395 · 94,895 | 89,806 · 106,306 ✗ |
| 11 | 102,416 · 120,416 ✗ | 84,497 · 102,497 | 97,537 · 115,537 ✗ |
| 12 | 113,147 · 128,147 ✗ | 93,599 · 108,599 | 106,639 · 121,639 ✗ |

About 8,000 units per Token Metadata pig and 6,300 per Core pig (median), on top of about 17,000 per claim. The dearest batch that fits took 108,599 units: 12 Core pigs at the worst seed.

### 4.2 Where the units go

From the traced variant, which logs the units left at each step (seed 0; each step includes one ~100-unit log call):

| Step | 8 Token Metadata | 12 Core |
| --- | ---: | ---: |
| Before the pigs: compute budget, entry, accounts, opening the position | 18,227 | 18,203 |
| Proof, per pig | 1,569 | 213 |
| Receipt, per pig: bump search, create, write | 5,726 | 5,539 |
| Counting the entries | 472 | 472 |
| After: accounts written back | 4,013 | 4,013 |
| **Total** | **81,077** | **91,721** |

The proof step unpacks the token account and decodes the 607- or 679-byte metadata account through `creators`. Together that's ~1,600 of a pig's ~7,300 units, so even a free decoder would barely move the total.

### 4.3 The receipt's bump search

A receipt has to be found with `find_program_address`. The program must use the canonical bump: with a bump from the client, a second bump would admit the same pig twice. Each try costs 1,500 units, and half of all addresses need more than one:

| Tries | Share of pigs needing at least that many | Extra units |
| ---: | ---: | ---: |
| 2 | 49.4% | 1,500 |
| 3 | 24.9% | 3,000 |
| 4 | 12.5% | 4,500 |
| 6 | 3.1% | 7,500 |
| 8 | 0.75% | 10,500 |
| 10 | 0.17% | 13,500 |
| 12 | 0.04% | 16,500 |

That's the spread between the median and maximum columns above. A pig's tries are fixed for a given raffle, so a simulation of the claim sees them exactly.

### 4.4 The limit to request

- **Simulate the claim and request the units it used plus 10%.** The simulation runs the same pigs at the same addresses, so it sees every bump search. The tightest limit keeps the priority fee and the block space the transaction reserves small.
- **Without a simulation, 150,000.**
  - That's about 40,000 above the dearest batch measured (108,599).
  - It holds even when the bump searches are unlucky. Take 12 Core pigs: the position and receipts make 13 searches, averaging 2 tries each.
    - In 1 batch in 10,000 they take 26 or more extra tries. That's 39,000 units over the median, about 132,600 in all.
    - In 1 in 100,000 the batch reaches about 141,600.
    - These are the exact negative-binomial tails.
  - At a priority fee of 10,000 micro-lamports per unit, 150,000 units cost 1,500 lamports, under a third of the signature fee.
- **Re-measure in PIG-84.** Events, or a listing check (E-E4), add units. The `budget` tests port as they are.

A transaction without a compute budget instruction gets 200,000 units, which would also cover every batch. The claim still sets a limit, because the priority fee is charged on it.

## 5. Variants and cases

Each pair runs the same pigs at the same addresses, so the difference is exact (median over 8 seeds):

| Case | Canonical | Variant | Difference |
| --- | ---: | ---: | ---: |
| 8 Token Metadata, decoded whole with mpl-token-metadata's `Metadata` | 79,271 | 85,593 | +6,322 (+790 per pig) |
| 6 Token Metadata, each with its mint | 66,809 | 68,539 | +1,730 (+288 per pig) |
| 8 Token Metadata, v0 through the per-raffle table | 79,271 | 79,271 | +0 |
| 8 Token Metadata, position already open | 79,271 | 76,898 | −2,373 |
| 8 Token Metadata, every receipt address funded first | 79,271 | 79,271 | +0 |

- **The full decode works on every pig layout too.** Its 790 extra units per pig don't change any limit. The prefix decode is still the better default: it reads nothing past `creators`, so the padding, a missing `token_standard` and the fields Token Metadata adds later can't affect it.
- **The mint costs 288 units per pig, and 33 bytes.** The bytes are what matter: 6 pigs per transaction instead of 8.
- **Opening the position costs about 2,400 units,** once per wallet per raffle.
- **Prefund-tolerant receipts cost nothing extra** (section 6).

## 6. What this means for `claim_entries` (PIG-84) and the account model (PIG-86)

- **Accounts:**
  - The wallet (signer, payer), `Config`, the `Raffle` (writable), the wallet's `EntryPosition` (opened if needed) and the system program.
  - Then, per pig in `rule_index` order:
    - a Token Metadata pig's token account and metadata, or a Core pig's asset
    - its new receipt
  - No mint account.
- **Data:** `rule_indexes: Vec<u8>`, one byte per pig.
- **The metadata address needs no re-derivation.** Token Metadata's ownership, the `MetadataV1` key and the `mint` field bind it to the mint (holder-proof §4, step 2). Re-deriving would add a bump search per pig for nothing.
- **Receipts:**
  - Find the bump with `find_program_address` (section 4.3).
  - Create with `create_account_allow_prefund`. This is SIMD-0312, active on mainnet since epoch 979 and on devnet since epoch 1052.
  - With plain `create_account`, anyone who sends 650,240 lamports (an empty account's rent-exempt minimum) to a pig's receipt address blocks that pig from the raffle for good. With the prefund variant the claim tops the balance up instead. The test `a_prefunded_receipt_address_still_takes_the_receipt` covers it, at no extra cost (section 5).
- **Seeds need prefixes.**
  - The spec puts receipts at `[raffle, nft_mint]` and positions at `[raffle, wallet]`: one address space. A wallet whose address is also an NFT's would collide with that NFT's receipt. A Core asset's address is an ordinary keypair's, so whoever holds that key would collide.
  - The spike uses `[b"receipt", raffle, nft]` and `[b"position", raffle, wallet]`.
- **Sizes:**
  - **Receipt:** 105 bytes, 1,183,640 lamports: raffle, NFT, wallet and bump. Its raffle and NFT are also its seeds. Dropping them (41 bytes) would save 325,120 lamports per pig, but then `close_entry_accounts` has to be passed them.
  - **Position:** 132 bytes, 1,320,800 lamports, with room for 11 entry ranges: ten single-entry claims under cap 10, and one that adds only the bonus.
  - **Total:** a full cap-10 claim fronts 13,157,200 lamports (0.0132 SOL), refunded after settlement.
- **At the cap,** the spike accepts a pig only if it adds a collection, for no entry (E-E16). It doesn't require that the claim complete the set; the app sends completing pigs only as a full set (spec §6). PIG-84 decides whether the program enforces the full set too. Either way the compute is the same.
- **`Account<Raffle>` and `Account<Config>`, Borsh-decoded,** take part of the ~18,000 units before the pigs and the ~4,000 after. Zero-copy could trim that, but compute isn't the limit, so Borsh is fine.

## 7. What this means for the eligible-entries endpoint (PIG-76) and the app

- **The response.**
  - Per pig, return the rule index plus either the token account and metadata, or the asset. That's a pair or a single, not a triple; no mint.
  - The client derives the receipt address. The token accounts still have to reach the indexer (PIG-135).
- **Chunking:**
  - Fill each transaction while `100 × Token Metadata + 67 × Core ≤ 866`. That's up to 8 pigs of any mix, and up to 12 when enough of them are Core.
  - Completing pigs go together in the last chunk (E-E7). There's always room for them: a chunk with two completing pigs has at least 666 bytes free.
- **How many transactions:**
  - **One:** any claim of up to 8 pigs, and up to 12 when the byte rule allows (10 Core pigs, for example).
  - **Two:** everything else. No claim the cap allows needs three.
- **The transaction:** legacy, with `SetComputeUnitLimit` (section 4.4) and `SetComputeUnitPrice` before the claim.

## 8. Docs and issues to update

These belong to piggygang/raffles and Linear, so they're proposals, not edits:

- **Architecture R9:** resolved.
  - 8 Token Metadata or 12 Core pigs per transaction, mixes by `366 + 100 × TM + 67 × Core ≤ 1,232`.
  - Transaction size binds, not compute.
  - No lookup table.
- **Architecture §3 step 4:**
  - "~6 NFT proofs per tx; a cap-10 claim is normally two" becomes "up to 8 pigs per transaction, 12 when Core; a claim takes at most two".
  - "pre-chunked account triples" becomes pairs or singles.
- **Spec §5, Batching:**
  - "~6 NFTs (mint + token account + metadata per NFT within account limits)" becomes the numbers above, without the mint.
  - "A cap-10 claim is normally two transactions" becomes "one for up to 8 pigs, two at most".
  - Spec §6's "account triples" changes the same way.
- **Spec §7:** prefixed seeds for `EntryReceipt` and `EntryPosition` (PIG-86).
- **Holder-proof §7, PIG-133 row:**
  - The mint leaves the per-pig account list.
  - The decode through `creators` is measured: under ~1,600 units together with the token account.
- **PIG-84:** section 6, and a compute unit limit from simulation plus 10% (fallback 150,000).
- **PIG-86:** prefixed seeds, receipt and position sizes, 11 entry ranges.
- **PIG-76:** section 7's response shape and chunking rule.
- **The entry-flow story:**
  - One signature for up to 8 pigs, two at most.
  - About 0.0012 SOL rent per pig plus 0.0013 SOL per raffle, fronted and refunded.

## 9. Rerun it

```sh
anchor build --ignore-keys
cargo test -p claim-spike
# The tables above:
cargo test -p claim-spike --test integration budget -- --nocapture --test-threads 1
```

The `budget` tests assert the byte costs, the batch limits and the compute ceiling, so a change that moves one fails there. `scripts/dump-claim-spike-fixtures.sh` refreshes the three mainnet pig accounts in `programs/claim-spike/tests/fixtures` (it needs network access).

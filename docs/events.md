# Events

|  |  |
| --- | --- |
| Issue | PIG-117 |
| Date | 2026-11-15 |
| Code | `programs/raffles/src/events.rs` |
| Status | The events, their layouts and how they are emitted. The four config and authority events are emitted ([instructions](instructions.md)). Every other one lands with its instruction's issue. |
| Built on | Architecture 1.2 §3b and §9, registries §4 and §8, rules §3, prize escrow §7, and the [account model](account-model.md) |

## Summary

- **Seventeen events.** Eight record a change to the config, the authority or a registry. Nine follow a raffle from create to close (section 2).
- **An event is an inner instruction, never a log line.** The program calls itself with the event as the data (section 1). Logs can be cut off, and any program can write one; neither is true of an inner instruction.
- **An event has two kinds of field.** A field named like an account field holds that field's value once the instruction has run: a reader keeps the value of the event that is later on the chain. Every other field says what that one instruction did, such as the range and the NFTs of a claim: a reader keeps each event's. An event is applied once (section 5).
- **`RaffleCreated` is a raffle's permanent record.** It has everything about the raffle that never changes, with the title, the description, `rules_uri` and `rules_hash`. So a raffle's history can be rebuilt after `close_raffle` has removed the account (architecture §9, flag 1).
- **The events add no enum.** Kinds, states and reasons are the account model's bytes (section 4).
- **The tests hold the tables here to the code.** A field, an offset, a size or a discriminator can't differ between the two (section 8).
- **Where the design documents left a choice, section 6 records it,** and section 7 lists what each later issue has to keep true.

## 1. How an event reaches a reader

The program emits events with Anchor's `emit_cpi!`: it invokes itself, and the event is that inner instruction's data.

**What a reader looks for,** in `meta.innerInstructions` of a transaction that succeeded:

| Part of the inner instruction | Is |
| --- | --- |
| Program | The raffles program, `RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM` |
| Accounts | One: the event authority, `3f6dc3nyP4ZooeA8GZcwSp1XnCHenMUR6XJjieozXaBC`, as a signer |
| Data, bytes 0 to 7 | `e445a52e51cb9a1d`: Anchor's tag for an event instruction |
| Data, from byte 8 | The event: its 8-byte discriminator, then its fields (section 3) |

- **The event authority** is the program's address at `[b"__event_authority"]`. Only the program can sign for it, and the program rejects a tagged instruction that it didn't sign. So a tagged instruction in a successful transaction is an event the program emitted.
- **Skip failed transactions.** They keep their inner instructions, and nothing in them happened.
- **An event sits right under the instruction that emitted it.** That instruction is top-level when a wallet sends it, and itself an inner instruction when a Squads vault does (registries §8). A reader therefore walks every inner instruction, at any depth.
- **Order.** Number a transaction's events as its instructions ran: each top-level instruction, then its inner instructions in order. That is architecture §3b's `event_index`. A transaction can hold several events, for example a Squads bundle or several closes.
- **Each successful instruction emits exactly one event,** after it has done its work.
- **Nothing is logged.** The program never uses `emit!`, so `logMessages` holds no event. A log line that looks like one comes from somewhere else.
- **Cost.** An instruction that emits names two more accounts, the event authority and the program. That is 34 bytes of a legacy transaction, and the self-call costs compute. Section 7 has what it means for `claim_entries`.

## 2. The events

| Event | Discriminator | Emitted by | The raffle is then |
| --- | --- | --- | --- |
| `ConfigInitialized` | `b531c89c13a7b25b` | `init_config` | |
| `ConfigUpdated` | `28f1e67a0b13c6c2` | `update_config` | |
| `AuthorityProposed` | `f4755e7035972359` | `set_authority` | |
| `AuthorityAccepted` | `a6c0dbbc29d1c31a` | `accept_authority` | |
| `CreatorAdded` | `5fd66b58091f6242` | `creator_add` | |
| `CreatorRemoved` | `5b3daf217ee1cc4c` | `creator_remove` | |
| `PrizeAllowlisted` | `db2ce32f2bec8e45` | `allowlist_add` | |
| `PrizeDelisted` | `e4fe5bbc5aa743c8` | `allowlist_remove` | |
| `RaffleCreated` | `b2acc960e9ab0663` | `create_raffle` | Active |
| `EntriesClaimed` | `d853894fdc70778e` | `claim_entries` | Active |
| `DrawRequested` | `3bacdb2a1c5e5d4b` | `request_draw`, the first request and every later one | Drawing |
| `WinnerDrawn` | `d5670576914b9278` | `fulfill_draw` and finalize | Drawn |
| `PrizeClaimed` | `d596c04cc721d426` | `claim_prize` | Settled |
| `PrizeReclaimed` | `bb51f4223f007ad8` | `reclaim_prize` | Settled, or still Cancelled: its `state` says which |
| `RaffleCancelled` | `7b53fe7f35f49f66` | `cancel_raffle`, the break-glass cancel included | Cancelled |
| `EntryAccountsClosed` | `99bd445f47abae2e` | `close_entry_accounts` | As before: Settled, Cancelled, or Drawn past the backstop |
| `RaffleClosed` | `942fccf61b6f7631` | `close_raffle` | Gone: the account is closed |

A discriminator is the first 8 bytes of `sha256("event:<name>")`, so it never changes while the name doesn't.

## 3. Layouts

Borsh, little-endian, like the accounts. Offsets count from the start of the event, its discriminator. In the inner instruction's data the event starts at byte 8, after the tag.

- **A raffle's event starts with `raffle`,** at offset 8. The config and registry events name no raffle.
- **Fixed-size fields come before variable-length ones,** so every fixed field has one offset.
- **A field named like an account field** carries that field's value once the instruction has run.
- **`Pubkey`** is 32 bytes and **`bool`** one byte, 0 or 1.
- **`String`** is a `u32` length, then that many bytes of UTF-8. **`Vec<Pubkey>`** is a `u32` count, then that many keys.
- **`MembershipRule`** (75 bytes) and **`EntryRange`** (5 bytes) are the account model's records ([account model](account-model.md), section 4).
- **The total** of an event with a string or a list is the least it can be: with all of them empty.

### ConfigInitialized

`init_config` created `Config`. The fields after `authority` are the settings it starts with.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `b531c89c13a7b25b` |
| 8 | `authority` | `Pubkey` | 32 | The signer: the program's upgrade authority |
| 40 | `paused` | `bool` | 1 | |
| 41 | `membership_rules` | `[MembershipRule; 3]` | 225 | |
| 266 | `entry_cap` | `u8` | 1 | |
| 267 | `full_gang_bonus` | `u8` | 1 | |
| 268 | `min_duration` | `u32` | 4 | Seconds |
| 272 | `max_duration` | `u32` | 4 | Seconds |
| | **total** | | **276** | |

### ConfigUpdated

`update_config` changed a setting, the pause flag included. The event carries every setting, changed or not: what differs from the config event before it is the change. An update that changed nothing emits it too.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `28f1e67a0b13c6c2` |
| 8 | `authority` | `Pubkey` | 32 | The signer. This event never changes it. |
| 40 | `paused` | `bool` | 1 | |
| 41 | `membership_rules` | `[MembershipRule; 3]` | 225 | They reach raffles created from here on (spec D14) |
| 266 | `entry_cap` | `u8` | 1 | |
| 267 | `full_gang_bonus` | `u8` | 1 | |
| 268 | `min_duration` | `u32` | 4 | Seconds |
| 272 | `max_duration` | `u32` | 4 | Seconds |
| | **total** | | **276** | |

### AuthorityProposed

`set_authority` named the authority's successor, or withdrew one. `authority` stays in charge either way.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `f4755e7035972359` |
| 8 | `authority` | `Pubkey` | 32 | The signer |
| 40 | `pending_authority` | `Pubkey` | 32 | Zero when the proposal was withdrawn |
| | **total** | | **72** | |

### AuthorityAccepted

`accept_authority` completed the transfer.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `a6c0dbbc29d1c31a` |
| 8 | `old_authority` | `Pubkey` | 32 | |
| 40 | `new_authority` | `Pubkey` | 32 | The signer, and `Config.authority` from here on |
| | **total** | | **72** | |

### CreatorAdded

`creator_add` created the wallet's `CreatorEntry`.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `5fd66b58091f6242` |
| 8 | `wallet` | `Pubkey` | 32 | |
| 40 | `label` | `[u8; 32]` | 32 | UTF-8 padded with zero bytes, as the entry stores it |
| | **total** | | **72** | |

### CreatorRemoved

`creator_remove` closed the wallet's `CreatorEntry`.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `5b3daf217ee1cc4c` |
| 8 | `wallet` | `Pubkey` | 32 | |
| | **total** | | **40** | |

### PrizeAllowlisted

`allowlist_add` created the `PrizeEntry` for `kind` and `address`.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `db2ce32f2bec8e45` |
| 8 | `kind` | `u8` | 1 | A `PrizeKind` |
| 9 | `address` | `Pubkey` | 32 | What the entry admits, read according to `kind` |
| | **total** | | **41** | |

### PrizeDelisted

`allowlist_remove` closed the `PrizeEntry` for `kind` and `address`.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `e4fe5bbc5aa743c8` |
| 8 | `kind` | `u8` | 1 | A `PrizeKind` |
| 9 | `address` | `Pubkey` | 32 | |
| | **total** | | **41** | |

### RaffleCreated

`create_raffle` created the raffle and escrowed its prize. The event is everything about the raffle that never changes.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `b2acc960e9ab0663` |
| 8 | `raffle` | `Pubkey` | 32 | |
| 40 | `creator` | `Pubkey` | 32 | |
| 72 | `nonce` | `u64` | 8 | With `creator`, the seeds of `raffle` |
| 80 | `created_by_authority` | `bool` | 1 | The creator signed as `Config.authority`, which needs no `CreatorEntry`. Otherwise its entry admitted it (registries §5). |
| 81 | `escrow_kind` | `u8` | 1 | An `EscrowKind` |
| 82 | `prize_mint` | `Pubkey` | 32 | A classic SPL Token mint, or the Core asset itself |
| 114 | `prize_amount` | `u64` | 8 | Base units escrowed: 1 for an NFT or a Core asset |
| 122 | `allowlist_kind` | `u8` | 1 | The `PrizeKind` of the `PrizeEntry` that admitted the prize |
| 123 | `allowlist_address` | `Pubkey` | 32 | That entry's `address`: the collection, or the mint itself |
| 155 | `start_ts` | `i64` | 8 | Cluster time of the create |
| 163 | `end_ts` | `i64` | 8 | |
| 171 | `draw_fee_lamports` | `u64` | 8 | What the creator put into the draw-fee vault |
| 179 | `membership_rules` | `[MembershipRule; 3]` | 225 | The raffle's copy of `Config`'s (spec D14) |
| 404 | `entry_cap` | `u8` | 1 | The cap this raffle applies, after the creator's override (spec D6) |
| 405 | `full_gang_bonus` | `u8` | 1 | |
| 406 | `min_duration` | `u32` | 4 | Seconds |
| 410 | `max_duration` | `u32` | 4 | Seconds |
| 414 | `rules_hash` | `[u8; 32]` | 32 | sha256 of the rules document (spec D20) |
| 446 | `rules_uri` | `String` | 4 + length | At most 96 bytes |
| 450 + `rules_uri` length | `title` | `String` | 4 + length | At most 64 bytes |
| 454 + `rules_uri` length + `title` length | `description` | `String` | 4 + length | The organizer's free text. The Raffle doesn't keep it: this event and the instruction's data are where it is on-chain. PIG-141 sets its limit. |
| | **total** | | **at least 458** | |

### EntriesClaimed

`claim_entries` entered `nfts` for `wallet`.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `d853894fdc70778e` |
| 8 | `raffle` | `Pubkey` | 32 | |
| 40 | `wallet` | `Pubkey` | 32 | Holds the entries, and paid the rent of every account the claim created |
| 72 | `range` | `EntryRange` | 5 | The entries this claim added, the bonus included: the range it appended to the wallet's position. A `count` of zero says it added none and appended nothing. |
| 77 | `bonus_entries` | `u8` | 1 | How many of the range's entries are the Full Gang bonus: zero unless this claim completed the set |
| 78 | `entries_claimed` | `u8` | 1 | The position's per-NFT entries so far, which the cap limits |
| 79 | `collections_seen` | `u8` | 1 | The position's bitmask: `0b111` is the Full Gang set |
| 80 | `bonus_awarded` | `bool` | 1 | |
| 81 | `total_entries` | `u32` | 4 | The raffle's entries so far: where `range` ends |
| 85 | `nfts` | `Vec<Pubkey>` | 4 + 32 × count | The NFTs entered, in the order the claim gave them: a Token Metadata mint or a Core asset each, and one new `EntryReceipt` each |
| | **total** | | **at least 89** | |

### DrawRequested

`request_draw` requested randomness: the first request, which moves the raffle to Drawing, or a later one after the retry timeout.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `3bacdb2a1c5e5d4b` |
| 8 | `raffle` | `Pubkey` | 32 | |
| 40 | `total_entries` | `u32` | 4 | What the draw picks from. The entries are final from the first request on. |
| 44 | `draw_requested_at` | `i64` | 8 | Cluster time of the first request, the same in every later one. The break-glass delay counts from it. |
| 52 | `vrf_requests` | `u32` | 4 | Requests so far, this one included: 1 for the first |
| 56 | `vrf_requested_slot` | `u64` | 8 | Slot of this request |
| 64 | `vrf_seed` | `[u8; 32]` | 32 | Seed of this request, the only one the draw accepts from here on |
| 96 | `vrf_request` | `Pubkey` | 32 | The VRF provider's request account for `vrf_seed`: where the randomness and its proof will be |
| | **total** | | **128** | |

### WinnerDrawn

The randomness arrived and the winner is recorded: the raffle is Drawn.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `d5670576914b9278` |
| 8 | `raffle` | `Pubkey` | 32 | |
| 40 | `winner` | `Pubkey` | 32 | The wallet of the position that holds `winner_index` |
| 72 | `winner_index` | `u32` | 4 | The first 8 bytes of `vrf_randomness` as a little-endian `u64`, modulo `total_entries` |
| 76 | `total_entries` | `u32` | 4 | What `winner_index` was drawn below |
| 80 | `drawn_at` | `i64` | 8 | Cluster time. The claim window and the backstop count from it. |
| 88 | `vrf_request` | `Pubkey` | 32 | The request account the randomness was read from |
| 120 | `vrf_randomness` | `[u8; 64]` | 64 | |
| | **total** | | **184** | |

### PrizeClaimed

`claim_prize` paid the prize to the winner: the raffle is Settled.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `d596c04cc721d426` |
| 8 | `raffle` | `Pubkey` | 32 | |
| 40 | `winner` | `Pubkey` | 32 | The signer, and where the prize went |
| 72 | `prize_mint` | `Pubkey` | 32 | |
| 104 | `amount` | `u64` | 8 | Base units paid out, which is all the escrow held: at least `prize_amount`, since anyone can add to the escrow. 1 for a Core asset. |
| | **total** | | **112** | |

### PrizeReclaimed

`reclaim_prize` returned the prize to the creator. Section 5 tells its three cases apart.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `bb51f4223f007ad8` |
| 8 | `raffle` | `Pubkey` | 32 | |
| 40 | `creator` | `Pubkey` | 32 | Where the prize went |
| 72 | `caller` | `Pubkey` | 32 | The signer. Anyone may call: the prize goes to `creator` whoever does (spec D21). |
| 104 | `state` | `u8` | 1 | A `RaffleState`: Settled, or Cancelled for the prize of a cancelled raffle, whose state the reclaim leaves alone |
| 105 | `prize_mint` | `Pubkey` | 32 | |
| 137 | `amount` | `u64` | 8 | Base units returned, which is all the escrow held. 1 for a Core asset. |
| | **total** | | **145** | |

### RaffleCancelled

`cancel_raffle` cancelled the raffle: its entries are void.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `7b53fe7f35f49f66` |
| 8 | `raffle` | `Pubkey` | 32 | |
| 40 | `authority` | `Pubkey` | 32 | The signer |
| 72 | `cancel_reason` | `u8` | 1 | A `CancelReason`, never `NotCancelled` |
| | **total** | | **73** | |

### EntryAccountsClosed

`close_entry_accounts` closed entry accounts and returned their rent to the wallets that paid it.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `99bd445f47abae2e` |
| 8 | `raffle` | `Pubkey` | 32 | |
| 40 | `open_entry_accounts` | `u32` | 4 | The raffle's entry accounts still open. `close_raffle` waits for zero. |
| 44 | `wallets` | `Vec<Pubkey>` | 4 + 32 × count | The wallets whose `EntryPosition` was closed |
| 48 + 32 × `wallets` count | `nfts` | `Vec<Pubkey>` | 4 + 32 × count | The NFTs whose `EntryReceipt` was closed |
| | **total** | | **at least 52** | |

### RaffleClosed

`close_raffle` closed the Raffle, its draw-fee vault and its token escrow.

| Offset | Field | Type | Bytes | Notes |
| ---: | --- | --- | ---: | --- |
| 0 | discriminator | `[u8; 8]` | 8 | `942fccf61b6f7631` |
| 8 | `raffle` | `Pubkey` | 32 | Free again: a later `RaffleCreated` there is another raffle |
| 40 | `creator` | `Pubkey` | 32 | Where the rent and the rest of the draw fee went |
| | **total** | | **72** | |

## 4. Kinds, states and reasons

Each is one byte, with the values of the [account model](account-model.md), section 5. The IDL shows them as `u8`.

| Field | Enum |
| --- | --- |
| `MembershipRule.kind`, inside `membership_rules` | `RuleKind` |
| `PrizeAllowlisted.kind`, `PrizeDelisted.kind`, `RaffleCreated.allowlist_kind` | `PrizeKind` |
| `RaffleCreated.escrow_kind` | `EscrowKind` |
| `PrizeReclaimed.state` | `RaffleState` |
| `RaffleCancelled.cancel_reason` | `CancelReason` |

## 5. What a reader rebuilds from them

- **A raffle, for good.** `RaffleCreated` has what the Raffle account holds from create and never changes, plus the description, which the account doesn't hold. The rules page reads `rules_uri`, `rules_hash`, `title` and `prize_amount` from it once the account is closed (rules §3).
- **The same values in instruction data.** `create_raffle`'s arguments carry the title, the description, `rules_uri` and `rules_hash` too (PIG-141), so they can be read from either place. A reader that takes the events needs to decode no instruction.
- **Entries.** Each `EntriesClaimed` gives the receipts (`raffle`, each of `nfts`, `wallet`), the range the position gained, and the position's and the raffle's totals after it. The wallet's ranges are every claim's `range` with a `count` above zero.
- **The draw.** `DrawRequested` has every attempt, and `WinnerDrawn` the result. Anyone can check it: `winner_index` is the first 8 bytes of `vrf_randomness` as a little-endian `u64`, modulo `total_entries`, and one of the winner's ranges holds it.
- **Who took the prize.** `PrizeClaimed` is the winner's claim. `PrizeReclaimed` is one of three returns to the creator:

  | `state` | A `WinnerDrawn` came before | Why the prize went back |
  | --- | --- | --- |
  | Settled | No | The raffle ended without an entry |
  | Settled | Yes | The winner didn't claim within the claim window |
  | Cancelled | Either | The authority cancelled the raffle |

- **Rent.** `EntryAccountsClosed` names every position and receipt that closed. Each one's rent went to its wallet: a position's to its own, a receipt's to the wallet of the `EntriesClaimed` that entered the NFT.
- **Config and registries.** Their events say that something changed and what the change log should show. The live values are read from the accounts: an add and a remove in one slot can't be ordered (architecture §3b).
- **An event is applied once.** Its identity is its transaction's signature and its `event_index` (section 1). A reader that gets an event again, from a replay or a second source, drops it: adding a claim's range and receipts a second time would count them twice.
- **Events can arrive in any order, and their fields are of two kinds.**
  - **A field named like an account field** is that field's value after the instruction, so the value of the event that is later on the chain wins: by slot, within a slot by the block's transaction order, and within a transaction by `event_index`. A reader compares before it replaces: an older event's value brings a stale one back, for example when the `ConfigUpdated` that paused arrives after the one that unpaused. A reader without the block's order can't place two events of one slot, which is why the config and the registries are read from the accounts. Most of a raffle's values need no comparison, because they move one way: `total_entries`, `entries_claimed` and `vrf_requests` only grow, `collections_seen` only gains bits, and `open_entry_accounts` in `EntryAccountsClosed` only shrinks. A raffle's state only moves forward ([account model](account-model.md), section 7).
  - **Every other field** says what that one instruction did: the `range`, `bonus_entries` and `nfts` of a claim, the `wallets` and `nfts` that a close removed, the `amount` that a prize transfer moved. No later event replaces these. A reader keeps each event's, whenever it arrives: a claim that arrives after a later one still adds its range and its receipts.
- **An address can name a second raffle.** After `RaffleClosed`, the same creator and nonce can create a raffle at the same address. Every event between a `RaffleCreated` and the next `RaffleClosed` for an address belongs to that raffle. A close and the next create can share a slot, and then only the block's transaction order tells whose an event is.
- **An unknown discriminator** is an event from a later release. Log it and go on.

## 6. Where this differs from the design documents

| Topic | The designs say | The events have | Why |
| --- | --- | --- | --- |
| How events are emitted | "The same event mechanism as the rest of the program" (registries §4) | Inner instructions, by `emit_cpi!` | A transaction's logs stop at 10,000 bytes, and a caller that wraps the instruction can fill them first. The indexer's projections are a fold of the events (architecture §3b), so a missed one is a wrong row. |
| `ConfigInitialized` | `authority` (registries §4) | Also the settings | The change log shows what the config started as |
| Config changes | "Emitted as events" (spec §10) | One `ConfigUpdated` with every setting | A change log entry is the difference between two events. The pause flag needs no event of its own. |
| Title and description | In `create_raffle`'s instruction data, "or the Raffle PDA" (architecture §9, flag 1) | Also in `RaffleCreated` | One place to read a raffle's record from |
| Why a prize was admitted | The admitting entry's kind and address, and whether the creator passed as admin (registries §5) | `allowlist_kind`, `allowlist_address`, `created_by_authority` | As asked |
| Escrow kind and reclaim caller | `escrow_kind` on `RaffleCreated`, the caller on the reclaim event (prize escrow §7) | `escrow_kind`, `PrizeReclaimed.caller` | As asked |
| A claim's ranges | "The new range(s)" (PIG-144) | One `range` | A claim appends at most one ([account model](account-model.md), section 9) |
| The draw's result | `fulfill_draw`, then finalize (spec §8) | One `WinnerDrawn`, when the raffle becomes Drawn | The winner, `drawn_at` and Drawn land together, whether PIG-146 makes that one instruction or two |
| Why a prize was reclaimed | Unspecified | `PrizeReclaimed.state` | A reclaim leaves a cancelled raffle Cancelled, so the event's name doesn't say what the raffle is |
| Proofs per claim transaction | `366 + 100 × Token Metadata + 67 × Core ≤ 1,232` (PIG-133 §2) | 34 bytes more per claim | Section 7 |

## 7. What the later issues have to keep true

- **Every instruction that emits.**
  - Its accounts struct has `#[event_cpi]`, and it calls `emit_cpi!` once, after its last change, with the values as they then are.
  - It never emits with `emit!`, and logs nothing a reader needs.
  - An event's name and fields never change: the discriminator derives from the name, and readers decode by position. A new need gets a new event.
- **`init_config` and `update_config` (PIG-81).** Landed. Both events carry every setting, read from `Config` after the change. The pause flag has no instruction of its own: pausing is an `update_config`, and emits `ConfigUpdated` ([instructions](instructions.md), section 5).
- **Registry instructions (PIG-140).** `CreatorAdded.label` is the entry's 32 bytes, padding included.
- **`create_raffle` (PIG-85, PIG-141, PIG-142).**
  - `created_by_authority` is true exactly when the creator is `Config.authority`, whether or not it also has a `CreatorEntry`.
  - `allowlist_kind` and `allowlist_address` are the `PrizeEntry` the instruction was given.
  - `description` rides in the event and the instruction's data only. PIG-141 sets its limit, which `create_raffle`'s transaction size bounds.
- **`claim_entries` (PIG-84, PIG-143, PIG-144, PIG-145).**
  - `nfts` are the mints and assets in the order of `rule_indexes`. A Token Metadata pig's mint comes from its token account, since the claim isn't passed the mint (PIG-133 §6).
  - `range.start` is `total_entries` before the claim, also when `count` is zero.
  - The event authority adds 34 bytes to the transaction, so PIG-133's rule becomes `400 + 100 × Token Metadata + 67 × Core ≤ 1,232`. 8 Token Metadata pigs or 12 Core pigs still fit. A mix has 34 bytes less: 5 of each no longer share a transaction. PIG-76's chunking rule follows: `100 × Token Metadata + 67 × Core ≤ 832`. No claim the cap allows needs a third transaction.
  - Re-measure the compute with the event (PIG-133 §4.4).
- **The draw (PIG-83, PIG-146, PIG-152).**
  - Every request emits `DrawRequested`, with the same `draw_requested_at`.
  - `vrf_request` is the provider's request account in both events: ORAO's, derived from `vrf_seed`.
  - `WinnerDrawn` is emitted by the instruction that moves the raffle to Drawn. If PIG-146 splits `fulfill_draw` from finalize, the first of the two emits nothing.
- **`cancel_raffle` (PIG-152).** One event for both edges. The break-glass cancel is the one whose raffle had a `DrawRequested`.
- **`claim_prize` and `reclaim_prize` (PIG-82).** `amount` is what left the escrow, not `prize_amount`.
- **`close_entry_accounts` (PIG-147).** `wallets` and `nfts` list what this instruction closed, each once.
- **`close_raffle` (PIG-153).** If it gets a way to return lamports that reach the vault after the close, that instruction needs an event of its own.
- **The indexer (PIG-119, PIG-78).** Section 1 for finding an event, section 5 for folding them, and the layouts here or the release's Rust client ([releases](releases.md)).
- **The IDL (PIG-117).** An event is in the IDL from the moment it is declared, whether or not an instruction emits it. An account is there only once an instruction names it ([account model](account-model.md), section 9).

## 8. Tests

```sh
cargo test -p raffles events
```

- **Layout.** For each event, a test in `programs/raffles/src/events.rs` lists the fields in order, and the compiler checks each name and type. The test checks the list against the bytes the program emits: the discriminator, each field at its offset, and nothing after the last. The table in section 3 has to be exactly those rows, and the bytes have to read back as the same event.
- **The list.** Section 2's table has to be exactly the seventeen events with their discriminators, and section 3 has to have a layout for each, in that order, and for nothing else.
- **The tag and the event authority.** Section 1's two values are checked against Anchor's constant and the program's address.
- **The IDL and the client.** `cargo test -p raffles --test integration interface` checks that the IDL of record lists these events and no others, and that the released Rust client reads what the program emits, an event taken from a transaction included ([releases](releases.md)).
- **The events as emitted.** The instruction tests take each event from the inner instructions of the transaction that emitted it, and compare it with the account, field by field ([instructions](instructions.md), section 10).

Each table is compared whole, up to its last column. The last column and the text around the tables are prose, which no test reads.

A change to an event fails these tests until this document says the same.

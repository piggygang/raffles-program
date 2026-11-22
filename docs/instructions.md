# Instructions

|  |  |
| --- | --- |
| Issue | PIG-81 |
| Date | 2026-11-17 |
| Code | `programs/raffles/src/instructions`, `state/config.rs`, `state/membership_rule.rs` and `error.rs` |
| Status | The config and its authority: `init_config`, `update_config`, `set_authority` and `accept_authority`. Every other instruction lands with its own issue, from PIG-140 on, and gets its section here. |
| Built on | Spec 1.6 §10, registries §4, §6 and §7, holder-proof §5, the [account model](account-model.md) and the [events](events.md) |

## Summary

- **Four instructions.** `init_config` creates `Config`, `update_config` changes its settings, and `set_authority` with `accept_authority` hands the authority on (section 2).
- **Only the program's upgrade authority can run `init_config`.** It is the one key known to be ours when the program is deployed, so nobody can create the config ahead of us (section 3).
- **`update_config` changes the settings it names and no other.** A pause is 7 bytes of arguments, and can't undo a change that landed while it waited for its signatures (section 4).
- **The authority moves in two steps.** The successor has to sign before it is the authority, so an address that can't sign never becomes one (section 7).
- **A config edit reaches later raffles only.** A raffle copies the rules, the cap, the bonus and the bounds when it is created. The pause flag and the authority are read live (section 5).
- **Each instruction emits one event,** as an inner instruction ([events](events.md)).
- **All four stay open while the program is paused,** like every instruction of the authority (section 5).
- **The tests hold the tables here to the code.** An instruction, an account, an argument or an error can't differ between the two (section 10).
- **Where the design documents left a choice, section 8 records it,** and section 9 lists what each later issue has to keep true.

## 1. How an instruction is called

- **The data** is the instruction's 8-byte discriminator, then its arguments in Borsh, little-endian. A discriminator is the first 8 bytes of `sha256("global:<name>")`.
- **The accounts** come in the order of the instruction's table in section 3. The IDL has the same order, and both released clients build from it ([releases](releases.md)).
- **An instruction that emits ends with two accounts:** the event authority and the program itself ([events](events.md), section 1). The TypeScript client fills both in, and the Rust client has `pda::event_authority`.
- **"Signer" means a signature the runtime vouches for.** A wallet signs the transaction. A program's address, such as a Squads vault, signs when its program calls the instruction for it (section 7).
- **Addresses are checked.** `Config`, `program_data` and the event authority each have one address, and another account in its place fails the instruction. The program itself only has to be among the instruction's accounts: the runtime needs it there for the event.
- **Errors.** The program's own start at 6000 (section 6). Anchor's account checks keep Anchor's numbers:

  - 3010 `AccountNotSigner`: an account that has to sign didn't.
  - 3012 `AccountNotInitialized`: `Config` doesn't exist yet.
  - 3007 `AccountOwnedByWrongProgram`: the account in `Config`'s place isn't the program's, or the one in `program_data`'s isn't the loader's.
  - 2006 `ConstraintSeeds`: the account in `Config`'s place is the program's, at another address. For `init_config`, any other address.
  - 2012 `ConstraintAddress`: `program_data` isn't this program's ProgramData account, or the event authority isn't the program's.

  Without the program among its accounts an instruction fails in the runtime, with `MissingAccount` and no number: it can't emit.

## 2. The instructions

| Instruction | Discriminator | Signed by | Writes | Emits |
| --- | --- | --- | --- | --- |
| `init_config` | `17eb73e8a86001e7` | The program's upgrade authority | Creates `Config` | `ConfigInitialized` |
| `update_config` | `1d9efcbf0a53db63` | `Config.authority` | `Config`'s settings | `ConfigUpdated` |
| `set_authority` | `85fa25156ea31a79` | `Config.authority` | `Config.pending_authority` | `AuthorityProposed` |
| `accept_authority` | `6b56c65b210c6ba0` | `Config.pending_authority` | `Config.authority` and `pending_authority` | `AuthorityAccepted` |

No instruction closes `Config`, and none of these four touches a raffle.

## 3. Accounts and checks

### init_config

Creates `Config` with the settings it is given. Argument: `settings`, a `ConfigSettings` (section 4).

| # | Account | Writable | Signer | Is |
| ---: | --- | --- | --- | --- |
| 0 | `authority` | yes | yes | The program's upgrade authority. It pays `Config`'s rent and becomes `Config.authority`. |
| 1 | `config` | yes | no | `Config`, at `[b"config"]`. Created here. |
| 2 | `program_data` | no | no | The program's ProgramData account, `FEHLLCTkmEfFUxmkVBnQ4dnhzekUFzS5tLiJMoLRsvtU`: where the upgradeable loader keeps the upgrade authority |
| 3 | `system_program` | no | no | |
| 4 | `event_authority` | no | no | The event authority |
| 5 | `program` | no | no | The raffles program |

**It fails:**

- with `InvalidInitializer`, unless `authority` is the upgrade authority that `program_data` names. A program without an upgrade authority can't be initialized by anyone.
- with the error of the rule that `settings` break (section 4).
- with the system program's error 0, "already in use", once `Config` exists. It runs once. Anchor creates the account before the program checks the signer, so from then on every caller gets this error, the upgrade authority included.

**It writes** `Config`: the signer as `authority`, no pending authority, the settings, and `_reserved` at zero. It emits `ConfigInitialized` with the authority and every setting.

- **Why the upgrade authority.** Anyone can read a deploy off the chain. If the first caller became the authority, whoever called first after the deploy would own the config. The upgrade authority is the one key that is ours at that moment (registries §6), and the program reads it from the loader's own account instead of holding a key in its code.
- **The ProgramData address is fixed.** The loader keeps a program's ProgramData at `[program id]` under its own address, so the program knows the address at build time, and the instruction needs no account for the program itself. The Rust client has it as `pda::program_data`. Another account in its place fails with `ConstraintAddress`, whoever signs.
- **It needs the upgradeable loader,** which `solana program deploy` uses. A program under another loader has no ProgramData account, and could not be initialized.
- **Lamports at `Config`'s address are no obstacle.** Anyone can send some there before the config exists. The signer then pays only what is missing.
- **The signer pays 2,545,080 lamports of rent** at today's rate ([account model](account-model.md), section 3). Nothing returns them: `Config` is never closed.
- **The upgrade authority and the config's authority are two keys from here on.** Moving one doesn't move the other. Once `Config` exists, the upgrade authority has no say over it, short of upgrading the program.

### update_config

Changes the settings its argument names. Argument: `update`, a `ConfigUpdate` (section 4).

| # | Account | Writable | Signer | Is |
| ---: | --- | --- | --- | --- |
| 0 | `authority` | no | yes | `Config.authority` |
| 1 | `config` | yes | no | `Config` |
| 2 | `event_authority` | no | no | The event authority |
| 3 | `program` | no | no | The raffles program |

**It fails:**

- with `Unauthorized`, unless `authority` is `Config.authority`.
- with the error of the rule that the settings would break once the update is applied (section 4). The update is judged by what it leaves, not by what it names: a new shortest duration fails when it passes the longest one the config has, and passes when the update moves both.

**It writes** each setting that `update` names, and leaves the others. It emits `ConfigUpdated` with every setting as it then is.

- **An update that changes nothing succeeds, and emits.** It names nothing, or names the value a setting already has. So the second of two pauses in an incident isn't an error, and the change log shows an entry without a difference.
- **It never touches the authority or a pending transfer,** and it writes no raffle.

### set_authority

Proposes the authority's successor. Argument: `new_authority`, a `Pubkey`.

| # | Account | Writable | Signer | Is |
| ---: | --- | --- | --- | --- |
| 0 | `authority` | no | yes | `Config.authority` |
| 1 | `config` | yes | no | `Config` |
| 2 | `event_authority` | no | no | The event authority |
| 3 | `program` | no | no | The raffles program |

**It fails** with `Unauthorized`, unless `authority` is `Config.authority`.

**It writes** `new_authority` to `Config.pending_authority`, and nothing else: `authority` stays in charge of everything. It emits `AuthorityProposed`.

- **A later call replaces an earlier proposal.** A typo is fixed by calling again.
- **The zero address withdraws a proposal.** `accept_authority` then fails for everyone.
- **`new_authority` isn't checked.** The program can't tell whether an address can sign. That is what `accept_authority` is for. It can also be the authority itself, or the zero address with nothing pending: both succeed, and emit.

### accept_authority

Completes the transfer. It takes no argument.

| # | Account | Writable | Signer | Is |
| ---: | --- | --- | --- | --- |
| 0 | `new_authority` | no | yes | `Config.pending_authority` |
| 1 | `config` | yes | no | `Config` |
| 2 | `event_authority` | no | no | The event authority |
| 3 | `program` | no | no | The raffles program |

**It fails:**

- with `NoPendingAuthority`, when `Config.pending_authority` is zero: nothing was proposed, the proposal was withdrawn, or it was already accepted.
- with `Unauthorized`, unless `new_authority` is `Config.pending_authority`.

**It writes** `Config.authority`, which becomes the signer, and `Config.pending_authority`, which becomes zero. It emits `AuthorityAccepted`.

- **The old authority is out at once.** Its next `update_config` or `set_authority` fails with `Unauthorized`.
- **Nothing else changes.** The settings stay, and so does every raffle.

## 4. Arguments

Borsh, little-endian, like the accounts. An instruction's data is its discriminator, then its argument if it takes one.

### ConfigSettings

Everything the authority sets in `Config`. `init_config` takes it whole.

| Field | Type | Bytes | Notes |
| --- | --- | ---: | --- |
| `paused` | `bool` | 1 | A config can start paused |
| `membership_rules` | `[MembershipRule; 3]` | 225 | Rule `i` is bit `i` of `EntryPosition.collections_seen` ([account model](account-model.md), section 4) |
| `entry_cap` | `u8` | 1 | Per-NFT entries per wallet per raffle |
| `full_gang_bonus` | `u8` | 1 | Entries the Full Gang bonus adds |
| `min_duration` | `u32` | 4 | Seconds |
| `max_duration` | `u32` | 4 | Seconds |
| **total** | | **236** | |

### ConfigUpdate

What `update_config` changes: one option per setting. `None` leaves the setting as it is.

| Field | Type | Bytes | Notes |
| --- | --- | ---: | --- |
| `paused` | `Option<bool>` | 1 or 2 | |
| `membership_rules` | `Option<[MembershipRule; 3]>` | 1 or 226 | All three rules or none: they are checked as a set |
| `entry_cap` | `Option<u8>` | 1 or 2 | |
| `full_gang_bonus` | `Option<u8>` | 1 or 2 | |
| `min_duration` | `Option<u32>` | 1 or 5 | |
| `max_duration` | `Option<u32>` | 1 or 5 | |
| **total** | | **6 to 242** | |

- **An option is one byte,** 0 for none and 1 for a value, and then the value.
- **A pause is 7 bytes:** `01 01 00 00 00 00 00`. With the discriminator, the instruction's data is 15 bytes.
- **Why options.** On mainnet an update is a Squads proposal, which waits for its signatures while other proposals land. An update that carried every setting would write back the values its author read, and undo whatever changed in between. An update that names one setting can't.

### What the settings have to be

`init_config` and `update_config` check the settings they are about to write, all of them. A setting that breaks a rule fails the instruction with that rule's error, and nothing is written.

| Rule | Error | Why |
| --- | --- | --- |
| `entry_cap` is 1 to `MAX_ENTRY_CAP`, which is 10 | `InvalidEntryCap` | An `EntryPosition` has room for the ranges of ten single claims and one bonus ([account model](account-model.md), section 4) |
| `entry_cap + full_gang_bonus` is at most 255 | `InvalidFullGangBonus` | One claim can add the cap and the bonus in one range, and `EntryRange.count` is a `u8` |
| `min_duration` is above zero, and at most `max_duration` | `InvalidDurationBounds` | No raffle could be created otherwise |
| A rule's `kind` is 0 or 1 | `InvalidRuleKind` | 2 is kept for a certified-collection rule, which isn't built (holder-proof §5) |
| A rule's `address` isn't zero | `InvalidRuleAddress` | |
| `TmCreator`: `update_authority` isn't zero. `CoreCollection`: it is zero. | `InvalidRuleUpdateAuthority` | A field its kind doesn't read stays zero, so two rules that admit the same NFTs are the same bytes |
| `TmCreator`: `symbol` is 1 to 10 bytes of UTF-8 without a zero byte, then zero bytes. `CoreCollection`: it is all zero. | `InvalidRuleSymbol` | A claim compares the NFT's symbol, without its trailing zero bytes, with the rule's |
| No two rules are the same | `DuplicateMembershipRule` | Two NFTs of one collection would set two bits, and earn the Full Gang bonus without the full set |

- **The program can't tell whether the addresses are the Piggy collections.** The authority vouches for that, and every change is an event in the change log (holder-proof §5).
- **Rules that differ can still overlap.** An NFT enters once per raffle whichever rule it is claimed under, so an overlap can't count one NFT twice. Keeping the three rules apart is the authority's job. Mainnet's are: two symbols under one creator, and a Core collection.
- **`full_gang_bonus` can be zero,** which turns the bonus off.
- **The bounds have no ceiling of their own.** A `u32` of seconds is 136 years, and the spec's 30 days is a setting (D4).

## 5. The pause, and what a change reaches

**The pause flag blocks three instructions,** and nothing that pays out a prize or returns rent (spec §10, D13):

| Instruction | While paused | Lands with |
| --- | --- | --- |
| `create_raffle` | Blocked | PIG-85 |
| `claim_entries` | Blocked | PIG-84 |
| `request_draw`, a re-request included | Blocked | PIG-83, PIG-152 |
| `claim_prize`, `reclaim_prize` | Open | PIG-82 |
| `close_entry_accounts`, `close_raffle` | Open | PIG-147, PIG-153 |
| The authority's instructions, these four among them | Open | |

- **One guard.** A blocked instruction calls `Config::require_not_paused`, which fails with `Paused`. Nothing else reads the flag.
- **Pausing is `update_config` with `paused` alone,** and unpausing the same. The flag has no instruction of its own.
- **A pause moves no time.** `end_ts` keeps running, and the claim window starts when a raffle is Drawn ([account model](account-model.md), section 7).
- **The authority keeps working under a pause.** It lifts the pause, changes a setting, and can hand the authority on.

**What a change reaches:**

| Field of `Config` | Read by | A change reaches |
| --- | --- | --- |
| `membership_rules`, `entry_cap`, `full_gang_bonus`, `min_duration`, `max_duration` | `create_raffle`, which copies them onto the raffle | Raffles created from then on (spec D14) |
| `paused` | The three blocked instructions | Every raffle, at once |
| `authority` | Every instruction of the authority, and `create_raffle`, where it needs no `CreatorEntry` | Everything, at once |

A raffle that exists is checked against its own copy and never against `Config`, so no update can change the rules of a raffle that is running.

## 6. Errors

| Number | Error | Message | Returned by |
| ---: | --- | --- | --- |
| 6000 | `InvalidRuleKind` | not a membership rule kind the program knows | `init_config`, `update_config` |
| 6001 | `InvalidPrizeKind` | not a prize kind the program knows | No instruction yet |
| 6002 | `InvalidEscrowKind` | not an escrow kind the program knows | No instruction yet |
| 6003 | `InvalidRaffleState` | not a raffle state the program knows | No instruction yet |
| 6004 | `InvalidCancelReason` | not a cancel reason the program knows | No instruction yet |
| 6005 | `InvalidTransition` | the raffle can't move to that state from the one it is in | No instruction yet |
| 6006 | `Unauthorized` | the signer isn't the authority this instruction takes | `update_config`, `set_authority`, `accept_authority` |
| 6007 | `InvalidInitializer` | the signer isn't the program's upgrade authority | `init_config` |
| 6008 | `NoPendingAuthority` | no authority transfer is pending | `accept_authority` |
| 6009 | `Paused` | the program is paused | No instruction yet: section 5 |
| 6010 | `InvalidEntryCap` | the entry cap isn't between 1 and the highest the program allows | `init_config`, `update_config` |
| 6011 | `InvalidFullGangBonus` | the entry cap and the Full Gang bonus don't fit one entry range | `init_config`, `update_config` |
| 6012 | `InvalidDurationBounds` | the shortest duration is zero, or longer than the longest | `init_config`, `update_config` |
| 6013 | `InvalidRuleAddress` | a membership rule's address is zero | `init_config`, `update_config` |
| 6014 | `InvalidRuleUpdateAuthority` | a membership rule's update authority doesn't fit its kind | `init_config`, `update_config` |
| 6015 | `InvalidRuleSymbol` | a membership rule's symbol doesn't fit its kind | `init_config`, `update_config` |
| 6016 | `DuplicateMembershipRule` | two membership rules are the same | `init_config`, `update_config` |

An error's number is 6000 plus its place in `RaffleError`, and clients match on the number. A new error goes at the end.

## 7. The authority

`Config.authority` is the admin: a Squads v4 vault on mainnet, and a single keypair on devnet and localnet (registries §6).

**From deploy to the vault** (registries §7a):

1. Deploy the program. The deployer's key is the upgrade authority.
2. `init_config`, signed by that key. It is now the config's authority too.
3. Seed the registries from the same key (PIG-140).
4. `set_authority` with the vault's address.
5. The vault runs `accept_authority`, as a vault transaction. Until it does, the deployer's key stays the authority.
6. Move the program's upgrade authority to the vault as well (PIG-67).

If the upgrade authority moves first, the vault runs `init_config` itself, as a vault transaction, and pays `Config`'s rent from its own balance. Steps 4 and 5 then fall away.

- **Propose the vault, never the multisig account.** Inside a vault transaction only the vault signs. Nothing can sign for the multisig account, so as the authority it would lock the config for good (registries §6).
- **The second step is what makes that mistake harmless.** A proposed address that can't sign can't accept. The proposal stays open, the authority stays in charge, and it proposes again.
- **An accepted transfer can't be taken back** by the old authority. The second step proves that the successor can sign, not that it is the right one.
- **Most key rotation never reaches the program.** Replacing a member or changing the threshold happens inside Squads, and the vault's address stays (registries §7g). `set_authority` is for moving to another multisig.
- **How a vault transaction looks to the program.** Squads' `vault_transaction_execute` calls the instruction with the vault as a signer. The instruction is then an inner instruction, and its event one level further down, at stack height 3 instead of 2 ([events](events.md), section 1).
- **A vault needs a balance for `init_config` only.** The other three instructions create no account.

## 8. Where this differs from the design documents

| Topic | The designs say | The program has | Why |
| --- | --- | --- | --- |
| `update_config`'s arguments | "Config updates" (spec §10); an event with every setting ([events](events.md)) | One option per setting. The event still carries every setting. | Section 4: a proposal that waits must not undo what lands meanwhile |
| The pause flag | A flag in `Config` (spec §10). The events left open whether it gets an instruction of its own. | A setting of `update_config` | One instruction and one event for every setting. A pause is 7 bytes either way. |
| An update without a change | Unspecified | Succeeds, and emits | Section 3: a second pause isn't an error |
| `init_config`'s arguments | "Config params" (registries §4) | `ConfigSettings`, the pause flag included | Mainnet can start paused, and open once the registries are seeded (PIG-175) |
| Who runs `init_config` on mainnet | The deployer's key, before the authority moves (registries §7a) | Whoever holds the upgrade authority: the deployer's key, or the vault once the upgrade authority has moved | PIG-67 moves the upgrade authority, and PIG-175 initializes after it |
| Limits on the settings | The cap, the bonus and the bounds are values the admin sets (spec §10); the rules' checks are holder-proof §5's | Also the three limits of section 4 on the cap, the bonus and the bounds | The entry accounts are sized for the first two ([account model](account-model.md), section 9), and bounds that fit no raffle are a mistake |
| The upgrade authority check | "Read from the upgradeable loader's ProgramData account" (registries §4) | The same, with the account's address fixed at build time | Section 3. Anchor's usual form takes the program account as well, to read the address from it. |
| `accept_authority` by the wrong signer | "`pending_authority != default` and signer matches it" (registries §4) | `NoPendingAuthority` when nothing is pending, `Unauthorized` otherwise | Two causes, two errors |
| A rule's symbol | "1–10 bytes, NUL-padded, with no NUL before the last non-NUL byte" (holder-proof §5) | Also UTF-8 | Token Metadata's symbol is a string. A rule that isn't one could never match. |
| Errors | `Unauthorized`, `InvalidInitializer`, `NoPendingAuthority` (registries §4) | Those, one per rule of section 4, and `Paused` | An admin page can say which setting is wrong |
| The scaffold's `hello` | "Goes once a real instruction exists" | Removed | It did |

## 9. What the later issues have to keep true

- **Every instruction.**
  - It has a module in `src/instructions`, a row in section 2 and a section in section 3, with its accounts in a table. The tests fail until it does.
  - Its accounts struct has `#[event_cpi]`, and it emits once, after its last change ([events](events.md), section 7).
  - A new error goes at the end of `RaffleError`, with a row in section 6.
- **The authority's instructions (PIG-140, PIG-152).** `Config` with `has_one = authority @ RaffleError::Unauthorized`, and no pause guard.
- **`create_raffle` (PIG-85).**
  - It calls `Config::require_not_paused`.
  - It copies the rules, the cap, the bonus and the bounds from `Config::settings` onto the raffle, and checks the duration and the cap override against that copy.
  - The creator is the authority exactly when it is `Config.authority` at that moment.
- **`claim_entries` and `request_draw` (PIG-84, PIG-83, PIG-152).** Both call `Config::require_not_paused`, and read nothing else of `Config`. A claim is checked against the raffle's rules.
- **Everything else.** It doesn't read `Config.paused`. Once `Config` exists, only `update_config` writes a setting, and only `accept_authority` writes `Config.authority`.
- **A release that tightens a rule of section 4.** `update_config` checks every setting, the ones an update doesn't name too. A stored setting that no longer passed would fail every update, a pause included, until one update names it. So such a release leaves the stored `Config` valid, or keeps accepting what is already stored.
- **The Squads rehearsal and mainnet (PIG-129, PIG-67, PIG-175).**
  - The rehearsal runs section 7 on devnet with the DAO's own multisig. The tests here run the same steps in LiteSVM, against the Squads program that mainnet runs (section 10).
  - `init_config` on mainnet takes holder-proof §5's values. Whoever holds the upgrade authority at that point signs it.
- **The admin page (PIG-169).**
  - An `update_config` proposal names only what the admin changed.
  - It shows the current settings from the `Config` account, and the change log from the events.
  - Section 4's rules are worth checking before a proposal is raised: a vault transaction that fails stays approved and unexecuted.
- **The indexer (PIG-119).** Config and authority events mark the mirror stale, and the values come from the account ([events](events.md), section 5).
- **The clients.** The TypeScript generator keeps an instruction's arguments as the IDL has them: `settings` and `update`, not their fields ([releases](releases.md)).

## 10. Tests

```sh
cargo test -p raffles                       # after anchor build
```

- **The settings.** Unit tests in `src/state` take each rule of section 4 to both sides of its limit, and check that an update is judged by what it leaves.
- **The tables.** Unit tests hold the two argument tables of section 4 to the bytes, section 2 to the discriminators, and section 6 to the error numbers and messages. The integration tests hold section 2, each accounts table of section 3 and section 6 to the IDL of record. So the document lists every instruction, every account and every error a client has, the accounts in the order and with the flags a client sends, and no other.
- **The instructions.** The `config` and `authority` modules of the integration tests run each one in LiteSVM: who can call it, what `Config` holds afterwards, each way it fails, and its event read from the inner instructions, field by field.
- **The upgrade authority.** The harness makes its deployer the program's upgrade authority, as a deploy does. The tests cover a stranger, a program without an upgrade authority, a deployer that gave the upgrade authority away, the ProgramData account of another program, and a missing signature when `Config`'s address already holds its rent.
- **A Squads vault.** The `vault` module runs all four instructions as vault transactions of the Squads v4 program, whose mainnet bytecode is among the fixtures: a multisig of two out of three without a time lock, a transaction for its vault, two approvals and the execution.
  - It covers the bootstrap of section 7 in both orders, a move from one vault to another, and the largest update in one vault transaction.
  - Each event is found at stack height 3.
  - One approval is below the threshold, and Squads doesn't execute.
  - The multisig account, proposed in place of its vault, never becomes the authority: Squads refuses a transaction that needs its signature, and the program refuses it without one.

A tested table is compared row for row, up to the columns the code gives: an instruction's name and discriminator, an account's place, name and flags, a field's name, type and size, an error's number, name and message. The columns after those are prose, which no test reads, and so are the rules table of section 4, the tables of sections 5 and 8, and the text.

A change to an instruction fails these tests until this document says the same.

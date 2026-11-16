//! The events: what the program tells readers that its accounts can't. The
//! indexer folds them into its projections and the admin change log, and they
//! are all that is left of a raffle once its accounts are closed.
//! docs/events.md has every layout and what emits it; the tests here fail
//! when it and the code disagree.
//!
//! Conventions, for every event:
//! - It is emitted with `emit_cpi!`, as an inner instruction of the program to
//!   itself, never as a log line: logs can be truncated, and anyone can write
//!   one.
//! - A raffle's event starts with `raffle`.
//! - Fixed-size fields come before variable-length ones.
//! - A field named like an account field carries that field's value once the
//!   instruction has run. So events hold values, not changes: applying one
//!   twice does no harm, and neither does a late one, as long as the reader
//!   keeps the value of the event that is later on the chain
//!   (docs/events.md, section 5).
//! - Kinds, states and reasons are a `u8`, as in the accounts.
//! - Its name and its fields never change: the discriminator derives from the
//!   name, and readers decode by position. A new need gets a new event.

use {
    crate::{
        constants::MEMBERSHIP_RULE_COUNT,
        state::{CreatorEntry, EntryRange, MembershipRule},
    },
    anchor_lang::prelude::*,
};

// Config and authority.

/// `init_config` created `Config`. The fields after `authority` are the
/// settings it starts with.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct ConfigInitialized {
    pub authority: Pubkey,
    pub paused: bool,
    pub membership_rules: [MembershipRule; MEMBERSHIP_RULE_COUNT],
    pub entry_cap: u8,
    pub full_gang_bonus: u8,
    pub min_duration: u32,
    pub max_duration: u32,
}

/// `update_config` changed a setting, the pause flag included. The event
/// carries every setting, changed or not: the difference from the event
/// before it is the change.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct ConfigUpdated {
    pub authority: Pubkey,
    pub paused: bool,
    pub membership_rules: [MembershipRule; MEMBERSHIP_RULE_COUNT],
    pub entry_cap: u8,
    pub full_gang_bonus: u8,
    pub min_duration: u32,
    pub max_duration: u32,
}

/// `set_authority` named the authority's successor, or withdrew one: then
/// `pending_authority` is zero. `authority` stays in charge either way.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct AuthorityProposed {
    pub authority: Pubkey,
    pub pending_authority: Pubkey,
}

/// `accept_authority` completed the transfer: `new_authority` signed, and is
/// `Config::authority` from here on.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct AuthorityAccepted {
    pub old_authority: Pubkey,
    pub new_authority: Pubkey,
}

// Registries.

/// `creator_add` created the wallet's `CreatorEntry`.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct CreatorAdded {
    pub wallet: Pubkey,
    pub label: [u8; CreatorEntry::LABEL_LEN],
}

/// `creator_remove` closed the wallet's `CreatorEntry`.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct CreatorRemoved {
    pub wallet: Pubkey,
}

/// `allowlist_add` created the `PrizeEntry` for `kind` and `address`.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct PrizeAllowlisted {
    /// A `PrizeKind`.
    pub kind: u8,
    pub address: Pubkey,
}

/// `allowlist_remove` closed the `PrizeEntry` for `kind` and `address`.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct PrizeDelisted {
    /// A `PrizeKind`.
    pub kind: u8,
    pub address: Pubkey,
}

// A raffle, from create to close.

/// `create_raffle` created the raffle and escrowed its prize. The event is
/// everything about the raffle that never changes, so its history can be
/// read after `close_raffle` has removed the account.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct RaffleCreated {
    pub raffle: Pubkey,
    pub creator: Pubkey,
    pub nonce: u64,
    /// The creator signed as `Config::authority`, which needs no
    /// `CreatorEntry`. Otherwise its entry admitted it.
    pub created_by_authority: bool,
    /// An `EscrowKind`.
    pub escrow_kind: u8,
    pub prize_mint: Pubkey,
    pub prize_amount: u64,
    /// The `PrizeKind` of the `PrizeEntry` that
    /// admitted the prize.
    pub allowlist_kind: u8,
    /// That entry's `address`: the collection, or the mint itself.
    pub allowlist_address: Pubkey,
    pub start_ts: i64,
    pub end_ts: i64,
    pub draw_fee_lamports: u64,
    pub membership_rules: [MembershipRule; MEMBERSHIP_RULE_COUNT],
    pub entry_cap: u8,
    pub full_gang_bonus: u8,
    pub min_duration: u32,
    pub max_duration: u32,
    pub rules_hash: [u8; 32],
    pub rules_uri: String,
    pub title: String,
    /// The organizer's free text. The Raffle doesn't keep it: this event and
    /// the instruction's data are where it is on-chain.
    pub description: String,
}

/// `claim_entries` entered `nfts` for `wallet`.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct EntriesClaimed {
    pub raffle: Pubkey,
    pub wallet: Pubkey,
    /// The entries this claim added, the bonus included, as the range it
    /// appended to the wallet's position. A `count` of zero says it added
    /// none and appended nothing: a pig at the cap that only adds a
    /// collection.
    pub range: EntryRange,
    /// How many of the range's entries are the Full Gang bonus: zero unless
    /// this claim completed the set.
    pub bonus_entries: u8,
    pub entries_claimed: u8,
    pub collections_seen: u8,
    pub bonus_awarded: bool,
    pub total_entries: u32,
    /// The NFTs entered, in the order the claim gave them: a Token Metadata
    /// mint or a Core asset each, and one new `EntryReceipt` each.
    pub nfts: Vec<Pubkey>,
}

/// `request_draw` requested randomness: the first request, which moves the
/// raffle to Drawing, or a later one after the retry timeout.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct DrawRequested {
    pub raffle: Pubkey,
    /// The entries the draw picks from. They are final from the first
    /// request on.
    pub total_entries: u32,
    /// Cluster time of the first request, the same in every later one.
    pub draw_requested_at: i64,
    /// Requests so far, this one included: 1 for the first.
    pub vrf_requests: u32,
    pub vrf_requested_slot: u64,
    pub vrf_seed: [u8; 32],
    /// The VRF provider's request account for `vrf_seed`: where the
    /// randomness and its proof will be.
    pub vrf_request: Pubkey,
}

/// The randomness arrived and the winner is recorded: the raffle is Drawn.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct WinnerDrawn {
    pub raffle: Pubkey,
    pub winner: Pubkey,
    pub winner_index: u32,
    /// What `winner_index` was drawn below.
    pub total_entries: u32,
    pub drawn_at: i64,
    /// The request account the randomness was read from.
    pub vrf_request: Pubkey,
    pub vrf_randomness: [u8; 64],
}

/// `claim_prize` paid the prize to the winner: the raffle is Settled.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct PrizeClaimed {
    pub raffle: Pubkey,
    pub winner: Pubkey,
    pub prize_mint: Pubkey,
    /// Base units paid out, which is all the escrow held: at least
    /// `Raffle::prize_amount`. 1 for a Core asset.
    pub amount: u64,
}

/// `reclaim_prize` returned the prize to the creator.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct PrizeReclaimed {
    pub raffle: Pubkey,
    pub creator: Pubkey,
    /// Signed the instruction. Anyone may: the prize goes to `creator`
    /// whoever calls.
    pub caller: Pubkey,
    /// A `RaffleState`: Settled, or Cancelled
    /// for the prize of a cancelled raffle, whose state the reclaim leaves
    /// alone.
    pub state: u8,
    pub prize_mint: Pubkey,
    /// Base units returned, which is all the escrow held. 1 for a Core
    /// asset.
    pub amount: u64,
}

/// `cancel_raffle` cancelled the raffle: its entries are void.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct RaffleCancelled {
    pub raffle: Pubkey,
    pub authority: Pubkey,
    /// A `CancelReason`, never `NotCancelled`.
    pub cancel_reason: u8,
}

/// `close_entry_accounts` closed entry accounts and returned their rent to
/// the wallets that paid it.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct EntryAccountsClosed {
    pub raffle: Pubkey,
    pub open_entry_accounts: u32,
    /// The wallets whose `EntryPosition` was closed.
    pub wallets: Vec<Pubkey>,
    /// The NFTs whose `EntryReceipt` was closed.
    pub nfts: Vec<Pubkey>,
}

/// `close_raffle` closed the Raffle, its draw-fee vault and its token escrow.
/// Its address is free again: a later `RaffleCreated` there is another
/// raffle.
#[event]
#[derive(Debug, PartialEq, Eq)]
pub struct RaffleClosed {
    pub raffle: Pubkey,
    pub creator: Pubkey,
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::{
            testing::{assert_event, field, EVENTS},
            CancelReason, EscrowKind, PrizeKind, RaffleState, RuleKind,
        },
        anchor_lang::Discriminator,
    };

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn membership_rules() -> [MembershipRule; MEMBERSHIP_RULE_COUNT] {
        [
            MembershipRule {
                kind: RuleKind::TmCreator.into(),
                address: key(6),
                update_authority: key(7),
                symbol: *b"PSG\0\0\0\0\0\0\0",
            },
            MembershipRule {
                kind: RuleKind::TmCreator.into(),
                address: key(6),
                update_authority: key(7),
                symbol: *b"PGG\0\0\0\0\0\0\0",
            },
            MembershipRule {
                kind: RuleKind::CoreCollection.into(),
                address: key(8),
                ..MembershipRule::default()
            },
        ]
    }

    /// `name` and its discriminator, as the summary table's first columns.
    macro_rules! summary_rows {
        ($($event:ident),+ $(,)?) => {
            vec![$({
                let discriminator: String = $event::DISCRIMINATOR
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                format!("| `{}` | `{discriminator}` |", stringify!($event))
            }),+]
        };
    }

    #[test]
    fn the_document_lists_every_event_with_its_discriminator() {
        let rows = summary_rows![
            ConfigInitialized,
            ConfigUpdated,
            AuthorityProposed,
            AuthorityAccepted,
            CreatorAdded,
            CreatorRemoved,
            PrizeAllowlisted,
            PrizeDelisted,
            RaffleCreated,
            EntriesClaimed,
            DrawRequested,
            WinnerDrawn,
            PrizeClaimed,
            PrizeReclaimed,
            RaffleCancelled,
            EntryAccountsClosed,
            RaffleClosed,
        ];

        EVENTS.assert_table("## 2. The events", "", &rows);
        // Each has its layout, in the same order, and nothing else has one.
        let names: Vec<String> = rows
            .iter()
            .map(|row| row.split('`').nth(1).unwrap().to_owned())
            .collect();
        assert_eq!(EVENTS.subsections("## 3. Layouts"), names);
    }

    /// What marks an inner instruction as an event, and the one account it
    /// names. Anchor defines both; readers match on them.
    #[test]
    fn the_document_gives_the_event_tag_and_the_event_authority() {
        let tag: String = anchor_lang::event::EVENT_IX_TAG_LE
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(tag, "e445a52e51cb9a1d");
        assert!(EVENTS.says(&format!("`{tag}`")), "{tag}");

        let (event_authority, _) =
            Pubkey::find_program_address(&[b"__event_authority"], &crate::ID);
        assert!(
            EVENTS.says(&format!("`{event_authority}`")),
            "{event_authority}"
        );
    }

    #[test]
    fn config_initialized_serializes_to_its_documented_layout() {
        let event = ConfigInitialized {
            authority: key(1),
            paused: true,
            membership_rules: membership_rules(),
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 2_592_000,
        };

        assert_event(
            "ConfigInitialized",
            &event,
            &[
                field!(event.authority: Pubkey),
                field!(event.paused: bool),
                field!(event.membership_rules: [MembershipRule; 3]),
                field!(event.entry_cap: u8),
                field!(event.full_gang_bonus: u8),
                field!(event.min_duration: u32),
                field!(event.max_duration: u32),
            ],
        );
    }

    #[test]
    fn config_updated_serializes_to_its_documented_layout() {
        let event = ConfigUpdated {
            authority: key(1),
            paused: true,
            membership_rules: membership_rules(),
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 2_592_000,
        };

        assert_event(
            "ConfigUpdated",
            &event,
            &[
                field!(event.authority: Pubkey),
                field!(event.paused: bool),
                field!(event.membership_rules: [MembershipRule; 3]),
                field!(event.entry_cap: u8),
                field!(event.full_gang_bonus: u8),
                field!(event.min_duration: u32),
                field!(event.max_duration: u32),
            ],
        );
    }

    #[test]
    fn authority_proposed_serializes_to_its_documented_layout() {
        let event = AuthorityProposed {
            authority: key(1),
            pending_authority: key(2),
        };

        assert_event(
            "AuthorityProposed",
            &event,
            &[
                field!(event.authority: Pubkey),
                field!(event.pending_authority: Pubkey),
            ],
        );
    }

    #[test]
    fn authority_accepted_serializes_to_its_documented_layout() {
        let event = AuthorityAccepted {
            old_authority: key(1),
            new_authority: key(2),
        };

        assert_event(
            "AuthorityAccepted",
            &event,
            &[
                field!(event.old_authority: Pubkey),
                field!(event.new_authority: Pubkey),
            ],
        );
    }

    #[test]
    fn creator_added_serializes_to_its_documented_layout() {
        let mut label = [0; CreatorEntry::LABEL_LEN];
        label[..6].copy_from_slice(b"GM bot");
        let event = CreatorAdded {
            wallet: key(1),
            label,
        };

        assert_event(
            "CreatorAdded",
            &event,
            &[field!(event.wallet: Pubkey), field!(event.label: [u8; 32])],
        );
    }

    #[test]
    fn creator_removed_serializes_to_its_documented_layout() {
        let event = CreatorRemoved { wallet: key(1) };

        assert_event("CreatorRemoved", &event, &[field!(event.wallet: Pubkey)]);
    }

    #[test]
    fn prize_allowlisted_serializes_to_its_documented_layout() {
        let event = PrizeAllowlisted {
            kind: PrizeKind::CoreCollection.into(),
            address: key(1),
        };

        assert_event(
            "PrizeAllowlisted",
            &event,
            &[field!(event.kind: u8), field!(event.address: Pubkey)],
        );
    }

    #[test]
    fn prize_delisted_serializes_to_its_documented_layout() {
        let event = PrizeDelisted {
            kind: PrizeKind::CoreCollection.into(),
            address: key(1),
        };

        assert_event(
            "PrizeDelisted",
            &event,
            &[field!(event.kind: u8), field!(event.address: Pubkey)],
        );
    }

    #[test]
    fn raffle_created_serializes_to_its_documented_layout() {
        let event = RaffleCreated {
            raffle: key(1),
            creator: key(2),
            nonce: 0x0807_0605_0403_0201,
            created_by_authority: true,
            escrow_kind: EscrowKind::Token.into(),
            prize_mint: key(3),
            prize_amount: 500_000_000,
            allowlist_kind: PrizeKind::CoreCollection.into(),
            allowlist_address: key(4),
            start_ts: 1_794_000_000,
            end_ts: 1_794_604_800,
            draw_fee_lamports: 15_515_720,
            membership_rules: membership_rules(),
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 2_592_000,
            rules_hash: [9; 32],
            rules_uri: "ar://bNbA3TEQVL60xlgCcqdz4ZPHFZ711cZ3hmkpGttDt_U".into(),
            title: "Piggy Gang #4521".into(),
            description: "Hold a Piggy, win a Piggy.".into(),
        };

        assert_event(
            "RaffleCreated",
            &event,
            &[
                field!(event.raffle: Pubkey),
                field!(event.creator: Pubkey),
                field!(event.nonce: u64),
                field!(event.created_by_authority: bool),
                field!(event.escrow_kind: u8),
                field!(event.prize_mint: Pubkey),
                field!(event.prize_amount: u64),
                field!(event.allowlist_kind: u8),
                field!(event.allowlist_address: Pubkey),
                field!(event.start_ts: i64),
                field!(event.end_ts: i64),
                field!(event.draw_fee_lamports: u64),
                field!(event.membership_rules: [MembershipRule; 3]),
                field!(event.entry_cap: u8),
                field!(event.full_gang_bonus: u8),
                field!(event.min_duration: u32),
                field!(event.max_duration: u32),
                field!(event.rules_hash: [u8; 32]),
                field!(event.rules_uri: String),
                field!(event.title: String),
                field!(event.description: String),
            ],
        );
    }

    #[test]
    fn entries_claimed_serializes_to_its_documented_layout() {
        let event = EntriesClaimed {
            raffle: key(1),
            wallet: key(2),
            range: EntryRange {
                start: 3_400,
                count: 5,
            },
            bonus_entries: 3,
            entries_claimed: 2,
            collections_seen: 0b111,
            bonus_awarded: true,
            total_entries: 3_405,
            nfts: vec![key(3), key(4)],
        };

        assert_event(
            "EntriesClaimed",
            &event,
            &[
                field!(event.raffle: Pubkey),
                field!(event.wallet: Pubkey),
                field!(event.range: EntryRange),
                field!(event.bonus_entries: u8),
                field!(event.entries_claimed: u8),
                field!(event.collections_seen: u8),
                field!(event.bonus_awarded: bool),
                field!(event.total_entries: u32),
                field!(event.nfts: Vec<Pubkey>),
            ],
        );
    }

    #[test]
    fn draw_requested_serializes_to_its_documented_layout() {
        let event = DrawRequested {
            raffle: key(1),
            total_entries: 3_405,
            draw_requested_at: 1_794_604_830,
            vrf_requests: 2,
            vrf_requested_slot: 512_345_678,
            vrf_seed: [3; 32],
            vrf_request: key(4),
        };

        assert_event(
            "DrawRequested",
            &event,
            &[
                field!(event.raffle: Pubkey),
                field!(event.total_entries: u32),
                field!(event.draw_requested_at: i64),
                field!(event.vrf_requests: u32),
                field!(event.vrf_requested_slot: u64),
                field!(event.vrf_seed: [u8; 32]),
                field!(event.vrf_request: Pubkey),
            ],
        );
    }

    #[test]
    fn winner_drawn_serializes_to_its_documented_layout() {
        let event = WinnerDrawn {
            raffle: key(1),
            winner: key(2),
            winner_index: 1_234,
            total_entries: 3_405,
            drawn_at: 1_794_604_860,
            vrf_request: key(3),
            vrf_randomness: [4; 64],
        };

        assert_event(
            "WinnerDrawn",
            &event,
            &[
                field!(event.raffle: Pubkey),
                field!(event.winner: Pubkey),
                field!(event.winner_index: u32),
                field!(event.total_entries: u32),
                field!(event.drawn_at: i64),
                field!(event.vrf_request: Pubkey),
                field!(event.vrf_randomness: [u8; 64]),
            ],
        );
    }

    #[test]
    fn prize_claimed_serializes_to_its_documented_layout() {
        let event = PrizeClaimed {
            raffle: key(1),
            winner: key(2),
            prize_mint: key(3),
            amount: 500_000_000,
        };

        assert_event(
            "PrizeClaimed",
            &event,
            &[
                field!(event.raffle: Pubkey),
                field!(event.winner: Pubkey),
                field!(event.prize_mint: Pubkey),
                field!(event.amount: u64),
            ],
        );
    }

    #[test]
    fn prize_reclaimed_serializes_to_its_documented_layout() {
        let event = PrizeReclaimed {
            raffle: key(1),
            creator: key(2),
            caller: key(3),
            state: RaffleState::Settled.into(),
            prize_mint: key(4),
            amount: 500_000_000,
        };

        assert_event(
            "PrizeReclaimed",
            &event,
            &[
                field!(event.raffle: Pubkey),
                field!(event.creator: Pubkey),
                field!(event.caller: Pubkey),
                field!(event.state: u8),
                field!(event.prize_mint: Pubkey),
                field!(event.amount: u64),
            ],
        );
    }

    #[test]
    fn raffle_cancelled_serializes_to_its_documented_layout() {
        let event = RaffleCancelled {
            raffle: key(1),
            authority: key(2),
            cancel_reason: CancelReason::DrawStuck.into(),
        };

        assert_event(
            "RaffleCancelled",
            &event,
            &[
                field!(event.raffle: Pubkey),
                field!(event.authority: Pubkey),
                field!(event.cancel_reason: u8),
            ],
        );
    }

    #[test]
    fn entry_accounts_closed_serializes_to_its_documented_layout() {
        let event = EntryAccountsClosed {
            raffle: key(1),
            open_entry_accounts: 7,
            wallets: vec![key(2), key(3)],
            nfts: vec![key(4), key(5), key(6)],
        };

        assert_event(
            "EntryAccountsClosed",
            &event,
            &[
                field!(event.raffle: Pubkey),
                field!(event.open_entry_accounts: u32),
                field!(event.wallets: Vec<Pubkey>),
                field!(event.nfts: Vec<Pubkey>),
            ],
        );
    }

    #[test]
    fn raffle_closed_serializes_to_its_documented_layout() {
        let event = RaffleClosed {
            raffle: key(1),
            creator: key(2),
        };

        assert_event(
            "RaffleClosed",
            &event,
            &[field!(event.raffle: Pubkey), field!(event.creator: Pubkey)],
        );
    }
}

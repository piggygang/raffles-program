//! PDA seed prefixes, the timers the spec fixes, and the limits that size the
//! accounts. `#[constant]` also publishes a value in the IDL, which knows no
//! `usize`, so the limits clients need are `u8`.

use anchor_lang::prelude::*;

// Seed prefixes. Each kind of address has its own, and none begins another, so
// no two kinds can ever derive the same address.

/// `Config`, the singleton: `[CONFIG_SEED]`.
#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

/// A `CreatorEntry`: `[CREATOR_SEED, wallet]`.
#[constant]
pub const CREATOR_SEED: &[u8] = b"creator";

/// A `PrizeEntry`: `[PRIZE_SEED, [kind], address]`.
#[constant]
pub const PRIZE_SEED: &[u8] = b"prize";

/// A `Raffle`: `[RAFFLE_SEED, creator, nonce]`, the nonce as a little-endian
/// `u64`.
#[constant]
pub const RAFFLE_SEED: &[u8] = b"raffle";

/// A raffle's draw-fee vault, a system account without data:
/// `[VAULT_SEED, raffle]`.
#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

/// An `EntryPosition`: `[POSITION_SEED, raffle, wallet]`.
#[constant]
pub const POSITION_SEED: &[u8] = b"position";

/// An `EntryReceipt`: `[RECEIPT_SEED, raffle, nft]`, the NFT being a Token
/// Metadata mint or a Core asset.
#[constant]
pub const RECEIPT_SEED: &[u8] = b"receipt";

// Timers. They are constants rather than `Config` values so that none can
// change under a raffle that is already running.

const DAY_SECS: i64 = 24 * 60 * 60;

/// How long the winner has to claim the prize, counted from
/// `Raffle::drawn_at` (spec D5).
#[constant]
pub const CLAIM_WINDOW_SECS: i64 = 30 * DAY_SECS;

/// How long after the claim window a Drawn raffle's entry accounts become
/// closeable, whatever happened to the prize (spec E-S8).
#[constant]
pub const CLOSE_GRACE_SECS: i64 = 30 * DAY_SECS;

/// How long a raffle has to sit in Drawing before the admin may cancel it,
/// counted from `Raffle::draw_requested_at` (spec D15, E-D5).
#[constant]
pub const BREAK_GLASS_DELAY_SECS: i64 = 7 * DAY_SECS;

// Limits.

/// Membership rules in `Config` and on every raffle: one per Piggy collection.
pub const MEMBERSHIP_RULE_COUNT: usize = 3;

/// `EntryPosition::collections_seen` once an NFT of every membership rule is
/// entered: the Full Gang set.
#[constant]
pub const FULL_GANG: u8 = (1 << MEMBERSHIP_RULE_COUNT) - 1;

/// The highest per-wallet entry cap `Config` may hold; spec D1 sets 10. An
/// `EntryPosition` is sized for it, so a higher cap needs a program upgrade.
#[constant]
pub const MAX_ENTRY_CAP: u8 = 10;

/// Entry ranges an `EntryPosition` holds. Only a claim that adds entries adds
/// a range: at most one per entry under the cap, plus one claim that adds only
/// the Full Gang bonus.
pub const MAX_ENTRY_RANGES: usize = MAX_ENTRY_CAP as usize + 1;

/// The longest `Raffle::rules_uri`, in bytes (rules design §3).
#[constant]
pub const MAX_RULES_URI_LEN: u8 = 96;

/// The longest `Raffle::title`, in bytes.
#[constant]
pub const MAX_TITLE_LEN: u8 = 64;

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::testing::{assert_table, grouped},
    };

    const SEEDS: [&[u8]; 7] = [
        CONFIG_SEED,
        CREATOR_SEED,
        PRIZE_SEED,
        RAFFLE_SEED,
        VAULT_SEED,
        POSITION_SEED,
        RECEIPT_SEED,
    ];

    /// The seeds of an address are hashed end to end, without separators. Two
    /// kinds of address can only hash the same bytes if one kind's prefix
    /// begins the other's.
    #[test]
    fn no_seed_prefix_begins_another() {
        for (i, prefix) in SEEDS.iter().enumerate() {
            for (j, other) in SEEDS.iter().enumerate() {
                assert!(
                    i == j || !other.starts_with(prefix),
                    "{prefix:?} begins {other:?}"
                );
            }
        }
    }

    #[test]
    fn the_document_lists_every_address_with_its_seeds() {
        let mut rows = [
            ("`Config`", CONFIG_SEED, ""),
            ("`CreatorEntry`", CREATOR_SEED, ", wallet"),
            ("`PrizeEntry`", PRIZE_SEED, ", [kind], address"),
            ("`Raffle`", RAFFLE_SEED, ", creator, nonce"),
            ("Draw-fee vault", VAULT_SEED, ", raffle"),
            ("`EntryPosition`", POSITION_SEED, ", raffle, wallet"),
            ("`EntryReceipt`", RECEIPT_SEED, ", raffle, nft"),
        ]
        .map(|(address, prefix, rest)| {
            let prefix = std::str::from_utf8(prefix).unwrap();
            format!("| {address} | `[b\"{prefix}\"{rest}]` |")
        })
        .to_vec();
        rows.push(
            "| Token escrow | The Raffle's associated token account for `prize_mint` |".into(),
        );

        assert_table("## 2. Addresses", "", &rows);
    }

    #[test]
    fn the_document_lists_every_constant_with_its_value() {
        let rows = [
            (
                "CLAIM_WINDOW_SECS",
                u64::try_from(CLAIM_WINDOW_SECS).unwrap(),
            ),
            ("CLOSE_GRACE_SECS", u64::try_from(CLOSE_GRACE_SECS).unwrap()),
            (
                "BREAK_GLASS_DELAY_SECS",
                u64::try_from(BREAK_GLASS_DELAY_SECS).unwrap(),
            ),
            (
                "MEMBERSHIP_RULE_COUNT",
                u64::try_from(MEMBERSHIP_RULE_COUNT).unwrap(),
            ),
            ("FULL_GANG", u64::from(FULL_GANG)),
            ("MAX_ENTRY_CAP", u64::from(MAX_ENTRY_CAP)),
            ("MAX_ENTRY_RANGES", u64::try_from(MAX_ENTRY_RANGES).unwrap()),
            ("MAX_RULES_URI_LEN", u64::from(MAX_RULES_URI_LEN)),
            ("MAX_TITLE_LEN", u64::from(MAX_TITLE_LEN)),
        ]
        .map(|(name, value)| format!("| `{name}` | {} |", grouped(value)));

        assert_table("## 6. Constants", "", &rows);
    }

    #[test]
    fn the_timers_are_the_ones_the_spec_decides() {
        // D5, E-S8 and D15.
        assert_eq!(CLAIM_WINDOW_SECS, 2_592_000);
        assert_eq!(CLOSE_GRACE_SECS, 2_592_000);
        assert_eq!(BREAK_GLASS_DELAY_SECS, 604_800);
    }

    #[test]
    fn full_gang_is_one_bit_per_membership_rule() {
        assert_eq!(FULL_GANG, 0b111);
        assert_eq!(
            usize::try_from(FULL_GANG.count_ones()).unwrap(),
            MEMBERSHIP_RULE_COUNT
        );
    }
}

//! The account model: what the program stores, at which address, and who pays
//! the rent. docs/account-model.md has every layout, size and rent figure and
//! the state machine; the tests in these modules fail when it and the code
//! disagree.
//!
//! Conventions, for every account:
//! - `bump` comes first, so it is always at offset 8.
//! - Fixed-size fields come before variable-length ones, so each has one
//!   offset to `memcmp` on.
//! - Kinds, states and reasons are stored as a `u8` next to an enum with the
//!   same values. A byte the program doesn't know is an error, never a
//!   default.
//! - `_reserved` bytes are written as zero and never read.

/// Declares an enum that accounts store as one byte. The values are part of
/// the account layout: add new ones, never renumber or reuse one.
macro_rules! byte_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident, $error:ident {
            $($(#[$variant_meta:meta])* $variant:ident = $value:literal,)+
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $name {
            $($(#[$variant_meta])* $variant = $value,)+
        }

        impl $name {
            /// Every value, in byte order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }

        impl From<$name> for u8 {
            fn from(value: $name) -> Self {
                value as u8
            }
        }

        impl TryFrom<u8> for $name {
            type Error = anchor_lang::error::Error;

            fn try_from(byte: u8) -> anchor_lang::Result<Self> {
                match byte {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(anchor_lang::error!(crate::error::RaffleError::$error)),
                }
            }
        }
    };
}

pub mod config;
pub mod creator_entry;
pub mod entry_position;
pub mod entry_receipt;
pub mod membership_rule;
pub mod prize_entry;
pub mod raffle;
#[cfg(test)]
pub(crate) mod testing;

pub use {
    config::*, creator_entry::*, entry_position::*, entry_receipt::*, membership_rule::*,
    prize_entry::*, raffle::*,
};

#[cfg(test)]
mod tests {
    use {
        super::{
            testing::{assert_table, doc_says, grouped},
            *,
        },
        anchor_spl::token::TokenAccount,
    };

    /// Rent per byte on devnet and mainnet on 2026-11-15, and the bytes each
    /// account is charged on top of its data. Only the document's figures
    /// come from these: the program reads the Rent sysvar.
    const LAMPORTS_PER_BYTE: u64 = 5_080;
    const ACCOUNT_OVERHEAD: u64 = 128;

    fn rent(space: usize) -> u64 {
        (ACCOUNT_OVERHEAD + u64::try_from(space).unwrap()) * LAMPORTS_PER_BYTE
    }

    #[test]
    fn every_account_has_its_documented_size_and_rent() {
        assert!(doc_says(&format!(
            "{} lamports per byte, plus {ACCOUNT_OVERHEAD} bytes of overhead",
            grouped(LAMPORTS_PER_BYTE)
        )));
        let rows = [
            ("`Config`", Config::SPACE),
            ("`CreatorEntry`", CreatorEntry::SPACE),
            ("`PrizeEntry`", PrizeEntry::SPACE),
            ("`Raffle`", Raffle::SPACE),
            // A system account without data.
            ("Draw-fee vault", 0),
            ("Token escrow", TokenAccount::LEN),
            ("`EntryPosition`", EntryPosition::SPACE),
            ("`EntryReceipt`", EntryReceipt::SPACE),
        ]
        .map(|(account, space)| format!("| {account} | {space} | {} |", grouped(rent(space))));
        assert_table("## 3. Sizes and rent", "", &rows);
    }

    #[test]
    fn each_actor_fronts_the_documented_rent() {
        let (position, receipt) = (rent(EntryPosition::SPACE), rent(EntryReceipt::SPACE));
        let rows = [
            (
                "A wallet's first pig in a raffle",
                "1 position, 1 receipt",
                position + receipt,
            ),
            ("Each further pig", "1 receipt", receipt),
            (
                "Ten pigs, the cap",
                "1 position, 10 receipts",
                position + 10 * receipt,
            ),
            (
                "Twelve pigs, the most: the cap plus two that complete the Full Gang set",
                "1 position, 12 receipts",
                position + 12 * receipt,
            ),
        ]
        .map(|(claim, accounts, lamports)| {
            format!("| {claim} | {accounts} | {} |", grouped(lamports))
        });
        // The table sits inside a list item.
        assert_table("## 3. Sizes and rent", "  ", &rows);

        let creator = grouped(rent(Raffle::SPACE) + rent(TokenAccount::LEN));
        assert!(
            doc_says(&format!(
                "The Raffle and a token escrow are {creator} lamports"
            )),
            "{creator}"
        );
        let authority = grouped(20 * rent(CreatorEntry::SPACE) + 60 * rent(PrizeEntry::SPACE));
        assert!(
            doc_says(&format!(
                "20 creators and 60 prize entries lock {authority} lamports"
            )),
            "{authority}"
        );
        // What keeping the raffle and the NFT in a receipt costs.
        let seeds = grouped(64 * LAMPORTS_PER_BYTE);
        assert!(
            doc_says(&format!("costs {seeds} lamports of fronted rent per pig")),
            "{seeds}"
        );
    }

    #[test]
    fn grouped_writes_thousands_separators() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(650_240), "650,240");
        assert_eq!(grouped(15_524_480), "15,524,480");
    }
}

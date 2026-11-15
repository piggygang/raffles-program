use {
    crate::constants::{MAX_ENTRY_RANGES, POSITION_SEED},
    anchor_lang::prelude::*,
};

/// A run of consecutive entry indexes that one claim added.
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub struct EntryRange {
    /// The first index: `Raffle::total_entries` when the claim ran.
    pub start: u32,
    /// How many entries: at most the cap plus the Full Gang bonus.
    pub count: u8,
}

impl EntryRange {
    pub fn contains(&self, index: u32) -> bool {
        index
            .checked_sub(self.start)
            .is_some_and(|offset| offset < u32::from(self.count))
    }
}

/// One wallet's entries in one raffle, at `[POSITION_SEED, raffle, wallet]`:
/// the cap counter, the collections it has entered, the Full Gang bonus flag
/// and the entry indexes it holds (spec R4).
///
/// The wallet's first claim in the raffle creates it and pays the rent, and
/// `close_entry_accounts` returns it there once the raffle allows closing.
#[account]
#[derive(Debug, PartialEq, Eq, InitSpace)]
pub struct EntryPosition {
    pub bump: u8,
    /// The raffle, also a seed.
    pub raffle: Pubkey,
    /// Holds the entries, so it wins the prize if one is drawn. Also a seed,
    /// and the payer of this account's rent: the only address the close may
    /// refund.
    pub wallet: Pubkey,
    /// Per-NFT entries claimed, which the raffle's cap limits. The bonus
    /// doesn't count.
    pub entries_claimed: u8,
    /// Bit `i` is set once an NFT of membership rule `i` is entered. All of
    /// them is the Full Gang set.
    pub collections_seen: u8,
    /// The Full Gang bonus was added. It is added once.
    pub bonus_awarded: bool,
    /// How many of `ranges` are in use.
    pub range_count: u8,
    /// The entry indexes this wallet holds, in claim order. A wallet that
    /// claims in several transactions gets several ranges, because other
    /// wallets claim in between.
    pub ranges: [EntryRange; MAX_ENTRY_RANGES],
}

impl EntryPosition {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    pub fn find_address(raffle: &Pubkey, wallet: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[POSITION_SEED, raffle.as_ref(), wallet.as_ref()],
            &crate::ID,
        )
    }

    /// The ranges in use.
    pub fn ranges(&self) -> &[EntryRange] {
        self.ranges
            .get(..usize::from(self.range_count))
            .unwrap_or(&self.ranges)
    }

    /// Whether this wallet holds entry `index`: the draw's test of the
    /// position passed as the winner's.
    pub fn holds_entry(&self, index: u32) -> bool {
        self.ranges().iter().any(|range| range.contains(index))
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::testing::{assert_layout, assert_record, field},
    };

    fn position(ranges: &[(u32, u8)]) -> EntryPosition {
        let mut position = EntryPosition {
            bump: 250,
            raffle: Pubkey::new_from_array([1; 32]),
            wallet: Pubkey::new_from_array([2; 32]),
            entries_claimed: 0,
            collections_seen: 0,
            bonus_awarded: false,
            range_count: u8::try_from(ranges.len()).unwrap(),
            ranges: [EntryRange::default(); MAX_ENTRY_RANGES],
        };
        for (slot, &(start, count)) in position.ranges.iter_mut().zip(ranges) {
            *slot = EntryRange { start, count };
        }
        position
    }

    #[test]
    fn entry_position_serializes_to_its_documented_layout() {
        // Every range in use: ten single-entry claims, then the bonus alone.
        let ranges: Vec<(u32, u8)> = (0..10).map(|claim| (claim * 100, 1)).collect();
        let mut position = position(&[ranges, vec![(1_000, 3)]].concat());
        position.entries_claimed = 10;
        position.collections_seen = 0b111;
        position.bonus_awarded = true;

        assert_layout(
            "EntryPosition",
            &position,
            &[
                field!(position.bump: u8),
                field!(position.raffle: Pubkey),
                field!(position.wallet: Pubkey),
                field!(position.entries_claimed: u8),
                field!(position.collections_seen: u8),
                field!(position.bonus_awarded: bool),
                field!(position.range_count: u8),
                field!(position.ranges: [EntryRange; 11]),
            ],
            EntryPosition::SPACE,
        );
        // The size PIG-133 measured the claim with.
        assert_eq!(EntryPosition::SPACE, 132);
    }

    #[test]
    fn entry_range_serializes_to_its_documented_layout() {
        let range = EntryRange {
            start: 0x0403_0201,
            count: 13,
        };

        assert_record(
            "EntryRange",
            &range,
            &[field!(range.start: u32), field!(range.count: u8)],
            EntryRange::INIT_SPACE,
        );
    }

    #[test]
    fn entry_position_address_derives_from_the_raffle_and_the_wallet() {
        let raffle = Pubkey::new_from_array([7; 32]);
        let wallet = Pubkey::new_from_array([8; 32]);

        assert_eq!(
            EntryPosition::find_address(&raffle, &wallet),
            Pubkey::find_program_address(
                &[b"position", raffle.as_ref(), wallet.as_ref()],
                &crate::ID
            )
        );
    }

    #[test]
    fn a_range_holds_its_first_index_through_its_last() {
        let range = EntryRange {
            start: 10,
            count: 3,
        };

        assert!(!range.contains(9));
        assert!(range.contains(10));
        assert!(range.contains(12));
        assert!(!range.contains(13));
    }

    #[test]
    fn an_empty_range_holds_nothing() {
        let range = EntryRange {
            start: 10,
            count: 0,
        };

        assert!(!range.contains(10));
    }

    #[test]
    fn a_range_at_the_top_of_the_index_space_does_not_overflow() {
        let range = EntryRange {
            start: u32::MAX - 1,
            count: 13,
        };

        assert!(range.contains(u32::MAX));
        assert!(!range.contains(0));
    }

    #[test]
    fn a_position_holds_the_entries_of_all_its_ranges_and_no_others() {
        // Three claims; other wallets claimed 3..10 and 12..20 in between.
        let position = position(&[(0, 3), (10, 2), (20, 4)]);

        for index in [0, 2, 10, 11, 20, 23] {
            assert!(position.holds_entry(index), "{index}");
        }
        for index in [3, 9, 12, 19, 24, u32::MAX] {
            assert!(!position.holds_entry(index), "{index}");
        }
    }

    #[test]
    fn a_position_ignores_ranges_past_its_range_count() {
        let mut position = position(&[(0, 3), (10, 2)]);
        position.range_count = 1;

        assert_eq!(position.ranges(), [EntryRange { start: 0, count: 3 }]);
        assert!(!position.holds_entry(10));
    }

    #[test]
    fn a_new_position_holds_nothing() {
        let position = position(&[]);

        assert!(position.ranges().is_empty());
        assert!(!position.holds_entry(0));
    }

    #[test]
    fn a_range_count_past_the_array_reads_as_every_range() {
        let mut position = position(&[(0, 1); MAX_ENTRY_RANGES]);
        position.range_count = u8::MAX;

        assert_eq!(position.ranges().len(), MAX_ENTRY_RANGES);
    }
}

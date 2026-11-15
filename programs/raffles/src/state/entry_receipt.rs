use {crate::constants::RECEIPT_SEED, anchor_lang::prelude::*};

/// Marks an NFT as entered in a raffle. The address derives from the raffle
/// and the NFT, so the account existing is the mark: an NFT enters a raffle
/// once, whoever holds it (spec §5).
///
/// The wallet that enters the NFT pays the rent, and `close_entry_accounts`
/// returns it there once the raffle allows closing.
///
/// It starts like an `EntryPosition`: `bump`, `raffle`, `wallet`. A close
/// reads the same two fields whichever of the two it is given.
#[account]
#[derive(Debug, PartialEq, Eq, InitSpace)]
pub struct EntryReceipt {
    pub bump: u8,
    /// The raffle, also a seed. Kept in the account so that every receipt of
    /// a raffle can be found and closed without an index.
    pub raffle: Pubkey,
    /// Entered the NFT and paid this account's rent: the only address the
    /// close may refund.
    pub wallet: Pubkey,
    /// The Token Metadata mint, or the Core asset. Also a seed.
    pub nft: Pubkey,
}

impl EntryReceipt {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    /// On-chain, the claim has to find the bump this way too, rather than
    /// take one from the client: a second bump would admit the NFT twice.
    pub fn find_address(raffle: &Pubkey, nft: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[RECEIPT_SEED, raffle.as_ref(), nft.as_ref()], &crate::ID)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::testing::{assert_layout, field},
    };

    #[test]
    fn entry_receipt_serializes_to_its_documented_layout() {
        let receipt = EntryReceipt {
            bump: 251,
            raffle: Pubkey::new_from_array([1; 32]),
            wallet: Pubkey::new_from_array([2; 32]),
            nft: Pubkey::new_from_array([3; 32]),
        };

        assert_layout(
            "EntryReceipt",
            &receipt,
            &[
                field!(receipt.bump: u8),
                field!(receipt.raffle: Pubkey),
                field!(receipt.wallet: Pubkey),
                field!(receipt.nft: Pubkey),
            ],
            EntryReceipt::SPACE,
        );
        // The size PIG-133 measured the claim with.
        assert_eq!(EntryReceipt::SPACE, 105);
    }

    #[test]
    fn a_receipt_and_a_position_start_with_the_same_fields() {
        use crate::{
            constants::MAX_ENTRY_RANGES,
            state::{EntryPosition, EntryRange},
        };
        let raffle = Pubkey::new_from_array([1; 32]);
        let wallet = Pubkey::new_from_array([2; 32]);
        let receipt = EntryReceipt {
            bump: 251,
            raffle,
            wallet,
            nft: Pubkey::new_from_array([3; 32]),
        };
        let position = EntryPosition {
            bump: 251,
            raffle,
            wallet,
            entries_claimed: 1,
            collections_seen: 1,
            bonus_awarded: false,
            range_count: 1,
            ranges: [EntryRange { start: 0, count: 1 }; MAX_ENTRY_RANGES],
        };
        let (mut receipt_bytes, mut position_bytes) = (Vec::new(), Vec::new());
        receipt.try_serialize(&mut receipt_bytes).unwrap();
        position.try_serialize(&mut position_bytes).unwrap();

        // After the discriminators: the bump, the raffle and the wallet.
        assert_eq!(receipt_bytes[8..73], position_bytes[8..73]);
        assert_eq!(receipt_bytes[9..41], raffle.to_bytes());
        assert_eq!(receipt_bytes[41..73], wallet.to_bytes());
    }

    #[test]
    fn entry_receipt_address_derives_from_the_raffle_and_the_nft() {
        let raffle = Pubkey::new_from_array([7; 32]);
        let nft = Pubkey::new_from_array([8; 32]);

        assert_eq!(
            EntryReceipt::find_address(&raffle, &nft),
            Pubkey::find_program_address(&[b"receipt", raffle.as_ref(), nft.as_ref()], &crate::ID)
        );
    }
}

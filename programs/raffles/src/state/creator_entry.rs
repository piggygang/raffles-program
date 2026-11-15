use {crate::constants::CREATOR_SEED, anchor_lang::prelude::*};

/// A wallet that may create raffles. The Creator Registry is the set of these
/// accounts (registries §3): an entry existing is the membership, so
/// `creator_add` creates one and `creator_remove` closes it.
///
/// Only `create_raffle` reads an entry, and a raffle keeps no reference to
/// one, so removing a creator never touches a raffle that already exists.
#[account]
#[derive(Debug, PartialEq, Eq, InitSpace)]
pub struct CreatorEntry {
    pub bump: u8,
    /// The approved creator, also the seed. It may be off-curve: a partner's
    /// own Squads vault can create raffles.
    pub wallet: Pubkey,
    /// Cluster time of `creator_add`.
    pub added_at: i64,
    /// Who the wallet belongs to: UTF-8 padded with zero bytes, never empty.
    /// A project's name, never personal data.
    pub label: [u8; CreatorEntry::LABEL_LEN],
    /// Zero. Room for a per-creator policy.
    pub _reserved: [u8; 32],
}

impl CreatorEntry {
    pub const LABEL_LEN: usize = 32;
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    pub fn find_address(wallet: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[CREATOR_SEED, wallet.as_ref()], &crate::ID)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::testing::{assert_layout, field},
    };

    #[test]
    fn creator_entry_serializes_to_the_registries_layout() {
        let mut label = [0; CreatorEntry::LABEL_LEN];
        label[..6].copy_from_slice(b"GM bot");
        let entry = CreatorEntry {
            bump: 253,
            wallet: Pubkey::new_from_array([1; 32]),
            added_at: 1_794_000_000,
            label,
            _reserved: [0; 32],
        };

        assert_layout(
            "CreatorEntry",
            &entry,
            &[
                field!(entry.bump: u8),
                field!(entry.wallet: Pubkey),
                field!(entry.added_at: i64),
                field!(entry.label: [u8; 32]),
                field!(entry._reserved: [u8; 32]),
            ],
            CreatorEntry::SPACE,
        );
        // The size the registries design fixes.
        assert_eq!(CreatorEntry::SPACE, 113);
    }

    #[test]
    fn creator_entry_address_derives_from_the_wallet() {
        let wallet = Pubkey::new_from_array([7; 32]);

        assert_eq!(
            CreatorEntry::find_address(&wallet),
            Pubkey::find_program_address(&[b"creator", wallet.as_ref()], &crate::ID)
        );
    }
}

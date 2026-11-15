use {crate::constants::PRIZE_SEED, anchor_lang::prelude::*};

byte_enum! {
    /// What a `PrizeEntry` admits as a prize (registries §3). Byte 3 is kept
    /// for a single Core asset.
    pub enum PrizeKind, InvalidPrizeKind {
        /// A Metaplex Certified Collection, named by its collection mint: any
        /// NFT whose Token Metadata collection is that mint, verified.
        Collection = 0,
        /// One classic SPL Token mint: a single NFT or a fungible token.
        Mint = 1,
        /// A Metaplex Core collection: any asset whose update authority is
        /// that collection (spec D22).
        CoreCollection = 2,
    }
}

/// An asset, or a collection of them, that may be a raffle's prize. The Prize
/// Allowlist is the set of these accounts (registries §3): an entry existing
/// is the permission, so `allowlist_add` creates one and `allowlist_remove`
/// closes it.
///
/// Only `create_raffle` reads an entry, and a raffle keeps no reference to
/// one, so delisting a prize never touches a raffle that already holds it.
#[account]
#[derive(Debug, PartialEq, Eq, InitSpace)]
pub struct PrizeEntry {
    pub bump: u8,
    /// A [`PrizeKind`]. It is a seed too, so an address can have one entry
    /// per kind and each says what it admits.
    pub kind: u8,
    /// What the entry admits, read according to `kind`. Also a seed.
    pub address: Pubkey,
    /// Cluster time of `allowlist_add`.
    pub added_at: i64,
    /// Zero. Room for a per-prize policy.
    pub _reserved: [u8; 32],
}

impl PrizeEntry {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    pub fn kind(&self) -> Result<PrizeKind> {
        PrizeKind::try_from(self.kind)
    }

    pub fn find_address(kind: PrizeKind, address: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[PRIZE_SEED, &[kind.into()], address.as_ref()], &crate::ID)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::testing::{assert_enum, assert_layout, field},
    };

    #[test]
    fn prize_entry_serializes_to_the_registries_layout() {
        let entry = PrizeEntry {
            bump: 252,
            kind: PrizeKind::CoreCollection.into(),
            address: Pubkey::new_from_array([1; 32]),
            added_at: 1_794_000_000,
            _reserved: [0; 32],
        };

        assert_layout(
            "PrizeEntry",
            &entry,
            &[
                field!(entry.bump: u8),
                field!(entry.kind: u8),
                field!(entry.address: Pubkey),
                field!(entry.added_at: i64),
                field!(entry._reserved: [u8; 32]),
            ],
            PrizeEntry::SPACE,
        );
        // The size the registries design fixes.
        assert_eq!(PrizeEntry::SPACE, 82);
        assert_eq!(entry.kind().unwrap(), PrizeKind::CoreCollection);
    }

    #[test]
    fn prize_kind_bytes_are_stable() {
        // 3 is kept for a single Core asset, and isn't valid yet.
        assert_enum("PrizeKind", PrizeKind::ALL, &[0, 1, 2]);
    }

    #[test]
    fn prize_entry_address_derives_from_the_kind_and_the_address() {
        let address = Pubkey::new_from_array([7; 32]);
        let (collection, _) = PrizeEntry::find_address(PrizeKind::Collection, &address);
        let (mint, _) = PrizeEntry::find_address(PrizeKind::Mint, &address);

        assert_eq!(
            collection,
            Pubkey::find_program_address(&[b"prize", &[0], address.as_ref()], &crate::ID).0
        );
        assert_eq!(
            mint,
            Pubkey::find_program_address(&[b"prize", &[1], address.as_ref()], &crate::ID).0
        );
        assert_ne!(collection, mint);
    }
}

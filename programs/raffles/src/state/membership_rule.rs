use anchor_lang::prelude::*;

byte_enum! {
    /// How a `MembershipRule` recognises a Piggy (holder-proof §4). Byte 2 is
    /// kept for a rule on a certified Token Metadata collection.
    pub enum RuleKind, InvalidRuleKind {
        /// A Token Metadata NFT with the rule's creator verified, and the
        /// rule's update authority and symbol.
        TmCreator = 0,
        /// A Metaplex Core asset whose update authority is the rule's
        /// collection.
        CoreCollection = 1,
    }
}

/// The test that makes an NFT a Piggy of one collection (spec D19).
///
/// A flat record of 75 bytes rather than a Rust enum, whose Borsh size would
/// vary: `Config` and every `Raffle` hold three, and stay fixed-size.
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub struct MembershipRule {
    /// A [`RuleKind`].
    pub kind: u8,
    /// `TmCreator`: the verified creator. `CoreCollection`: the collection.
    pub address: Pubkey,
    /// `TmCreator`: the metadata update authority. `CoreCollection`: zero.
    pub update_authority: Pubkey,
    /// `TmCreator`: the symbol, padded with zero bytes. `CoreCollection`:
    /// zero.
    pub symbol: [u8; MembershipRule::SYMBOL_LEN],
}

impl MembershipRule {
    /// Token Metadata's longest symbol.
    pub const SYMBOL_LEN: usize = 10;

    pub fn kind(&self) -> Result<RuleKind> {
        RuleKind::try_from(self.kind)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::testing::{assert_enum, assert_record, field},
    };

    #[test]
    fn membership_rule_is_the_holder_proof_layout() {
        let rule = MembershipRule {
            kind: RuleKind::TmCreator.into(),
            address: Pubkey::new_from_array([1; 32]),
            update_authority: Pubkey::new_from_array([2; 32]),
            symbol: *b"PSG\0\0\0\0\0\0\0",
        };

        assert_record(
            "MembershipRule",
            &rule,
            &[
                field!(rule.kind: u8),
                field!(rule.address: Pubkey),
                field!(rule.update_authority: Pubkey),
                field!(rule.symbol: [u8; 10]),
            ],
            MembershipRule::INIT_SPACE,
        );
        assert_eq!(MembershipRule::INIT_SPACE, 75);
    }

    #[test]
    fn rule_kind_bytes_are_stable() {
        // 2 is kept for the certified-collection rule, and isn't valid yet.
        assert_enum("RuleKind", RuleKind::ALL, &[0, 1]);
    }
}

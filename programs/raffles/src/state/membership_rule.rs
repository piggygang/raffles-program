use {crate::error::RaffleError, anchor_lang::prelude::*};

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

    /// What a rule has to pass before `Config` takes it (holder-proof §5).
    /// The program can't tell whether the addresses are the Piggy
    /// collections: the authority vouches for that.
    ///
    /// - The kind is one the program knows, and the address isn't zero.
    /// - `TmCreator`: an update authority, and a symbol Token Metadata could
    ///   hold, written the one way a claim compares it.
    /// - `CoreCollection`: neither. A field the kind doesn't read stays zero,
    ///   so two rules that admit the same NFTs are the same bytes.
    pub fn validate(&self) -> Result<()> {
        let kind = self.kind()?;
        require!(
            self.address != Pubkey::default(),
            RaffleError::InvalidRuleAddress
        );
        match kind {
            RuleKind::TmCreator => {
                require!(
                    self.update_authority != Pubkey::default(),
                    RaffleError::InvalidRuleUpdateAuthority
                );
                require!(self.has_padded_symbol(), RaffleError::InvalidRuleSymbol);
            }
            RuleKind::CoreCollection => {
                require!(
                    self.update_authority == Pubkey::default(),
                    RaffleError::InvalidRuleUpdateAuthority
                );
                require!(
                    self.symbol == [0; Self::SYMBOL_LEN],
                    RaffleError::InvalidRuleSymbol
                );
            }
        }
        Ok(())
    }

    /// Whether `symbol` is 1 to `SYMBOL_LEN` bytes of UTF-8 without a zero
    /// byte, then only zero bytes. Token Metadata's symbol is a string, so
    /// anything else could never match an NFT.
    fn has_padded_symbol(&self) -> bool {
        let len = self
            .symbol
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(Self::SYMBOL_LEN);
        let (text, padding) = self.symbol.split_at(len);
        len > 0 && padding.iter().all(|&byte| byte == 0) && core::str::from_utf8(text).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::testing::{assert_enum, assert_error, assert_record, field, key},
    };

    /// A Token Metadata rule with `symbol`, padded with zero bytes.
    fn tm_rule(symbol: &[u8]) -> MembershipRule {
        let mut padded = [0; MembershipRule::SYMBOL_LEN];
        padded[..symbol.len()].copy_from_slice(symbol);
        MembershipRule {
            kind: RuleKind::TmCreator.into(),
            address: key(1),
            update_authority: key(2),
            symbol: padded,
        }
    }

    fn core_rule() -> MembershipRule {
        MembershipRule {
            kind: RuleKind::CoreCollection.into(),
            address: key(3),
            ..MembershipRule::default()
        }
    }

    #[test]
    fn the_rules_of_the_piggy_collections_are_valid() {
        tm_rule(b"PSG").validate().unwrap();
        tm_rule(b"PGG").validate().unwrap();
        core_rule().validate().unwrap();
    }

    #[test]
    fn a_rule_of_a_kind_the_program_does_not_know_is_refused() {
        // 2 is kept for the certified-collection rule, which isn't built.
        for kind in [2, 3, 255] {
            let rule = MembershipRule {
                kind,
                ..tm_rule(b"PSG")
            };

            assert_error(rule.validate(), RaffleError::InvalidRuleKind);
        }
    }

    #[test]
    fn a_rule_needs_an_address() {
        for rule in [tm_rule(b"PSG"), core_rule()] {
            let rule = MembershipRule {
                address: Pubkey::default(),
                ..rule
            };

            assert_error(rule.validate(), RaffleError::InvalidRuleAddress);
        }
    }

    #[test]
    fn a_token_metadata_rule_needs_an_update_authority() {
        let rule = MembershipRule {
            update_authority: Pubkey::default(),
            ..tm_rule(b"PSG")
        };

        assert_error(rule.validate(), RaffleError::InvalidRuleUpdateAuthority);
    }

    #[test]
    fn a_token_metadata_symbol_is_one_to_ten_bytes() {
        tm_rule(b"P").validate().unwrap();
        // Token Metadata's longest: no padding is left.
        tm_rule(b"PIGGYGANGS").validate().unwrap();
        // Two bytes each in UTF-8.
        tm_rule("ŠČŽ".as_bytes()).validate().unwrap();

        assert_error(tm_rule(b"").validate(), RaffleError::InvalidRuleSymbol);
    }

    /// A claim compares the NFT's symbol, without its trailing zero bytes,
    /// with the rule's. A rule with a zero byte inside its symbol would
    /// compare differently from how it reads.
    #[test]
    fn a_token_metadata_symbol_has_no_zero_byte_before_its_padding() {
        for symbol in [&b"\0PSG"[..], b"P\0G", b"PS\0\0G", b"\0\0\0\0\0\0\0\0\0G"] {
            assert_error(tm_rule(symbol).validate(), RaffleError::InvalidRuleSymbol);
        }
    }

    #[test]
    fn a_token_metadata_symbol_is_utf8() {
        // A lone continuation byte, and a two-byte character cut in half.
        for symbol in [&[0x80][..], &[b'P', 0xc5]] {
            assert_error(tm_rule(symbol).validate(), RaffleError::InvalidRuleSymbol);
        }
    }

    /// Otherwise two Core rules for one collection could differ in bytes the
    /// claim never reads, pass as two rules, and fill two Full Gang bits.
    #[test]
    fn a_core_rule_has_no_update_authority_and_no_symbol() {
        let with_update_authority = MembershipRule {
            update_authority: key(2),
            ..core_rule()
        };
        let with_symbol = MembershipRule {
            symbol: *b"PG\0\0\0\0\0\0\0\0",
            ..core_rule()
        };

        assert_error(
            with_update_authority.validate(),
            RaffleError::InvalidRuleUpdateAuthority,
        );
        assert_error(with_symbol.validate(), RaffleError::InvalidRuleSymbol);
    }

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

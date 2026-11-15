use {
    super::MembershipRule,
    crate::constants::{CONFIG_SEED, MEMBERSHIP_RULE_COUNT},
    anchor_lang::prelude::*,
};

/// The program's settings: one account, at `[CONFIG_SEED]`, that `init_config`
/// creates and nothing closes.
///
/// A raffle copies the rules, the cap, the bonus and the duration bounds when
/// it is created (spec D14), so an edit here reaches only later raffles. The
/// authority and `paused` are the only fields read live.
#[account]
#[derive(Debug, PartialEq, Eq, InitSpace)]
pub struct Config {
    pub bump: u8,
    /// The admin. On mainnet the Squads vault, never the multisig account
    /// (registries §6). It creates raffles without a `CreatorEntry`.
    pub authority: Pubkey,
    /// Set by `set_authority`; becomes `authority` once it signs
    /// `accept_authority`. Zero when no transfer is pending.
    pub pending_authority: Pubkey,
    /// Blocks `create_raffle`, `claim_entries` and `request_draw`. Nothing
    /// that pays out a prize or returns rent reads it (spec §10).
    pub paused: bool,
    /// One rule per Piggy collection. Rule `i` is bit `i` of
    /// `EntryPosition::collections_seen`.
    pub membership_rules: [MembershipRule; MEMBERSHIP_RULE_COUNT],
    /// Per-NFT entries one wallet may claim in a raffle, 1 to
    /// `MAX_ENTRY_CAP`. A raffle may lower it for itself (spec D6).
    pub entry_cap: u8,
    /// Entries the Full Gang bonus adds, on top of the cap. Together with the
    /// cap it has to fit `EntryRange::count`.
    pub full_gang_bonus: u8,
    /// The shortest raffle, in seconds.
    pub min_duration: u32,
    /// The longest raffle, in seconds.
    pub max_duration: u32,
    /// Zero. Room for a later setting without a realloc.
    pub _reserved: [u8; 64],
}

impl Config {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    pub fn find_address() -> (Pubkey, u8) {
        Pubkey::find_program_address(&[CONFIG_SEED], &crate::ID)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::{
            testing::{assert_layout, field},
            RuleKind,
        },
    };

    #[test]
    fn config_serializes_to_its_documented_layout() {
        let config = Config {
            bump: 254,
            authority: Pubkey::new_from_array([1; 32]),
            pending_authority: Pubkey::new_from_array([2; 32]),
            paused: true,
            membership_rules: [
                MembershipRule {
                    kind: RuleKind::TmCreator.into(),
                    address: Pubkey::new_from_array([3; 32]),
                    update_authority: Pubkey::new_from_array([4; 32]),
                    symbol: *b"PSG\0\0\0\0\0\0\0",
                },
                MembershipRule {
                    kind: RuleKind::TmCreator.into(),
                    address: Pubkey::new_from_array([3; 32]),
                    update_authority: Pubkey::new_from_array([4; 32]),
                    symbol: *b"PGG\0\0\0\0\0\0\0",
                },
                MembershipRule {
                    kind: RuleKind::CoreCollection.into(),
                    address: Pubkey::new_from_array([5; 32]),
                    ..MembershipRule::default()
                },
            ],
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 2_592_000,
            _reserved: [0; 64],
        };

        assert_layout(
            "Config",
            &config,
            &[
                field!(config.bump: u8),
                field!(config.authority: Pubkey),
                field!(config.pending_authority: Pubkey),
                field!(config.paused: bool),
                field!(config.membership_rules: [MembershipRule; 3]),
                field!(config.entry_cap: u8),
                field!(config.full_gang_bonus: u8),
                field!(config.min_duration: u32),
                field!(config.max_duration: u32),
                field!(config._reserved: [u8; 64]),
            ],
            Config::SPACE,
        );
    }

    #[test]
    fn config_address_derives_from_its_seed() {
        assert_eq!(
            Config::find_address(),
            Pubkey::find_program_address(&[b"config"], &crate::ID)
        );
    }
}

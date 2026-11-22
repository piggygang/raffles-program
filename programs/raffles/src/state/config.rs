use {
    super::MembershipRule,
    crate::{
        constants::{CONFIG_SEED, MAX_ENTRY_CAP, MEMBERSHIP_RULE_COUNT},
        error::RaffleError,
    },
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

    /// The config `init_config` writes: `settings` under `authority`, with no
    /// transfer pending. The caller has validated `settings`.
    pub fn new(bump: u8, authority: Pubkey, settings: &ConfigSettings) -> Self {
        Self {
            bump,
            authority,
            pending_authority: Pubkey::default(),
            paused: settings.paused,
            membership_rules: settings.membership_rules,
            entry_cap: settings.entry_cap,
            full_gang_bonus: settings.full_gang_bonus,
            min_duration: settings.min_duration,
            max_duration: settings.max_duration,
            _reserved: [0; 64],
        }
    }

    /// Everything the authority sets, as it is now.
    pub fn settings(&self) -> ConfigSettings {
        ConfigSettings {
            paused: self.paused,
            membership_rules: self.membership_rules,
            entry_cap: self.entry_cap,
            full_gang_bonus: self.full_gang_bonus,
            min_duration: self.min_duration,
            max_duration: self.max_duration,
        }
    }

    /// Replaces the settings, and nothing else: not the authority, nor a
    /// pending transfer. The caller has validated `settings`.
    pub fn set_settings(&mut self, settings: &ConfigSettings) {
        self.paused = settings.paused;
        self.membership_rules = settings.membership_rules;
        self.entry_cap = settings.entry_cap;
        self.full_gang_bonus = settings.full_gang_bonus;
        self.min_duration = settings.min_duration;
        self.max_duration = settings.max_duration;
    }

    /// The guard of the pause matrix (spec §10): `create_raffle`,
    /// `claim_entries` and `request_draw` call it, and nothing else does.
    /// What pays out a prize, returns rent or belongs to the authority stays
    /// open under a pause.
    pub fn require_not_paused(&self) -> Result<()> {
        require!(!self.paused, RaffleError::Paused);
        Ok(())
    }
}

/// What the authority sets in `Config`: `init_config` takes all of it, and
/// both config events carry all of it.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigSettings {
    /// Whether the program starts, or is, paused.
    pub paused: bool,
    /// One rule per Piggy collection, each valid and no two the same.
    pub membership_rules: [MembershipRule; MEMBERSHIP_RULE_COUNT],
    /// 1 to `MAX_ENTRY_CAP`.
    pub entry_cap: u8,
    /// With `entry_cap`, at most 255.
    pub full_gang_bonus: u8,
    /// Seconds, above zero.
    pub min_duration: u32,
    /// Seconds, at least `min_duration`.
    pub max_duration: u32,
}

impl ConfigSettings {
    /// What `Config` always holds true (docs/instructions.md, section 4).
    /// `init_config` and `update_config` check the settings they are about to
    /// write, whole: an update is judged by what it leaves, not by what it
    /// names.
    pub fn validate(&self) -> Result<()> {
        require!(
            (1..=MAX_ENTRY_CAP).contains(&self.entry_cap),
            RaffleError::InvalidEntryCap
        );
        // The claim that completes the Full Gang set can add the cap and the
        // bonus in one range, and `EntryRange::count` is a `u8`.
        require!(
            self.entry_cap.checked_add(self.full_gang_bonus).is_some(),
            RaffleError::InvalidFullGangBonus
        );
        require!(
            self.min_duration > 0 && self.min_duration <= self.max_duration,
            RaffleError::InvalidDurationBounds
        );
        for (slot, rule) in self.membership_rules.iter().enumerate() {
            rule.validate()?;
            // Two slots with one rule would let two NFTs of one collection
            // set two bits of `collections_seen`, and earn the Full Gang
            // bonus without the full set (holder-proof §5).
            require!(
                !self.membership_rules[..slot].contains(rule),
                RaffleError::DuplicateMembershipRule
            );
        }
        Ok(())
    }

    /// The settings with what `update` names replaced.
    pub fn with(self, update: &ConfigUpdate) -> Self {
        Self {
            paused: update.paused.unwrap_or(self.paused),
            membership_rules: update.membership_rules.unwrap_or(self.membership_rules),
            entry_cap: update.entry_cap.unwrap_or(self.entry_cap),
            full_gang_bonus: update.full_gang_bonus.unwrap_or(self.full_gang_bonus),
            min_duration: update.min_duration.unwrap_or(self.min_duration),
            max_duration: update.max_duration.unwrap_or(self.max_duration),
        }
    }
}

/// What `update_config` changes: the settings it names, and no other. A
/// proposal can wait for its signatures while another one lands, so an update
/// that carried every setting would undo whatever changed in between. This
/// one can't: a pause names the pause flag and leaves the rest as it finds it.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfigUpdate {
    pub paused: Option<bool>,
    /// All three rules, or none: they are checked as a set.
    pub membership_rules: Option<[MembershipRule; MEMBERSHIP_RULE_COUNT]>,
    pub entry_cap: Option<u8>,
    pub full_gang_bonus: Option<u8>,
    pub min_duration: Option<u32>,
    pub max_duration: Option<u32>,
}

#[cfg(test)]
pub(crate) mod tests {
    use {
        super::*,
        crate::state::{
            testing::{
                assert_arguments, assert_error, assert_layout, doc_says, field, key, INSTRUCTIONS,
            },
            RuleKind,
        },
    };

    /// Rules shaped like mainnet's: two symbols under one creator and update
    /// authority, and a Core collection.
    pub(crate) fn membership_rules() -> [MembershipRule; MEMBERSHIP_RULE_COUNT] {
        [
            MembershipRule {
                kind: RuleKind::TmCreator.into(),
                address: key(3),
                update_authority: key(4),
                symbol: *b"PSG\0\0\0\0\0\0\0",
            },
            MembershipRule {
                kind: RuleKind::TmCreator.into(),
                address: key(3),
                update_authority: key(4),
                symbol: *b"PGG\0\0\0\0\0\0\0",
            },
            MembershipRule {
                kind: RuleKind::CoreCollection.into(),
                address: key(5),
                ..MembershipRule::default()
            },
        ]
    }

    /// The settings the spec decides: cap 10, bonus 3, one hour to 30 days
    /// (D1, D2, D4).
    pub(crate) fn settings() -> ConfigSettings {
        ConfigSettings {
            paused: false,
            membership_rules: membership_rules(),
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 2_592_000,
        }
    }

    #[test]
    fn config_serializes_to_its_documented_layout() {
        let config = Config {
            bump: 254,
            authority: key(1),
            pending_authority: key(2),
            paused: true,
            membership_rules: membership_rules(),
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
        // The one address of the model that needs no input, so the document
        // can give it.
        let (address, _) = Config::find_address();
        assert!(doc_says(&format!("`{address}`")), "{address}");
    }

    #[test]
    fn a_new_config_holds_its_settings_and_no_pending_transfer() {
        let settings = ConfigSettings {
            paused: true,
            ..settings()
        };

        let config = Config::new(254, key(1), &settings);

        assert_eq!(config.bump, 254);
        assert_eq!(config.authority, key(1));
        assert_eq!(config.pending_authority, Pubkey::default());
        assert_eq!(config.settings(), settings);
        assert_eq!(config._reserved, [0; 64]);
    }

    #[test]
    fn new_settings_leave_the_authority_and_a_pending_transfer_alone() {
        let mut config = Config::new(254, key(1), &settings());
        config.pending_authority = key(2);
        let next = ConfigSettings {
            paused: true,
            entry_cap: 5,
            full_gang_bonus: 2,
            min_duration: 60,
            max_duration: 86_400,
            membership_rules: {
                let mut rules = membership_rules();
                rules.swap(0, 2);
                rules
            },
        };

        config.set_settings(&next);

        assert_eq!(config.settings(), next);
        assert_eq!(
            (config.bump, config.authority, config.pending_authority),
            (254, key(1), key(2))
        );
    }

    #[test]
    fn only_a_paused_config_fails_the_pause_guard() {
        let mut config = Config::new(254, key(1), &settings());
        config.require_not_paused().unwrap();

        config.paused = true;
        assert_error(config.require_not_paused(), RaffleError::Paused);
    }

    #[test]
    fn the_settings_of_the_spec_are_valid() {
        settings().validate().unwrap();
        // A pause is no reason to refuse the rest.
        ConfigSettings {
            paused: true,
            ..settings()
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn the_entry_cap_is_one_to_the_most_a_position_holds() {
        for entry_cap in 1..=MAX_ENTRY_CAP {
            ConfigSettings {
                entry_cap,
                ..settings()
            }
            .validate()
            .unwrap();
        }
        for entry_cap in [0, MAX_ENTRY_CAP + 1, u8::MAX] {
            let settings = ConfigSettings {
                entry_cap,
                ..settings()
            };

            assert_error(settings.validate(), RaffleError::InvalidEntryCap);
        }
    }

    #[test]
    fn the_cap_and_the_bonus_fit_one_entry_range() {
        let most = u8::MAX - MAX_ENTRY_CAP;
        for full_gang_bonus in [0, 3, most] {
            ConfigSettings {
                entry_cap: MAX_ENTRY_CAP,
                full_gang_bonus,
                ..settings()
            }
            .validate()
            .unwrap();
        }
        let settings = ConfigSettings {
            entry_cap: MAX_ENTRY_CAP,
            full_gang_bonus: most + 1,
            ..settings()
        };

        assert_error(settings.validate(), RaffleError::InvalidFullGangBonus);
    }

    #[test]
    fn the_duration_bounds_are_above_zero_and_in_order() {
        for (min_duration, max_duration) in [(1, 1), (1, u32::MAX), (u32::MAX, u32::MAX)] {
            ConfigSettings {
                min_duration,
                max_duration,
                ..settings()
            }
            .validate()
            .unwrap();
        }
        for (min_duration, max_duration) in [(0, 0), (0, 3_600), (3_601, 3_600), (u32::MAX, 1)] {
            let settings = ConfigSettings {
                min_duration,
                max_duration,
                ..settings()
            };

            assert_error(settings.validate(), RaffleError::InvalidDurationBounds);
        }
    }

    #[test]
    fn every_membership_rule_has_to_be_valid() {
        for slot in 0..MEMBERSHIP_RULE_COUNT {
            let mut settings = settings();
            settings.membership_rules[slot].address = Pubkey::default();

            assert_error(settings.validate(), RaffleError::InvalidRuleAddress);

            settings.membership_rules[slot] = MembershipRule {
                kind: 2,
                ..membership_rules()[slot]
            };
            assert_error(settings.validate(), RaffleError::InvalidRuleKind);
        }
    }

    /// Two NFTs of one collection would otherwise set two bits of
    /// `collections_seen`, and earn the bonus without the Full Gang set.
    #[test]
    fn no_two_membership_rules_are_the_same() {
        for (kept, copied) in [(0, 1), (0, 2), (1, 2), (2, 0)] {
            let mut settings = settings();
            settings.membership_rules[copied] = settings.membership_rules[kept];

            assert_error(settings.validate(), RaffleError::DuplicateMembershipRule);
        }
    }

    /// Mainnet's first two rules differ in the symbol alone.
    #[test]
    fn rules_that_differ_in_one_field_are_different_rules() {
        let mut settings = settings();
        settings.membership_rules[1] = MembershipRule {
            update_authority: key(9),
            ..settings.membership_rules[0]
        };
        settings.validate().unwrap();

        settings.membership_rules[1] = MembershipRule {
            address: key(9),
            ..settings.membership_rules[0]
        };
        settings.validate().unwrap();
    }

    #[test]
    fn an_update_replaces_what_it_names_and_nothing_else() {
        let before = settings();

        assert_eq!(before.with(&ConfigUpdate::default()), before);
        assert_eq!(
            before.with(&ConfigUpdate {
                paused: Some(true),
                ..ConfigUpdate::default()
            }),
            ConfigSettings {
                paused: true,
                ..before
            }
        );
        assert_eq!(
            before.with(&ConfigUpdate {
                entry_cap: Some(5),
                max_duration: Some(86_400),
                ..ConfigUpdate::default()
            }),
            ConfigSettings {
                entry_cap: 5,
                max_duration: 86_400,
                ..before
            }
        );

        let mut rules = membership_rules();
        rules.swap(0, 2);
        let all = ConfigUpdate {
            paused: Some(true),
            membership_rules: Some(rules),
            entry_cap: Some(1),
            full_gang_bonus: Some(0),
            min_duration: Some(60),
            max_duration: Some(120),
        };
        assert_eq!(
            before.with(&all),
            ConfigSettings {
                paused: true,
                membership_rules: rules,
                entry_cap: 1,
                full_gang_bonus: 0,
                min_duration: 60,
                max_duration: 120,
            }
        );
    }

    /// An update is judged by the settings it leaves: one that names only the
    /// shortest duration fails when it passes the longest the config has.
    #[test]
    fn an_update_is_valid_only_with_the_settings_it_leaves() {
        let before = settings();
        let past_the_longest = ConfigUpdate {
            min_duration: Some(before.max_duration + 1),
            ..ConfigUpdate::default()
        };
        let both_bounds = ConfigUpdate {
            max_duration: Some(before.max_duration + 1),
            ..past_the_longest
        };

        assert_error(
            before.with(&past_the_longest).validate(),
            RaffleError::InvalidDurationBounds,
        );
        before.with(&both_bounds).validate().unwrap();
    }

    #[test]
    fn config_settings_serialize_to_their_documented_layout() {
        let settings = ConfigSettings {
            paused: true,
            ..settings()
        };

        assert_arguments(
            "ConfigSettings",
            &settings,
            &[
                field!(settings.paused: bool),
                field!(settings.membership_rules: [MembershipRule; 3]),
                field!(settings.entry_cap: u8),
                field!(settings.full_gang_bonus: u8),
                field!(settings.min_duration: u32),
                field!(settings.max_duration: u32),
            ],
        );
    }

    #[test]
    fn a_config_update_serializes_to_its_documented_layout() {
        let update = ConfigUpdate {
            paused: Some(true),
            membership_rules: Some(membership_rules()),
            entry_cap: Some(10),
            full_gang_bonus: Some(3),
            min_duration: Some(3_600),
            max_duration: Some(2_592_000),
        };

        assert_arguments(
            "ConfigUpdate",
            &update,
            &[
                field!(update.paused: Option<bool>),
                field!(update.membership_rules: Option<[MembershipRule; 3]>),
                field!(update.entry_cap: Option<u8>),
                field!(update.full_gang_bonus: Option<u8>),
                field!(update.min_duration: Option<u32>),
                field!(update.max_duration: Option<u32>),
            ],
        );
    }

    /// What a pause costs in instruction data: one byte per setting, and one
    /// more for the flag.
    #[test]
    fn an_update_that_names_nothing_is_six_zero_bytes() {
        let mut bytes = Vec::new();
        ConfigUpdate::default().serialize(&mut bytes).unwrap();
        assert_eq!(bytes, [0; 6]);

        let mut bytes = Vec::new();
        ConfigUpdate {
            paused: Some(true),
            ..ConfigUpdate::default()
        }
        .serialize(&mut bytes)
        .unwrap();
        assert_eq!(bytes, [1, 1, 0, 0, 0, 0, 0]);
        // The document gives the bytes, and the instruction's data with its
        // 8-byte discriminator.
        let hex: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let pause = format!(
            "**A pause is {} bytes:** `{}`. With the discriminator, the instruction's data is \
             {} bytes.",
            bytes.len(),
            hex.join(" "),
            8 + bytes.len()
        );
        assert!(INSTRUCTIONS.says(&pause), "{pause}");
    }
}

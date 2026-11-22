//! `init_config` and `update_config` (docs/instructions.md): who may run
//! each, what `Config` holds afterwards, what they refuse, and the event each
//! emits.

use {
    crate::harness::{
        accept_authority, init_config, set_authority, settings, update_config, Test,
        LAMPORTS_PER_SOL,
    },
    anchor_lang::{
        error::ErrorCode,
        prelude::Pubkey,
        solana_program::{bpf_loader_upgradeable, instruction::AccountMeta, system_program},
        AccountSerialize,
    },
    raffles::{
        constants::MAX_ENTRY_CAP,
        error::RaffleError,
        events::{ConfigInitialized, ConfigUpdated},
        state::{Config, ConfigSettings, ConfigUpdate, MembershipRule, RuleKind},
    },
    raffles_client::pda,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

/// A funded key that holds no authority of any kind.
fn stranger(test: &mut Test) -> Keypair {
    let key = Keypair::new();
    test.svm.airdrop(&key.pubkey(), LAMPORTS_PER_SOL).unwrap();
    key
}

fn initialized(settings: ConfigSettings) -> Test {
    let mut test = Test::new();
    test.init_config(settings);
    test
}

/// What `event` has to be after an instruction left `config` behind.
fn updated_event(config: &Config) -> ConfigUpdated {
    ConfigUpdated {
        authority: config.authority,
        paused: config.paused,
        membership_rules: config.membership_rules,
        entry_cap: config.entry_cap,
        full_gang_bonus: config.full_gang_bonus,
        min_duration: config.min_duration,
        max_duration: config.max_duration,
    }
}

#[test]
fn init_config_creates_the_config_under_the_upgrade_authority() {
    let mut test = Test::new();
    let deployer = test.deployer.pubkey();
    let (address, bump) = pda::config();
    let before = test.lamports(&deployer);
    assert!(test.try_config().is_none());

    let sent = test.init_config(settings(0));

    assert_eq!(
        test.config(),
        Config {
            bump,
            authority: deployer,
            pending_authority: Pubkey::default(),
            paused: false,
            membership_rules: settings(0).membership_rules,
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 2_592_000,
            _reserved: [0; 64],
        }
    );
    // The account is exactly `Config`, and the deployer paid its rent: the
    // payer covered the fee.
    let account = test.svm.get_account(&address).unwrap();
    assert_eq!(account.data.len(), Config::SPACE);
    let rent = test.svm.minimum_balance_for_rent_exemption(Config::SPACE);
    assert_eq!(account.lamports, rent);
    assert_eq!(test.lamports(&deployer), before - rent);

    assert_eq!(
        sent.event::<ConfigInitialized>(),
        ConfigInitialized {
            authority: deployer,
            paused: false,
            membership_rules: settings(0).membership_rules,
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 2_592_000,
        }
    );
    // Right under the instruction a wallet sent.
    assert_eq!(sent.events()[0].stack_height, 2);
}

#[test]
fn init_config_can_start_the_program_paused() {
    let mut test = Test::new();

    let sent = test.init_config(ConfigSettings {
        paused: true,
        ..settings(0)
    });

    assert!(test.config().paused);
    assert!(sent.event::<ConfigInitialized>().paused);
}

/// Without the gate, whoever saw the deploy first could make itself the
/// authority (registries §6).
#[test]
fn only_the_upgrade_authority_initializes() {
    let mut test = Test::new();
    let stranger = stranger(&mut test);

    let error = test.send_err(&[init_config(stranger.pubkey(), settings(0))], &[&stranger]);

    assert_eq!(error, u32::from(RaffleError::InvalidInitializer));
    assert!(test.try_config().is_none());
    // The deployer still can.
    test.init_config(settings(0));
}

#[test]
fn nobody_initializes_a_program_without_an_upgrade_authority() {
    let mut test = Test::new();
    test.set_upgrade_authority(None);
    let deployer = test.deployer.insecure_clone();

    let error = test.send_err(&[init_config(deployer.pubkey(), settings(0))], &[&deployer]);

    assert_eq!(error, u32::from(RaffleError::InvalidInitializer));
    assert!(test.try_config().is_none());
}

/// The upgrade authority of some other program is no key of ours: the
/// ProgramData account has to be this program's.
#[test]
fn init_config_reads_only_this_programs_program_data() {
    let mut test = Test::new();
    let stranger = stranger(&mut test);
    // Another program's ProgramData, which names the stranger: what deploying
    // a program of its own would give it.
    let (other_program_data, _) =
        Pubkey::find_program_address(&[mpl_core::ID.as_ref()], &bpf_loader_upgradeable::ID);
    let mut account = test.svm.get_account(&other_program_data).unwrap();
    assert_eq!(account.owner, bpf_loader_upgradeable::ID);
    account.data[12] = 1;
    account.data[13..45].copy_from_slice(stranger.pubkey().as_ref());
    test.svm.set_account(other_program_data, account).unwrap();

    let mut instruction = init_config(stranger.pubkey(), settings(0));
    instruction.accounts[2].pubkey = other_program_data;
    let error = test.send_err(&[instruction], &[&stranger]);

    assert_eq!(error, u32::from(ErrorCode::ConstraintAddress));
    assert!(test.try_config().is_none());

    // The deployer with the wrong account is told about the account, not
    // that it is the wrong signer.
    let deployer = test.deployer.insecure_clone();
    let mut instruction = init_config(deployer.pubkey(), settings(0));
    instruction.accounts[2].pubkey = other_program_data;
    let error = test.send_err(&[instruction], &[&deployer]);

    assert_eq!(error, u32::from(ErrorCode::ConstraintAddress));
    assert!(test.try_config().is_none());
}

/// Naming the upgrade authority isn't enough: it has to sign. When the
/// config's address already holds its rent, nothing is debited, so the system
/// program asks for no signature and this check is the only one.
#[test]
fn init_config_needs_the_upgrade_authoritys_signature() {
    for prefunded in [false, true] {
        let mut test = Test::new();
        if prefunded {
            let rent = test.svm.minimum_balance_for_rent_exemption(Config::SPACE);
            test.svm.airdrop(&pda::config().0, rent).unwrap();
        }
        let mut instruction = init_config(test.deployer.pubkey(), settings(0));
        instruction.accounts[0].is_signer = false;

        let error = test.send_err(&[instruction], &[]);

        assert_eq!(
            error,
            u32::from(ErrorCode::AccountNotSigner),
            "prefunded: {prefunded}"
        );
        assert!(test.try_config().is_none());
    }
}

/// Anyone can send lamports to the config's address before it exists. That
/// must not keep the config from being created there: a plain create fails on
/// an address that already holds lamports.
#[test]
fn lamports_sent_to_the_configs_address_are_no_obstacle() {
    for more_than_the_rent in [false, true] {
        let mut test = Test::new();
        let rent = test.svm.minimum_balance_for_rent_exemption(Config::SPACE);
        // Twice the rent, or the least an empty address can be sent, which is
        // less than the config's rent.
        let sent = if more_than_the_rent {
            2 * rent
        } else {
            test.svm.minimum_balance_for_rent_exemption(0)
        };
        assert!(more_than_the_rent || sent < rent);
        let (address, _) = pda::config();
        test.svm.airdrop(&address, sent).unwrap();
        let before = test.lamports(&test.deployer.pubkey());

        test.init_config(settings(0));

        assert_eq!(test.config().authority, test.deployer.pubkey());
        // The deployer pays what is missing, and nothing when nothing is.
        assert_eq!(test.lamports(&address), rent.max(sent));
        assert_eq!(
            test.lamports(&test.deployer.pubkey()),
            before - rent.saturating_sub(sent)
        );
    }
}

/// The config has one address, and `init_config` creates it nowhere else.
#[test]
fn init_config_creates_the_config_only_at_its_seed() {
    let mut test = Test::new();
    let deployer = test.deployer.insecure_clone();
    let elsewhere = Keypair::new();
    let mut instruction = init_config(deployer.pubkey(), settings(0));
    instruction.accounts[1].pubkey = elsewhere.pubkey();

    let error = test.send_err(&[instruction], &[&deployer]);

    assert_eq!(error, u32::from(ErrorCode::ConstraintSeeds));
    assert!(test.try_config().is_none());
}

#[test]
fn init_config_runs_once() {
    let mut test = initialized(settings(0));
    let deployer = test.deployer.insecure_clone();
    let before = test.config();

    let again = ConfigSettings {
        entry_cap: 1,
        ..settings(0)
    };
    let error = test.send_err(&[init_config(deployer.pubkey(), again)], &[&deployer]);

    // The system program's: the address is already in use.
    assert_eq!(error, 0);
    assert_eq!(test.config(), before);
}

#[test]
fn init_config_refuses_settings_it_could_not_keep() {
    let rule = |slot: usize| settings(0).membership_rules[slot];
    let with_rule = |slot: usize, rule: MembershipRule| {
        let mut settings = settings(0);
        settings.membership_rules[slot] = rule;
        settings
    };
    let cases = [
        (
            ConfigSettings {
                entry_cap: 0,
                ..settings(0)
            },
            RaffleError::InvalidEntryCap,
        ),
        (
            ConfigSettings {
                entry_cap: MAX_ENTRY_CAP + 1,
                ..settings(0)
            },
            RaffleError::InvalidEntryCap,
        ),
        (
            ConfigSettings {
                full_gang_bonus: u8::MAX - 9,
                ..settings(0)
            },
            RaffleError::InvalidFullGangBonus,
        ),
        (
            ConfigSettings {
                min_duration: 0,
                ..settings(0)
            },
            RaffleError::InvalidDurationBounds,
        ),
        (
            ConfigSettings {
                min_duration: 2_592_001,
                ..settings(0)
            },
            RaffleError::InvalidDurationBounds,
        ),
        (
            with_rule(
                0,
                MembershipRule {
                    // Kept for a certified-collection rule, and not built.
                    kind: 2,
                    ..rule(0)
                },
            ),
            RaffleError::InvalidRuleKind,
        ),
        (
            with_rule(
                2,
                MembershipRule {
                    address: Pubkey::default(),
                    ..rule(2)
                },
            ),
            RaffleError::InvalidRuleAddress,
        ),
        (
            with_rule(
                1,
                MembershipRule {
                    update_authority: Pubkey::default(),
                    ..rule(1)
                },
            ),
            RaffleError::InvalidRuleUpdateAuthority,
        ),
        (
            with_rule(
                2,
                MembershipRule {
                    update_authority: rule(0).update_authority,
                    ..rule(2)
                },
            ),
            RaffleError::InvalidRuleUpdateAuthority,
        ),
        (
            with_rule(
                0,
                MembershipRule {
                    symbol: [0; MembershipRule::SYMBOL_LEN],
                    ..rule(0)
                },
            ),
            RaffleError::InvalidRuleSymbol,
        ),
        (
            with_rule(
                0,
                MembershipRule {
                    symbol: *b"P\0G\0\0\0\0\0\0\0",
                    ..rule(0)
                },
            ),
            RaffleError::InvalidRuleSymbol,
        ),
        (
            with_rule(
                2,
                MembershipRule {
                    symbol: rule(0).symbol,
                    ..rule(2)
                },
            ),
            RaffleError::InvalidRuleSymbol,
        ),
        (with_rule(1, rule(0)), RaffleError::DuplicateMembershipRule),
    ];

    // One VM for all of them: a refused `init_config` leaves nothing behind.
    let mut test = Test::new();
    let deployer = test.deployer.insecure_clone();

    for (settings, expected) in cases {
        let error = test.send_err(&[init_config(deployer.pubkey(), settings)], &[&deployer]);

        assert_eq!(error, u32::from(expected), "{expected:?}");
        assert!(test.try_config().is_none(), "{expected:?}");
    }
    // And the settings the cases were made from are fine.
    test.init_config(settings(0));
}

#[test]
fn update_config_changes_the_settings_it_names_and_no_other() {
    let mut test = initialized(settings(0));
    let deployer = test.deployer.insecure_clone();
    let before = test.config();

    let sent = test.send(
        &[update_config(
            deployer.pubkey(),
            ConfigUpdate {
                entry_cap: Some(5),
                max_duration: Some(604_800),
                ..ConfigUpdate::default()
            },
        )],
        &[&deployer],
    );

    let after = test.config();
    assert_eq!(
        after,
        Config {
            entry_cap: 5,
            max_duration: 604_800,
            ..before
        }
    );
    // The event carries every setting, as it now is.
    assert_eq!(sent.event::<ConfigUpdated>(), updated_event(&after));
    assert_eq!(sent.events()[0].stack_height, 2);
}

#[test]
fn update_config_replaces_every_setting_at_once() {
    let mut test = initialized(settings(0));
    let deployer = test.deployer.insecure_clone();
    let before = test.config();
    // Other collections, in another order: the Core rule first.
    let mut rules = settings(100).membership_rules;
    rules.swap(0, 2);
    assert_eq!(rules[0].kind, u8::from(RuleKind::CoreCollection));

    let sent = test.send(
        &[update_config(
            deployer.pubkey(),
            ConfigUpdate {
                paused: Some(true),
                membership_rules: Some(rules),
                entry_cap: Some(1),
                full_gang_bonus: Some(0),
                min_duration: Some(60),
                max_duration: Some(120),
            },
        )],
        &[&deployer],
    );

    let after = test.config();
    assert_eq!(
        after,
        Config {
            paused: true,
            membership_rules: rules,
            entry_cap: 1,
            full_gang_bonus: 0,
            min_duration: 60,
            max_duration: 120,
            ..before
        }
    );
    assert_eq!(sent.event::<ConfigUpdated>(), updated_event(&after));
}

/// The pause is a setting like the others, and an update is how it lifts:
/// the authority's instructions stay open under it (spec §10).
#[test]
fn update_config_pauses_and_unpauses() {
    let mut test = initialized(settings(0));
    let deployer = test.deployer.insecure_clone();
    let before = test.config();
    let pause = |paused: bool| {
        update_config(
            deployer.pubkey(),
            ConfigUpdate {
                paused: Some(paused),
                ..ConfigUpdate::default()
            },
        )
    };

    let sent = test.send(&[pause(true)], &[&deployer]);
    assert_eq!(
        test.config(),
        Config {
            paused: true,
            ..before.clone()
        }
    );
    assert!(sent.event::<ConfigUpdated>().paused);

    // Pausing a paused program changes nothing, and doesn't fail: whoever
    // pauses second in an incident gets no error to read.
    test.send(&[pause(true)], &[&deployer]);
    assert!(test.config().paused);

    // Another setting changes while paused, and the pause stays.
    test.send(
        &[update_config(
            deployer.pubkey(),
            ConfigUpdate {
                full_gang_bonus: Some(5),
                ..ConfigUpdate::default()
            },
        )],
        &[&deployer],
    );
    assert!(test.config().paused);
    assert_eq!(test.config().full_gang_bonus, 5);

    let sent = test.send(&[pause(false)], &[&deployer]);
    assert!(!test.config().paused);
    assert!(!sent.event::<ConfigUpdated>().paused);
}

#[test]
fn an_update_that_names_nothing_leaves_the_config_as_it_is() {
    let mut test = initialized(settings(0));
    let deployer = test.deployer.insecure_clone();
    let before = test.config();

    let sent = test.send(
        &[update_config(deployer.pubkey(), ConfigUpdate::default())],
        &[&deployer],
    );

    assert_eq!(test.config(), before);
    assert_eq!(sent.event::<ConfigUpdated>(), updated_event(&before));
}

#[test]
fn only_the_authority_updates_the_config() {
    let mut test = initialized(settings(0));
    let stranger = stranger(&mut test);
    let before = test.config();
    let pause = ConfigUpdate {
        paused: Some(true),
        ..ConfigUpdate::default()
    };

    let error = test.send_err(&[update_config(stranger.pubkey(), pause)], &[&stranger]);

    assert_eq!(error, u32::from(RaffleError::Unauthorized));
    assert_eq!(test.config(), before);
}

/// Naming the authority isn't enough: it has to sign.
#[test]
fn update_config_needs_the_authoritys_signature() {
    let mut test = initialized(settings(0));
    let before = test.config();
    let mut instruction = update_config(
        test.deployer.pubkey(),
        ConfigUpdate {
            paused: Some(true),
            ..ConfigUpdate::default()
        },
    );
    instruction.accounts[0].is_signer = false;

    let error = test.send_err(&[instruction], &[]);

    assert_eq!(error, u32::from(ErrorCode::AccountNotSigner));
    assert_eq!(test.config(), before);
}

/// An update is judged by the settings it leaves, not by the ones it names.
#[test]
fn update_config_refuses_what_would_leave_the_config_invalid() {
    let mut test = initialized(settings(0));
    let deployer = test.deployer.insecure_clone();
    let before = test.config();
    let rules = settings(0).membership_rules;
    let cases = [
        (
            ConfigUpdate {
                entry_cap: Some(0),
                ..ConfigUpdate::default()
            },
            RaffleError::InvalidEntryCap,
        ),
        (
            ConfigUpdate {
                entry_cap: Some(MAX_ENTRY_CAP + 1),
                ..ConfigUpdate::default()
            },
            RaffleError::InvalidEntryCap,
        ),
        (
            // With the cap of 10 the config has, 246 no longer fits.
            ConfigUpdate {
                full_gang_bonus: Some(246),
                ..ConfigUpdate::default()
            },
            RaffleError::InvalidFullGangBonus,
        ),
        (
            // Past the longest duration the config has.
            ConfigUpdate {
                min_duration: Some(2_592_001),
                ..ConfigUpdate::default()
            },
            RaffleError::InvalidDurationBounds,
        ),
        (
            // Below the shortest.
            ConfigUpdate {
                max_duration: Some(3_599),
                ..ConfigUpdate::default()
            },
            RaffleError::InvalidDurationBounds,
        ),
        (
            ConfigUpdate {
                membership_rules: Some([rules[0], rules[1], rules[1]]),
                ..ConfigUpdate::default()
            },
            RaffleError::DuplicateMembershipRule,
        ),
        (
            ConfigUpdate {
                membership_rules: Some([rules[0], rules[1], MembershipRule::default()]),
                ..ConfigUpdate::default()
            },
            RaffleError::InvalidRuleAddress,
        ),
    ];

    for (update, expected) in cases {
        let error = test.send_err(&[update_config(deployer.pubkey(), update)], &[&deployer]);

        assert_eq!(error, u32::from(expected), "{expected:?}");
        assert_eq!(test.config(), before, "{expected:?}");
    }

    // The same bound passes once the other one moves with it.
    test.send(
        &[update_config(
            deployer.pubkey(),
            ConfigUpdate {
                min_duration: Some(2_592_001),
                max_duration: Some(5_184_000),
                ..ConfigUpdate::default()
            },
        )],
        &[&deployer],
    );
    assert_eq!(test.config().min_duration, 2_592_001);
}

#[test]
fn update_config_needs_a_config() {
    let mut test = Test::new();
    let deployer = test.deployer.insecure_clone();

    let error = test.send_err(
        &[update_config(deployer.pubkey(), ConfigUpdate::default())],
        &[&deployer],
    );

    assert_eq!(error, u32::from(ErrorCode::AccountNotInitialized));
}

/// The config is the account at its seed, and no other. An account of the
/// signer's own isn't read, and neither is one of the program's at another
/// address, even if it reads as a config that names the signer: nothing
/// creates such an account today, and the seeds would refuse it.
#[test]
fn the_config_is_the_account_at_its_seed() {
    let mut test = initialized(settings(0));
    let stranger = stranger(&mut test);
    let before = test.config();
    // What each instruction is, with the stranger as its signer and
    // `account` as its config.
    let with_config = |account: Pubkey| {
        let mut instructions = [
            update_config(stranger.pubkey(), ConfigUpdate::default()),
            set_authority(stranger.pubkey(), stranger.pubkey()),
            accept_authority(stranger.pubkey()),
        ];
        for instruction in &mut instructions {
            assert_eq!(instruction.accounts[1].pubkey, pda::config().0);
            instruction.accounts[1].pubkey = account;
        }
        instructions
    };

    // A system account, which the stranger controls.
    assert_eq!(
        test.svm.get_account(&stranger.pubkey()).unwrap().owner,
        system_program::ID
    );
    for instruction in with_config(stranger.pubkey()) {
        let error = test.send_err(&[instruction], &[&stranger]);
        assert_eq!(error, u32::from(ErrorCode::AccountOwnedByWrongProgram));
    }

    // An account of the program's, with a config's bytes, that names the
    // stranger as the authority and as its successor.
    let forged = Keypair::new().pubkey();
    let mut account = test.svm.get_account(&pda::config().0).unwrap();
    let mut data = Vec::new();
    Config {
        authority: stranger.pubkey(),
        pending_authority: stranger.pubkey(),
        ..before.clone()
    }
    .try_serialize(&mut data)
    .unwrap();
    account.data = data;
    test.svm.set_account(forged, account).unwrap();
    for instruction in with_config(forged) {
        let error = test.send_err(&[instruction], &[&stranger]);
        assert_eq!(error, u32::from(ErrorCode::ConstraintSeeds));
    }

    assert_eq!(test.config(), before);
}

/// An instruction that emits needs the event authority and the program among
/// its accounts (docs/events.md, section 1). Without either it can't emit, so
/// it does nothing at all.
#[test]
fn an_instruction_cannot_emit_without_its_event_accounts() {
    let mut test = initialized(settings(0));
    let deployer = test.deployer.insecure_clone();
    let before = test.config();
    let pause = ConfigUpdate {
        paused: Some(true),
        ..ConfigUpdate::default()
    };

    // The event authority is the program's own address, and no other.
    let mut instruction = update_config(deployer.pubkey(), pause);
    instruction.accounts[2].pubkey = system_program::ID;
    let error = test.send_err(&[instruction], &[&deployer]);
    assert_eq!(error, u32::from(ErrorCode::ConstraintAddress));

    // The program calls itself to emit, which the runtime allows only when
    // the program is one of the instruction's accounts.
    let mut instruction = update_config(deployer.pubkey(), pause);
    instruction.accounts[3].pubkey = system_program::ID;
    let failed = test.try_send(&[instruction], &[&deployer]).unwrap_err();
    assert_eq!(
        format!("{:?}", failed.err),
        "InstructionError(0, MissingAccount)",
        "{}",
        failed.meta.pretty_logs()
    );
    assert_eq!(test.config(), before);

    // Its place among the accounts is the IDL's, not something the program
    // checks: anywhere among them serves the runtime.
    let mut instruction = update_config(deployer.pubkey(), pause);
    instruction.accounts[3].pubkey = system_program::ID;
    instruction
        .accounts
        .push(AccountMeta::new_readonly(raffles::ID, false));
    let sent = test.send(&[instruction], &[&deployer]);
    assert!(sent.event::<ConfigUpdated>().paused);
    assert!(test.config().paused);
}

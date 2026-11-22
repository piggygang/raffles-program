//! The authority as a Squads vault (registries §6): every instruction of the
//! authority, run as a vault transaction of the Squads v4 program that
//! mainnet runs.
//!
//! A vault holds no key. It signs only inside Squads'
//! `vault_transaction_execute`, once enough of the multisig's members have
//! approved the transaction. So the raffles instruction runs one level down,
//! and its event one further.

use {
    crate::{
        harness::{
            accept_authority, init_config, set_authority, settings, update_config, Sent, Test,
        },
        squads::{self, Vault},
    },
    anchor_lang::{error::ErrorCode, prelude::Pubkey},
    raffles::{
        error::RaffleError,
        events::{AuthorityAccepted, AuthorityProposed, ConfigInitialized, ConfigUpdated},
        state::{Config, ConfigUpdate},
    },
    raffles_client::pda,
    solana_signer::Signer,
};

/// The transaction's one event sits under the instruction the vault ran,
/// which sits under the instruction that signed for the vault.
fn assert_emitted_under_the_vault(sent: &Sent) {
    let events = sent.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].stack_height, 3);
}

fn pause() -> ConfigUpdate {
    ConfigUpdate {
        paused: Some(true),
        ..ConfigUpdate::default()
    }
}

/// A config that the deployer created and handed to a vault, as the bootstrap
/// does (registries §7a).
fn handed_over() -> (Test, Vault) {
    let mut test = Test::new();
    let vault = Vault::new(&mut test);
    let deployer = test.deployer.insecure_clone();
    test.init_config(settings(0));
    test.send(
        &[set_authority(deployer.pubkey(), vault.address)],
        &[&deployer],
    );
    vault.run(&mut test, accept_authority(vault.address));
    (test, vault)
}

#[test]
fn a_vault_accepts_the_authority() {
    let mut test = Test::new();
    let vault = Vault::new(&mut test);
    let deployer = test.deployer.insecure_clone();
    test.init_config(settings(0));
    test.send(
        &[set_authority(deployer.pubkey(), vault.address)],
        &[&deployer],
    );
    let before = test.config();

    let sent = vault.run(&mut test, accept_authority(vault.address));

    assert_eq!(
        test.config(),
        Config {
            authority: vault.address,
            pending_authority: Pubkey::default(),
            ..before
        }
    );
    assert_eq!(
        sent.event::<AuthorityAccepted>(),
        AuthorityAccepted {
            old_authority: deployer.pubkey(),
            new_authority: vault.address,
        }
    );
    assert_emitted_under_the_vault(&sent);
    // The deployer is out.
    let error = test.send_err(
        &[set_authority(deployer.pubkey(), deployer.pubkey())],
        &[&deployer],
    );
    assert_eq!(error, u32::from(RaffleError::Unauthorized));
}

#[test]
fn a_vault_updates_the_config() {
    let (mut test, vault) = handed_over();
    let before = test.config();

    let sent = vault.run(
        &mut test,
        update_config(
            vault.address,
            ConfigUpdate {
                paused: Some(true),
                entry_cap: Some(5),
                ..ConfigUpdate::default()
            },
        ),
    );

    assert_eq!(
        test.config(),
        Config {
            paused: true,
            entry_cap: 5,
            ..before
        }
    );
    let event = sent.event::<ConfigUpdated>();
    assert_eq!(event.authority, vault.address);
    assert!(event.paused);
    assert_eq!(event.entry_cap, 5);
    assert_emitted_under_the_vault(&sent);
}

/// The largest of the authority's instructions still fits a vault
/// transaction: an update that names every setting, the three rules among
/// them.
#[test]
fn a_vault_replaces_every_setting_at_once() {
    let (mut test, vault) = handed_over();
    let rules = settings(100).membership_rules;

    vault.run(
        &mut test,
        update_config(
            vault.address,
            ConfigUpdate {
                paused: Some(true),
                membership_rules: Some(rules),
                entry_cap: Some(1),
                full_gang_bonus: Some(0),
                min_duration: Some(60),
                max_duration: Some(120),
            },
        ),
    );

    let config = test.config();
    assert_eq!(config.membership_rules, rules);
    assert_eq!(
        (config.paused, config.entry_cap, config.full_gang_bonus),
        (true, 1, 0)
    );
    assert_eq!((config.min_duration, config.max_duration), (60, 120));
}

/// Moving to another multisig: the old vault proposes, the new one accepts
/// (registries §7g).
#[test]
fn a_vault_hands_the_authority_to_another_vault() {
    let (mut test, vault) = handed_over();
    let next = Vault::new(&mut test);

    let sent = vault.run(&mut test, set_authority(vault.address, next.address));

    assert_eq!(test.config().authority, vault.address);
    assert_eq!(
        sent.event::<AuthorityProposed>(),
        AuthorityProposed {
            authority: vault.address,
            pending_authority: next.address,
        }
    );
    assert_emitted_under_the_vault(&sent);

    next.run(&mut test, accept_authority(next.address));

    assert_eq!(test.config().authority, next.address);
    // The old vault is out, and the new one in.
    let error = vault.run_err(&mut test, update_config(vault.address, pause()));
    assert_eq!(error, u32::from(RaffleError::Unauthorized));
    next.run(&mut test, update_config(next.address, pause()));
    assert!(test.config().paused);
}

/// When the upgrade authority already is the vault, the vault creates the
/// config itself, and pays its rent.
#[test]
fn a_vault_that_holds_the_upgrade_authority_initializes() {
    let mut test = Test::new();
    let vault = Vault::new(&mut test);
    test.set_upgrade_authority(Some(vault.address));
    // The deployer gave the upgrade authority away, and its say with it.
    let deployer = test.deployer.insecure_clone();
    let error = test.send_err(&[init_config(deployer.pubkey(), settings(0))], &[&deployer]);
    assert_eq!(error, u32::from(RaffleError::InvalidInitializer));
    assert!(test.try_config().is_none());
    let before = test.lamports(&vault.address);

    let sent = vault.run(&mut test, init_config(vault.address, settings(0)));

    let config = test.config();
    assert_eq!(config.authority, vault.address);
    assert_eq!(config.settings(), settings(0));
    let rent = test.svm.minimum_balance_for_rent_exemption(Config::SPACE);
    assert_eq!(test.lamports(&vault.address), before - rent);
    assert_eq!(test.lamports(&pda::config().0), rent);
    assert_eq!(sent.event::<ConfigInitialized>().authority, vault.address);
    assert_emitted_under_the_vault(&sent);
}

/// The members decide what the vault signs: one approval of three is below
/// the threshold, and Squads doesn't execute.
#[test]
fn a_vault_signs_nothing_below_the_threshold() {
    let (mut test, vault) = handed_over();
    let before = test.config();

    let error = vault.refused_below_the_threshold(&mut test, update_config(vault.address, pause()));

    assert_eq!(error, squads::INVALID_PROPOSAL_STATUS);
    assert_eq!(test.config(), before);
}

/// Nobody but Squads makes the vault sign: its address in a transaction of
/// someone else's is a name without a signature.
#[test]
fn a_vault_signs_only_through_squads() {
    let (mut test, vault) = handed_over();
    let before = test.config();
    let mut instruction = update_config(vault.address, pause());
    instruction.accounts[0].is_signer = false;

    let error = test.send_err(&[instruction], &[]);

    assert_eq!(error, u32::from(ErrorCode::AccountNotSigner));
    assert_eq!(test.config(), before);
}

/// The mistake the transfer's second step is there for (registries §6): the
/// authority proposes the multisig account in place of its vault. Nothing can
/// sign for the multisig account, so with a one-step transfer the config
/// would be lost. Here the proposal just stays open until the authority
/// replaces it.
#[test]
fn the_multisig_account_never_becomes_the_authority() {
    let mut test = Test::new();
    let vault = Vault::new(&mut test);
    let deployer = test.deployer.insecure_clone();
    test.init_config(settings(0));
    test.send(
        &[set_authority(deployer.pubkey(), vault.multisig)],
        &[&deployer],
    );

    // Squads signs for the vault alone: a vault transaction that needs the
    // multisig account's signature is one it refuses to execute.
    let error = vault.refused(&mut test, accept_authority(vault.multisig));
    assert_eq!(error, squads::INVALID_ACCOUNT);
    // Without the signature, the multisig account is a name the program
    // doesn't take.
    let mut unsigned = accept_authority(vault.multisig);
    unsigned.accounts[0].is_signer = false;
    let error = vault.run_err(&mut test, unsigned);
    assert_eq!(error, u32::from(ErrorCode::AccountNotSigner));
    // And the vault isn't the one proposed.
    let error = vault.run_err(&mut test, accept_authority(vault.address));
    assert_eq!(error, u32::from(RaffleError::Unauthorized));
    assert_eq!(test.config().authority, deployer.pubkey());
    assert_eq!(test.config().pending_authority, vault.multisig);

    // The deployer is still in charge, and proposes the vault itself.
    test.send(
        &[set_authority(deployer.pubkey(), vault.address)],
        &[&deployer],
    );
    vault.run(&mut test, accept_authority(vault.address));
    assert_eq!(test.config().authority, vault.address);
}

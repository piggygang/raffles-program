//! `set_authority` and `accept_authority` (docs/instructions.md): the
//! transfer in two steps, who may take each, and what stays as it was until
//! the second one.

use {
    crate::harness::{
        accept_authority, set_authority, settings, update_config, Test, LAMPORTS_PER_SOL,
    },
    anchor_lang::{error::ErrorCode, prelude::Pubkey},
    raffles::{
        error::RaffleError,
        events::{AuthorityAccepted, AuthorityProposed, ConfigUpdated},
        state::{Config, ConfigSettings, ConfigUpdate},
    },
    solana_keypair::Keypair,
    solana_signer::Signer,
};

/// A config under the deployer, and a funded key that has no part in it yet.
fn initialized() -> (Test, Keypair, Keypair) {
    let mut test = Test::new();
    test.init_config(settings(0));
    let other = Keypair::new();
    test.svm.airdrop(&other.pubkey(), LAMPORTS_PER_SOL).unwrap();
    let deployer = test.deployer.insecure_clone();
    (test, deployer, other)
}

fn pause() -> ConfigUpdate {
    ConfigUpdate {
        paused: Some(true),
        ..ConfigUpdate::default()
    }
}

#[test]
fn set_authority_proposes_and_leaves_the_authority_in_charge() {
    let (mut test, deployer, successor) = initialized();
    let before = test.config();

    let sent = test.send(
        &[set_authority(deployer.pubkey(), successor.pubkey())],
        &[&deployer],
    );

    assert_eq!(
        test.config(),
        Config {
            pending_authority: successor.pubkey(),
            ..before
        }
    );
    assert_eq!(
        sent.event::<AuthorityProposed>(),
        AuthorityProposed {
            authority: deployer.pubkey(),
            pending_authority: successor.pubkey(),
        }
    );
    assert_eq!(sent.events()[0].stack_height, 2);

    // Until the successor accepts, it has no say, and the authority keeps
    // all of its own.
    let proposed = test.config();
    let error = test.send_err(&[update_config(successor.pubkey(), pause())], &[&successor]);
    assert_eq!(error, u32::from(RaffleError::Unauthorized));
    test.send(&[update_config(deployer.pubkey(), pause())], &[&deployer]);
    // The update leaves the proposal as it is, and the successor can still
    // accept.
    assert_eq!(
        test.config(),
        Config {
            paused: true,
            ..proposed
        }
    );
    test.send(&[accept_authority(successor.pubkey())], &[&successor]);
    assert_eq!(test.config().authority, successor.pubkey());
}

#[test]
fn accept_authority_completes_the_transfer() {
    let (mut test, deployer, successor) = initialized();
    test.send(
        &[set_authority(deployer.pubkey(), successor.pubkey())],
        &[&deployer],
    );
    let before = test.config();

    let sent = test.send(&[accept_authority(successor.pubkey())], &[&successor]);

    assert_eq!(
        test.config(),
        Config {
            authority: successor.pubkey(),
            pending_authority: Pubkey::default(),
            ..before
        }
    );
    assert_eq!(
        sent.event::<AuthorityAccepted>(),
        AuthorityAccepted {
            old_authority: deployer.pubkey(),
            new_authority: successor.pubkey(),
        }
    );
    assert_eq!(sent.events()[0].stack_height, 2);

    // The old authority is out, the upgrade authority though it still is.
    for instruction in [
        update_config(deployer.pubkey(), pause()),
        set_authority(deployer.pubkey(), deployer.pubkey()),
    ] {
        let error = test.send_err(&[instruction], &[&deployer]);
        assert_eq!(error, u32::from(RaffleError::Unauthorized));
    }
    // The new one is in.
    let sent = test.send(&[update_config(successor.pubkey(), pause())], &[&successor]);
    assert_eq!(sent.event::<ConfigUpdated>().authority, successor.pubkey());
    assert!(test.config().paused);
}

#[test]
fn a_later_proposal_replaces_an_earlier_one() {
    let (mut test, deployer, first) = initialized();
    let second = Keypair::new();
    test.send(
        &[set_authority(deployer.pubkey(), first.pubkey())],
        &[&deployer],
    );

    let sent = test.send(
        &[set_authority(deployer.pubkey(), second.pubkey())],
        &[&deployer],
    );

    assert_eq!(test.config().pending_authority, second.pubkey());
    assert_eq!(
        sent.event::<AuthorityProposed>().pending_authority,
        second.pubkey()
    );
    // The first one's chance is gone.
    let error = test.send_err(&[accept_authority(first.pubkey())], &[&first]);
    assert_eq!(error, u32::from(RaffleError::Unauthorized));
    assert_eq!(test.config().authority, deployer.pubkey());
}

#[test]
fn the_zero_address_withdraws_a_proposal() {
    let (mut test, deployer, successor) = initialized();
    test.send(
        &[set_authority(deployer.pubkey(), successor.pubkey())],
        &[&deployer],
    );

    let sent = test.send(
        &[set_authority(deployer.pubkey(), Pubkey::default())],
        &[&deployer],
    );

    assert_eq!(test.config().pending_authority, Pubkey::default());
    assert_eq!(
        sent.event::<AuthorityProposed>(),
        AuthorityProposed {
            authority: deployer.pubkey(),
            pending_authority: Pubkey::default(),
        }
    );
    let error = test.send_err(&[accept_authority(successor.pubkey())], &[&successor]);
    assert_eq!(error, u32::from(RaffleError::NoPendingAuthority));
    assert_eq!(test.config().authority, deployer.pubkey());
}

/// The program can't tell which addresses can sign, so it checks none: not
/// the authority's own, nor a withdrawal when nothing is pending. Each is a
/// proposal like any other, with its event.
#[test]
fn set_authority_takes_any_address() {
    let (mut test, deployer, _) = initialized();
    let before = test.config();

    let sent = test.send(
        &[set_authority(deployer.pubkey(), Pubkey::default())],
        &[&deployer],
    );
    assert_eq!(test.config(), before);
    assert_eq!(
        sent.event::<AuthorityProposed>().pending_authority,
        Pubkey::default()
    );

    let sent = test.send(
        &[set_authority(deployer.pubkey(), deployer.pubkey())],
        &[&deployer],
    );
    assert_eq!(
        sent.event::<AuthorityProposed>().pending_authority,
        deployer.pubkey()
    );
    // Accepting its own proposal leaves the authority where it was.
    let sent = test.send(&[accept_authority(deployer.pubkey())], &[&deployer]);
    assert_eq!(test.config(), before);
    assert_eq!(
        sent.event::<AuthorityAccepted>(),
        AuthorityAccepted {
            old_authority: deployer.pubkey(),
            new_authority: deployer.pubkey(),
        }
    );
}

#[test]
fn only_the_authority_proposes_a_successor() {
    let (mut test, deployer, stranger) = initialized();
    let before = test.config();

    let error = test.send_err(
        &[set_authority(stranger.pubkey(), stranger.pubkey())],
        &[&stranger],
    );

    assert_eq!(error, u32::from(RaffleError::Unauthorized));
    assert_eq!(test.config(), before);

    // Nor does naming the authority without its signature.
    let mut instruction = set_authority(deployer.pubkey(), stranger.pubkey());
    instruction.accounts[0].is_signer = false;
    let error = test.send_err(&[instruction], &[]);
    assert_eq!(error, u32::from(ErrorCode::AccountNotSigner));
    assert_eq!(test.config(), before);
}

#[test]
fn only_the_proposed_authority_accepts() {
    let (mut test, deployer, successor) = initialized();
    let stranger = Keypair::new();
    test.send(
        &[set_authority(deployer.pubkey(), successor.pubkey())],
        &[&deployer],
    );
    let before = test.config();

    // Not a stranger, and not the authority for its successor.
    for key in [&stranger, &deployer] {
        let error = test.send_err(&[accept_authority(key.pubkey())], &[key]);
        assert_eq!(error, u32::from(RaffleError::Unauthorized));
    }
    // The successor has to sign: that it can is what the transfer waits for.
    let mut instruction = accept_authority(successor.pubkey());
    instruction.accounts[0].is_signer = false;
    let error = test.send_err(&[instruction], &[]);
    assert_eq!(error, u32::from(ErrorCode::AccountNotSigner));

    assert_eq!(test.config(), before);
}

#[test]
fn accept_authority_needs_a_pending_transfer() {
    let (mut test, deployer, stranger) = initialized();
    let before = test.config();

    for key in [&stranger, &deployer] {
        let error = test.send_err(&[accept_authority(key.pubkey())], &[key]);
        assert_eq!(error, u32::from(RaffleError::NoPendingAuthority));
    }
    assert_eq!(test.config(), before);

    // And a transfer is spent once it is accepted.
    test.send(
        &[set_authority(deployer.pubkey(), stranger.pubkey())],
        &[&deployer],
    );
    test.send(&[accept_authority(stranger.pubkey())], &[&stranger]);
    let error = test.send_err(&[accept_authority(stranger.pubkey())], &[&stranger]);
    assert_eq!(error, u32::from(RaffleError::NoPendingAuthority));
}

/// A transfer back is a transfer like any other: propose, then accept.
#[test]
fn the_authority_can_move_and_move_back() {
    let (mut test, deployer, successor) = initialized();
    for (from, to) in [(&deployer, &successor), (&successor, &deployer)] {
        test.send(&[set_authority(from.pubkey(), to.pubkey())], &[from]);
        test.send(&[accept_authority(to.pubkey())], &[to]);
        assert_eq!(test.config().authority, to.pubkey());
    }
}

/// Spec §10's pause matrix: the authority's instructions are open under a
/// pause, the transfer of the authority among them.
#[test]
fn the_authority_moves_while_the_program_is_paused() {
    let mut test = Test::new();
    test.init_config(ConfigSettings {
        paused: true,
        ..settings(0)
    });
    let deployer = test.deployer.insecure_clone();
    let successor = Keypair::new();

    test.send(
        &[set_authority(deployer.pubkey(), successor.pubkey())],
        &[&deployer],
    );
    test.send(&[accept_authority(successor.pubkey())], &[&successor]);

    assert_eq!(test.config().authority, successor.pubkey());
    // The successor lifts the pause.
    test.send(
        &[update_config(
            successor.pubkey(),
            ConfigUpdate {
                paused: Some(false),
                ..ConfigUpdate::default()
            },
        )],
        &[&successor],
    );
    assert!(!test.config().paused);
}

#[test]
fn the_transfer_needs_a_config() {
    let mut test = Test::new();
    let deployer = test.deployer.insecure_clone();

    for instruction in [
        set_authority(deployer.pubkey(), deployer.pubkey()),
        accept_authority(deployer.pubkey()),
    ] {
        let error = test.send_err(&[instruction], &[&deployer]);
        assert_eq!(error, u32::from(ErrorCode::AccountNotInitialized));
    }
}

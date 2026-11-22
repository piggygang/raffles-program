//! The typed client against its own IDL. What it shares with the program is
//! checked from the program's side, where both are at hand:
//! programs/raffles/tests/integration/interface.rs.

// Test code may panic; the lints are for the crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use {
    anchor_lang::{
        prelude::Pubkey, solana_program::instruction::Instruction, Discriminator, Event,
        InstructionData, ToAccountMetas,
    },
    raffles_client::{client, events, parsers, pda, read_event, types, EVENT_TAG, ID, IDL},
};

/// An event instruction's data: the tag, then the event.
fn event_instruction(event: &impl Event) -> Vec<u8> {
    [EVENT_TAG, &event.data()].concat()
}

#[test]
fn the_program_id_is_the_idls_address() {
    let idl: serde_json::Value = serde_json::from_str(IDL).unwrap();

    assert_eq!(idl["address"], ID.to_string());
}

#[test]
fn read_event_reads_an_event_instruction() {
    let closed = events::RaffleClosed {
        raffle: Pubkey::new_from_array([1; 32]),
        creator: Pubkey::new_from_array([2; 32]),
    };

    match read_event(&event_instruction(&closed)) {
        Some(Ok(parsers::Event::RaffleClosed(event))) => {
            assert_eq!(event.raffle, closed.raffle);
            assert_eq!(event.creator, closed.creator);
        }
        _ => panic!("not read as a RaffleClosed"),
    }
}

#[test]
fn read_event_passes_over_an_instruction_that_is_no_event() {
    // An instruction's own data starts with its discriminator, not the tag.
    assert!(read_event(client::args::AcceptAuthority::DISCRIMINATOR).is_none());
    assert!(read_event(&[]).is_none());
    // The tag is eight bytes, and all of them count.
    assert!(read_event(&EVENT_TAG[..7]).is_none());
}

#[test]
fn read_event_reports_an_event_this_release_does_not_know() {
    let unknown = [EVENT_TAG, &[0xff; 8], &[0; 32]].concat();

    assert!(matches!(read_event(&unknown), Some(Err(_))));
    // An event cut short is no better.
    let closed = events::RaffleClosed {
        raffle: Pubkey::new_from_array([1; 32]),
        creator: Pubkey::new_from_array([2; 32]),
    };
    let data = event_instruction(&closed);
    assert!(matches!(read_event(&data[..data.len() - 1]), Some(Err(_))));
}

/// What the client builds, its own parser reads back: the program, the
/// accounts with who signs and what is written, and the arguments as the IDL
/// lays them out.
#[test]
fn an_instruction_the_client_builds_reads_back_as_itself() {
    let authority = Pubkey::new_from_array([1; 32]);
    let accounts = client::accounts::UpdateConfig {
        authority,
        config: pda::config().0,
        event_authority: pda::event_authority().0,
        program: ID,
    };
    let update = types::ConfigUpdate {
        paused: Some(true),
        membership_rules: None,
        entry_cap: Some(5),
        full_gang_bonus: None,
        min_duration: None,
        max_duration: None,
    };
    let instruction = Instruction::new_with_bytes(
        ID,
        &client::args::UpdateConfig { update }.data(),
        accounts.to_account_metas(None),
    );

    // A pause and a cap: the discriminator, one byte per setting, and one
    // more for each of the two the update names.
    assert_eq!(instruction.data.len(), 8 + 6 + 2);
    match parsers::Instruction::parse(&instruction) {
        Ok(parsers::Instruction::UpdateConfig { accounts, args }) => {
            assert_eq!(accounts.authority, authority);
            assert_eq!(accounts.config, pda::config().0);
            assert_eq!(args.update.paused, Some(true));
            assert_eq!(args.update.entry_cap, Some(5));
            assert!(args.update.membership_rules.is_none());
            assert_eq!(args.update.max_duration, None);
        }
        _ => panic!("not read as an UpdateConfig"),
    }

    // Under another program's address it is no instruction of this one.
    let elsewhere = Instruction {
        program_id: Pubkey::new_from_array([9; 32]),
        ..instruction
    };
    assert!(parsers::Instruction::parse(&elsewhere).is_err());
}

/// The program's ProgramData account is the upgradeable loader's address for
/// the program, which the IDL gives `init_config` as a fixed account.
#[test]
fn the_program_data_address_is_the_one_the_idl_gives_init_config() {
    let idl: serde_json::Value = serde_json::from_str(IDL).unwrap();
    let init_config = idl["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instruction| instruction["name"] == "init_config")
        .unwrap();
    let program_data = init_config["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["name"] == "program_data")
        .unwrap();

    assert_eq!(program_data["address"], pda::program_data().0.to_string());
}

#[test]
fn the_seed_prefixes_are_the_idls_constants() {
    let raffle = Pubkey::new_from_array([7; 32]);
    let wallet = Pubkey::new_from_array([8; 32]);

    assert_eq!(
        pda::config(),
        Pubkey::find_program_address(&[b"config"], &ID)
    );
    assert_eq!(
        pda::entry_position(&raffle, &wallet),
        Pubkey::find_program_address(&[b"position", raffle.as_ref(), wallet.as_ref()], &ID)
    );
}

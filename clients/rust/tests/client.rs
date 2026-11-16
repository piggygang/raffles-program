//! The typed client against its own IDL. What it shares with the program is
//! checked from the program's side, where both are at hand:
//! programs/raffles/tests/integration/interface.rs.

// Test code may panic; the lints are for the crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use {
    anchor_lang::{prelude::Pubkey, Event},
    raffles_client::{events, parsers, pda, read_event, EVENT_TAG, ID, IDL},
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
    assert!(read_event(&[149, 118, 59, 220, 196, 127, 161, 179]).is_none());
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

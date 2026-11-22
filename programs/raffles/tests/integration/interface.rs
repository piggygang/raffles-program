//! The released interface against the program: the IDL of record
//! (idls/raffles.json), the event and instruction documents, and the Rust
//! client (clients/rust), which `declare_program!` generates from that IDL.
//!
//! `anchor build` compiles this file in order to write the IDL. So nothing
//! here may name what the client generates from the IDL's contents, such as
//! an event's type: a test that needed the new IDL to compile could never get
//! it. The tests use what the client writes by hand (`pda`, `read_event`) and
//! what any IDL gives it (`ID`, `IDL`, `VERSION`).

use {
    crate::harness::{settings, Test},
    anchor_lang::{
        prelude::Pubkey,
        solana_program::instruction::{AccountMeta, Instruction},
        Event,
    },
    raffles::{
        constants::PROGRAM_DATA,
        events::{EntriesClaimed, EntryAccountsClosed, RaffleClosed, RaffleCreated},
        state::{
            Config, CreatorEntry, EntryPosition, EntryRange, EntryReceipt, EscrowKind,
            MembershipRule, PrizeEntry, PrizeKind, Raffle,
        },
    },
    raffles_client::{pda, read_event, EVENT_TAG},
};

/// Written by `anchor build`, next to the program the tests load.
const BUILT_IDL: &str = concat!(env!("CARGO_TARGET_TMPDIR"), "/../idl/raffles.json");
const EVENTS_DOC: &str = include_str!("../../../../docs/events.md");
const INSTRUCTIONS_DOC: &str = include_str!("../../../../docs/instructions.md");

fn key(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

/// An event instruction's data: Anchor's tag, then the event.
fn event_instruction(event: &impl Event) -> Vec<u8> {
    [EVENT_TAG, &event.data()].concat()
}

fn idl() -> serde_json::Value {
    serde_json::from_str(raffles_client::IDL).unwrap()
}

/// The rows of the table under the heading `heading` of `document`, a whole
/// heading line such as `## 6. Errors`, each as its cells without their
/// backticks. The section ends at the next heading of that level or above.
fn table(document: &str, heading: &str) -> Vec<Vec<String>> {
    let level = heading.chars().take_while(|&c| c == '#').count();
    let ends_the_section = |line: &str| {
        let hashes = line.chars().take_while(|&c| c == '#').count();
        (1..=level).contains(&hashes) && line[hashes..].starts_with(' ')
    };
    let rows: Vec<Vec<String>> = document
        .lines()
        .skip_while(|line| *line != heading)
        .skip(1)
        .take_while(|line| !ends_the_section(line))
        .filter(|line| line.starts_with('|'))
        // The header and the line under it.
        .skip(2)
        .map(|line| {
            line.trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().trim_matches('`').to_owned())
                .collect()
        })
        .collect();
    assert!(!rows.is_empty(), "no table under `{heading}`");
    rows
}

fn hex(bytes: &serde_json::Value) -> String {
    bytes
        .as_array()
        .unwrap()
        .iter()
        .map(|byte| format!("{:02x}", byte.as_u64().unwrap()))
        .collect()
}

#[test]
fn the_idl_of_record_is_the_one_this_build_wrote() {
    let built = std::fs::read_to_string(BUILT_IDL)
        .unwrap_or_else(|err| panic!("reading {BUILT_IDL} ({err}); run `anchor build` first"));

    // Not `assert_eq!`, which would print both files.
    assert!(
        built == raffles_client::IDL,
        "idls/raffles.json isn't the IDL this build wrote: the program's interface changed. \
         Copy target/idl/raffles.json over it. If a release already has the old one, the \
         version has to change too (docs/releases.md)."
    );
}

#[test]
fn the_client_is_this_program_at_this_version() {
    assert_eq!(raffles_client::ID, raffles::ID);
    // This test belongs to the program's package, so this is its version.
    assert_eq!(raffles_client::VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn the_client_derives_every_address_as_the_program_does() {
    let (creator, wallet, nft, mint) = (key(1), key(2), key(3), key(4));
    let nonce = 0x0807_0605_0403_0201;
    let (raffle, _) = Raffle::find_address(&creator, nonce);

    assert_eq!(pda::config(), Config::find_address());
    assert_eq!(
        pda::creator_entry(&creator),
        CreatorEntry::find_address(&creator)
    );
    for &kind in PrizeKind::ALL {
        assert_eq!(
            pda::prize_entry(kind.into(), &mint),
            PrizeEntry::find_address(kind, &mint)
        );
    }
    assert_eq!(
        pda::raffle(&creator, nonce),
        Raffle::find_address(&creator, nonce)
    );
    assert_eq!(pda::vault(&raffle), Raffle::find_vault_address(&raffle));
    assert_eq!(
        pda::entry_position(&raffle, &wallet),
        EntryPosition::find_address(&raffle, &wallet)
    );
    assert_eq!(
        pda::entry_receipt(&raffle, &nft),
        EntryReceipt::find_address(&raffle, &nft)
    );
    assert_eq!(
        pda::token_escrow(&raffle, &mint).0,
        Raffle::escrow_address(&raffle, &mint)
    );
    assert_eq!(
        pda::event_authority(),
        Pubkey::find_program_address(&[b"__event_authority"], &raffles::ID)
    );
    assert_eq!(pda::program_data().0, PROGRAM_DATA);
}

/// The events with a string, a list or a nested record are the ones a
/// decoder could misread; `RaffleClosed` stands for the fixed-size rest.
#[test]
fn the_client_reads_the_events_the_program_emits() {
    let created = RaffleCreated {
        raffle: key(1),
        creator: key(2),
        nonce: 7,
        created_by_authority: false,
        escrow_kind: EscrowKind::Token.into(),
        prize_mint: key(3),
        prize_amount: 1,
        allowlist_kind: PrizeKind::Mint.into(),
        allowlist_address: key(3),
        start_ts: 1_794_000_000,
        end_ts: 1_794_604_800,
        draw_fee_lamports: 15_515_720,
        membership_rules: [MembershipRule::default(); 3],
        entry_cap: 10,
        full_gang_bonus: 3,
        min_duration: 3_600,
        max_duration: 2_592_000,
        rules_hash: [9; 32],
        rules_uri: "ar://bNbA3TEQVL60xlgCcqdz4ZPHFZ711cZ3hmkpGttDt_U".into(),
        title: "Piggy Gang #4521".into(),
        description: "Hold a Piggy, win a Piggy.".into(),
    };
    let claimed = EntriesClaimed {
        raffle: key(1),
        wallet: key(4),
        range: EntryRange { start: 0, count: 5 },
        bonus_entries: 3,
        entries_claimed: 2,
        collections_seen: 0b111,
        bonus_awarded: true,
        total_entries: 5,
        nfts: vec![key(5), key(6)],
    };
    let entry_accounts_closed = EntryAccountsClosed {
        raffle: key(1),
        open_entry_accounts: 0,
        wallets: vec![key(4)],
        nfts: vec![key(5), key(6)],
    };
    let closed = RaffleClosed {
        raffle: key(1),
        creator: key(2),
    };

    for (name, data) in [
        ("RaffleCreated", event_instruction(&created)),
        ("EntriesClaimed", event_instruction(&claimed)),
        (
            "EntryAccountsClosed",
            event_instruction(&entry_accounts_closed),
        ),
        ("RaffleClosed", event_instruction(&closed)),
    ] {
        assert!(matches!(read_event(&data), Some(Ok(_))), "{name}");
        // Borsh reads the whole event or fails: one byte short isn't an event.
        assert!(
            matches!(read_event(&data[..data.len() - 1]), Some(Err(_))),
            "{name}, cut short"
        );
    }
}

/// The same for an event the program emitted in a transaction: what a reader
/// takes from the inner instructions is what the client reads.
#[test]
fn the_client_reads_an_event_from_a_transaction() {
    let mut test = Test::new();

    let sent = test.init_config(settings(0));

    let events = sent.events();
    assert_eq!(events.len(), 1);
    let data = [EVENT_TAG, &events[0].data].concat();
    assert!(matches!(read_event(&data), Some(Ok(_))));
    assert!(matches!(read_event(&data[..data.len() - 1]), Some(Err(_))));
}

#[test]
fn the_document_has_a_layout_for_each_event_of_the_idl_and_no_other() {
    let idl = idl();
    let mut in_idl: Vec<&str> = idl["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["name"].as_str().unwrap())
        .collect();
    in_idl.sort_unstable();

    // The `###` headings of the document's section 3.
    let mut documented: Vec<&str> = EVENTS_DOC
        .lines()
        .skip_while(|line| *line != "## 3. Layouts")
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| line.strip_prefix("### "))
        .collect();
    documented.sort_unstable();

    assert!(!in_idl.is_empty());
    assert_eq!(documented, in_idl);
}

/// The instructions document lists the IDL's instructions, each with its
/// discriminator, and has a section for each: those and no others.
#[test]
fn the_document_lists_each_instruction_of_the_idl_and_no_other() {
    let idl = idl();
    let mut in_idl: Vec<(String, String)> = idl["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|instruction| {
            (
                instruction["name"].as_str().unwrap().to_owned(),
                hex(&instruction["discriminator"]),
            )
        })
        .collect();
    in_idl.sort_unstable();

    let rows = table(INSTRUCTIONS_DOC, "## 2. The instructions");
    let mut documented: Vec<(String, String)> = rows
        .iter()
        .map(|row| (row[0].clone(), row[1].clone()))
        .collect();
    documented.sort_unstable();

    assert!(!in_idl.is_empty());
    assert_eq!(documented, in_idl);

    // The `###` headings of the document's section 3, in the table's order.
    let sections: Vec<&str> = INSTRUCTIONS_DOC
        .lines()
        .skip_while(|line| *line != "## 3. Accounts and checks")
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| line.strip_prefix("### "))
        .collect();
    let listed: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(sections, listed);
}

/// Each instruction's section of the document has its accounts as the IDL
/// has them: in the order the instruction takes them, with what is written
/// and what signs. A client builds from the IDL, so this is the order and the
/// flags it sends.
#[test]
fn the_document_gives_each_instruction_the_accounts_of_the_idl() {
    let yes_no = |flag: &serde_json::Value| {
        if flag.as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        }
    };
    let idl = idl();
    let instructions = idl["instructions"].as_array().unwrap();
    assert!(!instructions.is_empty());

    for instruction in instructions {
        let name = instruction["name"].as_str().unwrap();
        let in_idl: Vec<Vec<String>> = instruction["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, account)| {
                assert!(
                    account.get("accounts").is_none(),
                    "{name}: nested accounts aren't covered"
                );
                vec![
                    index.to_string(),
                    account["name"].as_str().unwrap().to_owned(),
                    yes_no(&account["writable"]).to_owned(),
                    yes_no(&account["signer"]).to_owned(),
                ]
            })
            .collect();

        let documented: Vec<Vec<String>> = table(INSTRUCTIONS_DOC, &format!("### {name}"))
            .into_iter()
            .map(|row| row[..4].to_vec())
            .collect();

        assert_eq!(documented, in_idl, "{name}");
    }
}

/// The document's error table is the IDL's errors: every number, name and
/// message a client can be handed.
#[test]
fn the_document_lists_each_error_of_the_idl_and_no_other() {
    let idl = idl();
    let in_idl: Vec<Vec<String>> = idl["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|error| {
            vec![
                error["code"].as_u64().unwrap().to_string(),
                error["name"].as_str().unwrap().to_owned(),
                error["msg"].as_str().unwrap().to_owned(),
            ]
        })
        .collect();

    let documented: Vec<Vec<String>> = table(INSTRUCTIONS_DOC, "## 6. Errors")
        .into_iter()
        .map(|row| row[..3].to_vec())
        .collect();

    assert!(!in_idl.is_empty());
    assert_eq!(documented, in_idl);
}

/// Anyone can send the program an instruction that starts with the event
/// tag. Only the program can sign for the event authority, so it fails, and
/// readers skip failed transactions: an event instruction in a successful one
/// is the program's own (docs/events.md, section 1).
#[test]
fn the_program_refuses_an_event_instruction_it_did_not_send() {
    let mut test = Test::new();
    let (event_authority, _) = pda::event_authority();
    let forged = Instruction::new_with_bytes(
        raffles::ID,
        &event_instruction(&RaffleClosed {
            raffle: key(1),
            creator: key(2),
        }),
        vec![AccountMeta::new_readonly(event_authority, false)],
    );

    let failed = test.try_send(&[forged], &[]).unwrap_err();

    // Anchor's ConstraintSigner: the event authority didn't sign.
    assert_eq!(
        format!("{:?}", failed.err),
        "InstructionError(0, Custom(2002))",
        "{}",
        failed.meta.pretty_logs()
    );
}

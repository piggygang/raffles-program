//! What the prototype accepts and rejects, so the budget measures a claim
//! that does the real work: the holder-proof §4 checks, receipts, the cap,
//! and the Full Gang bonus.

use {
    crate::harness::{
        failed_with, Test, ASSET_COLLECTION, ASSET_UPDATE_AUTHORITY_TAG, END_TS, FIRST_CREATOR,
        FIRST_CREATOR_VERIFIED, GIRL_GANG, SOL_GANG, SYMBOL, UPDATE_AUTHORITY,
    },
    anchor_lang::prelude::{Clock, Pubkey},
    claim_spike::{
        client::{self, Claim, Pig},
        state::EntryRange,
    },
    solana_signer::Signer,
};

/// One pig of each collection: an entry each, then the bonus, in one range.
#[test]
fn a_full_gang_claim_enters_every_pig_and_awards_the_bonus() {
    let mut test = Test::new();
    let pigs = [
        test.tm_pig(SOL_GANG),
        test.tm_pig(GIRL_GANG),
        test.core_pig(),
    ];
    test.claim(&pigs).unwrap();

    let position = test.position();
    assert_eq!(position.wallet, test.wallet.pubkey());
    assert_eq!(position.entries_claimed, 3);
    assert_eq!(position.collections_seen, 0b111);
    assert!(position.bonus_awarded);
    assert_eq!(position.range_count, 1);
    assert_eq!(position.ranges[0], EntryRange { start: 0, count: 6 });
    assert_eq!(test.raffle_state().total_entries, 6);
    for pig in &pigs {
        let receipt = test.receipt(pig).unwrap();
        assert_eq!(
            (receipt.nft, receipt.wallet),
            (pig.nft(), test.wallet.pubkey())
        );
    }
}

/// Each claim takes the next range, and the bonus comes once, with the claim
/// that completes the set (E-E7).
#[test]
fn claims_add_up_across_transactions() {
    let mut test = Test::new();
    let first = [test.tm_pig(SOL_GANG), test.tm_pig(SOL_GANG)];
    test.claim(&first).unwrap();
    let second = [test.tm_pig(GIRL_GANG), test.core_pig()];
    test.claim(&second).unwrap();
    let third = [test.core_pig()];
    test.claim(&third).unwrap();

    let position = test.position();
    assert_eq!(position.entries_claimed, 5);
    assert_eq!(
        position.ranges[..3],
        [
            EntryRange { start: 0, count: 2 },
            EntryRange { start: 2, count: 5 },
            EntryRange { start: 7, count: 1 },
        ]
    );
    assert_eq!(test.raffle_state().total_entries, 8);
}

/// At the cap a pig of a collection already in counts for nothing and fails
/// the claim (E-E1); one that completes the set still earns the bonus
/// (E-E16).
#[test]
fn at_the_cap_only_a_completing_pig_counts() {
    let mut test = Test::new();
    let sol_gang: Vec<Pig> = (0..10).map(|_| test.tm_pig(SOL_GANG)).collect();
    test.claim(&sol_gang[..8]).unwrap();
    test.claim(&sol_gang[8..]).unwrap();

    let eleventh = test.tm_pig(SOL_GANG);
    assert!(failed_with(test.claim(&[eleventh]), "CapReached"));

    let completing = [test.tm_pig(GIRL_GANG), test.core_pig()];
    test.claim(&completing).unwrap();
    let position = test.position();
    assert_eq!(position.entries_claimed, 10);
    assert!(position.bonus_awarded);
    assert_eq!(
        position.ranges[2],
        EntryRange {
            start: 10,
            count: 3
        }
    );
}

/// A pig enters a raffle once: a second claim fails on its receipt, as does
/// the same pig twice in one claim (E-E3, E-E9).
#[test]
fn a_pig_enters_a_raffle_once() {
    let mut test = Test::new();
    let pig = test.core_pig();
    test.claim(&[pig]).unwrap();
    assert!(failed_with(test.claim(&[pig]), "AlreadyEntered"));

    let other = test.tm_pig(SOL_GANG);
    assert!(failed_with(test.claim(&[other, other]), "AlreadyEntered"));
}

/// Lamports someone sends to a receipt address first don't block the pig. The
/// least they can send is an empty account's rent-exempt minimum.
#[test]
fn a_prefunded_receipt_address_still_takes_the_receipt() {
    let mut test = Test::new();
    let pig = test.tm_pig(SOL_GANG);
    let receipt = client::receipt(&test.raffle, &pig.nft());
    let least = test.svm.minimum_balance_for_rent_exemption(0);
    test.svm.airdrop(&receipt, least).unwrap();
    test.claim(&[pig]).unwrap();

    assert!(test.receipt(&pig).is_some());
    let rent = test
        .svm
        .minimum_balance_for_rent_exemption(test.svm.get_account(&receipt).unwrap().data.len());
    assert_eq!(test.lamports(&receipt), rent);
}

/// Every rejection path in holder-proof §7's fixture list, plus a stranger's
/// token account. Each case edits one valid pig.
#[test]
fn the_rules_reject_lookalikes() {
    type Edit = fn(&mut Test, &Pig);
    let cases: [(&str, u8, Edit, &str); 10] = [
        (
            "creator unverified (#293)",
            SOL_GANG,
            |test, pig| {
                test.edit(&metadata(pig), |account| {
                    account.data[FIRST_CREATOR_VERIFIED] = 0
                });
            },
            "RuleMismatch",
        ),
        (
            "another update authority and symbol (Honorary)",
            SOL_GANG,
            |test, pig| {
                let stranger = test.key();
                test.edit(&metadata(pig), |account| {
                    account.data[UPDATE_AUTHORITY].copy_from_slice(stranger.as_ref());
                    account.data[SYMBOL].copy_from_slice(b"HPSG\0\0\0\0\0\0");
                });
            },
            "RuleMismatch",
        ),
        (
            "another symbol",
            SOL_GANG,
            |test, pig| {
                test.edit(&metadata(pig), |account| {
                    account.data[SYMBOL].copy_from_slice(b"PSGX\0\0\0\0\0\0")
                });
            },
            "RuleMismatch",
        ),
        (
            "another verified creator (Pig Mud)",
            SOL_GANG,
            |test, pig| {
                let creator = test.key();
                test.edit(&metadata(pig), |account| {
                    account.data[FIRST_CREATOR].copy_from_slice(creator.as_ref())
                });
            },
            "RuleMismatch",
        ),
        (
            "metadata of another mint",
            SOL_GANG,
            |test, pig| {
                let Pig::TokenMetadata { token, .. } = *pig else {
                    unreachable!()
                };
                let (other_mint, owner) = (test.key(), test.wallet.pubkey());
                test.set_token_account(token, other_mint, owner, 1);
            },
            "NotMetadata",
        ),
        (
            "metadata not owned by Token Metadata",
            SOL_GANG,
            |test, pig| {
                test.edit(&metadata(pig), |account| {
                    account.owner = Pubkey::new_unique()
                });
            },
            "NotMetadata",
        ),
        (
            "a stranger's token account",
            SOL_GANG,
            |test, pig| {
                let Pig::TokenMetadata { token, mint, .. } = *pig else {
                    unreachable!()
                };
                let stranger = test.key();
                test.set_token_account(token, mint, stranger, 1);
            },
            "NotHolder",
        ),
        (
            "a Core asset in another collection",
            2,
            |test, pig| {
                let collection = test.key();
                test.edit(&pig.nft(), |account| {
                    account.data[ASSET_COLLECTION].copy_from_slice(collection.as_ref())
                });
            },
            "RuleMismatch",
        ),
        (
            "a Core asset with update authority Address",
            2,
            |test, pig| {
                test.edit(&pig.nft(), |account| {
                    account.data[ASSET_UPDATE_AUTHORITY_TAG] = 1
                });
            },
            "RuleMismatch",
        ),
        (
            "a SOL Gang pig claimed under the Girl Gang rule",
            GIRL_GANG,
            |test, pig| {
                let Pig::TokenMetadata { mint, .. } = *pig else {
                    unreachable!()
                };
                let sol_gang = test.tm_pig(SOL_GANG);
                let data = test.svm.get_account(&metadata(&sol_gang)).unwrap().data;
                test.edit(&metadata(pig), |account| {
                    account.data = data;
                    account.data[crate::harness::MINT].copy_from_slice(mint.as_ref());
                });
            },
            "RuleMismatch",
        ),
    ];
    for (case, rule_index, edit, code) in cases {
        let mut test = Test::new();
        let pig = match rule_index {
            2 => test.core_pig(),
            rule_index => test.tm_pig(rule_index),
        };
        edit(&mut test, &pig);
        assert!(failed_with(test.claim(&[pig]), code), "{case}");
    }
}

/// Both metadata sizes pass, 607 and 679 bytes, and Girl Gang's missing
/// `token_standard` is never read (holder-proof §3).
#[test]
fn both_metadata_sizes_pass() {
    let mut test = Test::new();
    let padded = test.tm_pig(SOL_GANG);
    test.edit(&metadata(&padded), |account| {
        assert_eq!(account.data.len(), 607);
        account.data.resize(679, 0);
    });
    let pigs = [padded, test.tm_pig(GIRL_GANG)];
    test.claim(&pigs).unwrap();
}

/// The variants the budget compares accept the same claim.
#[test]
fn every_variant_claims() {
    for claim in [Claim::WithMints, Claim::FullDecode, Claim::Traced] {
        let mut test = Test::new();
        let pigs = [
            test.tm_pig(SOL_GANG),
            test.tm_pig(GIRL_GANG),
            test.core_pig(),
        ];
        let transaction = test.claim_transaction(claim, &pigs, None);
        test.svm
            .send_transaction(transaction)
            .unwrap_or_else(|failed| panic!("{claim:?}: {}", failed.meta.pretty_logs()));
        assert!(test.position().bonus_awarded, "{claim:?}");
    }
}

#[test]
fn the_guards_hold() {
    // The creator can't enter its own raffle (E-E12).
    let mut test = Test::new();
    let creator = test.wallet.pubkey();
    test.raffle = test.create_raffle(creator, 10);
    let pig = test.core_pig();
    assert!(failed_with(test.claim(&[pig]), "CreatorCannotEnter"));

    // Paused (E-E13).
    let mut test = Test::new();
    test.set_paused(true);
    let pig = test.core_pig();
    assert!(failed_with(test.claim(&[pig]), "Paused"));

    // After `end_ts` (E-E6).
    let mut test = Test::new();
    let mut clock: Clock = test.svm.get_sysvar();
    clock.unix_timestamp = END_TS;
    test.svm.set_sysvar(&clock);
    let pig = test.core_pig();
    assert!(failed_with(test.claim(&[pig]), "NotLive"));
}

fn metadata(pig: &Pig) -> Pubkey {
    client::metadata(&pig.nft())
}

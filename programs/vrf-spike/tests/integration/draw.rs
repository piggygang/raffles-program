use {
    crate::harness::{fail, Test, DEPOSIT},
    anchor_lang::{InstructionData, ToAccountMetas},
    litesvm::types::FailedTransactionMetadata,
    solana_keypair::Keypair,
    solana_signer::Signer,
    vrf_spike::{client, winner_index},
};

fn has_error(failed: &FailedTransactionMetadata, name: &str) -> bool {
    let code = format!("Error Code: {name}.");
    failed.meta.logs.iter().any(|log| log.contains(&code))
}

#[test]
fn request_is_permissionless_and_paid_from_the_vault() {
    let mut test = Test::new();
    test.init_draw();
    let vault = client::vault(&test.draw_address());
    let treasury = test.lamports(&test.treasury);
    assert_eq!(test.lamports(&vault), DEPOSIT);

    test.request().unwrap_or_else(fail);

    let draw = test.draw();
    assert_eq!(draw.requests, 1);
    assert_eq!(draw.seed, client::request_seed(&test.draw_address(), 0));
    assert_eq!(draw.requested_slot, test.slot());
    let request = test.request_account(&draw.seed);
    assert!(request.pending().is_some());
    assert_eq!(*request.client(), vault);
    let fee = test.request_fee();
    assert_eq!(test.lamports(&test.treasury), treasury + fee);
    let rent = test.lamports(&client::request(&draw.seed));
    assert_eq!(test.lamports(&vault), DEPOSIT - fee - rent);
}

#[test]
fn consume_waits_for_a_quorum_then_draws_from_the_combined_randomness() {
    let mut test = Test::new();
    test.init_draw();
    test.request().unwrap_or_else(fail);
    let seed = test.draw().seed;
    let quorum = orao_solana_vrf::majority(test.authorities.len());

    for authority in 0..quorum - 1 {
        test.fulfill(&seed, authority);
        let failed = test.consume(1_000).unwrap_err();
        assert!(
            has_error(&failed, "NotFulfilled"),
            "{}",
            failed.meta.pretty_logs()
        );
    }
    let vault = client::vault(&test.draw_address());
    let before = test.lamports(&vault);
    test.fulfill(&seed, quorum - 1);

    // The randomness is the XOR of the quorum's signatures of the seed, so
    // anyone can recompute it from the authorities' public responses.
    let mut expected = [0; 64];
    for authority in &test.authorities[..quorum] {
        let signature = authority.sign_message(&seed);
        orao_solana_vrf::xor_array(&mut expected, signature.as_array());
    }
    let request = test.request_account(&seed);
    assert_eq!(request.fulfilled().unwrap().randomness, expected);
    // Fulfilling shrinks the request to its final size and refunds the
    // difference to the client: the vault.
    assert!(test.lamports(&vault) > before);

    test.advance_slots(3);
    let meta = test.consume(1_000).unwrap_or_else(fail);
    let draw = test.draw();
    assert_eq!(draw.value, expected);
    assert_eq!(draw.consumed_slot, draw.requested_slot + 3);
    let winner = format!("winner {} of 1000", winner_index(&expected, 1_000).unwrap());
    assert!(
        meta.logs.iter().any(|log| log.ends_with(&winner)),
        "{}",
        meta.pretty_logs()
    );
}

#[test]
fn a_rerequest_moves_the_draw_to_a_new_seed() {
    let mut test = Test::new();
    test.init_draw();
    test.request().unwrap_or_else(fail);
    let first = test.draw().seed;
    test.request().unwrap_or_else(fail);

    let draw = test.draw();
    assert_eq!(draw.requests, 2);
    assert_ne!(draw.seed, first);
    assert_eq!(draw.seed, client::request_seed(&test.draw_address(), 1));
    // A late fulfillment of the first request doesn't settle the draw.
    for authority in 0..orao_solana_vrf::majority(test.authorities.len()) {
        test.fulfill(&first, authority);
    }
    let failed = test.consume(10).unwrap_err();
    assert!(
        has_error(&failed, "NotFulfilled"),
        "{}",
        failed.meta.pretty_logs()
    );
}

#[test]
fn request_adopts_a_request_someone_else_made_for_its_seed() {
    let mut test = Test::new();
    test.init_draw();
    let seed = client::request_seed(&test.draw_address(), 0);
    let stranger = Keypair::new();
    test.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let request = anchor_lang::solana_program::instruction::Instruction::new_with_bytes(
        orao_solana_vrf::ID,
        &orao_solana_vrf::instruction::RequestV2 { seed }.data(),
        orao_solana_vrf::accounts::RequestV2 {
            payer: stranger.pubkey(),
            network_state: client::network_state(),
            treasury: test.treasury,
            request: client::request(&seed),
            system_program: solana_sdk_ids::system_program::ID,
        }
        .to_account_metas(None),
    );
    test.send(&stranger, &[request]).unwrap_or_else(fail);
    let vault = client::vault(&test.draw_address());

    let meta = test.request().unwrap_or_else(fail);

    assert!(
        meta.logs.iter().any(|log| log.contains("adopting")),
        "{}",
        meta.pretty_logs()
    );
    assert_eq!(test.lamports(&vault), DEPOSIT);
    assert_eq!(test.draw().seed, seed);
    for authority in 0..orao_solana_vrf::majority(test.authorities.len()) {
        test.fulfill(&seed, authority);
    }
    test.consume(10).unwrap_or_else(fail);
}

#[test]
fn close_returns_the_vault_to_the_creator() {
    let mut test = Test::new();
    test.init_draw();
    test.request().unwrap_or_else(fail);
    let draw_address = test.draw_address();
    let vault = client::vault(&draw_address);
    let creator = test.creator.pubkey();
    let returned = test.lamports(&vault) + test.lamports(&draw_address);
    let before = test.lamports(&creator);

    let meta = test
        .send(
            &test.creator.insecure_clone(),
            &[client::close_draw(creator)],
        )
        .unwrap_or_else(fail);

    assert_eq!(test.lamports(&vault), 0);
    assert_eq!(test.lamports(&draw_address), 0);
    assert_eq!(test.lamports(&creator), before + returned - meta.fee);
}

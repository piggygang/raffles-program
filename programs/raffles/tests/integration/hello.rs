use {
    crate::harness::Test,
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    solana_signer::Signer,
};

#[test]
fn hello_logs_a_greeting_to_the_signer() {
    let mut test = Test::new();
    let caller = test.payer.pubkey();
    let instruction = Instruction::new_with_bytes(
        raffles::ID,
        &raffles::instruction::Hello {}.data(),
        raffles::accounts::Hello { caller }.to_account_metas(None),
    );

    let meta = test.send(&[instruction], &[]);

    let greeting = format!("Program log: Hello from PiggyGang Raffles, {caller}");
    assert!(meta.logs.contains(&greeting), "{}", meta.pretty_logs());
}

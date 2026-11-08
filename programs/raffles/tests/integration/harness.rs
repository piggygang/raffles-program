use {
    anchor_lang::solana_program::instruction::Instruction,
    anchor_spl::metadata::mpl_token_metadata,
    litesvm::{types::TransactionMetadata, LiteSVM},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

/// Written by `anchor build`; the tests never build the program themselves.
const RAFFLES_SO: &str = concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/raffles.so");
/// Written by `scripts/dump-fixtures.sh`.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

/// A fresh LiteSVM with the raffles program at its declared address, the
/// Metaplex programs, and a funded payer.
pub struct Test {
    pub svm: LiteSVM,
    pub payer: Keypair,
}

impl Test {
    pub fn new() -> Self {
        let mut svm = LiteSVM::new();
        svm.add_program_from_file(raffles::ID, RAFFLES_SO)
            .unwrap_or_else(|err| panic!("loading {RAFFLES_SO} ({err}); run `anchor build` first"));
        for (id, file) in [
            (mpl_token_metadata::ID, "mpl_token_metadata.so"),
            (mpl_core::ID, "mpl_core.so"),
        ] {
            svm.add_program_from_file(id, format!("{FIXTURES}/{file}"))
                .unwrap_or_else(|err| panic!("loading fixture {file}: {err}"));
        }

        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        Self { svm, payer }
    }

    /// Sends `instructions` paid for by the payer and also signed by
    /// `signers`. Panics with the program logs if the transaction fails.
    pub fn send(
        &mut self,
        instructions: &[Instruction],
        signers: &[&Keypair],
    ) -> TransactionMetadata {
        let message = Message::new_with_blockhash(
            instructions,
            Some(&self.payer.pubkey()),
            &self.svm.latest_blockhash(),
        );
        let signers: Vec<&Keypair> = std::iter::once(&self.payer)
            .chain(signers.iter().copied())
            .collect();
        let transaction =
            VersionedTransaction::try_new(VersionedMessage::Legacy(message), &signers).unwrap();
        self.svm
            .send_transaction(transaction)
            .unwrap_or_else(|failed| panic!("{}\n{}", failed.err, failed.meta.pretty_logs()))
    }
}

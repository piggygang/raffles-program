use {
    anchor_lang::{
        prelude::{Clock, Pubkey},
        solana_program::instruction::Instruction,
        AccountDeserialize, AccountSerialize, InstructionData, ToAccountMetas,
    },
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    litesvm::{
        types::{FailedTransactionMetadata, TransactionMetadata},
        LiteSVM,
    },
    orao_solana_vrf::state::{NetworkState, RandomnessV2},
    solana_account::Account,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    vrf_spike::{client, state::Draw},
};

/// Written by `anchor build`; the tests never build the program themselves.
const VRF_SPIKE_SO: &str = concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/vrf_spike.so");
/// Written by `scripts/dump-vrf-spike-fixtures.sh`.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
/// Enough for a handful of requests' rent and fees.
pub const DEPOSIT: u64 = LAMPORTS_PER_SOL / 10;

/// A fresh LiteSVM with vrf_spike and the devnet ORAO program, whose network
/// state names `authorities` as its fulfillment authorities. The creator and a
/// crank, which signs nothing but its own fees, are funded.
pub struct Test {
    pub svm: LiteSVM,
    pub creator: Keypair,
    pub crank: Keypair,
    pub treasury: Pubkey,
    pub authorities: Vec<Keypair>,
}

impl Test {
    pub fn new() -> Self {
        let mut svm = LiteSVM::new();
        svm.add_program_from_file(vrf_spike::ID, VRF_SPIKE_SO)
            .unwrap_or_else(|err| {
                panic!("loading {VRF_SPIKE_SO} ({err}); run `anchor build` first")
            });
        svm.add_program_from_file(orao_solana_vrf::ID, format!("{FIXTURES}/orao_vrf.so"))
            .unwrap_or_else(|err| panic!("loading fixture orao_vrf.so: {err}"));
        let treasury = load_account(&mut svm, "treasury");
        let network_state = load_account(&mut svm, "network_state");

        // Swap ORAO's authorities for keys the tests hold, as many as there are,
        // so the quorum is devnet's.
        let mut account = svm.get_account(&network_state).unwrap();
        let mut state = NetworkState::try_deserialize(&mut account.data.as_slice()).unwrap();
        assert_eq!(
            state.config.treasury, treasury,
            "the fixtures disagree; rerun scripts/dump-vrf-spike-fixtures.sh"
        );
        let authorities: Vec<Keypair> = state
            .config
            .fulfillment_authorities
            .iter()
            .map(|_| Keypair::new())
            .collect();
        state.config.fulfillment_authorities = authorities.iter().map(|key| key.pubkey()).collect();
        let mut data = Vec::new();
        state.try_serialize(&mut data).unwrap();
        account.data[..data.len()].copy_from_slice(&data);
        svm.set_account(network_state, account).unwrap();

        let (creator, crank) = (Keypair::new(), Keypair::new());
        for key in [&creator, &crank].into_iter().chain(&authorities) {
            svm.airdrop(&key.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        }
        svm.warp_to_slot(1_000);
        Self {
            svm,
            creator,
            crank,
            treasury,
            authorities,
        }
    }

    pub fn slot(&self) -> u64 {
        self.svm.get_sysvar::<Clock>().slot
    }

    pub fn advance_slots(&mut self, slots: u64) {
        self.svm.warp_to_slot(self.slot() + slots);
    }

    pub fn request_fee(&self) -> u64 {
        let account = self.svm.get_account(&client::network_state()).unwrap();
        NetworkState::try_deserialize(&mut account.data.as_slice())
            .unwrap()
            .config
            .request_fee
    }

    pub fn draw_address(&self) -> Pubkey {
        client::draw(&self.creator.pubkey())
    }

    pub fn draw(&self) -> Draw {
        let account = self.svm.get_account(&self.draw_address()).unwrap();
        Draw::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn request_account(&self, seed: &[u8; 32]) -> RandomnessV2 {
        let account = self.svm.get_account(&client::request(seed)).unwrap();
        assert_eq!(account.owner, orao_solana_vrf::ID);
        RandomnessV2::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn lamports(&self, address: &Pubkey) -> u64 {
        self.svm
            .get_account(address)
            .map_or(0, |account| account.lamports)
    }

    pub fn init_draw(&mut self) {
        let instruction = client::init_draw(self.creator.pubkey(), DEPOSIT);
        self.send(&self.creator.insecure_clone(), &[instruction])
            .unwrap_or_else(fail);
    }

    /// Requests the draw's next seed, paid by the crank.
    pub fn request(&mut self) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let instruction =
            client::request_randomness(self.draw_address(), self.draw().requests, self.treasury);
        self.send(&self.crank.insecure_clone(), &[instruction])
    }

    /// Fulfills `seed` the way an ORAO authority does: an Ed25519 signature of
    /// the seed, verified by the precompile in the same transaction.
    pub fn fulfill(&mut self, seed: &[u8; 32], authority: usize) {
        let authority = self.authorities[authority].insecure_clone();
        let client = *self.request_account(seed).client();
        let fulfill = Instruction::new_with_bytes(
            orao_solana_vrf::ID,
            &orao_solana_vrf::instruction::FulfillV2 {}.data(),
            orao_solana_vrf::accounts::FulfillV2 {
                payer: authority.pubkey(),
                instruction_acc: solana_sdk_ids::sysvar::instructions::ID,
                network_state: client::network_state(),
                request: client::request(seed),
                client,
                system_program: solana_sdk_ids::system_program::ID,
            }
            .to_account_metas(None),
        );
        self.send(
            &authority,
            &[ed25519_instruction(&authority, seed), fulfill],
        )
        .unwrap_or_else(fail);
    }

    pub fn consume(
        &mut self,
        entries: u64,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let instruction = client::consume(self.draw_address(), &self.draw().seed, entries);
        self.send(&self.crank.insecure_clone(), &[instruction])
    }

    /// Sends `instructions` paid for and signed by `payer` alone, with a fresh
    /// blockhash.
    pub fn send(
        &mut self,
        payer: &Keypair,
        instructions: &[Instruction],
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        self.svm.expire_blockhash();
        let message = Message::new_with_blockhash(
            instructions,
            Some(&payer.pubkey()),
            &self.svm.latest_blockhash(),
        );
        let transaction =
            VersionedTransaction::try_new(VersionedMessage::Legacy(message), &[payer]).unwrap();
        self.svm.send_transaction(transaction)
    }
}

/// For `unwrap_or_else`: panics with the program logs.
pub fn fail<T>(failed: FailedTransactionMetadata) -> T {
    panic!("{}\n{}", failed.err, failed.meta.pretty_logs())
}

/// The precompile instruction ORAO's fulfillment checks: one signature, with
/// every offset pointing into this instruction.
fn ed25519_instruction(signer: &Keypair, message: &[u8; 32]) -> Instruction {
    const PUBKEY: u16 = 16;
    const SIGNATURE: u16 = PUBKEY + 32;
    const MESSAGE: u16 = SIGNATURE + 64;
    let mut data = vec![1, 0];
    for value in [SIGNATURE, u16::MAX, PUBKEY, u16::MAX, MESSAGE, 32, u16::MAX] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(signer.pubkey().as_ref());
    data.extend_from_slice(signer.sign_message(message).as_ref());
    data.extend_from_slice(message);
    Instruction::new_with_bytes(orao_solana_vrf::ED25519_PROGRAM_ID, &data, vec![])
}

/// Loads a `solana account --output json` fixture and returns its address.
fn load_account(svm: &mut LiteSVM, name: &str) -> Pubkey {
    let path = format!("{FIXTURES}/{name}.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let account = &json["account"];
    let address = json["pubkey"].as_str().unwrap().parse().unwrap();
    svm.set_account(
        address,
        Account {
            lamports: account["lamports"].as_u64().unwrap(),
            data: BASE64.decode(account["data"][0].as_str().unwrap()).unwrap(),
            owner: account["owner"].as_str().unwrap().parse().unwrap(),
            executable: account["executable"].as_bool().unwrap(),
            rent_epoch: account["rentEpoch"].as_u64().unwrap(),
        },
    )
    .unwrap();
    address
}

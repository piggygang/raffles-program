use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{program_option::COption, program_pack::Pack},
        AccountDeserialize, AccountSerialize, Space,
    },
    anchor_spl::{associated_token::get_associated_token_address, token::spl_token},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    claim_spike::{
        client::{self, Claim, Pig},
        state::{Config, EntryPosition, EntryReceipt, MembershipRule, Raffle},
    },
    litesvm::{
        types::{FailedTransactionMetadata, TransactionMetadata},
        LiteSVM,
    },
    solana_account::Account,
    solana_address_lookup_table_interface::state::{AddressLookupTable, LookupTableMeta},
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_keypair::Keypair,
    solana_message::{v0, AddressLookupTableAccount, Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    std::borrow::Cow,
};

/// Written by `anchor build`; the tests never build the program themselves.
const CLAIM_SPIKE_SO: &str = concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/claim_spike.so");
/// Written by `scripts/dump-claim-spike-fixtures.sh`.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
/// The largest transaction the cluster accepts, serialized (`PACKET_DATA_SIZE`).
pub const MAX_TX_BYTES: usize = 1_232;
/// The most accounts a transaction may lock (`MAX_TX_ACCOUNT_LOCKS`).
pub const MAX_ACCOUNT_LOCKS: usize = 64;
/// What the measured transactions request. Any value costs the same five
/// bytes, so the most a transaction can have keeps a measurement from ever
/// running out.
pub const CU_LIMIT: u32 = 1_400_000;
/// A priority fee, so the transactions carry both compute budget
/// instructions, as a client's will.
pub const CU_PRICE: u64 = 10_000;
pub const END_TS: i64 = 4_102_444_800;

// The mainnet membership rules (holder-proof §5).
pub const SOL_GANG: u8 = 0;
pub const GIRL_GANG: u8 = 1;
pub const PIGGY_GANG: u8 = 2;
const PIGGY_CREATOR: &str = "Pigv3gFWLWJL8QwrFBkdZLe1RYzNJTSJPGEUNVimJjh";
const PIGGY_UPDATE_AUTHORITY: &str = "7NsngNMtXJNdHgeK4znQDZ5xVzT2nkGoGRGhMWApejCV";
const PIGGY_GANG_COLLECTION: &str = "J3nHgSDJj6CPj2ypuDWNuvuiYii965RcSmFAPQVDWA18";

// Offsets in a padded metadata account, every pig's layout today.
pub const UPDATE_AUTHORITY: std::ops::Range<usize> = 1..33;
pub const MINT: std::ops::Range<usize> = 33..65;
pub const SYMBOL: std::ops::Range<usize> = 105..115;
pub const FIRST_CREATOR: std::ops::Range<usize> = 326..358;
pub const FIRST_CREATOR_VERIFIED: usize = 358;
// In a Core asset.
pub const ASSET_OWNER: std::ops::Range<usize> = 1..33;
pub const ASSET_UPDATE_AUTHORITY_TAG: usize = 33;
pub const ASSET_COLLECTION: std::ops::Range<usize> = 34..66;

/// A raffle with the mainnet membership rules, a funded wallet, and pigs
/// cloned from the mainnet fixtures. Every address comes from `seed`, so a
/// measurement's PDAs, and with them its compute units, repeat exactly.
pub struct Test {
    pub svm: LiteSVM,
    pub wallet: Keypair,
    pub raffle: Pubkey,
    seed: u64,
    keys: u64,
    sol_gang: Account,
    girl_gang: Account,
    piggy_gang: Account,
}

impl Test {
    /// Cap 10 and bonus 3, the spec's values.
    pub fn new() -> Self {
        Self::with(0, 10)
    }

    pub fn with(seed: u64, entry_cap: u8) -> Self {
        let mut svm = LiteSVM::new();
        svm.add_program_from_file(claim_spike::ID, CLAIM_SPIKE_SO)
            .unwrap_or_else(|err| {
                panic!("loading {CLAIM_SPIKE_SO} ({err}); run `anchor build` first")
            });
        let mut test = Self {
            svm,
            wallet: Keypair::new_from_array(key_bytes(seed, u64::MAX)),
            raffle: Pubkey::default(),
            seed,
            keys: 0,
            sol_gang: load_fixture("sol_gang_metadata"),
            girl_gang: load_fixture("girl_gang_metadata"),
            piggy_gang: load_fixture("piggy_gang_asset"),
        };
        test.svm
            .airdrop(&test.wallet.pubkey(), 100 * LAMPORTS_PER_SOL)
            .unwrap();
        test.set_paused(false);
        let creator = test.key();
        test.raffle = test.create_raffle(creator, entry_cap);
        test
    }

    /// The next deterministic address.
    pub fn key(&mut self) -> Pubkey {
        self.keys += 1;
        Pubkey::new_from_array(key_bytes(self.seed, self.keys))
    }

    pub fn set_paused(&mut self, paused: bool) {
        let (address, bump) = Pubkey::find_program_address(&[Config::SEED], &claim_spike::ID);
        let config = Config {
            admin: Pubkey::new_unique(),
            membership_rules: mainnet_rules(),
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 30 * 86_400,
            paused,
            bump,
        };
        self.write(address, &config, 8 + Config::INIT_SPACE);
    }

    pub fn create_raffle(&mut self, creator: Pubkey, entry_cap: u8) -> Pubkey {
        let address = self.key();
        let raffle = Raffle {
            creator,
            prize_mint: self.key(),
            end_ts: END_TS,
            state: Raffle::ACTIVE,
            total_entries: 0,
            winner: Pubkey::default(),
            drawn_at: 0,
            vrf_seed: [0; 32],
            vrf_requests: 0,
            vrf_requested_slot: 0,
            vrf_value: [0; 64],
            rules_uri: format!("https://arweave.net/{}", "r".repeat(43)),
            rules_hash: [7; 32],
            draw_fee_lamports: 15_515_720,
            membership_rules: mainnet_rules(),
            entry_cap,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 30 * 86_400,
            bump: 255,
            vault_bump: 255,
        };
        self.write(address, &raffle, 8 + Raffle::INIT_SPACE);
        address
    }

    /// Writes an Anchor account owned by the spike program.
    fn write<T: AccountSerialize>(&mut self, address: Pubkey, value: &T, space: usize) {
        let mut data = Vec::with_capacity(space);
        value.try_serialize(&mut data).unwrap();
        data.resize(space, 0);
        let lamports = self.svm.minimum_balance_for_rent_exemption(space);
        self.svm
            .set_account(
                address,
                Account {
                    lamports,
                    data,
                    owner: claim_spike::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .unwrap();
    }

    /// A SOL Gang or Girl Gang pig the wallet holds: a copy of the fixture's
    /// metadata for a fresh mint, plus that mint and the wallet's token
    /// account for it.
    pub fn tm_pig(&mut self, rule_index: u8) -> Pig {
        let mint = self.key();
        let mut metadata = if rule_index == GIRL_GANG {
            self.girl_gang.clone()
        } else {
            self.sol_gang.clone()
        };
        metadata.data[MINT].copy_from_slice(mint.as_ref());
        self.svm
            .set_account(client::metadata(&mint), metadata)
            .unwrap();
        let mut data = vec![0; spl_token::state::Mint::LEN];
        spl_token::state::Mint {
            mint_authority: COption::None,
            supply: 1,
            decimals: 0,
            is_initialized: true,
            freeze_authority: COption::None,
        }
        .pack_into_slice(&mut data);
        self.set_token_program_account(mint, data);
        let token = get_associated_token_address(&self.wallet.pubkey(), &mint);
        self.set_token_account(token, mint, self.wallet.pubkey(), 1);
        Pig::TokenMetadata {
            rule_index,
            mint,
            token,
        }
    }

    /// A Piggy Gang pig the wallet owns: a copy of the fixture's asset at a
    /// fresh address.
    pub fn core_pig(&mut self) -> Pig {
        let asset = self.key();
        let mut account = self.piggy_gang.clone();
        account.data[ASSET_OWNER].copy_from_slice(self.wallet.pubkey().as_ref());
        self.svm.set_account(asset, account).unwrap();
        Pig::Core {
            rule_index: PIGGY_GANG,
            asset,
        }
    }

    pub fn set_token_account(&mut self, address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) {
        let mut data = vec![0; spl_token::state::Account::LEN];
        spl_token::state::Account {
            mint,
            owner,
            amount,
            delegate: COption::None,
            state: spl_token::state::AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        }
        .pack_into_slice(&mut data);
        self.set_token_program_account(address, data);
    }

    fn set_token_program_account(&mut self, address: Pubkey, data: Vec<u8>) {
        let lamports = self.svm.minimum_balance_for_rent_exemption(data.len());
        self.svm
            .set_account(
                address,
                Account {
                    lamports,
                    data,
                    owner: spl_token::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .unwrap();
    }

    /// Rewrites an account in place.
    pub fn edit(&mut self, address: &Pubkey, edit: impl FnOnce(&mut Account)) {
        let mut account = self.svm.get_account(address).unwrap();
        edit(&mut account);
        self.svm.set_account(*address, account).unwrap();
    }

    /// A frozen lookup table holding `addresses`, usable at once: LiteSVM
    /// starts well past slot 0, when it was "extended".
    pub fn lookup_table(&mut self, addresses: Vec<Pubkey>) -> AddressLookupTableAccount {
        let key = self.key();
        let data = AddressLookupTable {
            meta: LookupTableMeta::default(),
            addresses: Cow::Owned(addresses.clone()),
        }
        .serialize_for_tests()
        .unwrap();
        let lamports = self.svm.minimum_balance_for_rent_exemption(data.len());
        self.svm
            .set_account(
                key,
                Account {
                    lamports,
                    data,
                    owner: solana_address_lookup_table_interface::program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .unwrap();
        AddressLookupTableAccount { key, addresses }
    }

    /// A claim transaction as a client builds it: both compute budget
    /// instructions, then the claim, signed by the wallet alone. With `table`,
    /// a v0 transaction that loads whatever it can from the table.
    pub fn claim_transaction(
        &self,
        claim: Claim,
        pigs: &[Pig],
        table: Option<&AddressLookupTableAccount>,
    ) -> VersionedTransaction {
        let instructions = [
            ComputeBudgetInstruction::set_compute_unit_limit(CU_LIMIT),
            ComputeBudgetInstruction::set_compute_unit_price(CU_PRICE),
            client::claim_entries(claim, self.wallet.pubkey(), self.raffle, pigs),
        ];
        let payer = self.wallet.pubkey();
        let blockhash = self.svm.latest_blockhash();
        let message = match table {
            None => VersionedMessage::Legacy(Message::new_with_blockhash(
                &instructions,
                Some(&payer),
                &blockhash,
            )),
            Some(table) => VersionedMessage::V0(
                v0::Message::try_compile(
                    &payer,
                    &instructions,
                    std::slice::from_ref(table),
                    blockhash,
                )
                .unwrap(),
            ),
        };
        VersionedTransaction::try_new(message, &[&self.wallet]).unwrap()
    }

    /// Sends a canonical claim, on a fresh blockhash so that a repeat isn't a
    /// duplicate.
    pub fn claim(
        &mut self,
        pigs: &[Pig],
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        self.svm.expire_blockhash();
        let transaction = self.claim_transaction(Claim::Canonical, pigs, None);
        self.svm.send_transaction(transaction)
    }

    pub fn position(&self) -> EntryPosition {
        let address = client::position(&self.raffle, &self.wallet.pubkey());
        let account = self.svm.get_account(&address).unwrap();
        EntryPosition::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn raffle_state(&self) -> Raffle {
        let account = self.svm.get_account(&self.raffle).unwrap();
        Raffle::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn receipt(&self, pig: &Pig) -> Option<EntryReceipt> {
        self.svm
            .get_account(&client::receipt(&self.raffle, &pig.nft()))
            .filter(|account| account.owner == claim_spike::ID)
            .map(|account| EntryReceipt::try_deserialize(&mut account.data.as_slice()).unwrap())
    }

    /// Lamports at `address`, 0 if it doesn't exist.
    pub fn lamports(&self, address: &Pubkey) -> u64 {
        self.svm
            .get_account(address)
            .map_or(0, |account| account.lamports)
    }
}

/// The serialized size the cluster limits.
pub fn size(transaction: &VersionedTransaction) -> usize {
    bincode::serialize(transaction).unwrap().len()
}

/// The accounts a transaction locks: its own keys plus what it loads from
/// lookup tables.
pub fn account_count(transaction: &VersionedTransaction) -> usize {
    let loaded: usize = transaction
        .message
        .address_table_lookups()
        .unwrap_or_default()
        .iter()
        .map(|lookup| lookup.writable_indexes.len() + lookup.readonly_indexes.len())
        .sum();
    transaction.message.static_account_keys().len() + loaded
}

/// Whether the failed transaction ended on the spike's `code`.
pub fn failed_with(
    result: Result<TransactionMetadata, FailedTransactionMetadata>,
    code: &str,
) -> bool {
    let failed = result.expect_err("the claim should fail");
    let wanted = format!("Error Code: {code}.");
    failed.meta.logs.iter().any(|line| line.contains(&wanted))
}

pub fn mainnet_rules() -> [MembershipRule; 3] {
    let token_metadata = |symbol: &[u8]| {
        let mut padded = [0; 10];
        padded[..symbol.len()].copy_from_slice(symbol);
        MembershipRule {
            kind: MembershipRule::TM_CREATOR,
            address: PIGGY_CREATOR.parse().unwrap(),
            update_authority: PIGGY_UPDATE_AUTHORITY.parse().unwrap(),
            symbol: padded,
        }
    };
    [
        token_metadata(b"PSG"),
        token_metadata(b"PGG"),
        MembershipRule {
            kind: MembershipRule::CORE_COLLECTION,
            address: PIGGY_GANG_COLLECTION.parse().unwrap(),
            ..MembershipRule::default()
        },
    ]
}

/// 32 reproducible bytes for the `index`-th key of `seed`: splitmix64.
fn key_bytes(seed: u64, index: u64) -> [u8; 32] {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(index);
    let mut bytes = [0; 32];
    for chunk in bytes.chunks_mut(8) {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        chunk.copy_from_slice(&(z ^ (z >> 31)).to_le_bytes());
    }
    bytes
}

/// Loads a `solana account --output json` fixture.
fn load_fixture(name: &str) -> Account {
    let path = format!("{FIXTURES}/{name}.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let account = &json["account"];
    Account {
        lamports: account["lamports"].as_u64().unwrap(),
        data: BASE64.decode(account["data"][0].as_str().unwrap()).unwrap(),
        owner: account["owner"].as_str().unwrap().parse().unwrap(),
        executable: false,
        rent_epoch: 0,
    }
}

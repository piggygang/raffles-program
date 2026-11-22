//! As much of a Squads v4 client as a vault transaction needs: a multisig, a
//! transaction for its vault, the members' approvals, and the execution. The
//! program is the one mainnet runs, from `tests/fixtures`, so what it accepts
//! here it accepts there.
//!
//! The instructions are built by hand. Squads' own crate is on an older
//! Solana SDK, and the workspace has one.

use {
    crate::harness::{custom_error, failed_program, Sent, Test, FIXTURES, LAMPORTS_PER_SOL},
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{
            instruction::{AccountMeta, Instruction},
            system_program,
        },
        AnchorSerialize,
    },
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    solana_account::Account,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

/// Squads v4, at the same address on every cluster.
pub const ID: Pubkey = Pubkey::from_str_const("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf");

/// Squads' `InvalidAccount`: an account isn't what the transaction's message
/// says, for example a signer that didn't sign.
pub const INVALID_ACCOUNT: u32 = 6014;
/// Squads' `InvalidProposalStatus`: the proposal isn't approved.
pub const INVALID_PROPOSAL_STATUS: u32 = 6008;

// Squads' seeds. Every address of the program starts with the first.
const SEED_PREFIX: &[u8] = b"multisig";
const SEED_PROGRAM_CONFIG: &[u8] = b"program_config";
const SEED_MULTISIG: &[u8] = b"multisig";
const SEED_VAULT: &[u8] = b"vault";
const SEED_TRANSACTION: &[u8] = b"transaction";
const SEED_PROPOSAL: &[u8] = b"proposal";

// Discriminators: the first 8 bytes of `sha256("global:<name>")`, as for any
// Anchor program.
const MULTISIG_CREATE_V2: [u8; 8] = [50, 221, 199, 93, 40, 245, 139, 233];
const VAULT_TRANSACTION_CREATE: [u8; 8] = [48, 250, 78, 168, 208, 226, 218, 211];
const PROPOSAL_CREATE: [u8; 8] = [220, 60, 73, 224, 30, 108, 79, 159];
const PROPOSAL_APPROVE: [u8; 8] = [144, 37, 164, 136, 188, 216, 42, 248];
const VAULT_TRANSACTION_EXECUTE: [u8; 8] = [194, 8, 161, 87, 153, 164, 25, 171];

/// Initiate, vote and execute: a member's whole say.
const ALL_PERMISSIONS: u8 = 0b111;
/// Two of the three members approve.
const THRESHOLD: u16 = 2;

fn address(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &ID).0
}

/// A Squads instruction: its discriminator, then its arguments in Borsh.
fn instruction(
    discriminator: [u8; 8],
    arguments: &impl AnchorSerialize,
    accounts: Vec<AccountMeta>,
) -> Instruction {
    let mut data = discriminator.to_vec();
    arguments.serialize(&mut data).unwrap();
    Instruction::new_with_bytes(ID, &data, accounts)
}

/// The treasury that Squads' program config names: `data` is the config's,
/// which holds the config's authority, the creation fee and the treasury
/// after its discriminator.
fn treasury(data: &[u8]) -> Pubkey {
    Pubkey::try_from(&data[48..80]).unwrap()
}

/// Loads the Squads program and its program config, which
/// `multisig_create_v2` reads the creation fee and the treasury from. Returns
/// the treasury.
fn install(test: &mut Test) -> Pubkey {
    test.svm
        .add_program_from_file(ID, format!("{FIXTURES}/squads_multisig.so"))
        .unwrap_or_else(|err| panic!("loading fixture squads_multisig.so: {err}"));

    let path = format!("{FIXTURES}/squads_program_config.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let config: Pubkey = json["pubkey"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        config,
        address(&[SEED_PREFIX, SEED_PROGRAM_CONFIG]),
        "{path}"
    );
    let account = &json["account"];
    let data = BASE64.decode(account["data"][0].as_str().unwrap()).unwrap();
    let treasury = treasury(&data);
    test.svm
        .set_account(
            config,
            Account {
                lamports: account["lamports"].as_u64().unwrap(),
                data,
                owner: account["owner"].as_str().unwrap().parse().unwrap(),
                executable: account["executable"].as_bool().unwrap(),
                rent_epoch: account["rentEpoch"].as_u64().unwrap(),
            },
        )
        .unwrap();
    // The creation fee is a transfer. An address without an account can't
    // take one that is smaller than the rent, so the treasury gets an
    // account.
    test.svm.airdrop(&treasury, LAMPORTS_PER_SOL).unwrap();
    treasury
}

/// A vault transaction's message, in Squads' own compact form, and its
/// account keys as `vault_transaction_execute` takes them: in the message's
/// order, each with whether the message writes it.
///
/// The keys come as in a Solana transaction: the vault first, which signs and
/// is written like a fee payer, then any other signer, then what is written,
/// then what is only read, the programs among it.
fn compile(vault: Pubkey, instructions: &[Instruction]) -> (Vec<u8>, Vec<AccountMeta>) {
    let metas: Vec<AccountMeta> = instructions
        .iter()
        .flat_map(|instruction| {
            instruction
                .accounts
                .iter()
                .cloned()
                .chain([AccountMeta::new_readonly(instruction.program_id, false)])
        })
        .filter(|meta| meta.pubkey != vault)
        .collect();
    let mut keys = vec![vault];
    let mut add = |wanted: &dyn Fn(&AccountMeta) -> bool| {
        for meta in metas.iter().filter(|meta| wanted(meta)) {
            if !keys.contains(&meta.pubkey) {
                keys.push(meta.pubkey);
            }
        }
        keys.len()
    };
    let signers = add(&|meta| meta.is_signer);
    let written = add(&|meta| meta.is_writable) - signers;
    add(&|_| true);
    let index =
        |key: &Pubkey| u8::try_from(keys.iter().position(|each| each == key).unwrap()).unwrap();

    // How many keys sign, how many of those are written, and how many of the
    // rest are.
    let mut message = vec![
        u8::try_from(signers).unwrap(),
        1,
        u8::try_from(written).unwrap(),
    ];
    message.push(u8::try_from(keys.len()).unwrap());
    for key in &keys {
        message.extend_from_slice(key.as_ref());
    }
    message.push(u8::try_from(instructions.len()).unwrap());
    for instruction in instructions {
        message.push(index(&instruction.program_id));
        message.push(u8::try_from(instruction.accounts.len()).unwrap());
        message.extend(instruction.accounts.iter().map(|meta| index(&meta.pubkey)));
        message.extend_from_slice(&u16::try_from(instruction.data.len()).unwrap().to_le_bytes());
        message.extend_from_slice(&instruction.data);
    }
    // No address lookup tables.
    message.push(0);

    let accounts = keys
        .iter()
        .enumerate()
        .map(|(position, &pubkey)| AccountMeta {
            pubkey,
            // Nothing signs here. The vault signs inside Squads, and a
            // message that names another signer is one Squads has to refuse.
            is_signer: false,
            is_writable: position == 0 || (signers..signers + written).contains(&position),
        })
        .collect();
    (message, accounts)
}

/// A Squads multisig set up as registries §6 has it: two of three members,
/// no time lock, and no config authority.
pub struct Vault {
    /// The vault: the multisig's address at index 0, and the only one of its
    /// addresses that ever signs.
    pub address: Pubkey,
    /// The multisig account, which holds the members and the threshold.
    /// Nothing signs for it.
    pub multisig: Pubkey,
    members: [Keypair; 3],
}

impl Vault {
    /// A multisig whose vault has a balance of its own: it pays the rent of
    /// what it creates.
    pub fn new(test: &mut Test) -> Self {
        let program_config = address(&[SEED_PREFIX, SEED_PROGRAM_CONFIG]);
        let treasury = match test.svm.get_account(&program_config) {
            Some(account) => treasury(&account.data),
            None => install(test),
        };
        let create_key = Keypair::new();
        let members = [Keypair::new(), Keypair::new(), Keypair::new()];
        let multisig = address(&[SEED_PREFIX, SEED_MULTISIG, create_key.pubkey().as_ref()]);

        // `MultisigCreateArgsV2`: no config authority, the threshold, the
        // members with their permissions, no time lock, no rent collector
        // and no memo.
        let arguments = (
            None::<Pubkey>,
            THRESHOLD,
            members
                .iter()
                .map(|member| (member.pubkey(), ALL_PERMISSIONS))
                .collect::<Vec<_>>(),
            0_u32,
            None::<Pubkey>,
            None::<String>,
        );
        let create = instruction(
            MULTISIG_CREATE_V2,
            &arguments,
            vec![
                AccountMeta::new_readonly(program_config, false),
                AccountMeta::new(treasury, false),
                AccountMeta::new(multisig, false),
                AccountMeta::new_readonly(create_key.pubkey(), true),
                AccountMeta::new(test.payer.pubkey(), true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
        );
        test.send(&[create], &[&create_key]);

        // The multisig is what was asked for: after its create key, no config
        // authority, then the threshold and the time lock.
        let account = test.svm.get_account(&multisig).unwrap();
        assert_eq!(account.owner, ID);
        assert_eq!(account.data[40..72], [0; 32]);
        assert_eq!(account.data[72..74], THRESHOLD.to_le_bytes());
        assert_eq!(account.data[74..78], [0; 4]);

        let vault = address(&[SEED_PREFIX, multisig.as_ref(), SEED_VAULT, &[0]]);
        test.svm.airdrop(&vault, LAMPORTS_PER_SOL).unwrap();
        Self {
            address: vault,
            multisig,
            members,
        }
    }

    /// Creates a vault transaction for `instruction` and a proposal for it,
    /// with the first member's approval, and returns the proposal with the
    /// instruction that executes it.
    fn propose(&self, test: &mut Test, instruction: Instruction) -> (Pubkey, Instruction) {
        let [first, second, _] = &self.members;
        let payer = test.payer.pubkey();
        // The multisig counts its transactions: the count follows its time
        // lock.
        let account = test.svm.get_account(&self.multisig).unwrap();
        let index = u64::from_le_bytes(account.data[78..86].try_into().unwrap()) + 1;
        let transaction = address(&[
            SEED_PREFIX,
            self.multisig.as_ref(),
            SEED_TRANSACTION,
            &index.to_le_bytes(),
        ]);
        let proposal = address(&[
            SEED_PREFIX,
            self.multisig.as_ref(),
            SEED_TRANSACTION,
            &index.to_le_bytes(),
            SEED_PROPOSAL,
        ]);
        let (message, accounts) = compile(self.address, &[instruction]);

        let create = self::instruction(
            VAULT_TRANSACTION_CREATE,
            // The vault's index, no ephemeral signers, the message, no memo.
            &(0_u8, 0_u8, message, None::<String>),
            vec![
                AccountMeta::new(self.multisig, false),
                AccountMeta::new(transaction, false),
                AccountMeta::new_readonly(first.pubkey(), true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
        );
        let propose = self::instruction(
            PROPOSAL_CREATE,
            // Not a draft: open for votes at once.
            &(index, false),
            vec![
                AccountMeta::new_readonly(self.multisig, false),
                AccountMeta::new(proposal, false),
                AccountMeta::new_readonly(first.pubkey(), true),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
        );
        test.send(&[create, propose, self.approve(first, proposal)], &[first]);

        // The second member executes: the members come before the
        // transaction's own accounts.
        let execute = Instruction::new_with_bytes(
            ID,
            &VAULT_TRANSACTION_EXECUTE,
            [
                vec![
                    AccountMeta::new_readonly(self.multisig, false),
                    AccountMeta::new(proposal, false),
                    AccountMeta::new_readonly(transaction, false),
                    AccountMeta::new_readonly(second.pubkey(), true),
                ],
                accounts,
            ]
            .concat(),
        );
        (proposal, execute)
    }

    fn approve(&self, member: &Keypair, proposal: Pubkey) -> Instruction {
        instruction(
            PROPOSAL_APPROVE,
            // No memo.
            &None::<String>,
            vec![
                AccountMeta::new_readonly(self.multisig, false),
                AccountMeta::new(member.pubkey(), true),
                AccountMeta::new(proposal, false),
            ],
        )
    }

    /// The transaction in which the second member approves `instruction` and
    /// executes it: with that the threshold is met.
    fn approved(&self, test: &mut Test, instruction: Instruction) -> [Instruction; 2] {
        let (proposal, execute) = self.propose(test, instruction);
        [self.approve(&self.members[1], proposal), execute]
    }

    /// Runs `instruction` as the vault: a vault transaction that two members
    /// approve and one executes. Returns the execution.
    pub fn run(&self, test: &mut Test, instruction: Instruction) -> Sent {
        let instructions = self.approved(test, instruction);
        test.send(&instructions, &[&self.members[1]])
    }

    /// `run` for an instruction that the raffles program has to refuse, with
    /// the number of its error. Squads has its own errors in the same range,
    /// so the failure has to start in the raffles program.
    pub fn run_err(&self, test: &mut Test, instruction: Instruction) -> u32 {
        let instructions = self.approved(test, instruction);
        let failed = test
            .try_send(&instructions, &[&self.members[1]])
            .unwrap_err();
        assert_eq!(
            failed_program(&failed),
            raffles::ID,
            "{}",
            failed.meta.pretty_logs()
        );
        custom_error(&failed)
    }

    /// `run` for a vault transaction that Squads itself has to refuse to
    /// execute, with the number of Squads' error. The raffles program is
    /// never called.
    pub fn refused(&self, test: &mut Test, instruction: Instruction) -> u32 {
        let instructions = self.approved(test, instruction);
        self.refusal(test, &instructions)
    }

    /// The same with one approval only, which is below the threshold.
    pub fn refused_below_the_threshold(&self, test: &mut Test, instruction: Instruction) -> u32 {
        let (_, execute) = self.propose(test, instruction);
        self.refusal(test, &[execute])
    }

    fn refusal(&self, test: &mut Test, instructions: &[Instruction]) -> u32 {
        let failed = test
            .try_send(instructions, &[&self.members[1]])
            .unwrap_err();
        assert_eq!(failed_program(&failed), ID, "{}", failed.meta.pretty_logs());
        custom_error(&failed)
    }
}

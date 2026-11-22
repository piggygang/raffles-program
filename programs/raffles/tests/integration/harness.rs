use {
    anchor_lang::{
        prelude::{ProgramData, Pubkey},
        solana_program::{
            instruction::{error::InstructionError, Instruction},
            system_program,
        },
        AccountDeserialize, AnchorDeserialize, Discriminator, Event, InstructionData,
        ToAccountMetas,
    },
    anchor_spl::metadata::mpl_token_metadata,
    litesvm::{
        types::{FailedTransactionMetadata, TransactionMetadata},
        LiteSVM,
    },
    raffles::{
        constants::PROGRAM_DATA,
        state::{Config, ConfigSettings, ConfigUpdate, MembershipRule, RuleKind},
    },
    raffles_client::{pda, EVENT_TAG},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::{versioned::VersionedTransaction, TransactionError},
};

/// Written by `anchor build`; the tests never build the program themselves.
const RAFFLES_SO: &str = concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/raffles.so");
/// Written by `scripts/dump-fixtures.sh`.
pub const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

/// The upgradeable loader's ProgramData account starts with this many bytes:
/// the variant, the slot of the last deploy and the upgrade authority. The
/// program's bytes follow.
const PROGRAM_DATA_HEADER: usize = 45;

/// A fresh LiteSVM with the raffles program at its declared address, the
/// Metaplex programs, a funded payer, and a deployer that holds the program's
/// upgrade authority.
pub struct Test {
    pub svm: LiteSVM,
    /// Pays every transaction's fee, and signs each one.
    pub payer: Keypair,
    /// The program's upgrade authority, as a deploy leaves it: the one key
    /// `init_config` takes.
    pub deployer: Keypair,
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

        let (payer, deployer) = (Keypair::new(), Keypair::new());
        for key in [&payer, &deployer] {
            svm.airdrop(&key.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
        }
        let mut test = Self {
            svm,
            payer,
            deployer,
        };
        test.set_upgrade_authority(Some(test.deployer.pubkey()));
        test
    }

    /// Makes `authority` the program's upgrade authority, or the program
    /// immutable with `None`. LiteSVM loads a program under the upgradeable
    /// loader without an authority; a deploy leaves the deployer's.
    pub fn set_upgrade_authority(&mut self, authority: Option<Pubkey>) {
        let mut account = self.svm.get_account(&PROGRAM_DATA).unwrap();
        // `UpgradeableLoaderState::ProgramData`, as bincode writes it: the
        // variant's index, the slot, then the authority as an option.
        let mut header = [0; PROGRAM_DATA_HEADER];
        header[..4].copy_from_slice(&3_u32.to_le_bytes());
        header[4..12].copy_from_slice(&account.data[4..12]);
        if let Some(authority) = authority {
            header[12] = 1;
            header[13..].copy_from_slice(authority.as_ref());
        }
        account.data[..PROGRAM_DATA_HEADER].copy_from_slice(&header);
        self.svm.set_account(PROGRAM_DATA, account).unwrap();

        // The program reads the account the way this does.
        let account = self.svm.get_account(&PROGRAM_DATA).unwrap();
        let program_data = ProgramData::try_deserialize(&mut account.data.as_slice()).unwrap();
        assert_eq!(program_data.upgrade_authority_address, authority);
    }

    /// Sends `instructions` paid for by the payer and also signed by
    /// `signers`. Panics with the program logs if the transaction fails.
    pub fn send(&mut self, instructions: &[Instruction], signers: &[&Keypair]) -> Sent {
        self.try_send(instructions, signers)
            .unwrap_or_else(|failed| panic!("{}\n{}", failed.err, failed.meta.pretty_logs()))
    }

    /// `send`, for a transaction that may fail: the error comes back with the
    /// logs. It is boxed, because LiteSVM's is a few hundred bytes.
    pub fn try_send(
        &mut self,
        instructions: &[Instruction],
        signers: &[&Keypair],
    ) -> Result<Sent, Box<FailedTransactionMetadata>> {
        // A fresh blockhash, so that a test can send the same transaction
        // twice: the second would otherwise be the first one again.
        self.svm.expire_blockhash();
        let message = Message::new_with_blockhash(
            instructions,
            Some(&self.payer.pubkey()),
            &self.svm.latest_blockhash(),
        );
        let account_keys = message.account_keys.clone();
        let signers: Vec<&Keypair> = std::iter::once(&self.payer)
            .chain(signers.iter().copied())
            .collect();
        let transaction =
            VersionedTransaction::try_new(VersionedMessage::Legacy(message), &signers).unwrap();
        match self.svm.send_transaction(transaction) {
            Ok(meta) => Ok(Sent { meta, account_keys }),
            Err(failed) => Err(Box::new(failed)),
        }
    }

    /// `try_send` for a transaction that has to fail, with the program error
    /// it failed with: the number of a `RaffleError` or of one of Anchor's.
    pub fn send_err(&mut self, instructions: &[Instruction], signers: &[&Keypair]) -> u32 {
        match self.try_send(instructions, signers) {
            Ok(sent) => panic!("the transaction went through:\n{}", sent.meta.pretty_logs()),
            Err(failed) => custom_error(&failed),
        }
    }

    /// `Config`, or `None` before `init_config`: its address may hold
    /// lamports that someone sent there, and no config yet.
    pub fn try_config(&self) -> Option<Config> {
        let account = self.svm.get_account(&pda::config().0)?;
        (account.owner == raffles::ID)
            .then(|| Config::try_deserialize(&mut account.data.as_slice()).unwrap())
    }

    pub fn config(&self) -> Config {
        self.try_config().expect("no Config: init_config first")
    }

    pub fn lamports(&self, address: &Pubkey) -> u64 {
        self.svm
            .get_account(address)
            .map_or(0, |account| account.lamports)
    }

    /// `init_config` with `settings`, signed by the deployer.
    pub fn init_config(&mut self, settings: ConfigSettings) -> Sent {
        let deployer = self.deployer.insecure_clone();
        self.send(&[init_config(deployer.pubkey(), settings)], &[&deployer])
    }
}

/// A transaction that went through.
#[derive(Debug)]
pub struct Sent {
    pub meta: TransactionMetadata,
    /// The transaction's accounts, which its inner instructions name by
    /// index.
    account_keys: Vec<Pubkey>,
}

/// An event as a reader finds it (docs/events.md, section 1): an inner
/// instruction of the raffles program whose data starts with the event tag,
/// and whose one account is the event authority.
pub struct Emitted {
    /// 2 under an instruction a wallet sent, 3 under one a Squads vault ran.
    pub stack_height: u8,
    /// The event: its discriminator, then its fields.
    pub data: Vec<u8>,
}

impl Sent {
    /// Every event the transaction emitted, in order.
    pub fn events(&self) -> Vec<Emitted> {
        let (event_authority, _) = pda::event_authority();
        self.meta
            .inner_instructions
            .iter()
            .flatten()
            .filter_map(|inner| {
                let instruction = &inner.instruction;
                let program = self.account_keys[usize::from(instruction.program_id_index)];
                let data = instruction.data.strip_prefix(EVENT_TAG)?;
                if program != raffles::ID {
                    return None;
                }
                let accounts: Vec<Pubkey> = instruction
                    .accounts
                    .iter()
                    .map(|&index| self.account_keys[usize::from(index)])
                    .collect();
                assert_eq!(accounts, [event_authority], "an event's accounts");
                Some(Emitted {
                    stack_height: inner.stack_height,
                    data: data.to_vec(),
                })
            })
            .collect()
    }

    /// The transaction's one event, which has to be an `E`: an instruction
    /// emits exactly one.
    pub fn event<E: Event + Discriminator + AnchorDeserialize>(&self) -> E {
        let events = self.events();
        let [event] = events.as_slice() else {
            panic!(
                "{} events, not one:\n{}",
                events.len(),
                self.meta.pretty_logs()
            );
        };
        decode_event(&event.data)
    }
}

/// `data` as the event `E`: its discriminator, then exactly its fields.
pub fn decode_event<E: Event + Discriminator + AnchorDeserialize>(data: &[u8]) -> E {
    let fields = data
        .strip_prefix(E::DISCRIMINATOR)
        .unwrap_or_else(|| panic!("not a {}", std::any::type_name::<E>()));
    E::try_from_slice(fields).unwrap()
}

/// The custom error of a failed transaction's instruction.
pub fn custom_error(failed: &FailedTransactionMetadata) -> u32 {
    match &failed.err {
        TransactionError::InstructionError(_, InstructionError::Custom(code)) => *code,
        other => panic!(
            "no program error, but {other:?}\n{}",
            failed.meta.pretty_logs()
        ),
    }
}

/// The program a failed transaction failed in. An error passes up through
/// every program that called the one it started in, and each logs it on the
/// way: the first to log is where it started. Programs number their errors
/// from the same 6000, so the number alone doesn't say whose it is.
pub fn failed_program(failed: &FailedTransactionMetadata) -> Pubkey {
    failed
        .meta
        .logs
        .iter()
        .find_map(|line| {
            let (program, _) = line.strip_prefix("Program ")?.split_once(" failed: ")?;
            program.parse().ok()
        })
        .unwrap_or_else(|| {
            panic!(
                "no program logged a failure:\n{}",
                failed.meta.pretty_logs()
            )
        })
}

/// Settings like mainnet's (holder-proof §5), with rule addresses that differ
/// by `seed`: two symbols under one creator and update authority, a Core
/// collection, and the spec's cap, bonus and bounds (D1, D2, D4).
pub fn settings(seed: u8) -> ConfigSettings {
    let key = |byte: u8| Pubkey::new_from_array([seed.wrapping_add(byte); 32]);
    let tm_rule = |symbol: &[u8; 3]| {
        let mut padded = [0; MembershipRule::SYMBOL_LEN];
        padded[..3].copy_from_slice(symbol);
        MembershipRule {
            kind: RuleKind::TmCreator.into(),
            address: key(1),
            update_authority: key(2),
            symbol: padded,
        }
    };
    ConfigSettings {
        paused: false,
        membership_rules: [
            tm_rule(b"PSG"),
            tm_rule(b"PGG"),
            MembershipRule {
                kind: RuleKind::CoreCollection.into(),
                address: key(3),
                ..MembershipRule::default()
            },
        ],
        entry_cap: 10,
        full_gang_bonus: 3,
        min_duration: 3_600,
        max_duration: 2_592_000,
    }
}

// The instructions, as a client builds them. Every one that emits takes the
// event authority and the program last (docs/events.md, section 1).

pub fn init_config(authority: Pubkey, settings: ConfigSettings) -> Instruction {
    Instruction::new_with_bytes(
        raffles::ID,
        &raffles::instruction::InitConfig { settings }.data(),
        raffles::accounts::InitConfig {
            authority,
            config: pda::config().0,
            program_data: pda::program_data().0,
            system_program: system_program::ID,
            event_authority: pda::event_authority().0,
            program: raffles::ID,
        }
        .to_account_metas(None),
    )
}

pub fn update_config(authority: Pubkey, update: ConfigUpdate) -> Instruction {
    Instruction::new_with_bytes(
        raffles::ID,
        &raffles::instruction::UpdateConfig { update }.data(),
        raffles::accounts::UpdateConfig {
            authority,
            config: pda::config().0,
            event_authority: pda::event_authority().0,
            program: raffles::ID,
        }
        .to_account_metas(None),
    )
}

pub fn set_authority(authority: Pubkey, new_authority: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        raffles::ID,
        &raffles::instruction::SetAuthority { new_authority }.data(),
        raffles::accounts::SetAuthority {
            authority,
            config: pda::config().0,
            event_authority: pda::event_authority().0,
            program: raffles::ID,
        }
        .to_account_metas(None),
    )
}

pub fn accept_authority(new_authority: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        raffles::ID,
        &raffles::instruction::AcceptAuthority {}.data(),
        raffles::accounts::AcceptAuthority {
            new_authority,
            config: pda::config().0,
            event_authority: pda::event_authority().0,
            program: raffles::ID,
        }
        .to_account_metas(None),
    )
}

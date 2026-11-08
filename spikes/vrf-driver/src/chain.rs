//! The RPC side: reading the accounts the spike uses, and landing transactions
//! the way the crank will. Simulate to size the compute budget, send, poll the
//! signature, and re-sign with a fresh blockhash when one expires.

use {
    anchor_lang::solana_program::instruction::Instruction,
    anchor_lang::{prelude::Pubkey, AccountDeserialize},
    anyhow::{anyhow, bail, Context, Result},
    orao_solana_vrf::state::{NetworkState, RandomnessV2},
    solana_commitment_config::CommitmentConfig,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_keypair::{read_keypair_file, Keypair},
    solana_message::{Message, VersionedMessage},
    solana_rpc_client::nonblocking::rpc_client::RpcClient,
    solana_rpc_client_api::config::{
        RpcSendTransactionConfig, RpcSimulateTransactionConfig, RpcTransactionConfig,
    },
    solana_signature::Signature,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    solana_transaction_status_client_types::UiTransactionEncoding,
    std::time::{Duration, Instant},
    vrf_spike::state::Draw,
};

const DEVNET_GENESIS_HASH: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const MAINNET_GENESIS_HASH: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";
/// Signature and account polling interval: about a slot.
pub const POLL: Duration = Duration::from_millis(400);
const SEND_ATTEMPTS: u32 = 3;
const MAX_COMPUTE_UNITS: u32 = 1_400_000;

pub struct Chain {
    pub rpc: RpcClient,
    /// `None` for the read-only commands that need no wallet.
    payer: Option<Keypair>,
    /// Micro-lamports per compute unit; 0 sends no price instruction.
    pub priority_fee: u64,
}

/// A transaction that landed, at `confirmed`.
pub struct Landed {
    pub signature: Signature,
    pub slot: u64,
    /// Blockhashes it took; more than 1 means one expired unconfirmed.
    pub attempts: u32,
    pub confirmed_at: Instant,
    pub compute_units: Option<u64>,
    pub fee: Option<u64>,
}

impl Chain {
    pub fn new(url: &str, keypair: Option<&str>, priority_fee: u64) -> Result<Self> {
        let payer = keypair
            .map(|keypair| {
                let path = match keypair.strip_prefix("~/") {
                    Some(rest) => format!("{}/{rest}", std::env::var("HOME")?),
                    None => keypair.to_owned(),
                };
                read_keypair_file(&path).map_err(|err| anyhow!("reading {path}: {err}"))
            })
            .transpose()?;
        Ok(Self {
            rpc: RpcClient::new_with_commitment(url.to_owned(), CommitmentConfig::confirmed()),
            payer,
            priority_fee,
        })
    }

    pub fn payer(&self) -> &Keypair {
        self.payer
            .as_ref()
            .unwrap_or_else(|| unreachable!("commands that sign load the keypair"))
    }

    pub async fn cluster(&self) -> Result<&'static str> {
        Ok(
            match self.rpc.get_genesis_hash().await?.to_string().as_str() {
                DEVNET_GENESIS_HASH => "devnet",
                MAINNET_GENESIS_HASH => "mainnet-beta",
                _ => "unknown",
            },
        )
    }

    pub async fn require_devnet(&self) -> Result<()> {
        match self.cluster().await? {
            "devnet" => Ok(()),
            other => bail!("the RPC is {other}; this command only runs on devnet"),
        }
    }

    pub async fn account(&self, address: &Pubkey) -> Result<Option<solana_account::Account>> {
        Ok(self
            .rpc
            .get_account_with_commitment(address, CommitmentConfig::confirmed())
            .await?
            .value)
    }

    pub async fn lamports(&self, address: &Pubkey) -> Result<u64> {
        Ok(self
            .account(address)
            .await?
            .map_or(0, |account| account.lamports))
    }

    pub async fn draw(&self, address: &Pubkey) -> Result<Option<Draw>> {
        self.decode(address).await
    }

    pub async fn request(&self, address: &Pubkey) -> Result<Option<RandomnessV2>> {
        self.decode(address).await
    }

    pub async fn network_state(&self) -> Result<NetworkState> {
        self.decode(&vrf_spike::client::network_state())
            .await?
            .context("ORAO's network state doesn't exist on this cluster")
    }

    async fn decode<T: AccountDeserialize>(&self, address: &Pubkey) -> Result<Option<T>> {
        self.account(address)
            .await?
            .map(|account| T::try_deserialize(&mut account.data.as_slice()))
            .transpose()
            .with_context(|| format!("decoding {address}"))
    }

    /// Lands `instructions`, paid and signed by the payer alone.
    pub async fn send(&self, instructions: &[Instruction]) -> Result<Landed> {
        let units = self.simulate(instructions).await?;
        let limit = u32::try_from(units + units / 5 + 1_000).unwrap_or(MAX_COMPUTE_UNITS);
        let mut all = vec![ComputeBudgetInstruction::set_compute_unit_limit(
            limit.min(MAX_COMPUTE_UNITS),
        )];
        if self.priority_fee > 0 {
            all.push(ComputeBudgetInstruction::set_compute_unit_price(
                self.priority_fee,
            ));
        }
        all.extend_from_slice(instructions);

        for attempt in 1..=SEND_ATTEMPTS {
            let (blockhash, last_valid_block_height) = self
                .rpc
                .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
                .await?;
            let message =
                Message::new_with_blockhash(&all, Some(&self.payer().pubkey()), &blockhash);
            let transaction =
                VersionedTransaction::try_new(VersionedMessage::Legacy(message), &[self.payer()])?;
            let signature = self
                .rpc
                .send_transaction_with_config(
                    &transaction,
                    RpcSendTransactionConfig {
                        skip_preflight: true,
                        ..RpcSendTransactionConfig::default()
                    },
                )
                .await?;
            // An RPC error here is transient (a rate limit, a timeout): poll
            // again, until the blockhash has expired. A blockhash lives about
            // a minute; the deadline covers an RPC that stays down.
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                tokio::time::sleep(POLL).await;
                let status = match self.rpc.get_signature_statuses(&[signature]).await {
                    Ok(response) => response.value.into_iter().flatten().next(),
                    Err(_) => None,
                };
                if let Some(status) = status {
                    if let Some(err) = status.err {
                        bail!("{signature} failed: {err}");
                    }
                    if status.satisfies_commitment(CommitmentConfig::confirmed()) {
                        let confirmed_at = Instant::now();
                        let (compute_units, fee) = self.meta(&signature).await;
                        return Ok(Landed {
                            signature,
                            slot: status.slot,
                            attempts: attempt,
                            confirmed_at,
                            compute_units,
                            fee,
                        });
                    }
                }
                let expired = self
                    .rpc
                    .get_block_height()
                    .await
                    .is_ok_and(|height| height > last_valid_block_height);
                if expired || Instant::now() > deadline {
                    break;
                }
            }
        }
        bail!("not confirmed with {SEND_ATTEMPTS} blockhashes")
    }

    /// Compute units the instructions use, with the maximum budget.
    async fn simulate(&self, instructions: &[Instruction]) -> Result<u64> {
        let mut all = vec![ComputeBudgetInstruction::set_compute_unit_limit(
            MAX_COMPUTE_UNITS,
        )];
        all.extend_from_slice(instructions);
        let message = Message::new(&all, Some(&self.payer().pubkey()));
        let transaction =
            VersionedTransaction::try_new(VersionedMessage::Legacy(message), &[self.payer()])?;
        let result = self
            .rpc
            .simulate_transaction_with_config(
                &transaction,
                RpcSimulateTransactionConfig {
                    sig_verify: false,
                    replace_recent_blockhash: true,
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..RpcSimulateTransactionConfig::default()
                },
            )
            .await?
            .value;
        if let Some(err) = result.err {
            bail!(
                "simulation failed: {err}\n{}",
                result.logs.unwrap_or_default().join("\n")
            );
        }
        result
            .units_consumed
            .context("the simulation reported no compute units")
    }

    /// The landed transaction's compute units and fee. The RPC can lag the
    /// confirmation by a moment, so it retries briefly.
    async fn meta(&self, signature: &Signature) -> (Option<u64>, Option<u64>) {
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };
        for _ in 0..5 {
            if let Ok(transaction) = self
                .rpc
                .get_transaction_with_config(signature, config)
                .await
            {
                if let Some(meta) = transaction.transaction.meta {
                    return (meta.compute_units_consumed.into(), Some(meta.fee));
                }
            }
            tokio::time::sleep(POLL).await;
        }
        (None, None)
    }
}

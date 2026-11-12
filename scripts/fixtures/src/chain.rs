//! The RPC side: reading accounts in bulk, and landing transactions the way
//! spikes/vrf-driver does. That means simulating to size the compute budget,
//! sending, polling the signature, and re-signing with a fresh blockhash when
//! one expires.

use {
    crate::Cluster,
    anyhow::{anyhow, bail, Context, Result},
    solana_account::Account,
    solana_commitment_config::CommitmentConfig,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_instruction::Instruction,
    solana_keypair::{read_keypair_file, Keypair},
    solana_message::{Message, VersionedMessage},
    solana_pubkey::Pubkey,
    solana_rpc_client::nonblocking::rpc_client::RpcClient,
    solana_rpc_client_api::config::{RpcSendTransactionConfig, RpcSimulateTransactionConfig},
    solana_signature::Signature,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    std::{
        collections::HashMap,
        path::Path,
        time::{Duration, Instant},
    },
};

const DEVNET_GENESIS_HASH: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const TESTNET_GENESIS_HASH: &str = "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY";
const MAINNET_GENESIS_HASH: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";
/// Signature polling interval: about a slot.
const POLL: Duration = Duration::from_millis(400);
const SEND_ATTEMPTS: u32 = 3;
const MAX_COMPUTE_UNITS: u32 = 1_400_000;
/// `getMultipleAccounts` takes at most 100 addresses.
const ACCOUNTS_PER_CALL: usize = 100;

pub struct Chain {
    pub rpc: RpcClient,
    /// `None` for the commands that only read.
    payer: Option<Keypair>,
}

impl Chain {
    pub fn new(url: &str, payer: Option<&Path>) -> Result<Self> {
        let payer = payer
            .map(|path| {
                read_keypair_file(path).map_err(|err| anyhow!("reading {}: {err}", path.display()))
            })
            .transpose()?;
        Ok(Self {
            rpc: RpcClient::new_with_commitment(url.to_owned(), CommitmentConfig::confirmed()),
            payer,
        })
    }

    pub fn payer(&self) -> &Keypair {
        self.payer
            .as_ref()
            .unwrap_or_else(|| unreachable!("commands that sign load the payer"))
    }

    /// Refuses an RPC that isn't the cluster the command names, by its genesis
    /// hash: `--cluster localnet` against devnet would scatter localnet's
    /// public keys there.
    pub async fn require(&self, cluster: Cluster) -> Result<()> {
        let genesis = self.rpc.get_genesis_hash().await?.to_string();
        let ok = match cluster {
            Cluster::Devnet => genesis == DEVNET_GENESIS_HASH,
            Cluster::Localnet => ![
                DEVNET_GENESIS_HASH,
                TESTNET_GENESIS_HASH,
                MAINNET_GENESIS_HASH,
            ]
            .contains(&genesis.as_str()),
        };
        if !ok {
            bail!(
                "the RPC's genesis hash is {genesis}, which isn't {}",
                cluster.name()
            );
        }
        Ok(())
    }

    /// The accounts that exist among `addresses`.
    pub async fn accounts(&self, addresses: &[Pubkey]) -> Result<HashMap<Pubkey, Account>> {
        let mut found = HashMap::new();
        for chunk in addresses.chunks(ACCOUNTS_PER_CALL) {
            let accounts = self
                .rpc
                .get_multiple_accounts_with_commitment(chunk, CommitmentConfig::confirmed())
                .await?
                .value;
            found.extend(
                chunk
                    .iter()
                    .zip(accounts)
                    .filter_map(|(address, account)| Some((*address, account?))),
            );
        }
        Ok(found)
    }

    /// The account holding a single-supply token now, and its owner.
    pub async fn token_holder(&self, mint: &Pubkey) -> Result<Option<(Pubkey, Pubkey)>> {
        let largest = self.rpc.get_token_largest_accounts(mint).await?;
        let Some(held) = largest.iter().find(|account| account.amount.amount == "1") else {
            return Ok(None);
        };
        let address: Pubkey = held.address.parse()?;
        let account = self.rpc.get_account(&address).await?;
        let owner = account
            .data
            .get(32..64)
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            .context("a token account shorter than its owner field")?;
        Ok(Some((address, Pubkey::new_from_array(owner))))
    }

    /// Lands `instructions`, paid by the payer and signed by it and whichever
    /// of `signers` the instructions name.
    pub async fn send(
        &self,
        instructions: &[Instruction],
        signers: &[Keypair],
    ) -> Result<Signature> {
        let units = self.simulate(instructions, signers).await?;
        let limit = u32::try_from(units + units / 5 + 1_000).unwrap_or(MAX_COMPUTE_UNITS);
        let mut all = vec![ComputeBudgetInstruction::set_compute_unit_limit(
            limit.min(MAX_COMPUTE_UNITS),
        )];
        all.extend_from_slice(instructions);

        for _ in 0..SEND_ATTEMPTS {
            let (blockhash, last_valid_block_height) = self
                .rpc
                .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
                .await?;
            let message =
                Message::new_with_blockhash(&all, Some(&self.payer().pubkey()), &blockhash);
            let transaction = self.sign(message, signers)?;
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
            // Re-signing is safe only once the blockhash has expired with the
            // transaction nowhere; otherwise both could land. Nothing lands
            // after its blockhash expires, so a status read after the height
            // says expired is final.
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                tokio::time::sleep(POLL).await;
                let expired = self
                    .rpc
                    .get_block_height()
                    .await
                    .is_ok_and(|height| height > last_valid_block_height);
                // An RPC error here is transient (a rate limit, a timeout).
                if let Ok(response) = self.rpc.get_signature_statuses(&[signature]).await {
                    match response.value.into_iter().flatten().next() {
                        Some(status) => {
                            if let Some(err) = status.err {
                                bail!("{signature} failed: {err}");
                            }
                            if status.satisfies_commitment(CommitmentConfig::confirmed()) {
                                return Ok(signature);
                            }
                        }
                        None if expired => break,
                        None => {}
                    }
                }
                if Instant::now() > deadline {
                    bail!(
                        "{signature} is still unconfirmed, and its blockhash may not have \
                         expired. Rerun in a minute: apply skips whatever landed"
                    );
                }
            }
        }
        bail!("not confirmed with {SEND_ATTEMPTS} blockhashes")
    }

    /// Compute units the instructions use, with the maximum budget. A failure
    /// here stops the run with the program's logs.
    async fn simulate(&self, instructions: &[Instruction], signers: &[Keypair]) -> Result<u64> {
        let mut all = vec![ComputeBudgetInstruction::set_compute_unit_limit(
            MAX_COMPUTE_UNITS,
        )];
        all.extend_from_slice(instructions);
        let blockhash = self.rpc.get_latest_blockhash().await?;
        let message = Message::new_with_blockhash(&all, Some(&self.payer().pubkey()), &blockhash);
        let transaction = self.sign(message, signers)?;
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

    /// Signs with the payer and the `signers` the message asks for.
    fn sign(&self, message: Message, signers: &[Keypair]) -> Result<VersionedTransaction> {
        let required = &message.account_keys[..usize::from(message.header.num_required_signatures)];
        let mut keys: Vec<&Keypair> = vec![self.payer()];
        for key in required.iter().skip(1) {
            let signer = signers
                .iter()
                .find(|signer| signer.pubkey() == *key)
                .with_context(|| format!("no key to sign for {key}"))?;
            keys.push(signer);
        }
        Ok(VersionedTransaction::try_new(
            VersionedMessage::Legacy(message),
            &keys,
        )?)
    }
}

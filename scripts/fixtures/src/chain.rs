//! The RPC side: reading accounts in bulk, and landing transactions the way
//! spikes/vrf-driver does. That means simulating to size the compute budget,
//! sending, polling the signature, and re-signing with a fresh blockhash. This
//! one re-signs only once the finalized chain rules the transaction out.

use {
    crate::Cluster,
    anyhow::{anyhow, bail, Context, Result},
    serde_json::json,
    solana_account::Account,
    solana_commitment_config::CommitmentConfig,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_instruction::Instruction,
    solana_keypair::{read_keypair_file, Keypair},
    solana_message::{Message, VersionedMessage},
    solana_pubkey::Pubkey,
    solana_rpc_client::nonblocking::rpc_client::RpcClient,
    solana_rpc_client_api::{
        config::{
            RpcSendTransactionConfig, RpcSignaturesForAddressConfig, RpcSimulateTransactionConfig,
        },
        request::RpcRequest,
        response::RpcConfirmedTransactionStatusWithSignature,
    },
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
/// `getSignaturesForAddress` lists at most 1,000 signatures.
const SIGNATURES_PER_CALL: usize = 1_000;

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
            // For `never_landed`: what the payer holds before the send, and a
            // slot that is final by then.
            let balance = self.rpc.get_balance(&self.payer().pubkey()).await?;
            let sent_after = self
                .rpc
                .get_slot_with_commitment(CommitmentConfig::finalized())
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
            // Re-signing is safe only once the transaction can no longer land
            // and never did; otherwise both could land.
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                tokio::time::sleep(POLL).await;
                // An RPC error here is transient (a rate limit, a timeout).
                if let Ok(response) = self.rpc.get_signature_statuses(&[signature]).await {
                    if let Some(status) = response.value.into_iter().flatten().next() {
                        if let Some(err) = status.err {
                            bail!("{signature} failed: {err}");
                        }
                        if status.satisfies_commitment(CommitmentConfig::confirmed()) {
                            return Ok(signature);
                        }
                    }
                }
                if self
                    .never_landed(&signature, last_valid_block_height, balance, sent_after)
                    .await
                {
                    break;
                }
                if Instant::now() > deadline {
                    bail!(
                        "{signature} is unresolved: it isn't confirmed, and the finalized chain \
                         doesn't rule it out. Rerun in a minute: apply skips whatever landed"
                    );
                }
            }
        }
        bail!("not confirmed with {SEND_ATTEMPTS} blockhashes")
    }

    /// Whether the transaction `signature`, whose blockhash is valid up to
    /// `last_valid_block_height`, can no longer land and never did. `balance`
    /// is what its payer held before it was sent, and `sent_after` a slot that
    /// was final by then.
    ///
    /// The finalized chain has to say three things. Whichever node answers,
    /// none is on a fork there, and one that lags is at an earlier slot.
    /// - It is past the blockhash's last block, so nothing lands any more.
    /// - From that slot on, the payer's signatures since `sent_after` don't
    ///   include this one: the transaction is nowhere on the chain.
    /// - From that slot on, the payer holds `balance`, although every
    ///   transaction costs its payer a fee. This is the weaker proof, since a
    ///   deposit or a rollback can leave the balance whole, but it doesn't
    ///   depend on the RPC's transaction history.
    ///
    /// A missing status proves less: a node on a fork without the transaction
    /// has none either.
    async fn never_landed(
        &self,
        signature: &Signature,
        last_valid_block_height: u64,
        balance: u64,
        sent_after: u64,
    ) -> bool {
        let (payer, finalized) = (self.payer().pubkey(), CommitmentConfig::finalized());
        // An RPC error in any of these calls is transient (a rate limit, a
        // timeout): the answer is "not yet".
        let Ok(expiry) = self.rpc.get_epoch_info_with_commitment(finalized).await else {
            return false;
        };
        if expiry.block_height <= last_valid_block_height {
            return false;
        }
        let unpaid = self
            .rpc
            .get_balance_with_commitment(&payer, finalized)
            .await
            .is_ok_and(|now| now.context.slot >= expiry.absolute_slot && now.value == balance);
        if !unpaid {
            return false;
        }
        let config = RpcSignaturesForAddressConfig {
            limit: Some(SIGNATURES_PER_CALL),
            commitment: Some(finalized),
            min_context_slot: Some(expiry.absolute_slot),
            ..RpcSignaturesForAddressConfig::default()
        };
        let Ok(history) = self
            .rpc
            .send::<Vec<RpcConfirmedTransactionStatusWithSignature>>(
                RpcRequest::GetSignaturesForAddress,
                json!([payer.to_string(), config]),
            )
            .await
        else {
            return false;
        };
        // Newest first. The list is whole if it ends, or reaches back to
        // `sent_after`: the transaction can only be in a later slot.
        let whole = history.len() < SIGNATURES_PER_CALL
            || history
                .last()
                .is_some_and(|oldest| oldest.slot <= sent_after);
        let signature = signature.to_string();
        whole && history.iter().all(|entry| entry.signature != signature)
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

#[cfg(test)]
mod tests {
    use {super::*, serde_json::Value, solana_rpc_client::mock_sender::MocksMap};

    /// The last block height at which the mock RPC's blockhashes are valid.
    const LAST_VALID: u64 = 100;
    /// The finalized slot at which the mock RPC is past it.
    const FINALIZED: u64 = 500;
    /// The finalized slot before the payer sends, and what it holds then.
    const SENT_AFTER: u64 = 1;
    const BALANCE: u64 = 1_000_000;

    /// A poll that finds no status, and what the finalized chain says then:
    /// its block height, the payer's balance at `slot`, and the payer's
    /// signatures.
    #[derive(Clone, Copy)]
    struct Unseen {
        block_height: u64,
        slot: u64,
        balance: u64,
        history: History,
    }

    /// The payer's finalized signatures, as far as one call lists them.
    #[derive(Clone, Copy)]
    enum History {
        /// None since `SENT_AFTER`.
        Empty,
        /// The transaction.
        Lists(Signature),
        /// A full page of others, all since `SENT_AFTER`: there may be more.
        Cut,
        /// A full page of others that reaches back to `SENT_AFTER`.
        Long,
    }

    fn blockhash(seed: u8) -> Value {
        json!({
            "context": { "slot": SENT_AFTER },
            "value": {
                "blockhash": Pubkey::new_from_array([seed; 32]).to_string(),
                "lastValidBlockHeight": LAST_VALID,
            },
        })
    }

    fn balance(slot: u64, lamports: u64) -> Value {
        json!({ "context": { "slot": slot }, "value": lamports })
    }

    fn statuses(status: Value) -> Value {
        json!({ "context": { "slot": FINALIZED }, "value": [status] })
    }

    fn history(history: History) -> Value {
        let entry = |signature: Signature, slot: u64| {
            json!({
                "signature": signature.to_string(),
                "slot": slot,
                "err": null,
                "memo": null,
                "blockTime": null,
                "confirmationStatus": "finalized",
            })
        };
        let other = Signature::default();
        let mut page = vec![entry(other, SENT_AFTER + 1); SIGNATURES_PER_CALL];
        match history {
            History::Empty => page.clear(),
            History::Lists(signature) => page = vec![entry(signature, SENT_AFTER + 1)],
            History::Cut => {}
            History::Long => page[SIGNATURES_PER_CALL - 1] = entry(other, SENT_AFTER),
        }
        json!(page)
    }

    /// Sends a transfer through a mock RPC whose polls find no status, as
    /// `unseen` lists, and then the transaction finalized. The first
    /// transaction gets blockhash 2, a re-signed one blockhash 3.
    async fn send(payer: &Keypair, unseen: Vec<Unseen>) -> Result<Signature> {
        let mut mocks = MocksMap::default();
        mocks.insert(
            RpcRequest::SimulateTransaction,
            json!({ "context": { "slot": SENT_AFTER }, "value": { "unitsConsumed": 150 } }),
        );
        // The simulation takes blockhash 1.
        for seed in 1..=3 {
            mocks.insert(RpcRequest::GetLatestBlockhash, blockhash(seed));
        }
        // Before each of the two sends.
        for _ in 0..2 {
            mocks.insert(RpcRequest::GetSlot, json!(SENT_AFTER));
        }
        mocks.insert(RpcRequest::GetBalance, balance(SENT_AFTER, BALANCE));
        for poll in unseen {
            mocks.insert(RpcRequest::GetSignatureStatuses, statuses(Value::Null));
            mocks.insert(
                RpcRequest::GetEpochInfo,
                json!({
                    "epoch": 1,
                    "slotIndex": 68,
                    "slotsInEpoch": 432,
                    "absoluteSlot": FINALIZED,
                    "blockHeight": poll.block_height,
                }),
            );
            mocks.insert(RpcRequest::GetBalance, balance(poll.slot, poll.balance));
            mocks.insert(RpcRequest::GetSignaturesForAddress, history(poll.history));
        }
        mocks.insert(
            RpcRequest::GetSignatureStatuses,
            statuses(json!({
                "slot": 400,
                "confirmations": null,
                "status": { "Ok": null },
                "err": null,
                "confirmationStatus": "finalized",
            })),
        );
        // A re-signed transaction starts from the same balance.
        mocks.insert(RpcRequest::GetBalance, balance(SENT_AFTER, BALANCE));
        let chain = Chain {
            rpc: RpcClient::new_mock_with_mocks_map("succeeds", mocks),
            payer: Some(payer.insecure_clone()),
        };
        let transfer = solana_system_interface::instruction::transfer(
            &payer.pubkey(),
            &Pubkey::new_from_array([9; 32]),
            1,
        );
        chain.send(&[transfer], &[]).await
    }

    #[tokio::test]
    async fn send_resigns_only_once_the_finalized_chain_rules_the_transaction_out() -> Result<()> {
        let payer = Keypair::new();
        let first = send(&payer, vec![]).await?;
        // Expired, the transaction not among the payer's signatures, and the
        // payer's balance whole at the expiry: all it takes.
        let gone = Unseen {
            block_height: LAST_VALID + 1,
            slot: FINALIZED,
            balance: BALANCE,
            history: History::Empty,
        };
        let with = |history| Unseen { history, ..gone };
        let (unexpired, listed, cut, paid, behind, nowhere, long) = tokio::try_join!(
            // The blockhash is still valid: the transaction can land.
            send(
                &payer,
                vec![Unseen {
                    block_height: LAST_VALID,
                    ..gone
                }]
            ),
            // The transaction is on the chain, though the balance is whole, as
            // after a deposit or the rollback of an earlier fee: it landed.
            send(&payer, vec![with(History::Lists(first))]),
            // The signatures don't reach back to the send: it may be among
            // the older ones.
            send(&payer, vec![with(History::Cut)]),
            // The payer paid a fee: it landed.
            send(
                &payer,
                vec![Unseen {
                    balance: BALANCE - 5_000,
                    ..gone
                }]
            ),
            // The balance is from before the expiry: it can still change.
            send(
                &payer,
                vec![Unseen {
                    slot: FINALIZED - 1,
                    ..gone
                }]
            ),
            send(&payer, vec![gone]),
            send(&payer, vec![with(History::Long)]),
        )?;
        // The same signature means the same blockhash: no second transaction.
        assert_eq!(unexpired, first);
        assert_eq!(listed, first);
        assert_eq!(cut, first);
        assert_eq!(paid, first);
        assert_eq!(behind, first);
        assert_ne!(nowhere, first);
        assert_ne!(long, first);
        Ok(())
    }
}

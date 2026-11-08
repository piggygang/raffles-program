//! `observe`: how quickly ORAO fulfilled the cluster's latest requests,
//! anyone's, read from transaction history. It sends nothing, so it needs no
//! deployed spike and works on any cluster.

use {
    crate::chain::Chain,
    anchor_lang::{prelude::Pubkey, Discriminator},
    anyhow::{Context, Result},
    orao_solana_vrf::instruction::{FulfillV2, RequestV2},
    solana_commitment_config::CommitmentConfig,
    solana_rpc_client::rpc_client::GetConfirmedSignaturesForAddress2Config,
    solana_rpc_client_api::config::RpcTransactionConfig,
    solana_signature::Signature,
    solana_transaction_status_client_types::{
        EncodedConfirmedTransactionWithStatusMeta, UiInstruction, UiTransactionEncoding,
    },
    std::{collections::HashMap, time::Duration},
};

/// A request younger than this may still be in flight, so it doesn't count
/// as unfulfilled.
const IN_FLIGHT_SECS: i64 = 60;
/// Spacing between transaction reads, which keeps the public RPC from
/// rate-limiting.
const PACE: Duration = Duration::from_millis(120);

#[derive(Default)]
struct Request {
    slot: u64,
    time: Option<i64>,
    /// Slots of the successful fulfillments.
    fulfillments: Vec<u64>,
}

pub async fn observe(chain: &Chain, txs: usize) -> Result<()> {
    let cluster = chain.cluster().await?;
    let mut statuses = Vec::new();
    let mut before = None;
    while statuses.len() < txs {
        let page = chain
            .rpc
            .get_signatures_for_address_with_config(
                &orao_solana_vrf::ID,
                GetConfirmedSignaturesForAddress2Config {
                    before,
                    limit: Some((txs - statuses.len()).min(1_000)),
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..GetConfirmedSignaturesForAddress2Config::default()
                },
            )
            .await?;
        let Some(last) = page.last() else { break };
        before = Some(last.signature.parse()?);
        statuses.extend(page);
    }

    let mut requests: HashMap<Pubkey, Request> = HashMap::new();
    for (read, status) in statuses.iter().enumerate() {
        if status.err.is_some() {
            continue;
        }
        let transaction = fetch(chain, &status.signature.parse()?).await?;
        for (data, request) in orao_instructions(&transaction) {
            let entry = requests.entry(request).or_default();
            if data.starts_with(RequestV2::DISCRIMINATOR) {
                entry.slot = status.slot;
                entry.time = status.block_time;
            } else if data.starts_with(FulfillV2::DISCRIMINATOR) {
                entry.fulfillments.push(status.slot);
            }
        }
        if (read + 1) % 100 == 0 {
            eprintln!("read {} of {} transactions", read + 1, statuses.len());
        }
    }

    // Only requests made inside the window; fulfillments of older ones are
    // in it too.
    let newest = statuses
        .first()
        .and_then(|status| status.block_time)
        .unwrap_or_default();
    let oldest = statuses
        .last()
        .and_then(|status| status.block_time)
        .unwrap_or_default();
    let made: Vec<&Request> = requests
        .values()
        .filter(|request| request.slot > 0)
        .collect();
    let fulfilled: Vec<&&Request> = made
        .iter()
        .filter(|request| !request.fulfillments.is_empty())
        .collect();
    let in_flight = made
        .iter()
        .filter(|request| {
            request.fulfillments.is_empty()
                && request
                    .time
                    .is_some_and(|time| newest - time < IN_FLIGHT_SECS)
        })
        .count();
    let latency = |request: &Request| {
        request
            .fulfillments
            .iter()
            .max()
            .map(|slot| slot.saturating_sub(request.slot))
    };
    let slots: Vec<u64> = fulfilled
        .iter()
        .filter_map(|request| latency(request))
        .collect();
    let responses: Vec<u64> = fulfilled
        .iter()
        .filter_map(|request| u64::try_from(request.fulfillments.len()).ok())
        .collect();

    println!(
        "{cluster}: {} of ORAO's transactions over the last {} min",
        statuses.len(),
        (newest - oldest) / 60
    );
    println!(
        "requests {}: fulfilled {}, in flight {in_flight}, unfulfilled {}",
        made.len(),
        fulfilled.len(),
        made.len() - fulfilled.len() - in_flight
    );
    println!(
        "request → quorum fulfillment, slots: {}",
        percentiles(&slots)
    );
    println!("fulfillments per request: {}", percentiles(&responses));
    Ok(())
}

/// Reads a transaction, backing off while the RPC rate-limits.
async fn fetch(
    chain: &Chain,
    signature: &Signature,
) -> Result<EncodedConfirmedTransactionWithStatusMeta> {
    let config = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Base64),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };
    let mut wait = Duration::from_millis(500);
    for _ in 0..6 {
        match chain
            .rpc
            .get_transaction_with_config(signature, config)
            .await
        {
            Ok(transaction) => {
                tokio::time::sleep(PACE).await;
                return Ok(transaction);
            }
            Err(_) => {
                tokio::time::sleep(wait).await;
                wait *= 2;
            }
        }
    }
    chain
        .rpc
        .get_transaction_with_config(signature, config)
        .await
        .with_context(|| format!("reading {signature}"))
}

/// ORAO instructions in a transaction, top-level and inner (a program
/// requesting by CPI), with the request account each one names.
fn orao_instructions(
    transaction: &EncodedConfirmedTransactionWithStatusMeta,
) -> Vec<(Vec<u8>, Pubkey)> {
    let Some(decoded) = transaction.transaction.transaction.decode() else {
        return Vec::new();
    };
    let mut keys = decoded.message.static_account_keys().to_vec();
    let meta = transaction.transaction.meta.as_ref();
    if let Some(loaded) = meta.and_then(|meta| Option::from(meta.loaded_addresses.clone())) {
        let loaded: solana_transaction_status_client_types::UiLoadedAddresses = loaded;
        keys.extend(
            loaded
                .writable
                .iter()
                .chain(&loaded.readonly)
                .filter_map(|key| key.parse::<Pubkey>().ok()),
        );
    }
    let key = |index: u8| keys.get(usize::from(index)).copied();
    // Request and fulfill both name the request account fourth.
    let request_of = |accounts: &[u8]| accounts.get(3).and_then(|&index| key(index));

    let mut found = Vec::new();
    for instruction in decoded.message.instructions() {
        if key(instruction.program_id_index) == Some(orao_solana_vrf::ID) {
            if let Some(request) = request_of(&instruction.accounts) {
                found.push((instruction.data.clone(), request));
            }
        }
    }
    let inner: Option<Vec<_>> = meta.and_then(|meta| meta.inner_instructions.clone().into());
    for instruction in inner
        .into_iter()
        .flatten()
        .flat_map(|inner| inner.instructions)
    {
        let UiInstruction::Compiled(instruction) = instruction else {
            continue;
        };
        if key(instruction.program_id_index) != Some(orao_solana_vrf::ID) {
            continue;
        }
        let Ok(data) = bs58::decode(&instruction.data).into_vec() else {
            continue;
        };
        if let Some(request) = request_of(&instruction.accounts) {
            found.push((data, request));
        }
    }
    found
}

/// `p50 …, p90 …, p99 …, max …` by nearest rank, or `none`.
pub fn percentiles(values: &[u64]) -> String {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let Some(&max) = sorted.last() else {
        return "none".into();
    };
    let rank = |permille: usize| sorted[(sorted.len() * permille).div_ceil(1_000).max(1) - 1];
    format!(
        "p50 {}, p90 {}, p99 {}, max {max} (n = {})",
        rank(500),
        rank(900),
        rank(990),
        sorted.len()
    )
}

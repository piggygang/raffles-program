//! `run`: the measurement. Each cycle requests randomness through the spike
//! program, waits for ORAO's quorum to fulfill it and consumes it, and writes
//! one JSON line. A failed cycle is recorded too; the next one re-requests.

use {
    crate::{
        chain::{Chain, Landed, POLL},
        observe::percentiles,
    },
    anchor_lang::prelude::Pubkey,
    anyhow::{bail, Context, Result},
    orao_solana_vrf::state::RandomnessV2,
    serde::Serialize,
    solana_commitment_config::CommitmentConfig,
    solana_rpc_client::rpc_client::GetConfirmedSignaturesForAddress2Config,
    solana_signer::Signer,
    solana_system_interface::instruction::transfer,
    std::{
        fs::File,
        io::Write,
        path::PathBuf,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    },
    vrf_spike::{client, winner_index},
};

/// How long `fulfillments` waits for the history to catch up: about 10 s.
const HISTORY_POLLS: u32 = 25;

#[derive(clap::Args)]
pub struct Args {
    /// Request, fulfill and consume cycles.
    #[arg(long, default_value_t = 50)]
    runs: u32,
    /// Seconds to wait for a fulfillment before the cycle counts as failed.
    #[arg(long, default_value_t = 60)]
    timeout: u64,
    /// The entry count consume draws a winner from.
    #[arg(long, default_value_t = 1_000)]
    entries: u64,
    /// A JSON line per cycle [default: target/vrf-runs-<unix time>.jsonl].
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    #[default]
    Ok,
    RequestFailed,
    Timeout,
    ConsumeFailed,
}

/// One cycle. Slots and fees come from the chain; `latency_ms` is wall clock,
/// from the request's confirmation to the fulfillment showing on the RPC.
#[derive(Default, Serialize)]
struct Record {
    run: u32,
    started_at_unix_ms: u128,
    outcome: Outcome,
    error: Option<String>,
    /// The draw's request index, which the seed derives from.
    request: u32,
    request_account: String,
    request_signature: Option<String>,
    request_slot: Option<u64>,
    request_attempts: Option<u32>,
    request_compute_units: Option<u64>,
    request_fee: Option<u64>,
    /// The fulfillment that completed the quorum.
    fulfill_slot: Option<u64>,
    /// Successful fulfillments: one per authority, up to the quorum.
    responses: Option<usize>,
    latency_slots: Option<u64>,
    latency_ms: Option<u128>,
    /// What the request cost the vault once fulfilled: ORAO's fee plus the
    /// rent the fulfilled request keeps.
    vault_cost: Option<u64>,
    consume_signature: Option<String>,
    consume_slot: Option<u64>,
    consume_compute_units: Option<u64>,
    consume_fee: Option<u64>,
    winner: Option<u64>,
}

pub async fn run(chain: &Chain, args: &Args) -> Result<()> {
    chain.require_devnet().await?;
    if !chain
        .account(&vrf_spike::ID)
        .await?
        .is_some_and(|account| account.executable)
    {
        bail!("vrf-spike isn't deployed at {}", vrf_spike::ID);
    }
    let network = chain.network_state().await?.config;
    let rent = |bytes| chain.rpc.get_minimum_balance_for_rent_exemption(bytes);
    let pending_rent = rent(8 + RandomnessV2::PENDING_SIZE).await?;
    let fulfilled_rent = rent(8 + RandomnessV2::FULFILLED_SIZE).await?;
    // Every cycle keeps ORAO's fee and the fulfilled request's rent, and the
    // vault has to stay rent-exempt with one request in flight.
    let needed = u64::from(args.runs) * (network.request_fee + fulfilled_rent)
        + network.request_fee
        + pending_rent
        + rent(0).await?;
    let draw = client::draw(&chain.payer().pubkey());
    let vault = client::vault(&draw);
    fund(chain, &draw, &vault, needed).await?;
    let quorum = orao_solana_vrf::majority(network.fulfillment_authorities.len());

    let out = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("target/vrf-runs-{}.jsonl", unix_ms() / 1_000)));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = File::create(&out).with_context(|| format!("creating {}", out.display()))?;
    let mut records = Vec::new();
    for run in 1..=args.runs {
        let mut record = Record {
            run,
            started_at_unix_ms: unix_ms(),
            ..Record::default()
        };
        let accounts = Accounts {
            draw,
            vault,
            treasury: network.treasury,
            quorum,
        };
        if let Err(err) = cycle(chain, args, &accounts, &mut record).await {
            record.error = Some(format!("{err:#}"));
        }
        writeln!(file, "{}", serde_json::to_string(&record)?)?;
        print_progress(&record, args.runs);
        records.push(record);
    }
    summarize(&records, network.request_fee, fulfilled_rent);
    println!("\nWrote {}", out.display());
    Ok(())
}

/// Creates the draw with `needed` lamports in its vault, or tops the vault up.
async fn fund(chain: &Chain, draw: &Pubkey, vault: &Pubkey, needed: u64) -> Result<()> {
    let wallet = chain.payer().pubkey();
    let exists = chain.draw(draw).await?.is_some();
    let have = if exists {
        chain.lamports(vault).await?
    } else {
        0
    };
    let top_up = needed.saturating_sub(have);
    if top_up == 0 {
        return Ok(());
    }
    let balance = chain.lamports(&wallet).await?;
    if balance < top_up + 10_000_000 {
        bail!(
            "{wallet} holds {balance} lamports; the vault needs {top_up} more plus fees. \
             Fund it first, e.g. `solana airdrop 1 -u devnet`"
        );
    }
    let instruction = if exists {
        transfer(&wallet, vault, top_up)
    } else {
        client::init_draw(wallet, top_up)
    };
    chain.send(&[instruction]).await?;
    println!("Funded the vault {vault} with {top_up} lamports.");
    Ok(())
}

/// What every cycle works on.
struct Accounts {
    draw: Pubkey,
    vault: Pubkey,
    treasury: Pubkey,
    /// Fulfillments that settle a request.
    quorum: usize,
}

async fn cycle(chain: &Chain, args: &Args, accounts: &Accounts, record: &mut Record) -> Result<()> {
    let Accounts {
        draw,
        vault,
        quorum,
        ..
    } = accounts;
    record.outcome = Outcome::RequestFailed;
    let state = chain.draw(draw).await?.context("the draw is gone")?;
    let seed = client::request_seed(draw, state.requests);
    let request = client::request(&seed);
    record.request = state.requests;
    record.request_account = request.to_string();
    // The balances only feed `vault_cost`, so failing to read one doesn't fail
    // the cycle.
    let vault_before = chain.lamports(vault).await.ok();

    let landed = chain
        .send(&[client::request_randomness(
            *draw,
            state.requests,
            accounts.treasury,
        )])
        .await?;
    record.request_signature = Some(landed.signature.to_string());
    record.request_slot = Some(landed.slot);
    record.request_attempts = Some(landed.attempts);
    record.request_compute_units = landed.compute_units;
    record.request_fee = landed.fee;

    record.outcome = Outcome::Timeout;
    let deadline = landed.confirmed_at + Duration::from_secs(args.timeout);
    loop {
        // An RPC error is transient here too: keep polling until the deadline.
        let state = chain.request(&request).await.ok().flatten();
        if state.is_some_and(|state| state.fulfilled().is_some()) {
            break;
        }
        if Instant::now() > deadline {
            bail!("not fulfilled within {} s", args.timeout);
        }
        tokio::time::sleep(POLL).await;
    }
    record.latency_ms = Some(landed.confirmed_at.elapsed().as_millis());

    // Fulfilled: from here on only consume can fail the cycle, and the metrics
    // below are best effort.
    record.outcome = Outcome::ConsumeFailed;
    if let Some((fulfill_slot, responses)) = fulfillments(chain, &request, &landed, *quorum).await {
        record.fulfill_slot = Some(fulfill_slot);
        record.responses = Some(responses);
        record.latency_slots = fulfill_slot.checked_sub(landed.slot);
    }
    let vault_after = chain.lamports(vault).await.ok();
    record.vault_cost = vault_before
        .zip(vault_after)
        .and_then(|(before, after)| before.checked_sub(after));

    let landed = chain
        .send(&[client::consume(*draw, &seed, args.entries)])
        .await?;
    record.consume_signature = Some(landed.signature.to_string());
    record.consume_slot = Some(landed.slot);
    record.consume_compute_units = landed.compute_units;
    record.consume_fee = landed.fee;
    record.outcome = Outcome::Ok;
    record.winner = chain
        .draw(draw)
        .await
        .ok()
        .flatten()
        .and_then(|state| winner_index(&state.value, args.entries).ok());
    Ok(())
}

/// The slot of the fulfillment that completed the quorum, and how many
/// authorities' fulfillments landed. The RPC indexes signatures behind the
/// account state, so it polls until the history shows the whole quorum, and
/// gives `None` rather than a partial count.
async fn fulfillments(
    chain: &Chain,
    request: &Pubkey,
    landed: &Landed,
    quorum: usize,
) -> Option<(u64, usize)> {
    let request_signature = landed.signature.to_string();
    for _ in 0..HISTORY_POLLS {
        let history = chain
            .rpc
            .get_signatures_for_address_with_config(
                request,
                GetConfirmedSignaturesForAddress2Config {
                    limit: Some(25),
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..GetConfirmedSignaturesForAddress2Config::default()
                },
            )
            .await;
        if let Ok(history) = history {
            let slots: Vec<u64> = history
                .iter()
                .filter(|status| status.err.is_none() && status.signature != request_signature)
                .map(|status| status.slot)
                .collect();
            if slots.len() >= quorum {
                return slots.iter().max().map(|&slot| (slot, slots.len()));
            }
        }
        tokio::time::sleep(POLL).await;
    }
    None
}

fn print_progress(record: &Record, runs: u32) {
    let detail = match record.outcome {
        Outcome::Ok => format!(
            "fulfilled after {} slots ({} ms), winner {}",
            record.latency_slots.unwrap_or_default(),
            record.latency_ms.unwrap_or_default(),
            record.winner.unwrap_or_default()
        ),
        _ => record.error.clone().unwrap_or_default(),
    };
    println!(
        "run {:>3}/{runs}  {:<14}  {detail}",
        record.run,
        outcome_name(record.outcome)
    );
}

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Ok => "ok",
        Outcome::RequestFailed => "request failed",
        Outcome::Timeout => "timeout",
        Outcome::ConsumeFailed => "consume failed",
    }
}

/// A Markdown table for docs/spikes.
fn summarize(records: &[Record], request_fee: u64, fulfilled_rent: u64) {
    let runs = records.len();
    let ok = records.iter().filter(|r| r.outcome == Outcome::Ok).count();
    let collect = |field: fn(&Record) -> Option<u64>| -> Vec<u64> {
        records.iter().filter_map(field).collect()
    };
    let latency_ms = collect(|r| r.latency_ms.and_then(|ms| u64::try_from(ms).ok()));
    let crank_fees = collect(|r| Some(r.request_fee? + r.consume_fee?));
    let failures: Vec<String> = [
        Outcome::RequestFailed,
        Outcome::Timeout,
        Outcome::ConsumeFailed,
    ]
    .into_iter()
    .map(|outcome| {
        let n = records.iter().filter(|r| r.outcome == outcome).count();
        format!("{} {n}", outcome_name(outcome))
    })
    .collect();
    let retried = records
        .iter()
        .filter(|r| r.request_attempts.is_some_and(|attempts| attempts > 1))
        .count();

    println!("\n| Measure | Result |\n| --- | --- |");
    println!("| Cycles fulfilled and consumed | {ok} of {runs} |");
    println!("| Failures | {} |", failures.join(", "));
    println!("| Requests re-signed after a blockhash expired | {retried} |");
    println!(
        "| Request → quorum fulfillment, slots | {} |",
        percentiles(&collect(|r| r.latency_slots))
    );
    println!(
        "| Request confirmed → fulfillment seen, ms | {} |",
        percentiles(&latency_ms)
    );
    println!(
        "| Fulfillments per request | {} |",
        percentiles(&collect(|r| r
            .responses
            .and_then(|n| u64::try_from(n).ok())))
    );
    println!(
        "| `request` compute units | {} |",
        percentiles(&collect(|r| r.request_compute_units))
    );
    println!(
        "| `consume` compute units | {} |",
        percentiles(&collect(|r| r.consume_compute_units))
    );
    println!(
        "| Vault cost per request, lamports | {} (ORAO fee {request_fee} + fulfilled request rent {fulfilled_rent}) |",
        percentiles(&collect(|r| r.vault_cost))
    );
    println!(
        "| Crank fees per cycle (request + consume), lamports | {} |",
        percentiles(&crank_fees)
    );
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

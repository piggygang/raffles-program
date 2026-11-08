//! PIG-114 spike driver. Requests ORAO VRF randomness through
//! programs/vrf-spike on devnet, waits for ORAO to fulfill it, consumes it and
//! records each cycle. It is the seed of raffles-crank's VRF driver (PIG-160).
//!
//! `run` and `close` send transactions and refuse any cluster but devnet;
//! `status` and `observe` only read.

mod chain;
mod observe;
mod run;

use {
    anyhow::Result,
    chain::Chain,
    clap::{Parser, Subcommand},
    solana_signer::Signer,
    vrf_spike::client,
};

#[derive(Parser)]
#[command(about, version)]
struct Cli {
    /// RPC URL. The public devnet endpoint rate-limits; a Helius devnet URL is
    /// faster. Never printed, since it can carry an API key.
    #[arg(
        long,
        env = "RAFFLES_DEVNET_RPC_URL",
        default_value = "https://api.devnet.solana.com",
        global = true,
        hide_env_values = true
    )]
    url: String,
    /// Pays transaction fees and funds the draw's vault.
    #[arg(long, default_value = "~/.config/solana/id.json", global = true)]
    keypair: String,
    /// Priority fee for the transactions it sends, in micro-lamports per
    /// compute unit.
    #[arg(long, default_value_t = 0, global = true)]
    priority_fee: u64,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// ORAO's configuration and this wallet's draw.
    Status,
    /// How quickly ORAO fulfilled the cluster's latest requests, anyone's.
    /// Works on mainnet too.
    Observe {
        /// How many of ORAO's latest transactions to read.
        #[arg(long, default_value_t = 500)]
        txs: usize,
    },
    /// Requests, waits for the fulfillment and consumes, `--runs` times.
    Run(run::Args),
    /// Closes this wallet's draw and returns what its vault holds.
    Close,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let keypair = match cli.command {
        Command::Observe { .. } => None,
        _ => Some(cli.keypair.as_str()),
    };
    let chain = Chain::new(&cli.url, keypair, cli.priority_fee)?;
    match cli.command {
        Command::Status => status(&chain).await,
        Command::Observe { txs } => observe::observe(&chain, txs).await,
        Command::Run(args) => run::run(&chain, &args).await,
        Command::Close => close(&chain).await,
    }
}

async fn status(chain: &Chain) -> Result<()> {
    println!("cluster        {}", chain.cluster().await?);
    let network = chain.network_state().await?.config;
    let authorities = network.fulfillment_authorities.len();
    println!(
        "ORAO           fee {} lamports, {} fulfillment authorities, quorum {}",
        network.request_fee,
        authorities,
        orao_solana_vrf::majority(authorities)
    );
    let deployed = chain
        .account(&vrf_spike::ID)
        .await?
        .is_some_and(|account| account.executable);
    println!(
        "vrf-spike      {} ({})",
        vrf_spike::ID,
        if deployed { "deployed" } else { "not deployed" }
    );
    let wallet = chain.payer().pubkey();
    println!(
        "wallet         {wallet}, {} lamports",
        chain.lamports(&wallet).await?
    );
    let draw = client::draw(&wallet);
    match chain.draw(&draw).await? {
        None => println!("draw           none yet; `run` creates it"),
        Some(state) => {
            println!(
                "draw           {draw}: {} requests, latest in slot {}, consumed in slot {}",
                state.requests, state.requested_slot, state.consumed_slot
            );
            let vault = client::vault(&draw);
            println!(
                "vault          {vault}, {} lamports",
                chain.lamports(&vault).await?
            );
        }
    }
    Ok(())
}

async fn close(chain: &Chain) -> Result<()> {
    chain.require_devnet().await?;
    let wallet = chain.payer().pubkey();
    let vault = client::vault(&client::draw(&wallet));
    let returned = chain.lamports(&vault).await?;
    let landed = chain.send(&[client::close_draw(wallet)]).await?;
    println!(
        "Closed the draw in slot {}; the vault returned {returned} lamports ({}).",
        landed.slot, landed.signature
    );
    Ok(())
}

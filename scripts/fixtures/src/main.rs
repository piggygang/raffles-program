//! PIG-115: devnet stand-ins for the Piggy collections, the prize assets and
//! the test wallets, with mainnet's on-chain structure (holder-proof §7).
//!
//! `apply` sends transactions. `plan` only writes files, and `check`, `dump`
//! and `keys` only read. Every key derives from the seed, so `apply` can run
//! any number of times and `plan` needs no network.

mod apply;
mod build;
mod chain;
mod keys;
mod output;
mod spec;

use {
    anyhow::{bail, Result},
    chain::Chain,
    clap::{Parser, Subcommand, ValueEnum},
    keys::Keys,
    solana_signer::Signer,
    spec::{Authority, Spec, Wallet},
    std::path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Cluster {
    /// The team's fixture set, from the seed file.
    Devnet,
    /// A local validator's, from a public seed.
    Localnet,
}

impl Cluster {
    pub fn name(self) -> &'static str {
        match self {
            Self::Devnet => "devnet",
            Self::Localnet => "localnet",
        }
    }
}

#[derive(Parser)]
#[command(about, version)]
struct Cli {
    #[arg(long, value_enum, global = true, default_value = "localnet")]
    cluster: Cluster,
    /// RPC URL [default: $RAFFLES_DEVNET_RPC_URL or the public endpoint on
    /// devnet, http://127.0.0.1:8899 on localnet]. Never printed, since it can
    /// carry an API key.
    #[arg(long, global = true)]
    url: Option<String>,
    /// The devnet seed: any `solana-keygen` keypair file. Localnet's is public.
    #[arg(
        long,
        global = true,
        default_value = "~/.config/solana/piggygang/devnet-fixtures-seed.json"
    )]
    seed_file: String,
    /// Pays for everything `apply` creates.
    #[arg(long, global = true, default_value = "~/.config/solana/id.json")]
    keypair: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Writes the manifest, the indexer config and the mint lists. Needs no
    /// network.
    Plan,
    /// Creates what's missing, returns what wandered, checks the result and
    /// writes the outputs. Sends transactions.
    Apply {
        /// Replaces a manifest that another seed wrote.
        #[arg(long)]
        new_seed: bool,
    },
    /// Compares the cluster with the spec. Only reads.
    Check,
    /// Copies the first pig of each eligible collection and every look-alike,
    /// for the claim spike's tests. Only reads.
    Dump {
        #[arg(long)]
        out: PathBuf,
    },
    /// Writes the wallets' and authorities' keypair files, for the Solana CLI
    /// and the E2E run. Refuses a path inside the repo.
    Keys {
        #[arg(long)]
        out: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cluster = cli.cluster;
    let keys = match cluster {
        Cluster::Devnet => Keys::from_file(&expand(&cli.seed_file))?,
        Cluster::Localnet => Keys::localnet(),
    };
    let spec = Spec::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(cluster.name());
    let url = cli.url.clone().unwrap_or_else(|| match cluster {
        Cluster::Devnet => std::env::var("RAFFLES_DEVNET_RPC_URL")
            .unwrap_or_else(|_| "https://api.devnet.solana.com".into()),
        Cluster::Localnet => "http://127.0.0.1:8899".into(),
    });

    match cli.command {
        Command::Plan => {
            output::write(&dir, cluster, &keys, &spec)?;
            println!("Wrote {}.", dir.display());
        }
        Command::Apply { new_seed } => {
            if let Some(root) = output::manifest_root(&dir)? {
                if root != keys.root() && !new_seed {
                    bail!(
                        "{} was written with another seed (root {root}, this one is {}). Use \
                         that seed, or pass --new-seed to replace the fixture set",
                        dir.display(),
                        keys.root()
                    );
                }
            }
            let chain = Chain::new(&url, Some(&expand(&cli.keypair)))?;
            chain.require(cluster).await?;
            let payer = chain.payer().pubkey();
            println!(
                "{} fixtures, paid by {payer} ({} lamports; the wallets alone take up to {}).",
                cluster.name(),
                chain.rpc.get_balance(&payer).await?,
                spec.funding()
            );
            let lost = apply::apply(&chain, &keys, &spec).await?;
            let problems = apply::check(&chain, &keys, &spec).await?;
            output::write(&dir, cluster, &keys, &spec)?;
            println!("Wrote {}.", dir.display());
            report(&lost, &problems)?;
        }
        Command::Check => {
            let chain = Chain::new(&url, None)?;
            chain.require(cluster).await?;
            let problems = apply::check(&chain, &keys, &spec).await?;
            report(&[], &problems)?;
            println!("The {} fixtures match the spec.", cluster.name());
        }
        Command::Dump { out } => {
            let chain = Chain::new(&url, None)?;
            chain.require(cluster).await?;
            output::dump(&chain, &out, &keys).await?;
            println!("Wrote {}.", out.display());
        }
        Command::Keys { out } => {
            let out = std::path::absolute(expand(&out.to_string_lossy()))?;
            if out.starts_with(repo().canonicalize()?) || out.starts_with(repo()) {
                bail!("{} is inside the repo; keys stay outside it", out.display());
            }
            std::fs::create_dir_all(&out)?;
            let keypairs = Wallet::ALL
                .into_iter()
                .map(|wallet| (wallet.label().to_owned(), wallet.keypair(&keys)))
                .chain(Authority::ALL.into_iter().map(|authority| {
                    (
                        format!("authority-{}", authority.label()),
                        authority.keypair(&keys),
                    )
                }));
            for (name, keypair) in keypairs {
                let path = out.join(format!("{name}.json"));
                std::fs::write(&path, serde_json::to_string(&keypair.to_bytes().to_vec())?)?;
                restrict(&path)?;
            }
            println!(
                "Wrote the {} keypairs to {}.",
                cluster.name(),
                out.display()
            );
        }
    }
    Ok(())
}

fn report(lost: &[String], problems: &[String]) -> Result<()> {
    for line in lost {
        eprintln!("lost: {line}");
    }
    for line in problems {
        eprintln!("mismatch: {line}");
    }
    if !problems.is_empty() {
        bail!("{} differences from the spec", problems.len());
    }
    Ok(())
}

/// The repo root: this crate lives in scripts/fixtures.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `~/` means the home directory.
fn expand(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// Keypair files are readable by their owner only.
fn restrict(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    let _ = path;
    Ok(())
}

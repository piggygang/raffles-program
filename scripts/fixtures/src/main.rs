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
    anyhow::{anyhow, bail, Context, Result},
    chain::Chain,
    clap::{Parser, Subcommand, ValueEnum},
    keys::Keys,
    solana_keypair::Keypair,
    solana_signer::Signer,
    spec::{Authority, Spec, Wallet},
    std::{
        io::Write,
        path::{Component, Path, PathBuf},
    },
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
    Plan {
        /// Replaces a manifest that another seed wrote.
        #[arg(long)]
        new_seed: bool,
    },
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
    // An RPC error quotes the URL, which can carry an API key. Errors go out as
    // one redacted line, without the sources that quote it.
    run(Cli::parse())
        .await
        .map_err(|err| anyhow!(redact(&causes(&err))))
}

async fn run(cli: Cli) -> Result<()> {
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
        Command::Plan { new_seed } => {
            check_root(&dir, &keys, new_seed)?;
            output::write(&dir, cluster, &keys, &spec)?;
            println!("Wrote {}.", dir.display());
        }
        Command::Apply { new_seed } => {
            check_root(&dir, &keys, new_seed)?;
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
            let out = resolve(&expand(&out.to_string_lossy()))?;
            if out.starts_with(repo().canonicalize()?) {
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
                write_keypair(&out.join(format!("{name}.json")), &keypair)?;
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

/// Refuses to replace a manifest that another seed wrote, unless `--new-seed`
/// says so: once the manifest names the wrong seed, `apply` with it would build
/// a second fixture set.
fn check_root(dir: &Path, keys: &Keys, new_seed: bool) -> Result<()> {
    if let Some(root) = output::manifest_root(dir)? {
        if root != keys.root() && !new_seed {
            bail!(
                "{} was written with another seed (root {root}, this one is {}). Use that \
                 seed, or pass --new-seed to replace the fixture set",
                dir.display(),
                keys.root()
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

/// Where `path` leads, even if it doesn't exist yet: absolute, with each `..`
/// and symlink resolved one component at a time.
fn resolve(path: &Path) -> Result<PathBuf> {
    let mut resolved = PathBuf::new();
    for component in std::path::absolute(path)?.components() {
        match component {
            Component::CurDir => {}
            // What's resolved so far holds no symlink, so its parent is real.
            Component::ParentDir => {
                resolved.pop();
            }
            _ => {
                resolved.push(component);
                if resolved.exists() {
                    resolved = resolved.canonicalize()?;
                }
            }
        }
    }
    Ok(resolved)
}

/// Writes a keypair file only its owner can read, as `solana-keygen` does.
///
/// The key goes into a new file, which then takes the path's place. Writing
/// into what is already there would follow a symlink or a hard link to
/// anywhere, the repo included.
fn write_keypair(path: &Path, keypair: &Keypair) -> Result<()> {
    let new = path.with_added_extension("new");
    // One left behind by a run that died would be in the way.
    let _ = std::fs::remove_file(&new);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&new)
        .with_context(|| format!("writing {}", new.display()))?
        .write_all(serde_json::to_string(&keypair.to_bytes().to_vec())?.as_bytes())?;
    std::fs::rename(&new, path).with_context(|| format!("replacing {}", path.display()))
}

/// The error and its causes on one line. A client error prints the same
/// message as its kind, so repeats are dropped.
fn causes(err: &anyhow::Error) -> String {
    let mut causes: Vec<String> = err.chain().map(ToString::to_string).collect();
    causes.dedup();
    causes.join(": ")
}

/// `text` with each URL cut back to its scheme and host: an RPC URL can carry
/// an API key in its user info, path or query.
fn redact(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (before, after) = rest.split_at(at + 3);
        redacted.push_str(before);
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '"' | '\'' | '<' | '>'))
            .unwrap_or(after.len());
        let (url, tail) = after.split_at(end);
        let (authority, path) = url.split_at(url.find(['/', '?', '#']).unwrap_or(url.len()));
        redacted.push_str(authority.rsplit('@').next().unwrap_or_default());
        redacted.push_str(if path.len() > 1 { "/[redacted]" } else { path });
        rest = tail;
    }
    redacted.push_str(rest);
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_keeps_only_scheme_and_host() {
        assert_eq!(
            redact(
                "error sending request for url (https://devnet.helius-rpc.com/?api-key=SECRET): \
                 tcp connect error"
            ),
            "error sending request for url (https://devnet.helius-rpc.com/[redacted]): tcp \
             connect error"
        );
        assert_eq!(
            redact("https://user:SECRET@rpc.example:8899/v2/SECRET and http://127.0.0.1:8899/"),
            "https://rpc.example:8899/[redacted] and http://127.0.0.1:8899/"
        );
        assert_eq!(redact("no URL here"), "no URL here");
    }

    #[test]
    fn resolve_follows_dot_dot_out_of_missing_directories() {
        let repo = repo().canonicalize().unwrap();
        assert_eq!(
            resolve(&repo.join("scripts/missing/../../keys")).unwrap(),
            repo.join("keys")
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_keypair_replaces_a_link_without_writing_through_it() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("fixtures-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // What a hard link and a symlink in a key's place lead to.
        let target = dir.join("target");
        std::fs::write(&target, "not a key").unwrap();
        let (hard, soft) = (dir.join("hard.json"), dir.join("soft.json"));
        std::fs::hard_link(&target, &hard).unwrap();
        std::os::unix::fs::symlink(&target, &soft).unwrap();

        let keypair = Keypair::new();
        for path in [&hard, &soft] {
            write_keypair(path, &keypair).unwrap();
            let written = std::fs::symlink_metadata(path).unwrap();
            assert!(written.is_file());
            assert_eq!(written.permissions().mode() & 0o777, 0o600);
            assert_eq!(
                solana_keypair::read_keypair_file(path).unwrap().pubkey(),
                keypair.pubkey()
            );
            assert_eq!(std::fs::read_to_string(&target).unwrap(), "not a key");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

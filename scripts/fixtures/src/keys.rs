//! Every key in the fixture set derives from one seed, so a rerun finds the
//! accounts it made before and the manifest can be written without a network.
//! The devnet seed is a team secret outside the repo; the localnet seed is
//! public, because nothing on a local validator is worth protecting.

use {
    anyhow::{anyhow, bail, Context, Result},
    solana_keypair::Keypair,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    std::path::Path,
};

/// Seed material for a local validator: public by design.
const LOCALNET_SEED: &[u8] = b"piggygang-raffles localnet fixtures";
/// Separates fixture keys from anything else derived from the same bytes.
const DOMAIN: &[u8] = b"piggygang-fixtures/";

pub struct Keys {
    secret: Vec<u8>,
}

impl Keys {
    /// Reads a seed file: any `solana-keygen` keypair file, whose 64 bytes are
    /// used as the secret.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| {
            format!(
                "reading the seed file {}. Create it once with `solana-keygen new \
                 --no-bip39-passphrase -o {}` and keep a copy in the team password manager",
                path.display(),
                path.display()
            )
        })?;
        let secret: Vec<u8> = serde_json::from_str(&text)
            .map_err(|err| anyhow!("{} isn't a keypair file: {err}", path.display()))?;
        if secret.len() != 64 {
            bail!(
                "{} holds {} bytes, not a keypair's 64",
                path.display(),
                secret.len()
            );
        }
        Ok(Self { secret })
    }

    pub fn localnet() -> Self {
        Self {
            secret: LOCALNET_SEED.to_vec(),
        }
    }

    /// The keypair for `label`, the same on every run.
    pub fn keypair(&self, label: &str) -> Keypair {
        let hash = solana_sha256_hasher::hashv(&[DOMAIN, &self.secret, b"/", label.as_bytes()]);
        Keypair::new_from_array(hash.to_bytes())
    }

    pub fn address(&self, label: &str) -> Pubkey {
        self.keypair(label).pubkey()
    }

    /// Names the seed without revealing it: the manifest records it, and
    /// `apply` refuses to overwrite a manifest another seed wrote.
    pub fn root(&self) -> Pubkey {
        self.address("root")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stable_and_distinct() {
        let keys = Keys::localnet();
        assert_eq!(keys.address("wallet/whale"), keys.address("wallet/whale"));
        assert_ne!(keys.address("wallet/whale"), keys.address("wallet/buyer"));
        let other = Keys {
            secret: vec![7; 64],
        };
        assert_ne!(keys.address("wallet/whale"), other.address("wallet/whale"));
    }
}

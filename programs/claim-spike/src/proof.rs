//! The per-NFT checks of holder-proof §4, one function per rule kind. Any miss
//! fails the whole instruction (spec §6).

use {
    crate::{error::ClaimError, state::MembershipRule},
    anchor_lang::prelude::*,
    anchor_spl::{metadata::mpl_token_metadata, token::TokenAccount},
};

pub const TOKEN_METADATA_ID: Pubkey = mpl_token_metadata::ID;
pub const MPL_CORE_ID: Pubkey = pubkey!("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

/// Token Metadata's `Key::MetadataV1`.
const METADATA_V1: u8 = 4;
/// mpl-core's `Key::AssetV1`.
const ASSET_V1: u8 = 1;
/// mpl-core's `UpdateAuthority::Collection` tag.
const UPDATE_AUTHORITY_COLLECTION: u8 = 2;

/// How the metadata account is decoded: the claim's own prefix decode, or
/// mpl-token-metadata's whole `Metadata`, the variant measured against it.
#[derive(Clone, Copy)]
pub enum Decoder {
    Prefix,
    Full,
}

/// A metadata account up to and including `creators`, everything the rule
/// reads. Borsh stops there, so the padding and the fields after it don't
/// matter, and the strings stay bytes: no UTF-8 check, nothing to fail on.
#[derive(AnchorDeserialize)]
struct MetadataPrefix {
    key: u8,
    update_authority: Pubkey,
    mint: Pubkey,
    _name: Vec<u8>,
    symbol: Vec<u8>,
    _uri: Vec<u8>,
    _seller_fee_basis_points: u16,
    creators: Option<Vec<Creator>>,
}

#[derive(AnchorDeserialize)]
struct Creator {
    address: Pubkey,
    verified: bool,
    _share: u8,
}

/// What the rule needs from a metadata account, whichever decoder read it.
struct RuleFields {
    is_metadata_v1: bool,
    update_authority: Pubkey,
    mint: Pubkey,
    symbol: Vec<u8>,
    verified_creators: Vec<Pubkey>,
}

impl RuleFields {
    fn decode(data: &[u8], decoder: Decoder) -> Result<Self> {
        Ok(match decoder {
            Decoder::Prefix => {
                let prefix = MetadataPrefix::deserialize(&mut &data[..])
                    .map_err(|_| error!(ClaimError::NotMetadata))?;
                Self {
                    is_metadata_v1: prefix.key == METADATA_V1,
                    update_authority: prefix.update_authority,
                    mint: prefix.mint,
                    symbol: prefix.symbol,
                    verified_creators: verified(
                        prefix
                            .creators
                            .iter()
                            .flatten()
                            .map(|c| (c.address, c.verified)),
                    ),
                }
            }
            Decoder::Full => {
                let metadata = mpl_token_metadata::accounts::Metadata::from_bytes(data)
                    .map_err(|_| error!(ClaimError::NotMetadata))?;
                Self {
                    is_metadata_v1: metadata.key == mpl_token_metadata::types::Key::MetadataV1,
                    update_authority: metadata.update_authority,
                    mint: metadata.mint,
                    symbol: metadata.symbol.into_bytes(),
                    verified_creators: verified(
                        metadata
                            .creators
                            .iter()
                            .flatten()
                            .map(|c| (c.address, c.verified)),
                    ),
                }
            }
        })
    }
}

fn verified(creators: impl Iterator<Item = (Pubkey, bool)>) -> Vec<Pubkey> {
    creators
        .filter(|&(_, verified)| verified)
        .map(|(address, _)| address)
        .collect()
}

/// Checks a Token Metadata pig: the signer holds it, and its metadata meets
/// `rule`. Returns the mint, the key of its receipt.
pub fn check_token_metadata(
    rule: &MembershipRule,
    signer: &Pubkey,
    token: &AccountInfo,
    metadata: &AccountInfo,
    decoder: Decoder,
) -> Result<Pubkey> {
    // 1. A classic SPL Token account of the signer's, holding the one token.
    require_keys_eq!(
        *token.owner,
        anchor_spl::token::ID,
        ClaimError::NotTokenAccount
    );
    let token = TokenAccount::try_deserialize(&mut &token.try_borrow_data()?[..])?;
    require_keys_eq!(token.owner, *signer, ClaimError::NotHolder);
    require_eq!(token.amount, 1, ClaimError::NotHolder);

    // 2. Token Metadata's account for that mint. Token Metadata only creates
    // metadata at the mint's PDA, so its ownership plus the `mint` field bind
    // the account to the mint without re-deriving the address.
    require_keys_eq!(*metadata.owner, TOKEN_METADATA_ID, ClaimError::NotMetadata);
    let fields = RuleFields::decode(&metadata.try_borrow_data()?, decoder)?;
    require!(fields.is_metadata_v1, ClaimError::NotMetadata);
    require_keys_eq!(fields.mint, token.mint, ClaimError::NotMetadata);

    // 3–5. The rule: update authority, symbol without its NUL padding, and the
    // creator, verified, at any position.
    require_keys_eq!(
        fields.update_authority,
        rule.update_authority,
        ClaimError::RuleMismatch
    );
    require!(
        trim_nul(&fields.symbol) == trim_nul(&rule.symbol),
        ClaimError::RuleMismatch
    );
    require!(
        fields.verified_creators.contains(&rule.address),
        ClaimError::RuleMismatch
    );
    Ok(token.mint)
}

/// Checks a Core pig: an mpl-core asset the signer owns, in the rule's
/// collection. Its address is the key of its receipt.
pub fn check_core(rule: &MembershipRule, signer: &Pubkey, asset: &AccountInfo) -> Result<()> {
    require_keys_eq!(*asset.owner, MPL_CORE_ID, ClaimError::NotCoreAsset);
    let data = asset.try_borrow_data()?;
    // The base record's fixed offsets: key 0, owner 1..33, update authority
    // tag 33 and address 34..66.
    let base = data.get(..66).ok_or(ClaimError::NotCoreAsset)?;
    require_eq!(base[0], ASSET_V1, ClaimError::NotCoreAsset);
    require!(base[1..33] == signer.to_bytes(), ClaimError::NotHolder);
    require!(
        base[33] == UPDATE_AUTHORITY_COLLECTION && base[34..66] == rule.address.to_bytes(),
        ClaimError::RuleMismatch
    );
    Ok(())
}

/// `bytes` without its trailing NULs.
fn trim_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|&b| b != 0)
        .map_or(0, |last| last + 1);
    &bytes[..end]
}

#[cfg(test)]
mod tests {
    use super::trim_nul;

    #[test]
    fn trim_nul_strips_only_trailing_nuls() {
        assert_eq!(trim_nul(b"PSG\0\0\0\0\0\0\0"), b"PSG");
        assert_eq!(trim_nul(b"PSG"), b"PSG");
        assert_eq!(trim_nul(b"P\0G\0"), b"P\0G");
        assert_eq!(trim_nul(b"\0\0"), b"");
    }
}

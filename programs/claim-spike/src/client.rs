//! Instruction builders for the tests: the accounts and data a client sends.

use {
    crate::{
        accounts, instruction,
        proof::TOKEN_METADATA_ID,
        state::{Config, EntryPosition, EntryReceipt},
    },
    anchor_lang::{
        prelude::Pubkey,
        solana_program::instruction::{AccountMeta, Instruction},
        system_program, InstructionData, ToAccountMetas,
    },
};

/// The global config: the claim reads its pause flag.
pub fn config() -> Pubkey {
    Pubkey::find_program_address(&[Config::SEED], &crate::ID).0
}

/// The wallet's position in the raffle, opened by its first claim.
pub fn position(raffle: &Pubkey, wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[EntryPosition::SEED, raffle.as_ref(), wallet.as_ref()],
        &crate::ID,
    )
    .0
}

/// The receipt that marks `nft`, a mint or a Core asset, as entered.
pub fn receipt(raffle: &Pubkey, nft: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[EntryReceipt::SEED, raffle.as_ref(), nft.as_ref()],
        &crate::ID,
    )
    .0
}

/// Token Metadata's account for `mint`.
pub fn metadata(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"metadata", TOKEN_METADATA_ID.as_ref(), mint.as_ref()],
        &TOKEN_METADATA_ID,
    )
    .0
}

/// An NFT to claim with, as the eligible-entries service names it.
#[derive(Clone, Copy, Debug)]
pub enum Pig {
    TokenMetadata {
        rule_index: u8,
        mint: Pubkey,
        token: Pubkey,
    },
    Core {
        rule_index: u8,
        asset: Pubkey,
    },
}

impl Pig {
    pub fn rule_index(&self) -> u8 {
        match *self {
            Self::TokenMetadata { rule_index, .. } | Self::Core { rule_index, .. } => rule_index,
        }
    }

    /// What its receipt derives from: the mint, or the asset.
    pub fn nft(&self) -> Pubkey {
        match *self {
            Self::TokenMetadata { mint, .. } => mint,
            Self::Core { asset, .. } => asset,
        }
    }
}

/// The canonical claim, or one of the variants measured against it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    Canonical,
    WithMints,
    FullDecode,
    Traced,
}

/// Signed by `wallet`, which pays the rent of the receipts and, on its first
/// claim in `raffle`, of its position.
pub fn claim_entries(claim: Claim, wallet: Pubkey, raffle: Pubkey, pigs: &[Pig]) -> Instruction {
    let rule_indexes = pigs.iter().map(Pig::rule_index).collect();
    let data = match claim {
        Claim::Canonical => instruction::ClaimEntries { rule_indexes }.data(),
        Claim::WithMints => instruction::ClaimEntriesWithMints { rule_indexes }.data(),
        Claim::FullDecode => instruction::ClaimEntriesFullDecode { rule_indexes }.data(),
        Claim::Traced => instruction::ClaimEntriesTraced { rule_indexes }.data(),
    };
    let mut metas = accounts::ClaimEntries {
        wallet,
        config: config(),
        raffle,
        position: position(&raffle, &wallet),
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    for pig in pigs {
        match *pig {
            Pig::TokenMetadata { mint, token, .. } => {
                if claim == Claim::WithMints {
                    metas.push(AccountMeta::new_readonly(mint, false));
                }
                metas.push(AccountMeta::new_readonly(token, false));
                metas.push(AccountMeta::new_readonly(metadata(&mint), false));
            }
            Pig::Core { asset, .. } => metas.push(AccountMeta::new_readonly(asset, false)),
        }
        metas.push(AccountMeta::new(receipt(&raffle, &pig.nft()), false));
    }
    Instruction::new_with_bytes(crate::ID, &data, metas)
}

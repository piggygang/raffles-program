//! PIG-133 spike: `claim_entries` shaped the way PIG-84 will build it, so its
//! compute and transaction size can be measured before the account model is
//! fixed. Each NFT is checked against the raffle's membership rule
//! (holder-proof §4) and gets its EntryReceipt. The wallet's EntryPosition
//! counts the entries against the cap and awards the Full Gang bonus.
//!
//! LiteSVM only, never deployed: no keypair exists for its address. Delete it
//! once `claim_entries` lands in the raffles program (PIG-84). Findings:
//! docs/spikes/pig-133-claim-entries-budget.md.

#[cfg(not(target_os = "solana"))]
pub mod client;
pub mod error;
pub mod instructions;
pub mod proof;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
use proof::Decoder;

declare_id!("CLMsQPHKpKDBUnKGEYeKR3AtRq3HKtU5BrdSfDrEGSz1");

#[program]
pub mod claim_spike {
    use super::*;

    /// Claims an entry per NFT. `rule_indexes` names each NFT's membership
    /// rule, and the remaining accounts carry each NFT in that order: the
    /// token account and metadata of a Token Metadata pig, or the asset of a
    /// Core pig, then its new EntryReceipt.
    pub fn claim_entries<'info>(
        ctx: Context<'info, ClaimEntries<'info>>,
        rule_indexes: Vec<u8>,
    ) -> Result<()> {
        handle_claim_entries(ctx, &rule_indexes, Variant::CANONICAL)
    }

    /// `claim_entries` with each Token Metadata pig's mint before its token
    /// account: the account list behind R9's estimate of ~6 per transaction.
    pub fn claim_entries_with_mints<'info>(
        ctx: Context<'info, ClaimEntries<'info>>,
        rule_indexes: Vec<u8>,
    ) -> Result<()> {
        let variant = Variant {
            mints: true,
            ..Variant::CANONICAL
        };
        handle_claim_entries(ctx, &rule_indexes, variant)
    }

    /// `claim_entries` decoding each metadata account whole, with
    /// mpl-token-metadata's `Metadata`.
    pub fn claim_entries_full_decode<'info>(
        ctx: Context<'info, ClaimEntries<'info>>,
        rule_indexes: Vec<u8>,
    ) -> Result<()> {
        let variant = Variant {
            decoder: Decoder::Full,
            ..Variant::CANONICAL
        };
        handle_claim_entries(ctx, &rule_indexes, variant)
    }

    /// `claim_entries` logging the compute units left at each step.
    pub fn claim_entries_traced<'info>(
        ctx: Context<'info, ClaimEntries<'info>>,
        rule_indexes: Vec<u8>,
    ) -> Result<()> {
        let variant = Variant {
            trace: true,
            ..Variant::CANONICAL
        };
        handle_claim_entries(ctx, &rule_indexes, variant)
    }
}

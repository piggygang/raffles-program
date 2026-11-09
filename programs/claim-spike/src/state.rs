//! The accounts `claim_entries` reads and writes, shaped like the spec's
//! account model (§7) so their decode and rent cost what PIG-86's will. Only
//! the fields the claim touches matter; the rest stand in for their size.

use anchor_lang::prelude::*;

/// A Piggy collection's membership test: holder-proof §5, 75 bytes, fixed.
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub struct MembershipRule {
    /// `TM_CREATOR` or `CORE_COLLECTION`.
    pub kind: u8,
    /// The verified creator, or the Core collection.
    pub address: Pubkey,
    /// The metadata update authority the rule requires; zero for Core.
    pub update_authority: Pubkey,
    /// The symbol, NUL-padded; zero for Core.
    pub symbol: [u8; 10],
}

impl MembershipRule {
    pub const TM_CREATOR: u8 = 0;
    pub const CORE_COLLECTION: u8 = 1;
}

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,
    pub membership_rules: [MembershipRule; 3],
    pub entry_cap: u8,
    pub full_gang_bonus: u8,
    pub min_duration: i64,
    pub max_duration: i64,
    /// The one field the claim reads: a paused program takes no claims.
    pub paused: bool,
    pub bump: u8,
}

impl Config {
    pub const SEED: &'static [u8] = b"config";
}

#[account]
#[derive(InitSpace)]
pub struct Raffle {
    pub creator: Pubkey,
    pub prize_mint: Pubkey,
    pub end_ts: i64,
    pub state: u8,
    /// Entries handed out so far; the next claim's range starts here.
    pub total_entries: u32,
    pub winner: Pubkey,
    pub drawn_at: i64,
    pub vrf_seed: [u8; 32],
    pub vrf_requests: u32,
    pub vrf_requested_slot: u64,
    pub vrf_value: [u8; 64],
    #[max_len(96)]
    pub rules_uri: String,
    pub rules_hash: [u8; 32],
    pub draw_fee_lamports: u64,
    /// Snapshotted from `Config` at create (D14, D19). Rule `i` is bit `i` of
    /// `EntryPosition::collections_seen`.
    pub membership_rules: [MembershipRule; 3],
    pub entry_cap: u8,
    pub full_gang_bonus: u8,
    pub min_duration: i64,
    pub max_duration: i64,
    pub bump: u8,
    pub vault_bump: u8,
}

impl Raffle {
    pub const ACTIVE: u8 = 0;
}

/// A wallet's entries in one raffle.
#[account]
#[derive(InitSpace)]
pub struct EntryPosition {
    pub raffle: Pubkey,
    /// Claimed the entries and paid the rent.
    pub wallet: Pubkey,
    /// Per-NFT entries, which the cap limits.
    pub entries_claimed: u8,
    /// Bit `i`: an NFT of membership rule `i` was entered.
    pub collections_seen: u8,
    pub bonus_awarded: bool,
    pub range_count: u8,
    /// The raffle's entry ranges this wallet holds, in claim order.
    pub ranges: [EntryRange; EntryPosition::MAX_RANGES],
    pub bump: u8,
}

impl EntryPosition {
    /// Prefixed, unlike the spec's `[raffle, wallet]`: without a prefix a
    /// position and a receipt share one address space, and a wallet whose
    /// address is also an NFT's (a Core asset's keypair) would collide.
    pub const SEED: &'static [u8] = b"position";
    /// Only a claim that adds entries adds a range: with cap 10, at most ten
    /// single-entry claims and one that adds only the bonus.
    pub const MAX_RANGES: usize = 11;
    /// Bits 0–2: one NFT of each membership rule.
    pub const FULL_GANG: u8 = 0b111;
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub struct EntryRange {
    pub start: u32,
    pub count: u8,
}

/// Marks an NFT as entered in a raffle, so it enters once (spec §5).
#[account]
#[derive(InitSpace)]
pub struct EntryReceipt {
    pub raffle: Pubkey,
    /// The Token Metadata mint, or the Core asset.
    pub nft: Pubkey,
    /// Entered the NFT and paid the rent, which the close refunds.
    pub wallet: Pubkey,
    pub bump: u8,
}

impl EntryReceipt {
    pub const SEED: &'static [u8] = b"receipt";
}

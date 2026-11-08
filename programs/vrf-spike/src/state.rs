use anchor_lang::prelude::*;

/// Seed of the per-draw vault PDA, which holds no data and pays ORAO.
pub const VAULT_SEED: &[u8] = b"vault";

/// A draw and its ORAO requests: the VRF state the Raffle will carry (PIG-83).
#[account]
#[derive(InitSpace)]
pub struct Draw {
    /// Funded the vault; gets what is left in it on withdraw.
    pub creator: Pubkey,
    /// Requests so far, re-requests included. The next request's seed derives
    /// from it.
    pub requests: u32,
    /// Seed of the latest request.
    pub seed: [u8; 32],
    /// Slot of the latest request.
    pub requested_slot: u64,
    /// Slot the latest randomness was consumed in; 0 while a request is
    /// outstanding.
    pub consumed_slot: u64,
    /// The latest randomness.
    pub value: [u8; 64],
    pub bump: u8,
    pub vault_bump: u8,
}

impl Draw {
    pub const SEED: &'static [u8] = b"draw";
}

/// Seed of a draw's `index`-th ORAO request: unique per draw and attempt, and
/// never zero, which ORAO rejects.
pub fn request_seed(draw: &Pubkey, index: u32) -> [u8; 32] {
    solana_sha256_hasher::hashv(&[b"vrf-spike", draw.as_ref(), &index.to_le_bytes()]).to_bytes()
}

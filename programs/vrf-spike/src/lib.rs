//! PIG-114 spike: ORAO VRF on devnet, driven and measured by
//! `spikes/vrf-driver`. Delete it once `request_draw` and `fulfill_draw` land
//! in the raffles program (PIG-83).
//!
//! It has the shape those instructions will have. A data-less vault PDA per
//! draw holds the creator's deposit and pays ORAO, so whoever cranks pays only
//! the transaction fee, and `request` and `consume` are permissionless.

#[cfg(not(target_os = "solana"))]
pub mod client;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;

declare_id!("VRFsDurdc8NMVFAS5G8XVGzGjnBkRm1Rv7ZiEckYUCw");

#[program]
pub mod vrf_spike {
    use super::*;

    /// Creates the creator's draw and moves `deposit` lamports into its vault.
    pub fn init_draw(ctx: Context<InitDraw>, deposit: u64) -> Result<()> {
        instructions::init_draw::handle_init_draw(ctx, deposit)
    }

    /// Requests randomness from ORAO for the draw's next seed, paid from the
    /// vault.
    pub fn request(ctx: Context<Request>) -> Result<()> {
        instructions::request::handle_request(ctx)
    }

    /// Reads the fulfilled randomness of the draw's latest request and picks a
    /// winner among `entries`.
    pub fn consume(ctx: Context<Consume>, entries: u64) -> Result<()> {
        instructions::consume::handle_consume(ctx, entries)
    }

    /// Returns what the vault holds to the creator. The draw stays, so its
    /// request count, and with it every seed, never repeats.
    pub fn withdraw(ctx: Context<Withdraw>) -> Result<()> {
        instructions::withdraw::handle_withdraw(ctx)
    }
}

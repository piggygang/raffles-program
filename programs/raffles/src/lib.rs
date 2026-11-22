//! PiggyGang Raffles: free holder-rewards raffles with escrowed prizes and a
//! Switchboard On-Demand draw. All authoritative raffle state lives here.

pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
use state::{ConfigSettings, ConfigUpdate};

declare_id!("RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM");

#[program]
pub mod raffles {
    use super::*;

    /// Creates `Config` with `settings`. Only the program's upgrade authority
    /// can, and it becomes the config's authority.
    pub fn init_config(ctx: Context<InitConfig>, settings: ConfigSettings) -> Result<()> {
        instructions::init_config::handle_init_config(ctx, settings)
    }

    /// Changes the settings that `update` names, the pause flag among them,
    /// and leaves the others as they are. A raffle keeps the settings it
    /// copied when it was created.
    pub fn update_config(ctx: Context<UpdateConfig>, update: ConfigUpdate) -> Result<()> {
        instructions::update_config::handle_update_config(ctx, update)
    }

    /// Proposes `new_authority` as the config's authority, in place of any
    /// earlier proposal. The zero address withdraws the proposal. The
    /// authority doesn't change until `new_authority` signs
    /// `accept_authority`.
    pub fn set_authority(ctx: Context<SetAuthority>, new_authority: Pubkey) -> Result<()> {
        instructions::set_authority::handle_set_authority(ctx, new_authority)
    }

    /// Makes the proposed authority the config's authority. It has to sign.
    pub fn accept_authority(ctx: Context<AcceptAuthority>) -> Result<()> {
        instructions::accept_authority::handle_accept_authority(ctx)
    }
}

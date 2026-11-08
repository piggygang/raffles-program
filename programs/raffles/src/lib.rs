//! PiggyGang Raffles: free holder-rewards raffles with escrowed prizes and a
//! Switchboard On-Demand draw. All authoritative raffle state lives here.

pub mod instructions;

use anchor_lang::prelude::*;

pub use instructions::*;

declare_id!("RAFA9shRkA8Q9h2bSzCHGtaXNTE39yHV8DtxdguSsYM");

#[program]
pub mod raffles {
    use super::*;

    /// Scaffold smoke test: logs a greeting to the signer. Goes away once the
    /// first real instruction lands.
    pub fn hello(ctx: Context<Hello>) -> Result<()> {
        instructions::hello::handle_hello(ctx)
    }
}

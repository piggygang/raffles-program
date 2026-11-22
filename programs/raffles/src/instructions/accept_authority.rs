use {
    crate::{constants::CONFIG_SEED, error::RaffleError, events::AuthorityAccepted, state::Config},
    anchor_lang::prelude::*,
};

#[event_cpi]
#[derive(Accounts)]
pub struct AcceptAuthority<'info> {
    /// The pending authority. Its signature is the proof that it can sign at
    /// all, which is what the transfer waits for.
    pub new_authority: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = config.pending_authority != Pubkey::default()
            @ RaffleError::NoPendingAuthority,
        constraint = config.pending_authority == new_authority.key()
            @ RaffleError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

pub fn handle_accept_authority(ctx: Context<AcceptAuthority>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let old_authority = config.authority;
    config.authority = config.pending_authority;
    config.pending_authority = Pubkey::default();

    let new_authority = config.authority;
    emit_cpi!(AuthorityAccepted {
        old_authority,
        new_authority,
    });
    Ok(())
}

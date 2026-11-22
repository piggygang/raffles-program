use {
    crate::{constants::CONFIG_SEED, error::RaffleError, events::AuthorityProposed, state::Config},
    anchor_lang::prelude::*,
};

#[event_cpi]
#[derive(Accounts)]
pub struct SetAuthority<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ RaffleError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

/// Only proposes: `authority` stays in charge until `new_authority` signs
/// `accept_authority`. So an address that can't sign, such as a Squads
/// multisig account in place of its vault, never becomes the authority
/// (registries §6). The zero address withdraws a proposal, and a later call
/// replaces an earlier one.
pub fn handle_set_authority(ctx: Context<SetAuthority>, new_authority: Pubkey) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.pending_authority = new_authority;

    let (authority, pending_authority) = (config.authority, config.pending_authority);
    emit_cpi!(AuthorityProposed {
        authority,
        pending_authority,
    });
    Ok(())
}

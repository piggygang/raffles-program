use {
    crate::state::{Draw, VAULT_SEED},
    anchor_lang::{
        prelude::*,
        system_program::{transfer, Transfer},
    },
};

#[derive(Accounts)]
pub struct CloseDraw<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(
        mut,
        close = creator,
        has_one = creator,
        seeds = [Draw::SEED, creator.key().as_ref()],
        bump = draw.bump,
    )]
    pub draw: Account<'info, Draw>,
    #[account(mut, seeds = [VAULT_SEED, draw.key().as_ref()], bump = draw.vault_bump)]
    pub vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// ORAO's request accounts stay behind: ORAO has no instruction to close them,
/// so a fulfilled request keeps its rent.
pub fn handle_close_draw(ctx: Context<CloseDraw>) -> Result<()> {
    let accounts = &ctx.accounts;
    let draw_key = accounts.draw.key();
    let lamports = accounts.vault.lamports();
    transfer(
        CpiContext::new_with_signer(
            accounts.system_program.key(),
            Transfer {
                from: accounts.vault.to_account_info(),
                to: accounts.creator.to_account_info(),
            },
            &[&[VAULT_SEED, draw_key.as_ref(), &[accounts.draw.vault_bump]]],
        ),
        lamports,
    )?;
    msg!("close: returned {} lamports", lamports);
    Ok(())
}

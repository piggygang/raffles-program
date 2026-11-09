use {
    crate::state::{Draw, VAULT_SEED},
    anchor_lang::{
        prelude::*,
        system_program::{transfer, Transfer},
    },
};

#[derive(Accounts)]
pub struct InitDraw<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(
        init,
        payer = creator,
        space = 8 + Draw::INIT_SPACE,
        seeds = [Draw::SEED, creator.key().as_ref()],
        bump,
    )]
    pub draw: Account<'info, Draw>,
    #[account(mut, seeds = [VAULT_SEED, draw.key().as_ref()], bump)]
    pub vault: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// `deposit` has to leave the vault rent-exempt, and the vault has to stay
/// rent-exempt after paying for each request.
pub fn handle_init_draw(ctx: Context<InitDraw>, deposit: u64) -> Result<()> {
    let accounts = &ctx.accounts;
    transfer(
        CpiContext::new(
            accounts.system_program.key(),
            Transfer {
                from: accounts.creator.to_account_info(),
                to: accounts.vault.to_account_info(),
            },
        ),
        deposit,
    )?;

    let draw = Draw {
        creator: accounts.creator.key(),
        requests: 0,
        seed: [0; 32],
        requested_slot: 0,
        consumed_slot: 0,
        value: [0; 64],
        bump: ctx.bumps.draw,
        vault_bump: ctx.bumps.vault,
    };
    ctx.accounts.draw.set_inner(draw);
    Ok(())
}

use {
    crate::{
        error::SpikeError,
        state::{request_seed, Draw, VAULT_SEED},
    },
    anchor_lang::prelude::*,
    orao_solana_vrf::{cpi::accounts::RequestV2, program::OraoVrf, randomness_account_address},
};

#[derive(Accounts)]
pub struct Request<'info> {
    #[account(mut, seeds = [Draw::SEED, draw.creator.as_ref()], bump = draw.bump)]
    pub draw: Account<'info, Draw>,
    #[account(mut, seeds = [VAULT_SEED, draw.key().as_ref()], bump = draw.vault_bump)]
    pub vault: SystemAccount<'info>,
    /// CHECK: ORAO's network state PDA, checked by ORAO.
    #[account(mut)]
    pub network_state: UncheckedAccount<'info>,
    /// CHECK: ORAO checks it against the network state.
    #[account(mut)]
    pub treasury: UncheckedAccount<'info>,
    /// CHECK: the ORAO request for the draw's next seed. ORAO creates it and
    /// checks its address; when it already exists, the handler checks it.
    #[account(mut)]
    pub request: UncheckedAccount<'info>,
    pub vrf: Program<'info, OraoVrf>,
    pub system_program: Program<'info, System>,
}

pub fn handle_request(ctx: Context<Request>) -> Result<()> {
    let accounts = &ctx.accounts;
    let draw_key = accounts.draw.key();
    let seed = request_seed(&draw_key, accounts.draw.requests);
    if accounts.request.data_is_empty() {
        let vault_seeds: &[&[u8]] = &[VAULT_SEED, draw_key.as_ref(), &[accounts.draw.vault_bump]];
        orao_solana_vrf::cpi::request_v2(
            CpiContext::new_with_signer(
                accounts.vrf.key(),
                RequestV2 {
                    payer: accounts.vault.to_account_info(),
                    network_state: accounts.network_state.to_account_info(),
                    treasury: accounts.treasury.to_account_info(),
                    request: accounts.request.to_account_info(),
                    system_program: accounts.system_program.to_account_info(),
                },
                &[vault_seeds],
            ),
            seed,
        )?;
    } else {
        // The seed is predictable, so anyone can request it first. ORAO's
        // randomness depends only on the seed, so the draw adopts that request
        // rather than being blocked by it.
        require_keys_eq!(
            accounts.request.key(),
            randomness_account_address(&orao_solana_vrf::ID, &seed),
            SpikeError::NotOraoRequest
        );
        require_keys_eq!(
            *accounts.request.owner,
            orao_solana_vrf::ID,
            SpikeError::NotOraoRequest
        );
        msg!("request: adopting the existing request for this seed");
    }

    let slot = Clock::get()?.slot;
    msg!("request: #{} in slot {}", accounts.draw.requests, slot);
    let draw = &mut ctx.accounts.draw;
    draw.seed = seed;
    draw.requests = draw.requests.checked_add(1).ok_or(SpikeError::Overflow)?;
    draw.requested_slot = slot;
    draw.consumed_slot = 0;
    Ok(())
}

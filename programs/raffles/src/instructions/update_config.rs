use {
    crate::{
        constants::CONFIG_SEED,
        error::RaffleError,
        events::ConfigUpdated,
        state::{Config, ConfigSettings, ConfigUpdate},
    },
    anchor_lang::prelude::*,
};

#[event_cpi]
#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ RaffleError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

/// Open under a pause, like every instruction of the authority: it is also
/// what lifts one (spec §10).
pub fn handle_update_config(ctx: Context<UpdateConfig>, update: ConfigUpdate) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let settings = config.settings().with(&update);
    settings.validate()?;
    config.set_settings(&settings);

    let authority = config.authority;
    let ConfigSettings {
        paused,
        membership_rules,
        entry_cap,
        full_gang_bonus,
        min_duration,
        max_duration,
    } = config.settings();
    emit_cpi!(ConfigUpdated {
        authority,
        paused,
        membership_rules,
        entry_cap,
        full_gang_bonus,
        min_duration,
        max_duration,
    });
    Ok(())
}

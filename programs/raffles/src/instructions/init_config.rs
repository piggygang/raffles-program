use {
    crate::{
        constants::{CONFIG_SEED, PROGRAM_DATA},
        error::RaffleError,
        events::ConfigInitialized,
        state::{Config, ConfigSettings},
    },
    anchor_lang::prelude::*,
};

#[event_cpi]
#[derive(Accounts)]
pub struct InitConfig<'info> {
    /// The program's upgrade authority. It pays `Config`'s rent and becomes
    /// its authority.
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = Config::SPACE,
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Account<'info, Config>,
    /// Where the upgradeable loader keeps the program's upgrade authority,
    /// which the signer has to be.
    #[account(address = PROGRAM_DATA)]
    pub program_data: Account<'info, ProgramData>,
    pub system_program: Program<'info, System>,
}

pub fn handle_init_config(ctx: Context<InitConfig>, settings: ConfigSettings) -> Result<()> {
    let authority = ctx.accounts.authority.key();
    // Without this gate, whoever saw the deploy first could make itself the
    // authority (registries §6). It is checked here rather than on the
    // account, where it would run before the account's address is checked: a
    // wrong account would then be reported as a wrong signer.
    require!(
        ctx.accounts.program_data.upgrade_authority_address == Some(authority),
        RaffleError::InvalidInitializer
    );
    settings.validate()?;

    ctx.accounts
        .config
        .set_inner(Config::new(ctx.bumps.config, authority, &settings));

    let ConfigSettings {
        paused,
        membership_rules,
        entry_cap,
        full_gang_bonus,
        min_duration,
        max_duration,
    } = ctx.accounts.config.settings();
    emit_cpi!(ConfigInitialized {
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

use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Hello<'info> {
    pub caller: Signer<'info>,
}

pub fn handle_hello(ctx: Context<Hello>) -> Result<()> {
    msg!(
        "Hello from PiggyGang Raffles, {}",
        ctx.accounts.caller.key()
    );
    Ok(())
}

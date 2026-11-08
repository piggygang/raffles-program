use {
    crate::{error::SpikeError, state::Draw},
    anchor_lang::prelude::*,
    orao_solana_vrf::{state::RandomnessV2, RANDOMNESS_ACCOUNT_SEED},
};

#[derive(Accounts)]
pub struct Consume<'info> {
    #[account(mut, seeds = [Draw::SEED, draw.creator.as_ref()], bump = draw.bump)]
    pub draw: Account<'info, Draw>,
    /// The ORAO request for the draw's latest seed.
    #[account(
        seeds = [RANDOMNESS_ACCOUNT_SEED, draw.seed.as_ref()],
        bump,
        seeds::program = orao_solana_vrf::ID,
    )]
    pub request: Account<'info, RandomnessV2>,
}

/// `entries` stands in for a raffle's entry count.
pub fn handle_consume(ctx: Context<Consume>, entries: u64) -> Result<()> {
    let randomness = ctx
        .accounts
        .request
        .fulfilled()
        .ok_or(SpikeError::NotFulfilled)?
        .randomness;
    let winner = winner_index(&randomness, entries)?;
    let slot = Clock::get()?.slot;
    let draw = &mut ctx.accounts.draw;
    msg!(
        "consume: requested in slot {}, consumed in slot {}, winner {} of {}",
        draw.requested_slot,
        slot,
        winner,
        entries
    );
    draw.value = randomness;
    draw.consumed_slot = slot;
    Ok(())
}

/// The spec's draw: the randomness's low 8 bytes, little-endian, modulo the
/// entry count (spec §8; the modulo bias is accepted, E-D8).
pub fn winner_index(randomness: &[u8; 64], entries: u64) -> Result<u64> {
    let mut low = [0; 8];
    low.copy_from_slice(&randomness[..8]);
    u64::from_le_bytes(low)
        .checked_rem(entries)
        .ok_or_else(|| error!(SpikeError::NoEntries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winner_index_reduces_the_low_eight_bytes_little_endian() {
        let mut randomness = [0xff; 64];
        randomness[..8].copy_from_slice(&1_000_003u64.to_le_bytes());
        assert_eq!(winner_index(&randomness, 1_000).unwrap(), 3);
        assert_eq!(winner_index(&randomness, 1).unwrap(), 0);
    }

    #[test]
    fn winner_index_rejects_zero_entries() {
        assert!(winner_index(&[7; 64], 0).is_err());
    }
}

use anchor_lang::prelude::*;

#[error_code]
pub enum RaffleError {
    #[msg("not a membership rule kind the program knows")]
    InvalidRuleKind,
    #[msg("not a prize kind the program knows")]
    InvalidPrizeKind,
    #[msg("not an escrow kind the program knows")]
    InvalidEscrowKind,
    #[msg("not a raffle state the program knows")]
    InvalidRaffleState,
    #[msg("not a cancel reason the program knows")]
    InvalidCancelReason,
    #[msg("the raffle can't move to that state from the one it is in")]
    InvalidTransition,
}

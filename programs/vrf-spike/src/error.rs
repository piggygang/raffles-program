use anchor_lang::prelude::*;

#[error_code]
pub enum SpikeError {
    #[msg("The account at the request address is not an ORAO request")]
    NotOraoRequest,
    #[msg("ORAO has not fulfilled the request yet")]
    NotFulfilled,
    #[msg("A draw needs at least one entry")]
    NoEntries,
    #[msg("Arithmetic overflow")]
    Overflow,
}

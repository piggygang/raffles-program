//! The program's errors. An error's number is 6000 plus its place here, and
//! clients match on the number: add new ones at the end, never reorder or
//! remove one. docs/instructions.md lists them with what returns each; the
//! tests here fail when it and the code disagree.

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
    #[msg("the signer isn't the authority this instruction takes")]
    Unauthorized,
    #[msg("the signer isn't the program's upgrade authority")]
    InvalidInitializer,
    #[msg("no authority transfer is pending")]
    NoPendingAuthority,
    #[msg("the program is paused")]
    Paused,
    #[msg("the entry cap isn't between 1 and the highest the program allows")]
    InvalidEntryCap,
    #[msg("the entry cap and the Full Gang bonus don't fit one entry range")]
    InvalidFullGangBonus,
    #[msg("the shortest duration is zero, or longer than the longest")]
    InvalidDurationBounds,
    #[msg("a membership rule's address is zero")]
    InvalidRuleAddress,
    #[msg("a membership rule's update authority doesn't fit its kind")]
    InvalidRuleUpdateAuthority,
    #[msg("a membership rule's symbol doesn't fit its kind")]
    InvalidRuleSymbol,
    #[msg("two membership rules are the same")]
    DuplicateMembershipRule,
}

#[cfg(test)]
mod tests {
    use {super::*, crate::state::testing::INSTRUCTIONS};

    /// Each error with its message, in the order of the enum. The compiler
    /// checks the names; the numbers are checked against the place in this
    /// list, so an error that moved, or one added anywhere but at the end,
    /// fails here.
    const ERRORS: &[RaffleError] = &[
        RaffleError::InvalidRuleKind,
        RaffleError::InvalidPrizeKind,
        RaffleError::InvalidEscrowKind,
        RaffleError::InvalidRaffleState,
        RaffleError::InvalidCancelReason,
        RaffleError::InvalidTransition,
        RaffleError::Unauthorized,
        RaffleError::InvalidInitializer,
        RaffleError::NoPendingAuthority,
        RaffleError::Paused,
        RaffleError::InvalidEntryCap,
        RaffleError::InvalidFullGangBonus,
        RaffleError::InvalidDurationBounds,
        RaffleError::InvalidRuleAddress,
        RaffleError::InvalidRuleUpdateAuthority,
        RaffleError::InvalidRuleSymbol,
        RaffleError::DuplicateMembershipRule,
    ];

    #[test]
    fn the_document_lists_every_error_with_its_number_and_message() {
        let rows: Vec<String> = ERRORS
            .iter()
            .enumerate()
            .map(|(place, &error)| {
                let number = u32::from(error);
                assert_eq!(
                    number,
                    anchor_lang::error::ERROR_CODE_OFFSET + u32::try_from(place).unwrap(),
                    "{error:?}"
                );
                format!("| {number} | `{error:?}` | {error} |")
            })
            .collect();

        INSTRUCTIONS.assert_table("## 6. Errors", "", &rows);
    }

    /// The numbers the account model's and the registries' errors already
    /// had: a client that matched on one keeps matching.
    #[test]
    fn the_first_errors_keep_their_numbers() {
        assert_eq!(u32::from(RaffleError::InvalidRuleKind), 6000);
        assert_eq!(u32::from(RaffleError::InvalidTransition), 6005);
        assert_eq!(u32::from(RaffleError::Unauthorized), 6006);
        assert_eq!(u32::from(RaffleError::DuplicateMembershipRule), 6016);
    }
}

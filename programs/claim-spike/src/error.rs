use anchor_lang::prelude::*;

#[error_code]
pub enum ClaimError {
    #[msg("claims are paused")]
    Paused,
    #[msg("the raffle isn't taking entries")]
    NotLive,
    #[msg("the creator can't enter its own raffle")]
    CreatorCannotEnter,
    #[msg("no NFTs to claim with")]
    NoNfts,
    #[msg("the raffle has no such membership rule")]
    NoSuchRule,
    #[msg("an NFT's accounts are missing")]
    MissingAccounts,
    #[msg("accounts left over after the last NFT")]
    ExtraAccounts,
    #[msg("not a classic SPL Token account")]
    NotTokenAccount,
    #[msg("the signer doesn't hold this NFT")]
    NotHolder,
    #[msg("not this mint's Token Metadata account")]
    NotMetadata,
    #[msg("not an mpl-core asset")]
    NotCoreAsset,
    #[msg("the NFT doesn't meet its membership rule")]
    RuleMismatch,
    #[msg("not the NFT's receipt address")]
    WrongReceipt,
    #[msg("this NFT is already entered in this raffle")]
    AlreadyEntered,
    #[msg("the wallet is at its entry cap")]
    CapReached,
    #[msg("the position holds no more entry ranges")]
    TooManyRanges,
    #[msg("arithmetic overflow")]
    Overflow,
}

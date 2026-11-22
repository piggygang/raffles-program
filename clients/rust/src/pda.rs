//! The program's addresses: section 2 of docs/account-model.md, the event
//! authority of docs/events.md, and the ProgramData account `init_config`
//! reads. Each returns the address with its bump.
//!
//! The seed prefixes are the IDL's constants. The program's tests check every
//! function here against the program's own.

use {
    crate::{constants, ID},
    anchor_lang::prelude::Pubkey,
};

/// The classic SPL Token program: every token prize is under it.
const TOKEN_PROGRAM: Pubkey = Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ASSOCIATED_TOKEN_PROGRAM: Pubkey =
    Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
/// The loader every upgradeable program is deployed under.
const UPGRADEABLE_LOADER: Pubkey =
    Pubkey::from_str_const("BPFLoaderUpgradeab1e11111111111111111111111");

/// `Config`, the singleton.
pub fn config() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[constants::CONFIG_SEED], &ID)
}

/// The `CreatorEntry` of `wallet`.
pub fn creator_entry(wallet: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[constants::CREATOR_SEED, wallet.as_ref()], &ID)
}

/// The `PrizeEntry` of `address`, of one `PrizeKind`: its byte.
pub fn prize_entry(kind: u8, address: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[constants::PRIZE_SEED, &[kind], address.as_ref()], &ID)
}

/// The `Raffle` that `creator` creates with `nonce`.
pub fn raffle(creator: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            constants::RAFFLE_SEED,
            creator.as_ref(),
            &nonce.to_le_bytes(),
        ],
        &ID,
    )
}

/// The draw-fee vault of `raffle`.
pub fn vault(raffle: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[constants::VAULT_SEED, raffle.as_ref()], &ID)
}

/// The `EntryPosition` of `wallet` in `raffle`.
pub fn entry_position(raffle: &Pubkey, wallet: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[constants::POSITION_SEED, raffle.as_ref(), wallet.as_ref()],
        &ID,
    )
}

/// The `EntryReceipt` of `nft` in `raffle`: `nft` is a Token Metadata mint or
/// a Core asset.
pub fn entry_receipt(raffle: &Pubkey, nft: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[constants::RECEIPT_SEED, raffle.as_ref(), nft.as_ref()],
        &ID,
    )
}

/// The escrow of a token prize: the associated token account of `raffle` for
/// `prize_mint`, under the classic Token program. A Core prize has none: the
/// Raffle owns the asset.
pub fn token_escrow(raffle: &Pubkey, prize_mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[raffle.as_ref(), TOKEN_PROGRAM.as_ref(), prize_mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM,
    )
}

/// The event authority: the one account of every event instruction.
pub fn event_authority() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"__event_authority"], &ID)
}

/// The program's ProgramData account, which holds its upgrade authority:
/// `init_config` takes it to check its signer (docs/instructions.md).
pub fn program_data() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ID.as_ref()], &UPGRADEABLE_LOADER)
}

//! Instruction builders for off-chain callers, `spikes/vrf-driver` and the
//! tests, so the tests exercise exactly what the driver sends.

pub use crate::state::request_seed;
use {
    crate::{accounts, instruction, state::Draw, state::VAULT_SEED},
    anchor_lang::{
        prelude::Pubkey, solana_program::instruction::Instruction, system_program, InstructionData,
        ToAccountMetas,
    },
};

/// The creator's draw. A wallet has one, and it is never closed.
pub fn draw(creator: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[Draw::SEED, creator.as_ref()], &crate::ID).0
}

/// The data-less PDA that holds the draw's lamports and pays ORAO.
pub fn vault(draw: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[VAULT_SEED, draw.as_ref()], &crate::ID).0
}

/// ORAO's configuration: its fee, treasury and fulfillment authorities.
pub fn network_state() -> Pubkey {
    orao_solana_vrf::network_state_account_address(&orao_solana_vrf::ID)
}

/// The ORAO request account for `seed`.
pub fn request(seed: &[u8; 32]) -> Pubkey {
    orao_solana_vrf::randomness_account_address(&orao_solana_vrf::ID, seed)
}

/// Signed by `creator`, who pays the draw's rent and the deposit.
pub fn init_draw(creator: Pubkey, deposit: u64) -> Instruction {
    let draw = draw(&creator);
    Instruction::new_with_bytes(
        crate::ID,
        &instruction::InitDraw { deposit }.data(),
        accounts::InitDraw {
            creator,
            draw,
            vault: vault(&draw),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Needs no signer besides the fee payer. `requests` is the draw's request
/// count before this one, and `treasury` comes from ORAO's network state.
pub fn request_randomness(draw: Pubkey, requests: u32, treasury: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        crate::ID,
        &instruction::Request {}.data(),
        accounts::Request {
            draw,
            vault: vault(&draw),
            network_state: network_state(),
            treasury,
            request: request(&request_seed(&draw, requests)),
            vrf: orao_solana_vrf::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Needs no signer besides the fee payer. `seed` is the draw's latest.
pub fn consume(draw: Pubkey, seed: &[u8; 32], entries: u64) -> Instruction {
    Instruction::new_with_bytes(
        crate::ID,
        &instruction::Consume { entries }.data(),
        accounts::Consume {
            draw,
            request: request(seed),
        }
        .to_account_metas(None),
    )
}

/// Signed by the creator, who gets the vault's lamports back.
pub fn withdraw(creator: Pubkey) -> Instruction {
    let draw = draw(&creator);
    Instruction::new_with_bytes(
        crate::ID,
        &instruction::Withdraw {}.data(),
        accounts::Withdraw {
            creator,
            draw,
            vault: vault(&draw),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

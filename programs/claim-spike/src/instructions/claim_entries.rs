use {
    crate::{
        error::ClaimError,
        proof::{self, Decoder},
        state::{Config, EntryPosition, EntryRange, EntryReceipt, MembershipRule, Raffle},
    },
    anchor_lang::{prelude::*, system_program},
};

#[derive(Accounts)]
pub struct ClaimEntries<'info> {
    /// Holds the NFTs and pays the rent of the receipts and the position.
    #[account(mut)]
    pub wallet: Signer<'info>,
    #[account(
        seeds = [Config::SEED],
        bump = config.bump,
        constraint = !config.paused @ ClaimError::Paused,
    )]
    pub config: Account<'info, Config>,
    #[account(mut, constraint = raffle.creator != wallet.key() @ ClaimError::CreatorCannotEnter)]
    pub raffle: Account<'info, Raffle>,
    #[account(
        init_if_needed,
        payer = wallet,
        space = 8 + EntryPosition::INIT_SPACE,
        seeds = [EntryPosition::SEED, raffle.key().as_ref(), wallet.key().as_ref()],
        bump,
    )]
    pub position: Account<'info, EntryPosition>,
    pub system_program: Program<'info, System>,
}

/// Which `claim_entries` runs: the canonical one, or a variant measured
/// against it.
#[derive(Clone, Copy)]
pub struct Variant {
    /// Each Token Metadata pig's mint comes before its token account.
    pub mints: bool,
    pub decoder: Decoder,
    /// Logs the compute units left before the NFTs, after each NFT's proof
    /// and receipt, and at the end.
    pub trace: bool,
}

impl Variant {
    pub const CANONICAL: Self = Self {
        mints: false,
        decoder: Decoder::Prefix,
        trace: false,
    };
}

/// Proves each NFT against its membership rule and creates its receipt, then
/// counts the entries against the cap and awards the Full Gang bonus.
pub fn handle_claim_entries<'info>(
    ctx: Context<'info, ClaimEntries<'info>>,
    rule_indexes: &[u8],
    variant: Variant,
) -> Result<()> {
    let accounts = &mut *ctx.accounts;
    let raffle_key = accounts.raffle.key();
    let wallet_key = accounts.wallet.key();
    require!(
        accounts.raffle.state == Raffle::ACTIVE
            && Clock::get()?.unix_timestamp < accounts.raffle.end_ts,
        ClaimError::NotLive
    );
    require!(!rule_indexes.is_empty(), ClaimError::NoNfts);
    // `init_if_needed` hands over a new position zeroed.
    let position = &mut accounts.position;
    if position.wallet == Pubkey::default() {
        position.raffle = raffle_key;
        position.wallet = wallet_key;
        position.bump = ctx.bumps.position;
    }

    let receipt_space = 8 + EntryReceipt::INIT_SPACE;
    let receipt_rent = Rent::get()?.minimum_balance(receipt_space);
    let mut remaining = ctx.remaining_accounts.iter();
    let mut next = || {
        remaining
            .next()
            .ok_or_else(|| error!(ClaimError::MissingAccounts))
    };
    let mut entries: u8 = 0;
    trace(variant);
    for &rule_index in rule_indexes {
        let rule = accounts
            .raffle
            .membership_rules
            .get(usize::from(rule_index))
            .ok_or(ClaimError::NoSuchRule)?;
        let nft = match rule.kind {
            MembershipRule::TM_CREATOR => {
                let mint = if variant.mints { Some(next()?) } else { None };
                let (token, metadata) = (next()?, next()?);
                let nft = proof::check_token_metadata(
                    rule,
                    &wallet_key,
                    token,
                    metadata,
                    variant.decoder,
                )?;
                if let Some(mint) = mint {
                    require_keys_eq!(mint.key(), nft, ClaimError::NotMetadata);
                }
                nft
            }
            MembershipRule::CORE_COLLECTION => {
                let asset = next()?;
                proof::check_core(rule, &wallet_key, asset)?;
                asset.key()
            }
            _ => return err!(ClaimError::NoSuchRule),
        };
        trace(variant);
        create_receipt(
            &accounts.wallet,
            next()?,
            &raffle_key,
            &nft,
            receipt_space,
            receipt_rent,
        )?;
        trace(variant);

        // At the cap, only an NFT that adds a collection still counts: it
        // earns no entry but can complete the Full Gang set (E-E16).
        let bit = 1 << rule_index;
        if position.entries_claimed < accounts.raffle.entry_cap {
            position.entries_claimed += 1;
            entries += 1;
        } else {
            require!(position.collections_seen & bit == 0, ClaimError::CapReached);
        }
        position.collections_seen |= bit;
    }
    require!(next().is_err(), ClaimError::ExtraAccounts);

    if position.collections_seen == EntryPosition::FULL_GANG && !position.bonus_awarded {
        position.bonus_awarded = true;
        entries = entries
            .checked_add(accounts.raffle.full_gang_bonus)
            .ok_or(ClaimError::Overflow)?;
    }
    if entries > 0 {
        let raffle = &mut accounts.raffle;
        let index = usize::from(position.range_count);
        *position
            .ranges
            .get_mut(index)
            .ok_or(ClaimError::TooManyRanges)? = EntryRange {
            start: raffle.total_entries,
            count: entries,
        };
        position.range_count += 1;
        raffle.total_entries = raffle
            .total_entries
            .checked_add(u32::from(entries))
            .ok_or(ClaimError::Overflow)?;
    }
    trace(variant);
    Ok(())
}

/// Creates the receipt at `[b"receipt", raffle, nft]`. The bump has to be the
/// canonical one, or a second bump would admit the NFT twice, so it is found
/// here rather than taken from the client. A receipt that already exists
/// fails the claim, and lamports someone sent to the address first don't:
/// `create_account_allow_prefund` (SIMD-0312) tops them up.
fn create_receipt<'info>(
    wallet: &Signer<'info>,
    receipt: &AccountInfo<'info>,
    raffle: &Pubkey,
    nft: &Pubkey,
    space: usize,
    rent: u64,
) -> Result<()> {
    let (address, bump) = Pubkey::find_program_address(
        &[EntryReceipt::SEED, raffle.as_ref(), nft.as_ref()],
        &crate::ID,
    );
    require_keys_eq!(receipt.key(), address, ClaimError::WrongReceipt);
    require_keys_eq!(
        *receipt.owner,
        system_program::ID,
        ClaimError::AlreadyEntered
    );
    system_program::create_account_allow_prefund(
        CpiContext::new_with_signer(
            system_program::ID,
            system_program::CreateAccountAllowPrefund {
                from: wallet.to_account_info(),
                to: receipt.clone(),
            },
            &[&[EntryReceipt::SEED, raffle.as_ref(), nft.as_ref(), &[bump]]],
        ),
        rent.saturating_sub(receipt.lamports()),
        space as u64,
        &crate::ID,
    )?;
    EntryReceipt {
        raffle: *raffle,
        nft: *nft,
        wallet: wallet.key(),
        bump,
    }
    .try_serialize(&mut &mut receipt.try_borrow_mut_data()?[..])
}

/// Logs the compute units left, in the traced variant only.
#[cfg(target_os = "solana")]
fn trace(variant: Variant) {
    if variant.trace {
        // SAFETY: the syscall takes no arguments and only writes a log line.
        unsafe { anchor_lang::compat::solana_define_syscall::definitions::sol_log_compute_units_() }
    }
}

#[cfg(not(target_os = "solana"))]
fn trace(_: Variant) {}

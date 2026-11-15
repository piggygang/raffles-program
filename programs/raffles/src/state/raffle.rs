use {
    super::MembershipRule,
    crate::{
        constants::{
            CLAIM_WINDOW_SECS, CLOSE_GRACE_SECS, MAX_RULES_URI_LEN, MAX_TITLE_LEN,
            MEMBERSHIP_RULE_COUNT, RAFFLE_SEED, VAULT_SEED,
        },
        error::RaffleError,
    },
    anchor_lang::prelude::*,
    anchor_spl::associated_token::get_associated_token_address,
};

byte_enum! {
    /// Where a raffle is in its life (spec §7).
    ///
    /// The spec's "Ended" isn't a value: no instruction ends a raffle. It has
    /// ended when it is still Active and the clock has passed `end_ts`
    /// (`Raffle::has_ended`).
    pub enum RaffleState, InvalidRaffleState {
        /// Created, with the prize in escrow. Entries can be claimed until
        /// `end_ts`.
        Active = 0,
        /// Randomness is requested. The entries are final.
        Drawing = 1,
        /// The winner is recorded and can claim the prize.
        Drawn = 2,
        /// The prize left escrow, to the winner or back to the creator.
        Settled = 3,
        /// The admin cancelled the raffle. Its entries are void.
        Cancelled = 4,
    }
}

impl RaffleState {
    /// Whether the state machine has an edge from this state to `next`. These
    /// six are all of them (spec §7), so state only ever moves forward.
    pub fn can_become(self, next: Self) -> bool {
        matches!(
            (self, next),
            // request_draw: ended, with at least one entry.
            (Self::Active, Self::Drawing)
                // reclaim_prize: ended, with no entry.
                | (Self::Active, Self::Settled)
                // cancel_raffle: before the draw is requested.
                | (Self::Active, Self::Cancelled)
                // The randomness arrived and the winner is recorded.
                | (Self::Drawing, Self::Drawn)
                // cancel_raffle, break-glass: stuck in Drawing.
                | (Self::Drawing, Self::Cancelled)
                // claim_prize, or reclaim_prize after the claim window.
                | (Self::Drawn, Self::Settled)
        )
    }

    /// Settled and Cancelled: the state never changes again, and the entry
    /// accounts can close.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Settled | Self::Cancelled)
    }
}

byte_enum! {
    /// How the Raffle PDA holds the prize.
    pub enum EscrowKind, InvalidEscrowKind {
        /// In its associated token account for `prize_mint`
        /// (`Raffle::escrow_address`).
        Token = 0,
        /// As the owner of the Core asset `prize_mint` names (spec D22).
        Core = 1,
    }
}

byte_enum! {
    /// Why the admin cancelled a raffle (spec E-A1).
    pub enum CancelReason, InvalidCancelReason {
        /// The raffle isn't cancelled.
        NotCancelled = 0,
        /// The prize isn't what the rules promise, or can't be given away.
        PrizeDefect = 1,
        /// The published rules misstate the raffle (spec E-I7).
        RulesDefect = 2,
        /// The entries can't be trusted, for example because a membership
        /// rule's key leaked (holder-proof §4).
        EntriesCompromised = 3,
        /// The randomness never arrived: the break-glass cancel (spec E-D5).
        DrawStuck = 4,
        /// None of the above.
        Other = 255,
    }
}

/// One raffle, at `[RAFFLE_SEED, creator, nonce]`.
///
/// The creator pays its rent and gets it back from `close_raffle`. The Raffle
/// PDA is also the escrow's authority: it signs the prize out to the winner,
/// or back to the creator, and to nobody else.
#[account]
#[derive(Debug, PartialEq, Eq, InitSpace)]
pub struct Raffle {
    pub bump: u8,
    /// Bump of the draw-fee vault, `[VAULT_SEED, raffle]`.
    pub vault_bump: u8,
    /// Created the raffle and escrowed the prize; also a seed. A reclaimed
    /// prize, the vault's remainder and this account's rent go here and
    /// nowhere else. It can't enter its own raffle.
    pub creator: Pubkey,
    /// Chosen by the creator; also a seed. The address is therefore known
    /// before the raffle exists, so the rules document can name it. The GM
    /// bot derives the nonce from the period, so a second raffle for a
    /// period can't be created while the first exists.
    pub nonce: u64,
    /// A [`RaffleState`].
    pub state: u8,
    /// A [`CancelReason`].
    pub cancel_reason: u8,
    /// An [`EscrowKind`].
    pub escrow_kind: u8,
    /// Set once the prize has left escrow, by `claim_prize` or
    /// `reclaim_prize`. `close_raffle` requires it: nothing could sign a Core
    /// asset out of a closed Raffle (spec D22).
    pub prize_out: bool,
    /// The prize: a classic SPL Token mint, or the Core asset itself.
    pub prize_mint: Pubkey,
    /// Base units escrowed at create: 1 for an NFT or a Core asset.
    pub prize_amount: u64,
    /// Cluster time of `create_raffle`.
    pub start_ts: i64,
    /// Cluster time at which entries close and the draw opens. It never
    /// moves, not even under a pause.
    pub end_ts: i64,
    /// Entries handed out so far, bonuses included. The next claim's range
    /// starts here, and the draw picks an index below it.
    pub total_entries: u32,
    /// `EntryPosition`s and `EntryReceipt`s that aren't closed yet.
    /// `close_raffle` requires zero: closing one needs this account (spec
    /// E-S9).
    pub open_entry_accounts: u32,
    /// Lamports the creator put into the vault at create, to pay for the
    /// draw (spec R14).
    pub draw_fee_lamports: u64,
    /// Cluster time of the first `request_draw`, when the raffle entered
    /// Drawing; zero before. The break-glass cancel counts from it.
    pub draw_requested_at: i64,
    /// Randomness requests so far, re-requests included.
    pub vrf_requests: u32,
    /// Slot of the latest request. The retry timeout counts from it.
    pub vrf_requested_slot: u64,
    /// Seed of the latest request, the only one the draw accepts; zero
    /// before the first.
    pub vrf_seed: [u8; 32],
    /// The randomness the winner was drawn from; zero until Drawn.
    pub vrf_randomness: [u8; 64],
    /// The winning entry's index, below `total_entries`. Set when Drawn.
    pub winner_index: u32,
    /// The wallet of the `EntryPosition` that holds `winner_index`, and the
    /// only one `claim_prize` pays. Zero until Drawn, and for good when a
    /// raffle ends without a draw.
    pub winner: Pubkey,
    /// Cluster time at which the raffle became Drawn; zero before. The claim
    /// window and the backstop count from it, not from `end_ts`: a draw can
    /// finish long after the raffle ended.
    pub drawn_at: i64,
    /// `Config::membership_rules` at create (spec D14). Claims are checked
    /// against these, never against `Config`.
    pub membership_rules: [MembershipRule; MEMBERSHIP_RULE_COUNT],
    /// The cap this raffle applies: `Config::entry_cap` at create, or the
    /// creator's lower override (spec D6).
    pub entry_cap: u8,
    /// `Config::full_gang_bonus` at create.
    pub full_gang_bonus: u8,
    /// `Config::min_duration` at create: the bound the raffle was checked
    /// against.
    pub min_duration: u32,
    /// `Config::max_duration` at create.
    pub max_duration: u32,
    /// Zero. Room for a later field on raffles that are already running.
    pub _reserved: [u8; 32],
    /// sha256 of the rules document's exact bytes (spec D20). Never zero.
    pub rules_hash: [u8; 32],
    /// Where the rules document is published, `ar://<id>` today: 1 to
    /// `MAX_RULES_URI_LEN` bytes.
    #[max_len(MAX_RULES_URI_LEN)]
    pub rules_uri: String,
    /// The title, as the rules document states it: at most `MAX_TITLE_LEN`
    /// bytes.
    #[max_len(MAX_TITLE_LEN)]
    pub title: String,
}

impl Raffle {
    pub const SPACE: usize = 8 + Self::INIT_SPACE;

    pub fn find_address(creator: &Pubkey, nonce: u64) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[RAFFLE_SEED, creator.as_ref(), &nonce.to_le_bytes()],
            &crate::ID,
        )
    }

    /// The draw-fee vault of the raffle at `raffle`: a system account without
    /// data, so that it can pay the VRF provider by CPI. It holds
    /// `draw_fee_lamports` from create, takes the provider's refunds, and
    /// `close_raffle` returns what is left to the creator.
    pub fn find_vault_address(raffle: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[VAULT_SEED, raffle.as_ref()], &crate::ID)
    }

    /// The escrow of a token prize: the associated token account of the
    /// Raffle PDA at `raffle` for `prize_mint`, under the classic Token
    /// program. A Core prize has none: the Raffle PDA owns the asset.
    pub fn escrow_address(raffle: &Pubkey, prize_mint: &Pubkey) -> Pubkey {
        get_associated_token_address(raffle, prize_mint)
    }

    pub fn state(&self) -> Result<RaffleState> {
        RaffleState::try_from(self.state)
    }

    pub fn escrow_kind(&self) -> Result<EscrowKind> {
        EscrowKind::try_from(self.escrow_kind)
    }

    pub fn cancel_reason(&self) -> Result<CancelReason> {
        CancelReason::try_from(self.cancel_reason)
    }

    /// Moves the raffle to `next`, if the state machine has that edge. Every
    /// state change goes through here.
    pub fn transition(&mut self, next: RaffleState) -> Result<()> {
        require!(
            self.state()?.can_become(next),
            RaffleError::InvalidTransition
        );
        self.state = next.into();
        Ok(())
    }

    /// Entries can be claimed: the raffle is Active and `now` is before
    /// `end_ts`.
    pub fn is_live(&self, now: i64) -> bool {
        self.state == u8::from(RaffleState::Active) && now < self.end_ts
    }

    /// The spec's Ended: still Active, with `end_ts` reached. A draw can be
    /// requested, or with no entries the prize reclaimed.
    pub fn has_ended(&self, now: i64) -> bool {
        self.state == u8::from(RaffleState::Active) && now >= self.end_ts
    }

    /// When the winner's claim window closes and the prize can be reclaimed
    /// instead.
    ///
    /// `None` unless the raffle is Drawn, so a comparison with the clock can
    /// only hold for a raffle that has a winner and still has its prize. In
    /// any other state `drawn_at` is zero or the window no longer matters.
    pub fn claim_expires_at(&self) -> Option<i64> {
        (self.state == u8::from(RaffleState::Drawn))
            .then(|| self.drawn_at.saturating_add(CLAIM_WINDOW_SECS))
    }

    /// When a raffle still Drawn lets its entry accounts close whatever
    /// happened to the prize (spec E-S8). `None` unless the raffle is Drawn,
    /// like `claim_expires_at`.
    pub fn backstop_at(&self) -> Option<i64> {
        self.claim_expires_at()
            .map(|expiry| expiry.saturating_add(CLOSE_GRACE_SECS))
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::state::{
            testing::{assert_enum, assert_error, assert_layout, assert_table, field},
            RuleKind,
        },
    };

    const DAY: i64 = 86_400;

    /// A raffle with a different value in every field, and both strings at
    /// their longest.
    fn raffle() -> Raffle {
        Raffle {
            bump: 253,
            vault_bump: 254,
            creator: Pubkey::new_from_array([1; 32]),
            nonce: 0x0807_0605_0403_0201,
            state: RaffleState::Drawn.into(),
            cancel_reason: CancelReason::Other.into(),
            escrow_kind: EscrowKind::Core.into(),
            prize_out: false,
            prize_mint: Pubkey::new_from_array([2; 32]),
            prize_amount: 500_000_000,
            start_ts: 1_794_000_000,
            end_ts: 1_794_604_800,
            total_entries: 3_400,
            open_entry_accounts: 2_750,
            draw_fee_lamports: 15_515_720,
            draw_requested_at: 1_794_604_830,
            vrf_requests: 2,
            vrf_requested_slot: 512_345_678,
            vrf_seed: [3; 32],
            vrf_randomness: [4; 64],
            winner_index: 1_234,
            winner: Pubkey::new_from_array([5; 32]),
            drawn_at: 1_794_604_860,
            membership_rules: [
                MembershipRule {
                    kind: RuleKind::TmCreator.into(),
                    address: Pubkey::new_from_array([6; 32]),
                    update_authority: Pubkey::new_from_array([7; 32]),
                    symbol: *b"PSG\0\0\0\0\0\0\0",
                },
                MembershipRule {
                    kind: RuleKind::TmCreator.into(),
                    address: Pubkey::new_from_array([6; 32]),
                    update_authority: Pubkey::new_from_array([7; 32]),
                    symbol: *b"PGG\0\0\0\0\0\0\0",
                },
                MembershipRule {
                    kind: RuleKind::CoreCollection.into(),
                    address: Pubkey::new_from_array([8; 32]),
                    ..MembershipRule::default()
                },
            ],
            entry_cap: 10,
            full_gang_bonus: 3,
            min_duration: 3_600,
            max_duration: 2_592_000,
            _reserved: [0; 32],
            rules_hash: [9; 32],
            rules_uri: "u".repeat(usize::from(MAX_RULES_URI_LEN)),
            title: "t".repeat(usize::from(MAX_TITLE_LEN)),
        }
    }

    /// A raffle in `state` that ends at `end_ts`.
    fn raffle_in(state: RaffleState, end_ts: i64) -> Raffle {
        Raffle {
            state: state.into(),
            end_ts,
            ..raffle()
        }
    }

    #[test]
    fn raffle_serializes_to_its_documented_layout() {
        let raffle = raffle();

        assert_layout(
            "Raffle",
            &raffle,
            &[
                field!(raffle.bump: u8),
                field!(raffle.vault_bump: u8),
                field!(raffle.creator: Pubkey),
                field!(raffle.nonce: u64),
                field!(raffle.state: u8),
                field!(raffle.cancel_reason: u8),
                field!(raffle.escrow_kind: u8),
                field!(raffle.prize_out: bool),
                field!(raffle.prize_mint: Pubkey),
                field!(raffle.prize_amount: u64),
                field!(raffle.start_ts: i64),
                field!(raffle.end_ts: i64),
                field!(raffle.total_entries: u32),
                field!(raffle.open_entry_accounts: u32),
                field!(raffle.draw_fee_lamports: u64),
                field!(raffle.draw_requested_at: i64),
                field!(raffle.vrf_requests: u32),
                field!(raffle.vrf_requested_slot: u64),
                field!(raffle.vrf_seed: [u8; 32]),
                field!(raffle.vrf_randomness: [u8; 64]),
                field!(raffle.winner_index: u32),
                field!(raffle.winner: Pubkey),
                field!(raffle.drawn_at: i64),
                field!(raffle.membership_rules: [MembershipRule; 3]),
                field!(raffle.entry_cap: u8),
                field!(raffle.full_gang_bonus: u8),
                field!(raffle.min_duration: u32),
                field!(raffle.max_duration: u32),
                field!(raffle._reserved: [u8; 32]),
                field!(raffle.rules_hash: [u8; 32]),
                field!(raffle.rules_uri: String),
                field!(raffle.title: String),
            ],
            Raffle::SPACE,
        );
    }

    #[test]
    fn a_raffle_with_shorter_strings_reads_back_from_a_full_size_account() {
        let raffle = Raffle {
            rules_uri: "ar://bNbA3TEQVL60xlgCcqdz4ZPHFZ711cZ3hmkpGttDt_U".into(),
            title: "Piggy Gang #4521".into(),
            ..raffle()
        };
        // The account is allocated at `SPACE`, so zero bytes follow the data.
        let mut data = Vec::new();
        raffle.try_serialize(&mut data).unwrap();
        assert!(data.len() < Raffle::SPACE);
        data.resize(Raffle::SPACE, 0);

        assert_eq!(Raffle::try_deserialize(&mut &data[..]).unwrap(), raffle);
    }

    #[test]
    fn raffle_state_bytes_are_stable() {
        assert_enum("RaffleState", RaffleState::ALL, &[0, 1, 2, 3, 4]);
    }

    #[test]
    fn escrow_kind_bytes_are_stable() {
        assert_enum("EscrowKind", EscrowKind::ALL, &[0, 1]);
    }

    #[test]
    fn cancel_reason_bytes_are_stable() {
        assert_enum("CancelReason", CancelReason::ALL, &[0, 1, 2, 3, 4, 255]);
    }

    #[test]
    fn a_raffle_reads_its_bytes_as_enums_and_rejects_unknown_ones() {
        let mut raffle = raffle();
        assert_eq!(raffle.state().unwrap(), RaffleState::Drawn);
        assert_eq!(raffle.escrow_kind().unwrap(), EscrowKind::Core);
        assert_eq!(raffle.cancel_reason().unwrap(), CancelReason::Other);

        raffle.state = 5;
        raffle.escrow_kind = 2;
        raffle.cancel_reason = 5;
        assert_error(raffle.state(), RaffleError::InvalidRaffleState);
        assert_error(raffle.escrow_kind(), RaffleError::InvalidEscrowKind);
        assert_error(raffle.cancel_reason(), RaffleError::InvalidCancelReason);
    }

    #[test]
    fn the_state_machine_has_exactly_the_edges_of_the_spec() {
        use RaffleState::{Active, Cancelled, Drawing, Drawn, Settled};
        let edges = [
            (Active, Drawing),
            (Active, Settled),
            (Active, Cancelled),
            (Drawing, Drawn),
            (Drawing, Cancelled),
            (Drawn, Settled),
        ];

        for &from in RaffleState::ALL {
            for &to in RaffleState::ALL {
                let edge = edges.contains(&(from, to));
                assert_eq!(from.can_become(to), edge, "{from:?} -> {to:?}");
            }
        }
        // The document's table of state changes is these edges and no others.
        let rows: Vec<String> = edges
            .iter()
            .map(|(from, to)| format!("| {from:?} → {to:?} |"))
            .collect();
        assert_table("### Changes of state", "", &rows);
    }

    #[test]
    fn terminal_states_have_no_way_out() {
        for &state in RaffleState::ALL {
            let has_exit = RaffleState::ALL.iter().any(|&next| state.can_become(next));
            assert_eq!(state.is_terminal(), !has_exit, "{state:?}");
        }
        assert!(RaffleState::Settled.is_terminal());
        assert!(RaffleState::Cancelled.is_terminal());
    }

    #[test]
    fn transition_follows_an_edge_and_refuses_anything_else() {
        let mut raffle = raffle_in(RaffleState::Active, 0);

        raffle.transition(RaffleState::Drawing).unwrap();
        raffle.transition(RaffleState::Drawn).unwrap();
        assert_eq!(raffle.state().unwrap(), RaffleState::Drawn);

        // Not backwards, not to itself, and no cancel once a winner is drawn.
        for next in [
            RaffleState::Active,
            RaffleState::Drawing,
            RaffleState::Drawn,
            RaffleState::Cancelled,
        ] {
            assert_error(raffle.transition(next), RaffleError::InvalidTransition);
            assert_eq!(raffle.state().unwrap(), RaffleState::Drawn);
        }

        raffle.transition(RaffleState::Settled).unwrap();
        for &next in RaffleState::ALL {
            assert_error(raffle.transition(next), RaffleError::InvalidTransition);
        }
    }

    #[test]
    fn transition_refuses_a_state_byte_it_does_not_know() {
        let mut raffle = raffle();
        raffle.state = 9;

        assert_error(
            raffle.transition(RaffleState::Settled),
            RaffleError::InvalidRaffleState,
        );
        assert_eq!(raffle.state, 9);
    }

    #[test]
    fn an_active_raffle_is_live_until_end_ts_and_ended_from_it() {
        let raffle = raffle_in(RaffleState::Active, 1_000);

        assert!(raffle.is_live(999));
        assert!(!raffle.has_ended(999));
        // `end_ts` itself is already too late to enter.
        assert!(!raffle.is_live(1_000));
        assert!(raffle.has_ended(1_000));
        assert!(raffle.has_ended(1_000 + 365 * DAY));
    }

    #[test]
    fn only_an_active_raffle_is_live_or_ended() {
        for &state in RaffleState::ALL {
            if state == RaffleState::Active {
                continue;
            }
            let raffle = raffle_in(state, 1_000);

            assert!(!raffle.is_live(999), "{state:?}");
            assert!(!raffle.has_ended(1_000), "{state:?}");
        }
    }

    #[test]
    fn the_claim_window_and_the_backstop_count_from_drawn_at() {
        let raffle = Raffle {
            // The draw finished a week after the raffle ended.
            drawn_at: 1_000 + 7 * DAY,
            ..raffle_in(RaffleState::Drawn, 1_000)
        };

        assert_eq!(raffle.claim_expires_at(), Some(1_000 + 37 * DAY));
        assert_eq!(raffle.backstop_at(), Some(1_000 + 67 * DAY));
    }

    /// With `drawn_at` still zero, a deadline computed anyway would lie in
    /// 1970: every clock would be past it, and a guard written as
    /// `now >= deadline` would open on raffles that were never drawn.
    #[test]
    fn only_a_drawn_raffle_has_a_claim_window_or_a_backstop() {
        for &state in RaffleState::ALL {
            let never_drawn = Raffle {
                drawn_at: 0,
                ..raffle_in(state, 1_000)
            };
            let drawn_before = Raffle {
                drawn_at: 2_000,
                ..raffle_in(state, 1_000)
            };

            for raffle in [never_drawn, drawn_before] {
                let is_drawn = state == RaffleState::Drawn;
                assert_eq!(raffle.claim_expires_at().is_some(), is_drawn, "{state:?}");
                assert_eq!(raffle.backstop_at().is_some(), is_drawn, "{state:?}");
            }
        }
    }

    #[test]
    fn the_deadlines_saturate_instead_of_overflowing() {
        let raffle = Raffle {
            drawn_at: i64::MAX - 1,
            ..raffle_in(RaffleState::Drawn, 1_000)
        };

        assert_eq!(raffle.claim_expires_at(), Some(i64::MAX));
        assert_eq!(raffle.backstop_at(), Some(i64::MAX));
    }

    #[test]
    fn raffle_address_derives_from_the_creator_and_the_nonce() {
        let creator = Pubkey::new_from_array([7; 32]);

        assert_eq!(
            Raffle::find_address(&creator, 0x0102),
            Pubkey::find_program_address(
                &[b"raffle", creator.as_ref(), &[2, 1, 0, 0, 0, 0, 0, 0]],
                &crate::ID
            )
        );
        assert_ne!(
            Raffle::find_address(&creator, 1).0,
            Raffle::find_address(&creator, 2).0
        );
    }

    #[test]
    fn vault_address_derives_from_the_raffle() {
        let raffle = Pubkey::new_from_array([7; 32]);

        assert_eq!(
            Raffle::find_vault_address(&raffle),
            Pubkey::find_program_address(&[b"vault", raffle.as_ref()], &crate::ID)
        );
    }

    #[test]
    fn escrow_is_the_raffles_associated_token_account() {
        let raffle = Pubkey::new_from_array([7; 32]);
        let mint = Pubkey::new_from_array([8; 32]);

        assert_eq!(
            Raffle::escrow_address(&raffle, &mint),
            Pubkey::find_program_address(
                &[
                    raffle.as_ref(),
                    anchor_spl::token::ID.as_ref(),
                    mint.as_ref()
                ],
                &anchor_spl::associated_token::ID
            )
            .0
        );
    }
}

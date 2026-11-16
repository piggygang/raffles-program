//! The interface of the PiggyGang Raffles program at one release: its IDL,
//! and the Rust client Anchor generates from it.
//!
//! Depend on it by the release's tag, and bump the tag to move to another
//! release:
//!
//! ```toml
//! raffles-client = { git = "https://github.com/piggygang/raffles-program", tag = "v0.1.0" }
//! ```
//!
//! - [`IDL`] and [`VERSION`] need no dependency: take them alone with
//!   `default-features = false`.
//! - The `anchor` feature, on by default, adds what `declare_program!` makes
//!   of the IDL: [`accounts`], [`events`] and [`types`] with their Borsh
//!   layouts, [`constants`], [`error`], the instruction builders in
//!   [`client`], and [`parsers`] for accounts, events and instructions. It
//!   also adds the program's addresses in [`pda`], and [`read_event`].
//!
//! Nothing here is written by hand except [`pda`] and [`read_event`], and the
//! program's tests hold those to the program. So the crate can't describe an
//! interface other than the one its release has.

/// The release this crate belongs to: its tag without the `v`, and the IDL's
/// `metadata.version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The IDL of record, byte for byte: `idls/raffles.json` at this release,
/// which is also the release's `raffles.json`.
pub const IDL: &str = include_str!("../../../idls/raffles.json");

/// The Anchor these types are built with, for `Pubkey` and the `Event` and
/// `InstructionData` traits: a consumer needs no `anchor-lang` of its own,
/// which could be another version.
///
/// `declare_program!` also names the crate as `super::anchor_lang`, so it has
/// to be an item of this module.
#[cfg(feature = "anchor")]
pub use anchor_lang;

#[cfg(feature = "anchor")]
anchor_lang::declare_program!(raffles);

#[cfg(feature = "anchor")]
pub use raffles::*;

#[cfg(feature = "anchor")]
pub mod pda;

/// The eight bytes an event instruction's data starts with: Anchor's tag
/// (docs/events.md, section 1).
#[cfg(feature = "anchor")]
pub const EVENT_TAG: &[u8] = anchor_lang::event::EVENT_IX_TAG_LE;

/// Reads the event out of the data of one of the program's inner
/// instructions.
///
/// - `None`: the instruction isn't an event.
/// - `Some(Err(_))`: it is an event, but none this release knows. A later
///   release emitted it.
///
/// The caller has checked what the data can't show: that the instruction is
/// the raffles program's, and that its transaction succeeded.
#[cfg(all(feature = "anchor", not(target_os = "solana")))]
pub fn read_event(instruction_data: &[u8]) -> Option<anchor_lang::Result<parsers::Event>> {
    instruction_data
        .strip_prefix(EVENT_TAG)
        .map(parsers::Event::parse)
}

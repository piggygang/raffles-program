//! The spike program on LiteSVM, against pigs cloned from mainnet accounts:
//! `claim` checks what it accepts and rejects, `budget` measures it, and
//! `stand_ins` runs the devnet fixture set (PIG-115) through the rule.

// Test code may panic; the workspace lints are for program code. The harness
// passes on LiteSVM's own `Result`, whose error variant is large.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::result_large_err
)]

mod budget;
mod claim;
mod harness;
mod stand_ins;

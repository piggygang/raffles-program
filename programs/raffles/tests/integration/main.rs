//! Integration tests on LiteSVM: the program `anchor build` produced, next to
//! Token Metadata and mpl-core dumped from mainnet into `tests/fixtures`.
//!
//! One test binary with modules, so the suite links LiteSVM once.

// Test code may panic; the workspace lints are for program code.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod harness;
mod hello;
mod metaplex;

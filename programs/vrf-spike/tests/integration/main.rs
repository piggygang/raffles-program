//! The spike program against the devnet ORAO VRF program on LiteSVM. Test keys
//! stand in for ORAO's fulfillment authorities, so the whole request, fulfill
//! and consume path runs for real.

// Test code may panic; the workspace lints are for program code. The harness
// passes on LiteSVM's own `Result`, whose error variant is large.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::result_large_err
)]

mod draw;
mod harness;

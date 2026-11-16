//! The crate without a feature: the IDL and its version, and nothing else.

// Test code may panic; the lints are for the crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use raffles_client::{IDL, VERSION};

#[test]
fn the_idl_is_this_release_of_the_raffles_program() {
    let idl: serde_json::Value = serde_json::from_str(IDL).unwrap();

    assert_eq!(idl["metadata"]["name"], "raffles");
    assert_eq!(idl["metadata"]["version"], VERSION);
}

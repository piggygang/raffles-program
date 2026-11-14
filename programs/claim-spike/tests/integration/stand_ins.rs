//! The devnet fixture set (PIG-115), as `scripts/fixtures` builds it with the
//! real Token Metadata and mpl-core programs: each stand-in passes its
//! membership rule, and every look-alike fails it. The accounts come from
//! `scripts/fixtures/localnet.sh`; devnet's have the same structure, from
//! another seed.

use {
    crate::harness::{failed_with, load_fixture, Test},
    anchor_lang::prelude::Pubkey,
    claim_spike::{proof::MPL_CORE_ID, state::MembershipRule},
    serde_json::Value,
};

const CASES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/stand-ins/cases.json"
);

#[test]
fn stand_ins_pass_the_rule_and_lookalikes_fail_it() {
    let cases: Value = serde_json::from_str(&std::fs::read_to_string(CASES).unwrap()).unwrap();
    let rules = membership_rules(&cases["membership_rules"]);
    let cases = cases["cases"].as_array().unwrap();
    let rejected = cases
        .iter()
        .filter(|case| case["accepted"] == false)
        .count();
    assert!(rejected >= 8, "only {rejected} cases the rule rejects");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let rule_index = u8::try_from(case["rule_slot"].as_u64().unwrap()).unwrap();
        let account = load_fixture(&format!("stand-ins/{}", case["file"].as_str().unwrap()));
        let mut test = Test::new();
        let creator = test.key();
        test.raffle = test.create_raffle_with(creator, 10, rules);
        let pig = if account.owner == MPL_CORE_ID {
            test.core_pig_from(account, rule_index)
        } else {
            test.tm_pig_from(account, rule_index)
        };
        match (case["accepted"].as_bool().unwrap(), test.claim(&[pig])) {
            (true, Ok(_)) => {}
            (true, Err(failed)) => panic!("{name} should pass: {}", failed.meta.pretty_logs()),
            (false, Ok(_)) => panic!("{name} should fail the rule"),
            (false, result) => assert!(failed_with(result, "RuleMismatch"), "{name}"),
        }
    }
}

/// The fixture set's rules, in the manifest's form.
fn membership_rules(json: &Value) -> [MembershipRule; 3] {
    let rule = |slot: usize| {
        let rule = &json[slot];
        let key = |field: &str| {
            rule[field]
                .as_str()
                .map_or(Pubkey::default(), |key| key.parse().unwrap())
        };
        let mut symbol = [0; 10];
        if let Some(text) = rule["symbol"].as_str() {
            symbol[..text.len()].copy_from_slice(text.as_bytes());
        }
        MembershipRule {
            kind: u8::try_from(rule["kind"].as_u64().unwrap()).unwrap(),
            address: key("address"),
            update_authority: key("update_authority"),
            symbol,
        }
    };
    [rule(0), rule(1), rule(2)]
}

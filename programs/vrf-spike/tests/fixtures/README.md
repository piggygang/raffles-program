# ORAO fixtures

Devnet state the vrf-spike LiteSVM tests load, written by `scripts/dump-vrf-spike-fixtures.sh` at slot 520109371. Don't edit by hand.

- `orao_vrf.so`: the ORAO VRF program `VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y`, last deployed in slot 517655076.
- `network_state.json`: its configuration PDA `5ER1oENnV4srxYdAynUfRzWeQCPQaqMiAp4VqyMbSqnK`, in the `solana account --output json` format. The tests replace its fulfillment authorities with their own keys.
- `treasury.json`: the treasury `9ZTHWWZDpB36UFe1vszf2KEpt83vwi27jDqtHQ7NSXyR` the request fee goes to.

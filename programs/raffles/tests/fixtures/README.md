# Program fixtures

Mainnet programs the LiteSVM tests load, written by `scripts/dump-fixtures.sh`. Don't edit by hand.

| File | Program | Last deployed slot | Upgradeable |
| --- | --- | --- | --- |
| `mpl_token_metadata.so` | `metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s` | 380725176 | no |
| `mpl_core.so` | `CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d` | 455005931 | yes |
| `squads_multisig.so` | `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf` | 302582236 | no |

Accounts of those programs, in the `solana account --output json` format, read at slot 470512205:

| File | Account | Is |
| --- | --- | --- |
| `squads_program_config.json` | `BSTq9w3kZwNwpBXJEvTZz2G9ZTNyKBvoSeXMvwb4cNZr` | Squads' program config: the multisig creation fee, and the treasury it goes to |

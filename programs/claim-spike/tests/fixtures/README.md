# Pig fixtures

Mainnet accounts the claim-spike LiteSVM tests decode, written by `scripts/dump-claim-spike-fixtures.sh` at slot 467373812, in the `solana account --output json` format. Don't edit by hand.

- `sol_gang_metadata.json`: the Token Metadata account `AzDYEUbWsA3z5QvmQU3M22yYctnmap2KEWgQHLgsazoD` of SOL Gang mint `CVqmUYDPX4FgVjoTCLASdt4Nz3TUgKEV8KCYoxrwcUuv`.
- `girl_gang_metadata.json`: the Token Metadata account `EeXZ1TeLuBTfDh3VA1uMx1b7WD13984Pf28xQ7fZE3gM` of Girl Gang mint `Dk1cJAAYPMa6Y39Qs31Jo8EofTJqJFFE62FXDdtyRQmN`.
- `piggy_gang_asset.json`: the Piggy Gang Core asset `12ruPvhouMCmAQRTMXhxmeu4NsTd3BXv6ZjCqgXvXvvA`.

The tests clone them: each metadata copy gets its own mint, and each asset copy its own address and owner.

The interface of the PiggyGang Raffles program at release ${VERSION}.

|  |  |
| --- | --- |
| Program | `${PROGRAM_ID}` |
| `raffles.json`, sha256 | `${IDL_SHA256}` |
| `raffles.so`, executable hash | `${EXECUTABLE_HASH}` |
| Commit | `${COMMIT}` |

To build against it, pin this release ([docs/releases.md](${REPOSITORY}/blob/${TAG}/docs/releases.md)):

- **Rust:** `raffles-client = { git = "${REPOSITORY}", tag = "${TAG}" }`
- **TypeScript:** `"@piggygang/raffles-client": "${REPOSITORY}/releases/download/${TAG}/raffles-client.tgz"`
- **A vendored IDL:** `raffles.json` below, checked with `uses: piggygang/raffles-program/.github/actions/idl-drift@${TAG}`

[deployments.json](${REPOSITORY}/blob/main/deployments.json) says which cluster runs this release.

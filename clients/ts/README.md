# @piggygang/raffles-client

The TypeScript client of the [PiggyGang Raffles program](https://github.com/piggygang/raffles-program) at one release. [Codama](https://github.com/codama-idl/codama) generates it from the program's IDL, so it can't describe an interface other than its release's.

```ts
import {
  RAFFLES_PROGRAM_ADDRESS,
  VERSION,
  getEventData,
  identifyRafflesEvent,
  parseRaffleCreatedEvent,
  RafflesEvent,
} from "@piggygang/raffles-client";
import idl from "@piggygang/raffles-client/idl.json" with { type: "json" };
```

An instruction takes its signer and its arguments, and derives the rest:

```ts
import { getUpdateConfigInstructionAsync } from "@piggygang/raffles-client";

// A pause: the one setting it names. `null` leaves a setting as it is.
const pause = await getUpdateConfigInstructionAsync({
  authority, // a TransactionSigner: `Config.authority`
  update: {
    paused: true,
    membershipRules: null,
    entryCap: null,
    fullGangBonus: null,
    minDuration: null,
    maxDuration: null,
  },
});
```

- **What it has:** an instruction builder and parser for each instruction, a codec for each account, event and type, the errors with their messages, and the constants.
- **An instruction's arguments are the IDL's:** `update_config` takes `update`, a `ConfigUpdate`. The `…Async` builders place `Config`, the event authority and the program themselves.
- **It is built on [`@solana/kit`](https://github.com/anza-xyz/kit),** a peer dependency, version 8.5 or later.
- **ES modules only.**
- **One version with the program.** `VERSION` is the release, and the program's repository says which cluster runs it: [`deployments.json`](https://github.com/piggygang/raffles-program/blob/main/deployments.json).

How to pin a release, and what the client doesn't have yet: [docs/releases.md](https://github.com/piggygang/raffles-program/blob/main/docs/releases.md). The accounts, the events and the instructions: [docs/account-model.md](https://github.com/piggygang/raffles-program/blob/main/docs/account-model.md), [docs/events.md](https://github.com/piggygang/raffles-program/blob/main/docs/events.md) and [docs/instructions.md](https://github.com/piggygang/raffles-program/blob/main/docs/instructions.md).

## Building it

Nothing generated is committed. From `clients/ts`:

```sh
npm ci --ignore-scripts
npm run build    # scripts/generate.mjs, then tsc
npm test
```

`npm run build` reads `idls/raffles.json`, the IDL of record, and writes `src/generated`, `src/release.ts`, `idl/` and `dist/`.

The tests encode a value of every type in the IDL with a second implementation of Borsh, `tests/reference.mjs`, and require each generated codec to agree in both directions. They build every instruction too, and require its accounts to be where the IDL places them. They name nothing from the interface, so they need no change when it grows.

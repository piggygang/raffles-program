// The built client against the IDL it was generated from.
//
// Nothing here names an instruction, an account or an event: every name
// comes from the IDL. So the tests cover whatever the program's interface
// holds, and need no change when it grows.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  createNoopSigner,
  getAddressDecoder,
  getProgramDerivedAddress,
  isSignerRole,
  isWritableRole,
} from "@solana/kit";

import * as client from "../dist/index.js";
import { camelCase, constantCase, createReference, pascalCase } from "./reference.mjs";

const idl = JSON.parse(readFileSync(new URL("../idl/raffles.json", import.meta.url), "utf8"));
const program = pascalCase(idl.metadata.name);
const PROGRAM = constantCase(idl.metadata.name);

const accounts = idl.accounts ?? [];
const events = idl.events ?? [];
const instructions = idl.instructions ?? [];
const eventNames = new Set(events.map((event) => event.name));
const accountNames = new Set(accounts.map((account) => account.name));
// Anchor lists an account's and an event's fields among the types.
const records = (idl.types ?? []).filter(
  (type) => !eventNames.has(type.name) && !accountNames.has(type.name),
);

/** The export `name`, which has to exist. */
function exported(name) {
  assert.ok(name in client, `the client exports no ${name}`);
  return client[name];
}

test("the client is the IDL's program, at its version", () => {
  assert.equal(exported(`${PROGRAM}_PROGRAM_ADDRESS`), idl.address);
  assert.equal(client.VERSION, idl.metadata.version);
});

test("the package exports the IDL of record", () => {
  const manifest = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  const record = readFileSync(new URL("../../../idls/raffles.json", import.meta.url));

  assert.equal(manifest.exports["./idl.json"], "./idl/raffles.json");
  assert.deepEqual(readFileSync(new URL("../idl/raffles.json", import.meta.url)), record);
  assert.equal(manifest.version, idl.metadata.version);
});

test("every record's codec reads and writes Borsh as the IDL lays it out", () => {
  const reference = createReference(idl);
  for (const { name } of records) {
    const value = reference.sampleDefined(name);
    const bytes = reference.definedBytes([], name, value);

    assert.deepEqual(exported(`get${name}Decoder`)().decode(bytes), value, name);
    assert.deepEqual(exported(`get${name}Encoder`)().encode(value), bytes, name);
  }
});

test("every event's codec reads and writes the event as the program emits it", () => {
  const reference = createReference(idl);
  const identify = events.length > 0 ? exported(`identify${program}Event`) : undefined;
  for (const { name, discriminator } of events) {
    const value = reference.sampleDefined(name);
    const bytes = reference.definedBytes(discriminator, name, value);

    assert.deepEqual(exported(`${constantCase(name)}_EVENT_DISCRIMINATOR`), Uint8Array.from(discriminator));
    assert.deepEqual(exported(`parse${name}Event`)(bytes), value, name);
    assert.deepEqual(exported(`get${name}EventEncoder`)().encode(value), bytes, name);
    assert.equal(identify(bytes), exported(`${program}Event`)[name], name);
    // In an event instruction the tag comes first.
    const instructionData = Uint8Array.from([...client.EVENT_TAG, ...bytes]);
    assert.deepEqual(client.getEventData(instructionData), bytes, name);
  }
});

test("every account's codec reads and writes the account as the program stores it", () => {
  const reference = createReference(idl);
  for (const { name, discriminator } of accounts) {
    const value = reference.sampleDefined(name);
    const bytes = reference.definedBytes(discriminator, name, value);

    assert.deepEqual(
      exported(`get${name}Decoder`)().decode(bytes),
      { discriminator: Uint8Array.from(discriminator), ...value },
      name,
    );
    assert.deepEqual(exported(`get${name}Encoder`)().encode(value), bytes, name);
  }
});

test("every instruction's data is its discriminator, then its arguments", () => {
  const reference = createReference(idl);
  for (const { name, discriminator, args } of instructions) {
    const type = pascalCase(name);
    const value = reference.sampleFields(args);
    const bytes = reference.fieldBytes(discriminator, args, value);

    assert.deepEqual(exported(`get${type}InstructionDataEncoder`)().encode(value), bytes, name);
    assert.deepEqual(
      exported(`get${type}InstructionDataDecoder`)().decode(bytes),
      { discriminator: Uint8Array.from(discriminator), ...value },
      name,
    );
    assert.equal(
      exported(`identify${program}Instruction`)(bytes),
      exported(`${program}Instruction`)[type],
      name,
    );
    assert.equal(typeof exported(`get${type}Instruction`), "function", name);
  }
});

test("every instruction's builder places the accounts as the IDL does", async () => {
  const reference = createReference(idl);
  const utf8 = new TextEncoder();
  let next = 0;
  /** An address no other account of the test has. */
  const address = () => {
    next += 1;
    return getAddressDecoder().decode(Uint8Array.from({ length: 32 }, (_, index) => (next * 17 + index) % 256));
  };
  const derived = async (seeds) => (await getProgramDerivedAddress({ programAddress: idl.address, seeds }))[0];

  for (const { name, accounts: instructionAccounts, args } of instructions) {
    const type = pascalCase(name);
    const input = reference.sampleFields(args);
    const expected = [];
    for (const account of instructionAccounts) {
      if (account.accounts) throw new Error(`${name}: nested accounts aren't covered`);
      const seeds = account.pda?.seeds ?? [];
      if (account.address) {
        // The IDL fixes the address, so the caller passes nothing.
        expected.push(account.address);
      } else if (account.pda && !account.pda.program && seeds.every((seed) => seed.kind === "const")) {
        // So does an address made of constant seeds alone.
        expected.push(await derived(seeds.map((seed) => Uint8Array.from(seed.value))));
      } else if (account.name === "event_authority") {
        // Anchor's two event accounts, which the IDL names without an
        // address: the generator places them (docs/events.md, section 1).
        expected.push(await derived([utf8.encode("__event_authority")]));
      } else if (account.name === "program") {
        expected.push(idl.address);
      } else {
        const given = address();
        input[camelCase(account.name)] = account.signer ? createNoopSigner(given) : given;
        expected.push(given);
      }
    }

    const build = client[`get${type}InstructionAsync`] ?? exported(`get${type}Instruction`);
    const instruction = await build(input);

    assert.equal(instruction.programAddress, idl.address, name);
    assert.deepEqual(
      instruction.accounts.map((account) => account.address),
      expected,
      name,
    );
    instructionAccounts.forEach((account, index) => {
      const { role } = instruction.accounts[index];
      assert.equal(isSignerRole(role), account.signer === true, `${name}: ${account.name} signs`);
      assert.equal(isWritableRole(role), account.writable === true, `${name}: ${account.name} is writable`);
    });
  }
});

test("every error has its code", () => {
  for (const { name, code } of idl.errors ?? []) {
    assert.equal(exported(`${PROGRAM}_ERROR__${constantCase(name)}`), code, name);
  }
});

test("every constant has its value", () => {
  for (const { name, type, value } of idl.constants ?? []) {
    const constant = exported(name);
    if (type === "bytes") {
      assert.deepEqual(constant, Uint8Array.from(JSON.parse(value)), name);
    } else if (type === "string") {
      assert.equal(constant, JSON.parse(value), name);
    } else {
      assert.equal(BigInt(constant), BigInt(value), name);
    }
  }
});

test("the event tag is Anchor's, and only an instruction that starts with it is an event", () => {
  // Anchor reads the hash's first eight bytes as a number and writes it
  // little-endian, which reverses them.
  const tag = createHash("sha256").update("anchor:event").digest().subarray(0, 8).reverse();

  assert.deepEqual(client.EVENT_TAG, Uint8Array.from(tag));
  assert.deepEqual(client.getEventData(Uint8Array.from([...tag, 1, 2, 3])), Uint8Array.from([1, 2, 3]));
  // An instruction's own data starts with its discriminator.
  assert.ok(instructions.length > 0);
  for (const { name, discriminator } of instructions) {
    assert.equal(client.getEventData(Uint8Array.from(discriminator)), undefined, name);
  }
  assert.equal(client.getEventData(Uint8Array.from(tag.subarray(0, 7))), undefined);
  assert.equal(client.getEventData(new Uint8Array()), undefined);
});

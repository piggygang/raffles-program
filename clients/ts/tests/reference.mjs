// A second reading of the IDL, independent of Codama's: Borsh as an Anchor
// IDL spells its types. The tests make a value of every type in the IDL,
// encode it here, and require the generated codecs to agree in both
// directions.
//
// A type this file doesn't know throws, so a new kind of type in the IDL
// fails the tests until it is covered here.

import { getAddressDecoder, none, some } from "@solana/kit";

const INTEGERS = {
  u8: 8, i8: 8, u16: 16, i16: 16, u32: 32, i32: 32, u64: 64, i64: 64, u128: 128, i128: 128,
};

/** `vrf_requested_slot` and `_reserved` as the client names them. */
export function camelCase(name) {
  const [first, ...rest] = name.split("_").filter(Boolean);
  return first + rest.map((word) => word[0].toUpperCase() + word.slice(1)).join("");
}

/** `claim_entries` and `RaffleCreated` as the client names their types. */
export function pascalCase(name) {
  const camel = camelCase(name);
  return camel[0].toUpperCase() + camel.slice(1);
}

/** `RaffleCreated` and `claim_spike` as the client names their constants. */
export function constantCase(name) {
  return name
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/([A-Z])([A-Z][a-z])/g, "$1_$2")
    .toUpperCase();
}

export function createReference(idl) {
  let counter = 0;
  const utf8 = new TextEncoder();

  function definitionOf(name) {
    const definition = idl.types.find((type) => type.name === name);
    if (!definition) throw new Error(`the IDL has no type ${name}`);
    return definition.type;
  }

  /** A struct's or a variant's fields: named, in a tuple, or none. */
  function shapeOf(fields = []) {
    if (fields.length === 0) return { kind: "none" };
    if (fields.every((field) => field.name !== undefined)) return { kind: "named", fields };
    if (fields.every((field) => field.name === undefined)) return { kind: "tuple", types: fields };
    throw new Error(`fields are either all named or all unnamed: ${JSON.stringify(fields)}`);
  }

  function integer(bits, value) {
    const bytes = [];
    let rest = BigInt.asUintN(bits, BigInt(value));
    for (let index = 0; index < bits / 8; index++) {
      bytes.push(Number(rest & 0xffn));
      rest >>= 8n;
    }
    return bytes;
  }

  /** A value of `type`, in the shape the client's codecs take and return. */
  function sample(type) {
    counter += 1;
    if (type === "bool") return counter % 2 === 1;
    if (type === "pubkey") {
      const bytes = Uint8Array.from({ length: 32 }, (_, index) => (counter * 31 + index) % 256);
      return getAddressDecoder().decode(bytes);
    }
    // The length prefix counts bytes, not characters: the pig is four.
    if (type === "string") return `raffle 🐷 ${counter}`;
    if (type === "bytes") {
      return Uint8Array.from({ length: (counter % 5) + 1 }, (_, index) => (counter + index) % 256);
    }
    if (typeof type === "string" && type in INTEGERS) {
      const bits = INTEGERS[type];
      const signed = type.startsWith("i");
      const magnitude = (BigInt(counter) * 1_000_003n + 7n) % (1n << BigInt(bits - 1));
      const value = signed && counter % 2 === 0 ? -magnitude : magnitude;
      return bits <= 32 ? Number(value) : value;
    }
    if (type.array) {
      const [item, length] = type.array;
      if (item === "u8") {
        return Uint8Array.from({ length }, (_, index) => (counter * 7 + index) % 256);
      }
      return Array.from({ length }, () => sample(item));
    }
    if (type.vec) return [sample(type.vec), sample(type.vec)];
    if (type.option) return counter % 2 === 0 ? none() : some(sample(type.option));
    if (type.defined) return sampleDefined(type.defined.name);
    throw new Error(`no sample for the IDL type ${JSON.stringify(type)}`);
  }

  function encode(type, value) {
    if (type === "bool") return [value ? 1 : 0];
    if (type === "pubkey") return addressBytes(value);
    if (type === "string") {
      const bytes = utf8.encode(value);
      return [...integer(32, bytes.length), ...bytes];
    }
    if (type === "bytes") return [...integer(32, value.length), ...value];
    if (typeof type === "string" && type in INTEGERS) return integer(INTEGERS[type], value);
    if (type.array) {
      const [item] = type.array;
      if (item === "u8") return [...value];
      return value.flatMap((each) => encode(item, each));
    }
    if (type.vec) {
      return [...integer(32, value.length), ...value.flatMap((each) => encode(type.vec, each))];
    }
    if (type.option) {
      return value.__option === "Some" ? [1, ...encode(type.option, value.value)] : [0];
    }
    if (type.defined) return encodeDefined(type.defined.name, value);
    throw new Error(`no encoding for the IDL type ${JSON.stringify(type)}`);
  }

  function sampleFields(fields) {
    return Object.fromEntries(fields.map((field) => [camelCase(field.name), sample(field.type)]));
  }

  function encodeFields(fields, value) {
    return fields.flatMap((field) => encode(field.type, value[camelCase(field.name)]));
  }

  /** A value of the IDL's own type `name`: a struct, an enum or an alias. */
  function sampleDefined(name) {
    const definition = definitionOf(name);
    if (definition.kind === "type") return sample(definition.alias);
    if (definition.kind === "struct") {
      const shape = shapeOf(definition.fields);
      if (shape.kind === "named") return sampleFields(shape.fields);
      throw new Error(`${name}: a struct without named fields isn't covered`);
    }
    if (definition.kind === "enum") {
      counter += 1;
      const index = counter % definition.variants.length;
      const variant = definition.variants[index];
      // An enum without data is a number: its variant's index.
      if (definition.variants.every((each) => shapeOf(each.fields).kind === "none")) return index;
      const shape = shapeOf(variant.fields);
      if (shape.kind === "named") return { __kind: variant.name, ...sampleFields(shape.fields) };
      if (shape.kind === "tuple") return { __kind: variant.name, fields: shape.types.map(sample) };
      return { __kind: variant.name };
    }
    throw new Error(`${name}: no sample for a ${definition.kind}`);
  }

  function encodeDefined(name, value) {
    const definition = definitionOf(name);
    if (definition.kind === "type") return encode(definition.alias, value);
    if (definition.kind === "struct") return encodeFields(definition.fields ?? [], value);
    if (definition.kind === "enum") {
      if (typeof value === "number") return [value];
      const index = definition.variants.findIndex((variant) => variant.name === value.__kind);
      const shape = shapeOf(definition.variants[index].fields);
      if (shape.kind === "named") return [index, ...encodeFields(shape.fields, value)];
      if (shape.kind === "tuple") {
        return [index, ...shape.types.flatMap((type, at) => encode(type, value.fields[at]))];
      }
      return [index];
    }
    throw new Error(`${name}: no encoding for a ${definition.kind}`);
  }

  return {
    /** A value of the IDL's type `name`, in the client's shape. */
    sampleDefined,
    /** A value for `fields`, keyed as the client keys it. */
    sampleFields,
    /** `discriminator`, then a value of the IDL's type `name` as Borsh lays it out. */
    definedBytes(discriminator, name, value) {
      return Uint8Array.from([...discriminator, ...encodeDefined(name, value)]);
    },
    /** `discriminator`, then `value` as Borsh lays out `fields`. */
    fieldBytes(discriminator, fields, value) {
      return Uint8Array.from([...discriminator, ...encodeFields(fields, value)]);
    },
  };
}

// Base58, written out: the reference must not decode an address with the
// library it is checking.
const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

function addressBytes(address) {
  let number = 0n;
  for (const character of address) {
    const digit = ALPHABET.indexOf(character);
    if (digit < 0) throw new Error(`${address} isn't base58`);
    number = number * 58n + BigInt(digit);
  }
  const bytes = [];
  for (let index = 0; index < 32; index++) {
    bytes.unshift(Number(number & 0xffn));
    number >>= 8n;
  }
  return bytes;
}

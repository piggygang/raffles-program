import type { ReadonlyUint8Array } from "@solana/kit";

/**
 * The eight bytes an event instruction's data starts with: Anchor's tag, the
 * first eight bytes of `sha256("anchor:event")` in reverse.
 *
 * The program emits an event by calling itself, so an event is an inner
 * instruction of the program whose data is this tag and then the event
 * (docs/events.md, section 1).
 */
export const EVENT_TAG: ReadonlyUint8Array = new Uint8Array([
  0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d,
]);

/**
 * The event in the data of one of the program's inner instructions, or
 * `undefined` if the instruction isn't an event.
 *
 * Pass the result to `identifyRafflesEvent`, and then to that event's parser:
 * `parseRaffleCreatedEvent` and its like.
 *
 * The caller has checked what the data can't show: that the instruction is
 * the raffles program's, and that its transaction succeeded.
 */
export function getEventData(
  instructionData: ReadonlyUint8Array,
): ReadonlyUint8Array | undefined {
  if (instructionData.length < EVENT_TAG.length) return undefined;
  for (let index = 0; index < EVENT_TAG.length; index++) {
    if (instructionData[index] !== EVENT_TAG[index]) return undefined;
  }
  return instructionData.subarray(EVENT_TAG.length);
}

/**
 * The master knob's scale.
 *
 * The knob reads 0 to 10, and 10 is a decibel under full: the headroom
 * every output is given. Past 10 is a notch, and past the notch is 11,
 * full level, drawn at five o'clock beyond the half-past-four 10. Between 0
 * and 10 the taper is logarithmic, forty decibels across the last decade,
 * so the bottom of the travel is quiet rather than silent-until-nine.
 */

/** The reading at the top of the ordinary travel. */
export const KNOB_TOP = 10;
/** The reading past the notch. */
export const KNOB_FULL = 11;
/** What the top of the travel is, in decibels. */
export const TOP_DB = -1;

/** Decibels for a reading; −Infinity at 0. */
export function knobToDb(reading: number): number {
  if (!(reading > 0)) return Number.NEGATIVE_INFINITY;
  if (reading >= KNOB_FULL) return 0;
  const inTravel = Math.min(reading, KNOB_TOP);
  return TOP_DB + 40 * Math.log10(inTravel / KNOB_TOP);
}

/** The engine's linear gain for a reading, 0 to 1. */
export function knobToGain(reading: number): number {
  const db = knobToDb(reading);
  return Number.isFinite(db) ? Math.min(10 ** (db / 20), 1) : 0;
}

/** The reading the engine's gain stands for: 11 at full, else on the taper. */
export function gainToKnob(gain: number): number {
  if (!(gain > 0)) return 0;
  const db = 20 * Math.log10(Math.min(gain, 1));
  // Anything past the top of the travel is the notch's own setting.
  if (db > TOP_DB + 0.05) return KNOB_FULL;
  return Math.min(KNOB_TOP * 10 ** ((db - TOP_DB) / 40), KNOB_TOP);
}

/** The readout while the knob turns: whole numbers, `10`, then `11`. */
export function knobLabel(reading: number): string {
  return String(Math.round(Math.min(Math.max(reading, 0), KNOB_FULL)));
}

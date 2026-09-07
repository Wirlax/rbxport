import { describe, expect, it } from "vitest";
import { compatibleKeys, fromCamelot, normalizeKey, toCamelot } from "./camelot";

describe("camelot", () => {
  it("maps minor and major keys", () => {
    expect(toCamelot("Abm")).toBe("1A");
    expect(toCamelot("Ebm")).toBe("2A");
    expect(toCamelot("Am")).toBe("8A");
    expect(toCamelot("C")).toBe("8B");
    expect(toCamelot("B")).toBe("1B");
  });
  it("normalises enharmonic spellings", () => {
    expect(normalizeKey("G#m")).toBe("Abm");
    expect(toCamelot("G#m")).toBe(toCamelot("Abm"));
    expect(toCamelot("Gb")).toBe(toCamelot("F#"));
  });
  it("round-trips", () => {
    for (const k of ["Abm", "Ebm", "Am", "C", "G", "F#m"]) {
      expect(fromCamelot(toCamelot(k))).toBe(normalizeKey(k));
    }
  });
  it("returns empty for unknown input instead of guessing", () => {
    expect(toCamelot("H minor")).toBe("");
    expect(fromCamelot("13A")).toBe("");
    expect(fromCamelot("")).toBe("");
  });
  it("gives the three harmonic neighbours, wrapping the wheel", () => {
    expect(compatibleKeys("Am").sort()).toEqual(["C", "Dm", "Em"].sort());
    // 1A wraps to 12A, not 0A
    expect(compatibleKeys("Abm")).toContain(fromCamelot("12A"));
  });
});

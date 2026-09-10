import { describe, expect, it } from "vitest";
import {
  compatibleKeys, fromCamelot, normalizeKey, toCamelot, trafficLightCodes, trafficLightLit,
} from "./camelot";

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

describe("the Traffic Light's reach", () => {
  it("lights what rekordbox's own definition says, for a track in 2A", () => {
    // Ebm is 2A.
    expect(trafficLightCodes("Ebm", "same")).toEqual(["2A"]);
    expect(trafficLightCodes("Ebm", "related1")).toEqual(["2A", "2B"]);
    expect(trafficLightCodes("Ebm", "related2")).toEqual(["2A", "2B", "1A", "3A"]);
    expect(trafficLightCodes("Ebm", "related3")).toEqual(["2A", "2B", "1A", "3A", "1B", "3B"]);
  });

  it("wraps around the wheel and ignores keys it does not know", () => {
    // Abm is 1A: its neighbours are 12A and 2A.
    expect(trafficLightCodes("Abm", "related2")).toEqual(["1A", "1B", "12A", "2A"]);
    expect(trafficLightCodes("Unknown", "related3")).toEqual([]);
    expect(trafficLightLit("F#", "Ebm", "related1")).toBe(true);
    expect(trafficLightLit("Am", "Ebm", "related3")).toBe(false);
    expect(trafficLightLit("", "Ebm", "related3")).toBe(false);
  });
});


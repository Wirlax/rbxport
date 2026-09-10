import { describe, expect, it } from "vitest";

import { fieldsOf } from "./InfoPanel";
import type { RowDto } from "@/ipc/types";

const track: RowDto = {
  id: "1",
  trackNo: 1,
  title: "All U Need",
  artist: "TRIODE",
  album: "Single",
  genre: "House",
  label: "Anjuna",
  comment: "",
  bpmX100: 12_800,
  key: "Am",
  durationSec: 305,
  rating: 4,
  analysed: 1,
  dateAdded: "2026-09-06 10:00:00.000 +00:00",
  releaseDate: "",
  hotCues: [["A", 46, "#77E866"], ["B", 90_000, "#77E866"], ["C", 120_000, null]],
  artworkHue: 40,
  hasArtwork: true,
};

describe("the information panel's fields", () => {
  it("shows the key with its camelot code, which is what a DJ mixes on", () => {
    const key = fieldsOf(track).find((f) => f.label === "Key");
    expect(key?.value).toBe("Am / 8A");
  });

  it("leaves a key alone when it has no camelot code", () => {
    const key = fieldsOf({ ...track, key: "Weird" }).find((f) => f.label === "Key");
    expect(key?.value).toBe("Weird");
  });

  it("formats the numbers the way the track list does", () => {
    const byLabel = Object.fromEntries(fieldsOf(track).map((f) => [f.label, f.value]));
    expect(byLabel["BPM"]).toBe("128.00");
    expect(byLabel["Time"]).toBe("05:05");
    expect(byLabel["Rating"]).toBe("★★★★");
  });

  it("keeps every row when a value is empty, so the panel does not jump", () => {
    const sparse = fieldsOf({ ...track, album: "", label: "", comment: "" });
    expect(sparse).toHaveLength(fieldsOf(track).length);
    expect(sparse.find((f) => f.label === "Album")?.value).toBe("");
  });

  it("says nothing about a track with no BPM rather than zero", () => {
    const bpm = fieldsOf({ ...track, bpmX100: 0 }).find((f) => f.label === "BPM");
    expect(bpm?.value).toBe("");
  });
});

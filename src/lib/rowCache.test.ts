import { describe, expect, it } from "vitest";
import { RowCache } from "./rowCache";

const page = (n: number, size = 4) => Array.from({ length: size }, (_, i) => `r${n * size + i}`);

describe("RowCache", () => {
  it("returns rows by absolute index", () => {
    const c = new RowCache<string>(4, 10);
    c.setPage(0, "v1:1", page(0));
    c.setPage(2, "v1:1", page(2));
    expect(c.get(0, "v1:1")).toBe("r0");
    expect(c.get(3, "v1:1")).toBe("r3");
    expect(c.get(9, "v1:1")).toBe("r9");
    expect(c.get(5, "v1:1")).toBeUndefined();
  });

  it("treats a different view token as a miss, so a sort never shows stale rows", () => {
    const c = new RowCache<string>(4, 10);
    c.setPage(0, "v1:1", page(0));
    expect(c.get(1, "v1:1")).toBe("r1");
    expect(c.get(1, "v2:1")).toBeUndefined();
  });

  it("reports exactly the pages a range needs", () => {
    const c = new RowCache<string>(4, 10);
    c.setPage(1, "v1:1", page(1));
    expect(c.missingPages(0, 12, "v1:1")).toEqual([0, 2]);
    expect(c.missingPages(4, 8, "v1:1")).toEqual([]);
  });

  it("evicts least-recently-used pages past the cap", () => {
    const c = new RowCache<string>(4, 3);
    c.setPage(0, "v1:1", page(0));
    c.setPage(1, "v1:1", page(1));
    c.setPage(2, "v1:1", page(2));
    c.get(0, "v1:1");            // touch page 0 so page 1 becomes oldest
    c.setPage(3, "v1:1", page(3));
    expect(c.size).toBe(3);
    expect(c.hasPage(0, "v1:1")).toBe(true);
    expect(c.hasPage(1, "v1:1")).toBe(false);
    expect(c.hasPage(3, "v1:1")).toBe(true);
  });

  it("stays bounded under sustained scrolling", () => {
    const c = new RowCache<string>(64, 100);
    for (let p = 0; p < 1000; p++) c.setPage(p, "v1:1", page(p, 64));
    expect(c.size).toBe(100);
  });
});

describe("view identity", () => {
  it("does not serve pages from another view that shares a generation", () => {
    // The bug this guards: the backend returns the same `gen` for every view,
    // so keying on `gen` alone let a previous view's rows render after a sort.
    const c = new RowCache<string>(4, 10);
    c.setPage(0, "1:1", page(0));
    expect(c.get(0, "1:1")).toBe("r0");
    expect(c.get(0, "2:1")).toBeUndefined();
    expect(c.missingPages(0, 4, "2:1")).toEqual([0]);
  });
});

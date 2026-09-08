import { describe, expect, it } from "vitest";

import {
  cancel, emptyQueue, enqueue, fail, isRunning, reset, start, succeed, total,
  type QueueItem,
} from "./queue";

const items = (...ids: string[]): QueueItem[] =>
  ids.map((id) => ({ id, title: `Track ${id}` }));

describe("enqueue", () => {
  it("adds in order", () => {
    const q = enqueue(emptyQueue, items("a", "b", "c"));
    expect(q.pending.map((i) => i.id)).toEqual(["a", "b", "c"]);
  });

  it("skips a track already queued", () => {
    // Queueing twice analyses twice and counts twice, which makes the
    // progress meaningless.
    let q = enqueue(emptyQueue, items("a", "b"));
    q = enqueue(q, items("b", "c"));
    expect(q.pending.map((i) => i.id)).toEqual(["a", "b", "c"]);
  });

  it("skips duplicates within one batch", () => {
    const q = enqueue(emptyQueue, items("a", "a", "b"));
    expect(q.pending.map((i) => i.id)).toEqual(["a", "b"]);
  });

  it("skips the track already running", () => {
    let q = start(enqueue(emptyQueue, items("a")));
    q = enqueue(q, items("a", "b"));
    expect(q.pending.map((i) => i.id)).toEqual(["b"]);
  });

  it("returns the same state when nothing is new", () => {
    const q = enqueue(emptyQueue, items("a"));
    expect(enqueue(q, items("a"))).toBe(q);
  });

  it("clears a previous cancellation, since this is a new request", () => {
    const q = enqueue(cancel(enqueue(emptyQueue, items("a"))), items("b"));
    expect(q.cancelling).toBe(false);
  });
});

describe("start", () => {
  it("takes the first waiting track", () => {
    const q = start(enqueue(emptyQueue, items("a", "b")));
    expect(q.current?.id).toBe("a");
    expect(q.pending.map((i) => i.id)).toEqual(["b"]);
  });

  it("does nothing while one is running", () => {
    const q = start(enqueue(emptyQueue, items("a", "b")));
    expect(start(q)).toBe(q);
  });

  it("does nothing on an empty queue", () => {
    expect(start(emptyQueue)).toBe(emptyQueue);
  });

  it("drops what is waiting when the run is cancelling", () => {
    const q = start(cancel(enqueue(emptyQueue, items("a", "b"))));
    expect(q.current).toBeNull();
    expect(q.pending).toEqual([]);
  });
});

describe("succeed and fail", () => {
  it("counts a finished track", () => {
    const q = succeed(start(enqueue(emptyQueue, items("a"))));
    expect(q.done).toBe(1);
    expect(q.current).toBeNull();
  });

  it("keeps why a track failed", () => {
    const q = fail(start(enqueue(emptyQueue, items("a"))), "could not decode");
    expect(q.failed).toEqual([{ id: "a", title: "Track a", reason: "could not decode" }]);
    expect(q.done).toBe(0);
  });

  it("does nothing when nothing is running", () => {
    expect(succeed(emptyQueue)).toBe(emptyQueue);
    expect(fail(emptyQueue, "x")).toBe(emptyQueue);
  });

  it("keeps going after a failure", () => {
    let q = enqueue(emptyQueue, items("a", "b"));
    q = fail(start(q), "bad file");
    q = succeed(start(q));
    expect(q.done).toBe(1);
    expect(q.failed).toHaveLength(1);
    expect(isRunning(q)).toBe(false);
  });
});

describe("cancel", () => {
  it("lets the running track finish but drops the rest", () => {
    // The running track is most of a second already spent; abandoning it
    // would leave half a result.
    let q = start(enqueue(emptyQueue, items("a", "b", "c")));
    q = cancel(q);
    expect(q.current?.id).toBe("a");
    expect(q.pending).toEqual([]);
    q = succeed(q);
    expect(isRunning(q)).toBe(false);
  });
});

describe("total and isRunning", () => {
  it("counts everything the run covers", () => {
    let q = enqueue(emptyQueue, items("a", "b", "c"));
    expect(total(q)).toBe(3);
    q = succeed(start(q));
    expect(total(q)).toBe(3);
    q = fail(start(q), "x");
    expect(total(q)).toBe(3);
  });

  it("is running while anything is waiting or going", () => {
    expect(isRunning(emptyQueue)).toBe(false);
    const q = enqueue(emptyQueue, items("a"));
    expect(isRunning(q)).toBe(true);
    expect(isRunning(succeed(start(q)))).toBe(false);
  });
});

describe("reset", () => {
  it("clears a finished run", () => {
    const q = succeed(start(enqueue(emptyQueue, items("a"))));
    expect(reset(q)).toEqual(emptyQueue);
  });

  it("refuses to clear a run still going", () => {
    const q = start(enqueue(emptyQueue, items("a", "b")));
    expect(reset(q)).toBe(q);
  });
});

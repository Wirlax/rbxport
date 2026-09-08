/**
 * The analysis queue's state, as pure transitions.
 *
 * Analysis is slow — decoding and a DSP pass per track — so it runs one at a
 * time against the backend and the queue tracks where it has got to. Kept
 * apart from the component so the sequencing is testable without a backend:
 * ordering, cancellation and the counting are exactly the parts that go wrong.
 */

export interface QueueItem {
  id: string;
  title: string;
}

export interface QueueState {
  /** Waiting, in order. */
  pending: QueueItem[];
  /** Being analysed now, or `null` between tracks. */
  current: QueueItem | null;
  done: number;
  /** One entry per track that failed, with why. */
  failed: { id: string; title: string; reason: string }[];
  /** Set when the user asks to stop; the running track still finishes. */
  cancelling: boolean;
}

export const emptyQueue: QueueState = {
  pending: [],
  current: null,
  done: 0,
  failed: [],
  cancelling: false,
};

/** Total tracks this run covers, finished or not. */
export function total(state: QueueState): number {
  return state.done + state.failed.length + state.pending.length + (state.current ? 1 : 0);
}

/** Whether anything is left to do. */
export function isRunning(state: QueueState): boolean {
  return state.current !== null || state.pending.length > 0;
}

/**
 * Adds tracks, skipping any already queued or running.
 *
 * Queueing the same track twice would analyse it twice and count it twice,
 * which makes the progress meaningless.
 */
export function enqueue(state: QueueState, items: readonly QueueItem[]): QueueState {
  const known = new Set(state.pending.map((i) => i.id));
  if (state.current) known.add(state.current.id);
  const fresh = items.filter((item) => {
    if (known.has(item.id)) return false;
    known.add(item.id);
    return true;
  });
  if (fresh.length === 0) return state;
  return { ...state, pending: [...state.pending, ...fresh], cancelling: false };
}

/** Takes the next track, or parks if there is none or the run is cancelling. */
export function start(state: QueueState): QueueState {
  if (state.current !== null) return state;
  if (state.cancelling || state.pending.length === 0) {
    return state.pending.length > 0 ? { ...state, pending: [] } : state;
  }
  const [next, ...rest] = state.pending;
  return next ? { ...state, current: next, pending: rest } : state;
}

/** Records the running track as finished. */
export function succeed(state: QueueState): QueueState {
  if (!state.current) return state;
  return { ...state, current: null, done: state.done + 1 };
}

/** Records the running track as failed, keeping why. */
export function fail(state: QueueState, reason: string): QueueState {
  if (!state.current) return state;
  return {
    ...state,
    current: null,
    failed: [...state.failed, { ...state.current, reason }],
  };
}

/**
 * Asks the run to stop.
 *
 * The track already being analysed finishes: it is most of a second's work
 * that is already spent, and abandoning it would leave half a result.
 */
export function cancel(state: QueueState): QueueState {
  return { ...state, cancelling: true, pending: [] };
}

/** Clears a finished run so the next one starts from nothing. */
export function reset(state: QueueState): QueueState {
  return isRunning(state) ? state : emptyQueue;
}

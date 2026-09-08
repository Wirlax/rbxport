/**
 * Owns a track view: opens it on the backend, pages rows in as they are
 * scrolled into range, and drops stale pages when the view changes.
 *
 * The library itself is never held here — only a bounded LRU of fetched pages.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getBackend } from "@/ipc/client";
import type { RowDto, ViewSpec } from "@/ipc/types";
import { RowCache, PAGE_SIZE, type CacheToken } from "@/lib/rowCache";
import { planFetches } from "@/lib/virtual";

export interface TrackView {
  /** Row count of the current view. */
  count: number;
  /** Identity of the current view's data; changes whenever rows must be refetched. */
  token: CacheToken;
  loading: boolean;
  error: string | null;
  /** Row at an absolute index, or undefined while its page is in flight. */
  rowAt: (index: number) => RowDto | undefined;
  /** Ask for the pages covering [start, end); safe to call every frame. */
  ensureRange: (start: number, end: number) => void;
  /** Ids between two row indices inclusive, resolved by the backend. */
  idsInRange: (from: number, to: number) => Promise<string[]>;
}

export function useTrackView(spec: ViewSpec): TrackView {
  // `specKey` records which spec this state describes. Loading is derived from
  // comparing it against the current spec rather than set by an effect: an
  // effect runs *after* the render that changed the spec, so for one frame the
  // new playlist's title was drawn beside the previous view's row count.
  const [state, setState] = useState({
    viewId: 0,
    count: 0,
    gen: 0,
    specKey: "",
    error: null as string | null,
  });
  const cache = useRef(new RowCache<RowDto>(PAGE_SIZE));
  const inFlight = useRef(new Set<number>());
  // Bumped when a page lands, to re-render the rows it filled.
  const [pagesLoaded, setPagesLoaded] = useState(0);

  const specKey = useMemo(
    () => JSON.stringify([spec.source, spec.sort, spec.descending, spec.query]),
    [spec.source, spec.sort, spec.descending, spec.query],
  );

  // View identity for the cache: a new view id, or a library change, invalidates pages.
  const token: CacheToken = `${state.viewId}:${state.gen}`;

  useEffect(() => {
    let cancelled = false;
    cache.current.clear();
    inFlight.current.clear();
    setState((s) => ({ ...s, error: null }));

    void (async () => {
      try {
        const backend = await getBackend();
        const handle = await backend.openView(spec);
        if (cancelled) return;
        setState({
          viewId: handle.viewId,
          count: handle.len,
          gen: handle.gen,
          specKey,
          error: null,
        });
      } catch (e) {
        if (cancelled) return;
        // Mark the failure as belonging to this spec, or it reads as still
        // loading and retries forever.
        setState((s) => ({ ...s, specKey, error: e instanceof Error ? e.message : String(e) }));
      }
    })();

    return () => {
      cancelled = true;
    };
    // specKey captures every field that changes the view.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [specKey]);

  const ensureRange = useCallback(
    (start: number, end: number) => {
      const { viewId, count } = state;
      const loading = state.specKey !== specKey;
      // Fetching while a view swap is in flight would fill the cache from the
      // outgoing view under the incoming token.
      if (!viewId || count === 0 || loading) return;
      const missing = cache.current.missingPages(start, Math.min(end, count), token);
      const toFetch = planFetches(missing, inFlight.current);
      if (toFetch.length === 0) return;

      for (const page of toFetch) {
        inFlight.current.add(page);
        void (async () => {
          try {
            const backend = await getBackend();
            const rows = await backend.fetchRows(viewId, page * PAGE_SIZE, PAGE_SIZE);
            // A view swap between request and response makes this page stale.
            if (cache.current.hasPage(page, token)) return;
            cache.current.setPage(page, token, rows);
            setPagesLoaded((n) => n + 1);
          } catch {
            // Leave the page missing; the next scroll retries it.
          } finally {
            inFlight.current.delete(page);
          }
        })();
      }
    },
    [state, token],
  );

  const rowAt = useCallback((index: number) => cache.current.get(index, token), [token, pagesLoaded]);

  const idsInRange = useCallback(
    async (from: number, to: number) => {
      if (!state.viewId) return [];
      const backend = await getBackend();
      return backend.viewIdsInRange(state.viewId, from, to);
    },
    [state.viewId],
  );

  return {
    count: state.count,
    token,
    loading: state.specKey !== specKey,
    error: state.error,
    rowAt,
    ensureRange,
    idsInRange,
  };
}

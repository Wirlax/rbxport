/**
 * An element's size in CSS pixels, kept current as the window changes.
 *
 * A `ResizeObserver` rather than a window listener: the player's waveforms
 * change width when the tree splitter moves as well as when the window does,
 * and only the element itself sees both.
 */
import { useEffect, useRef, useState } from "react";

export interface Size {
  width: number;
  height: number;
}

export function useElementSize<T extends HTMLElement>(): [
  React.RefObject<T | null>,
  Size,
] {
  const ref = useRef<T>(null);
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const box = entry.contentRect;
      // Only a real change: an observer that sets state to an equal object on
      // every callback would rerender the player for nothing.
      setSize((current) =>
        current.width === box.width && current.height === box.height
          ? current
          : { width: box.width, height: box.height },
      );
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  return [ref, size];
}

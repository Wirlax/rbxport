/**
 * The picture that travels with a dragged row.
 *
 * Left to the browser, the drag image is a snapshot of the row as it stands,
 * and a virtualised row does not stand still: it is absolutely placed by a
 * transform inside a scroller that clips it, and WebKit's snapshot of that
 * came out blank or cut off as often as not, and never faded. So the row is
 * copied, laid flat, faded, and handed to the browser as the image; the copy
 * lives only as long as the snapshot takes.
 */

/** How faded the ghost is: the same as a column heading being dragged. */
const GHOST_OPACITY = "0.45";

/**
 * Sets `row`'s faded copy as the drag image of `transfer`, and removes the
 * copy once the browser has taken its picture.
 */
export function setRowDragImage(
  row: HTMLElement,
  transfer: DataTransfer,
  pointer: { x: number; y: number },
): void {
  // Beside the row rather than on the body, so the copy inherits the grid's
  // column template and colours from the same ancestors the row does.
  const host = row.parentElement;
  if (!host) return;
  const box = row.getBoundingClientRect();
  const ghost = row.cloneNode(true) as HTMLElement;
  ghost.removeAttribute("draggable");
  ghost.setAttribute("aria-hidden", "true");
  ghost.style.transform = "none";
  ghost.style.top = "-9999px";
  ghost.style.left = "0";
  ghost.style.width = `${box.width}px`;
  ghost.style.opacity = GHOST_OPACITY;
  ghost.style.pointerEvents = "none";
  // A cloned canvas is blank; the row's waveform is painted across.
  const from = row.querySelectorAll("canvas");
  ghost.querySelectorAll("canvas").forEach((canvas, i) => {
    const source = from[i];
    const ctx = canvas.getContext("2d");
    // A canvas with no size yet cannot be drawn from; it is left blank.
    if (source && ctx && source.width > 0 && source.height > 0) ctx.drawImage(source, 0, 0);
  });
  host.appendChild(ghost);
  transfer.setDragImage(ghost, pointer.x - box.left, pointer.y - box.top);
  // The snapshot is taken as the drag starts; the copy has no other job.
  requestAnimationFrame(() => ghost.remove());
}

/**
 * What the app is costing, in the title bar's corner.
 *
 * Its own component so that a reading re-renders these four figures and
 * nothing else: as a value of the shell it re-rendered the whole window
 * every time, which was most of the webview's idle load.
 *
 * A dash is a figure the platform will not give rather than a zero, which
 * would be a claim. Processor lives in the top bar's own meter, and GPU was
 * always a dash: macOS accounts it per process only to root.
 */
import { formatCount, formatMemory, useAppCost } from "@/store/useDiagnostics";
import { useTooltip } from "@/store/usePreferences";

export function AppCost({ className }: { className?: string | undefined }) {
  const cost = useAppCost();
  const tip = useTooltip();
  return (
    <div className={className} data-testid="app-cost">
      <span title={tip("Resident memory")}>MEM {formatMemory(cost.memoryMb)}</span>
      <span title={tip("Threads in the process")}>THR {formatCount(cost.threads)}</span>
      <span title={tip("Open file descriptors")}>FD {formatCount(cost.openFiles)}</span>
      <span title={tip("Frames a second, timed in the window")}>FPS {formatCount(cost.fps)}</span>
    </div>
  );
}

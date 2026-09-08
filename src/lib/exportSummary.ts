/**
 * What to say when an export finishes.
 *
 * An export to a stick that already holds one is a sync: most of it is usually
 * "left alone", and saying "exported 200 tracks" when 198 of them were never
 * touched tells someone nothing about what just happened to their stick.
 */

export interface ExportCounts {
  tracks: number;
  reused: number;
  removed: number;
  skipped: string[];
  verified: boolean;
}

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

export function exportSummary(name: string, report: ExportCounts): string {
  const copied = Math.max(0, report.tracks - report.reused);
  const parts: string[] = [];

  // A first export has nothing to compare against, so the interesting number
  // is simply how big it is.
  if (report.reused === 0 && report.removed === 0) {
    parts.push(`Exported ${plural(report.tracks, "track")} to ${name}`);
  } else {
    const changes: string[] = [];
    if (copied > 0) changes.push(`${plural(copied, "track")} copied`);
    if (report.reused > 0) changes.push(`${report.reused} unchanged`);
    if (report.removed > 0) changes.push(`${plural(report.removed, "track")} removed`);
    parts.push(`Synced ${name}: ${changes.join(", ")}`);
  }

  if (report.skipped.length > 0) {
    parts.push(`${plural(report.skipped.length, "track")} skipped — the audio was missing`);
  }
  parts.push(report.verified ? "read back and verified" : "but the result did not read back");
  return `${parts.join(". ")}.`;
}

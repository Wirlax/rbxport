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
  playlistsAdded?: number;
  playlistsRemoved?: number;
  skipped: string[];
  verified: boolean;
}

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

export function exportSummary(name: string, report: ExportCounts): string {
  const copied = Math.max(0, report.tracks - report.reused);
  const parts = [
    `${name}: Updated ${plural(copied, "track")}`,
    `Skipped ${plural(report.reused, "track")} (no change)`,
    `Added ${plural(report.playlistsAdded ?? 0, "playlist")}`,
    `Removed ${plural(report.playlistsRemoved ?? 0, "playlist")}`,
  ];

  if (report.skipped.length > 0) {
    parts.push(`${plural(report.skipped.length, "track")} missing`);
  }
  if (!report.verified) parts.push("but the result did not read back");
  return `${parts.join(". ")}.`;
}

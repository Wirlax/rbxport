/**
 * Connected volumes, as the tree and the device panel need them.
 *
 * Kept apart from the components because the interesting parts — what a stick
 * already holds, how full it is, and whether the next export can be a sync —
 * are all decisions rather than rendering.
 */
import type { Device, TreeNode } from "@/ipc/types";
import { formatBytes } from "./format";

/** A device's id in the tree. Prefixed so it cannot collide with a playlist. */
export function deviceId(device: Device): string {
  return `device:${device.path}`;
}

/** The mount point behind a tree id, or null when the id is not a device. */
export function devicePath(id: string): string | null {
  return id.startsWith("device:") ? id.slice("device:".length) : null;
}

/**
 * Devices as tree nodes, so the Devices section renders like every other
 * section rather than needing its own list component.
 */
export function deviceNodes(devices: readonly Device[]): TreeNode[] {
  return devices.map((device) => ({
    id: deviceId(device),
    name: device.name,
    kind: "device" as const,
    depth: 0,
  }));
}

/** `12.4 GB free of 32.0 GB`, or nothing when the size is not known. */
export function capacityText(device: Device): string {
  if (device.totalBytes <= 0) return "";
  return `${formatBytes(device.freeBytes)} free of ${formatBytes(device.totalBytes)}`;
}

/** How full the volume is, 0 to 1, or null when the size is not known. */
export function fullness(device: Device): number | null {
  if (device.totalBytes <= 0) return null;
  const used = Math.max(0, device.totalBytes - device.freeBytes);
  return Math.min(1, used / device.totalBytes);
}

/** What is already on the volume, in a sentence. */
export function contentsText(device: Device): string {
  const found = device.export;
  if (!found) return "No export on this device yet.";
  const tracks = `${found.tracks} track${found.tracks === 1 ? "" : "s"}`;
  const playlists = `${found.playlists} playlist${found.playlists === 1 ? "" : "s"}`;
  if (!found.ours) {
    // Without our manifest there is nothing to diff against, so the next
    // export rewrites the stick. Saying so beforehand beats surprising
    // someone with a twenty-minute copy.
    return `${tracks} in ${playlists}, written by rekordbox. Exporting here writes everything again.`;
  }
  const when = found.written.slice(0, 16);
  return `${tracks} in ${playlists}, last synced ${when}. Only what changed will be copied.`;
}

/**
 * Whether there is enough room for a rough estimate of what an export needs.
 *
 * An estimate, and shown as one: the real figure depends on what is already
 * there, which is only known once the sync has diffed.
 */
export function hasRoomFor(device: Device, bytes: number): boolean {
  return device.totalBytes <= 0 || bytes <= device.freeBytes;
}

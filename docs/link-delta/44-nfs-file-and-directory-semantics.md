# 44. Resolve file read and directory edge behavior

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] The report identifies 64,512 versus 8192 read caps, zero-count status, attribute and directory freshness, count budgets, and symlink lookup differences.

## Task

For each difference, determine client reachability and expected wire result, then implement behavior required by the full claim while preserving path boundaries and read-only NFS semantics.

## Completion evidence

Captured or firmware-driven tests cover large/zero/EOF reads, repeated attributes, directory mutation/pagination/cookies, symlinks, and malformed handles, with exact statuses and data.

## Sources and limits

V7 NFS READ/READDIR/attributes/lookup; R5 NFS/VFS.

Do not enlarge exports or follow escaping symlinks to copy an internal implementation detail.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.


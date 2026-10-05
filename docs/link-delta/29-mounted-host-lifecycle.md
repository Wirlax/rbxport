# 29. Reconcile mounted-host state with device disconnect and stop

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] RBX beacon disconnect/reset leaves NFS mounted-host state alive. Vendor has mounted-host structures and UMNT cleanup, but the exact device-disconnect path is unresolved.

## Task

Trace vendor UMNT, device-disconnect, restart, and stop paths, then make RBX mount identity and cleanup match the observed event boundaries.

## Completion evidence

For each traced event, test whether an old mount handle remains valid, whether a reconnect can remount, and whether one deck's disconnect affects another's mount.

## Sources and limits

V7 mount/UMNT helpers; R1/R2/R5 service lifetime.

Do not clear all mounts on every packet merely to remove stale state.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **explicit unmount mechanics established; cross-service lifecycle blocked**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V7 mount dispatcher `_tkfMountIncomingData` (3641 onward,
UMNT 3801–3832, UMNTALL 3835–3858) tracks hosts by peer IP for each export.
UMNT removes that host from the decoded path's export; UMNTALL does so for
all exports. Only removal of the last host calls `_tkfFSUnMount`
(1661–1690), which frees cached file-tree/handle entries. UMNT returns an
accepted void RPC response even when its path decoder fails. No NFS data
write is implied by unmount. `_tkfFSUnMountAll` returning a constant alone
does not prove global teardown mechanics.

[OBS] R5 tracks mounted host/path associations but does not invalidate VFS
handles on final unmount. Its UMNT decode uses `.ok()`, then
`path.is_none_or(...)`: a malformed UMNT currently takes the same
host-wide removal path as UMNTALL. That differs from the inspected vendor
branch's failed-decode no-removal behavior. R1 service shutdown drops the
servers; R2 numbered disconnect is not connected to per-export host cleanup.
V4 `InnerLinkAPI::stop` (1171–1203) terminates NFS then system manager
after stopping timers; it does not prove one-device-disconnect semantics.

### Unknowns and bounded evidence attempt

Read mount dispatch, VFS unmount, local stop and current maps.
[UNKNOWN] how Link member removal reaches mount-host cleanup, validity of
old handles while another host remains, and outstanding-read behavior at
shutdown. Evidence task: trace IPC removal handlers and capture two-host
mount/read/unmount plus explicit Link disconnect. Paired logical decks
sharing an IP must not be treated as separate host records without proof.

### Implementation handoff

Affected: R5 RPC/VFS/network ownership and R1/R2 lifecycle integration.
Dependencies: 05–08, 19–20, 26–27, 43–44. Fixtures: two hosts on one export,
one host on two exports, duplicate mount, unknown/malformed UMNT,
UMNTALL, last-host removal and old-handle lookup/read before/after. Assert
complete XDR replies and storage/map effects; no installed-library writes.
Smallest check: `RB_LITE_TEST=1 cargo test -p rbl-nfs`.
The malformed-UMNT distinction is source-backed; automatic Link cleanup
remains blocked pending its callback evidence.

## Step 2 — implementation (2026-10-05)

Narrow implementation: malformed specific UMNT now emits the established
accepted void reply without removing any host. Valid specific UMNT retains
path-specific removal, while UMNTALL remains host-wide across exports.

A two-host/two-export fixture covers absent, truncated and odd-length UTF-16
arguments, complete void envelopes, valid path removal, one host's surviving
second export and another host's independent mounts.
`RB_LITE_TEST=1 cargo test -p rbl-nfs` passed **57 tests**.
Automatic Link-disconnect cleanup, last-host VFS handle invalidation and
outstanding-read shutdown remain blocked; this does not fabricate that
cross-service lifecycle.

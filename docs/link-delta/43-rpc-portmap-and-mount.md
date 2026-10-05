# 43. Match portmapper and mount procedure behavior

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor portmapper checks service version and accepts SET/UNSET only from loopback; its mount dispatcher handles EXPORTALL but not DUMP. RBX differs on these procedures.

## Task

Implement version-aware GETPORT, loopback SET/UNSET results, and the observed mount procedure set only where required for the claimed behavior; verify map lifetime and reply envelopes.

## Completion evidence

Program/version/transport combinations, local versus remote registration, DUMP, EXPORTALL, MNT, UMNT, and unknown procedures have verified replies and side effects.

## Sources and limits

V7 portmapper/mount dispatch; R5 portmap/mount.

A broad host export is not proven and must not be introduced to imitate an untraced UI decision.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **core procedure differences established; map lifetime/malformed edges need evidence**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V7 `_tkfPmapIncomingData`, SET/UNSET/GETPORT/DUMP and
`_tkfFindMapList` (2836–3096) accept portmap version 2. GETPORT matches
program, service version and transport; SET/UNSET operate only for source
IP exactly 127.0.0.1. SET prepends a mapping and returns true on allocation;
UNSET finds/removes one exact program/version/protocol/port match and returns
true only then. Remote registration returns false, not PROC_UNAVAIL.
GETPORT unknown tuple returns zero. DUMP reflects the map list;
CALLIT/unknown procedures return accepted PROC_UNAVAIL.

[OBS] Mount version 1 dispatcher (3641–3931) supports NULL, MNT, UMNT,
UMNTALL, EXPORT and EXPORTALL; DUMP is PROC_UNAVAIL, not an export list.
MNT checks decoded UTF-16 path, export permission IP/mask and host list,
returns status + 32-byte handle on success, ACCES for unknown/denied export,
IO for relevant allocation failure. Duplicate host mount does not add another
host. EXPORTALL delegates to the same export builder (1711–1784), not a
broader host-tree export. UMNT/UMNTALL and final-host cleanup are issue 29.

Replies preserve XID with accepted RPC envelopes, from the receiving socket
to the requesting IP/port (V7 reply builders 927–998 and 1186–1238).
Unsupported program-version replies carry the supported low/high versions.
[OBS] R5 ignores requested service version in GETPORT, omits SET/UNSET,
uses a static DUMP list, treats mount DUMP as EXPORT and lacks EXPORTALL.
Its code comment claiming vendor registration accepts any host is contradicted
by the actual loopback check.

### Unknowns and bounded evidence attempt

Read the full dispatchers, map lookup, export and reply builders, and
enumerated link-export RPC calls: four GETPORT calls (NFS v2/UDP and
mount v1/UDP), one MNT, one UMNT and two EXPORT calls. No SET/UNSET,
wrong-version, DUMP or EXPORTALL scenario is established there.
[UNKNOWN] map initialization/shutdown lifetime across service restart,
malformed argument behavior and DUMP trailing allocation/length details.
Evidence task: trace map registration/cleanup and capture local/remote
registration and restart; retain exact replies before copying unusual
length behavior. Do not broaden export roots or add remote registration.

### Implementation handoff

Affected: R5 RPC/server state/portmap caller identity and mount dispatch.
Dependencies: 19, 29, 44. Fixtures: all tuple combinations, exact loopback vs
127.0.0.2/LAN, duplicate maps, wrong port UNSET, unknown procedures,
malformed MNT/UMNT, two hosts, readiness and restart. Assert complete XDR,
source/destination and map/mount state. Smallest check:
`RB_LITE_TEST=1 cargo test -p rbl-nfs`.
Version matching, loopback result and mount procedure distinctions can be
isolated; full dynamic-map lifecycle remains unresolved.

## Step 2 — implementation (2026-10-05)

Narrow implementation: GETPORT matches program, service version and UDP
transport; unknown tuples return zero. Remote SET/UNSET (including 127.0.0.2)
return accepted false without mutation. Exactly 127.0.0.1 registration remains
explicitly unsupported until the dynamic-map lifetime contract is recovered.
Mount DUMP is PROC_UNAVAIL; EXPORTALL uses the same bounded export builder
as EXPORT.

Fixtures compare complete XDR replies for version/transport tuples, both
remote registration procedures, unchanged real mappings, DUMP and identical
EXPORT/EXPORTALL output. `RB_LITE_TEST=1 cargo test -p rbl-nfs` passed
**57 tests**.
Local registration semantics, dynamic DUMP/map initialization/shutdown,
vendor malformed-argument policy and broader mount/VFS lifecycle remain
blocked; export roots and filesystem protections were not broadened.

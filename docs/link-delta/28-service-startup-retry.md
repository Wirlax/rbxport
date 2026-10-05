# 28. Recover from transient Link service startup failures

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] Vendor InnerLinkAPI startup retries up to three times with one-second delays; RBX returns a first startup error.

## Task

Implement the observed retry boundary for recoverable bind/start failures, including cleanup of partially started services and a final contextual error. Confirm which failures are retryable before applying the loop.

## Completion evidence

Fault-injected first and second transient failures recover with no duplicate listeners; persistent failure leaves no sockets/threads and reports an error. Normal startup still works.

## Sources and limits

V3 InnerLinkAPI::start; R1 Link::start.

The vendor retries do not justify hiding permanent port conflicts or mutating unrelated services.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.

## Step 1 — investigation (2026-10-05)

Disposition: **original retry premise narrowed; implementation blocked on failure reachability**.
[Evidence key](investigation.md).

### Contract and current implementation

[OBS] V3 `InnerLinkAPI::start` (87–190) initializes its message manager,
then calls NFS start, then system-manager start. Each has an initial call
and at most two retry calls with 1000 ms sleeps after failures (including
the final failure), not three retries. Message-manager initialization
failure returns 0. Port-occupied detection reports/logs an error but does
not abort this routine. After helper attempts, it sets the started flag
and starts the remaining receiver without checking a retained all-failed
result. There is no packet ACK for this local API.

Crucial [OBS]: V1 `PSvLinkProcess::executeSystemManager` (17111–17155)
and `executeNFSd` (17158–17206) both return 1 on their normal paths in
this build. Their retry flag skips thread startup; they send an IPC message.
A failed socket bind is not demonstrated to make either helper return 0.
Thus the caller's conditional retry code does not establish reachable bind
retry behavior. R1 starts its beacon/database/file services once and uses
their Drop cleanup on failure; this is not equivalent to the vendor IPC API.

### Unknowns and bounded evidence attempt

Followed the cited caller down into both actual helpers and checked R1 and
service Drop implementations. [UNKNOWN] any reachable false-return path,
other-platform helper implementation, and externally visible rollback or
retry timing after a genuine helper failure. The supplied captures cannot
resolve local startup failures. Evidence task: inspect the relevant other
build/helper or instrument a controlled startup failure. Do not implement
“retry every RBX bind three times” as a parity fix from this evidence.

### Implementation handoff

Affected: R1 service ownership/startup, R2 beacon startup, R3/R5 networking
constructors, if failure behavior is established. Dependencies: 11 and 20.
Fixtures: injected service failures/temporary occupied ports, not production
services, to assert ordering, attempt count, cleanup, retry cancellation and
a subsequent successful start. Separate an RBX robustness design from a
vendor parity claim. Smallest prerequisite:
`RB_LITE_TEST=1 cargo test -p rbl-link --lib`; a fake-clock startup test
must be added once the reachable contract is known.

# 28. Recover from transient Link service startup failures

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

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


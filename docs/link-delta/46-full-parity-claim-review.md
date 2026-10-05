# 46. Publish a bounded full behavior parity claim

Status: planned. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](../../link-delta.md). Priority follows the basic parity worklist, 01–24.

## Observed gap

[OBS] A generic statement that RBX matches rekordbox cannot follow from completed basic tasks or static dispatcher coverage.

## Task

Review tasks 25–45 and the complete request inventory. Publish the exact feature, device, OS, firmware, and version scope; list tolerated differences only when their observable effects are proven equivalent or explicitly outside scope.

## Completion evidence

A reviewer can reproduce the evidence for every included operation and see which gaps remain. Any unresolved reachable behavior prevents an unqualified full-parity claim.

## Sources and limits

task 25 matrix; task 24 basic acceptance; tasks 26–45; source report evidence appendix.

Absolute parity across all inputs and future builds is not a testable promise; claims must be versioned and scoped.

Recheck the current implementation and source build before making a change. Record request arguments, complete replies, state and storage effects, and client follow-up. Use temporary fixture libraries for write tests. Separate static, socket test, firmware, and physical-device evidence.


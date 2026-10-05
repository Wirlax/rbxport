# 46. Publish a bounded full behavior parity claim

Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Scope: broader Link behavior parity. Source: [rekordbox/RBX comparison](link-delta.md). Priority follows the basic parity worklist, 01–24.

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

## Step 1 — investigation (2026-10-05)

Disposition: **claim-review contract established; broad parity claim currently prohibited**.
[Evidence key](investigation.md).

### Review contract

This issue consumes evidence, not packets. Input: issue 25's closed,
versioned request inventory; contracts/results 01–44; basic acceptance 24;
and per-cell acceptance 45. The output is a reproducible statement naming
included features, reference build/OS, devices/firmware, network modes and
known exceptions. Each included operation must resolve accepted inputs/
guards, reply or intentional silence/destination, state/storage mutation
and client follow-up through direct evidence. A matching unit-test output
without a vendor/client oracle is not enough.

A difference may be retained only as a clearly named exclusion or with
evidence that its observable effect is equivalent in the claimed scope.
For example, 8192 versus 64512 READ caps may be a compatibility choice,
but identical file bytes alone do not establish equivalent timing/retries;
symlink/export restrictions cannot be erased from the claim (issue 44).
Future/uninspected builds and devices remain outside scope.

### Current evidence and bounded attempt

[OBS] Reviewed the source report's provenance/limits, current R1–R5 paths,
the issue investigations and strategy/capture artifacts. Concrete open
examples include retry/callback state (04/10/20/28), unavailable payload
or follow-up contracts (13–15/18/30–42), and no fresh acceptance traces
for 21–24/45. Issue 01's focused passing regression is only a model test.
There is therefore no evidence basis for publishing “full parity” now.

[UNKNOWN] final claim scope and future closure/results for reachable gaps.
Evidence task: review every inventory row against retained evidence, resolve
all included unknowns, explicitly classify every excluded feature, and
independently reproduce representative success/failure/reconnect cases.
No marketing/publication or code change follows from this investigation.

### Review handoff

Affected: this directory's versioned evidence index, test strategy and the
eventual user-facing compatibility statement; no production source change
is required by the review itself. Dependencies: all 01–45, especially 25
and 45. Require hashes/trace links and pass/fail/blocked status per row/cell,
with static, socket, booted-firmware and physical evidence kept separate.
Fixtures: reuse the retained, versioned fixtures and traces from the included
issues; no new packet fixture is required for the prose review itself.
Smallest document prerequisite: `git diff --check -- docs/link-delta`;
technical prerequisites remain each issue's focused commands and actual
acceptance scenarios, not a single broad “green tests” claim.

The step-1 deliverable is this review criterion plus the documented current
failure to qualify. It does not declare implementation, acceptance or an
unqualified parity statement complete.

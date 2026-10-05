# Step 2 implementation record

Date: 2026-10-05. Baseline: `8b04894`.

Step 3 review and remediation: [review.md](review.md) records the initial
three P2 contract mismatches, their corrections and Astra's final closure.
The bounded patch now passes source/diff review; device acceptance and
full-parity claims remain gated. The Step 2 validation history below is
retained separately from the post-review checks. The [Step 4 commit ledger](commits.md)
records the per-issue commits and validation.

Scope: step 2 of [todo.md](todo.md), using the contracts established in the
[step 1 investigation](investigation.md). The implementation model is
`gpt-6.1-sol`, reasoning `high`, as requested by that workflow. Each changed
issue records its implemented scope and focused tests in a dated section.

The table covers all 46 issues. An implemented part of an issue does not
close its remaining evidence tasks. Full wireless negotiation, device UI,
local callback delivery and hardware acceptance retain their documented
limits. The firmware-coverage checklist informs byte/state regressions and
keeps source, fixture/socket, firmware and physical evidence distinct.

## Implementation disposition

| Issue | Step 2 scope or blocker |
| --- | --- |
| [01](01-unsupported-command-errors.md) | Existing error handler retained; add complete wire, setup, multi-location, filter, silent-command and malformed-codec regressions. |
| [02](02-id-block-probe-replies.md) | Separate subtype-2 masks from ordinary probes; explicit mode, exact counter replies and destination tests. Full wireless negotiation remains outside the established contract. |
| [03](03-occupied-number-during-probing.md) | Record valid foreign keepalive occupancy during probing; skip occupied candidates. |
| [04](04-running-number-collision.md) | Blocked: collision retry timing and timer callbacks remain unresolved. |
| [05](05-numbered-player-disconnect.md) | Remove the numbered member and the evidenced 9→10 / 11→12 pairs from peer/player state; retain shared-IP survivors and pending slot timers. Apply the shared initialized-interface sender guard. |
| [06](06-peer-address-change.md) | Bounded correction for matching payload/sender IP; update peer MAC and command destination. Disagreeing addresses and vendor greeting follow-up remain unresolved. |
| [07](07-rediscovery-reset.md) | Type-scoped MAC rediscovery cleanup using coherent numbered removal. |
| [08](08-periodic-peer-expiry.md) | Invoke peer expiry periodically, preserving the existing six-second timeout. Issue 26 adds the established received-timer ownership and paired synthetic removal; full vendor thresholds and refresh sources remain gated. |
| [09](09-rejection-handling.md) | Add known wired/wireless terminal rejection: source-backed sender/runtime-model guards, conditional exact disconnect, immediate zero readiness and coherent state clearing. No collision retry or ordinary-frame recovery; Unknown mode and full vendor session/latch-clear lifecycle remain gated. |
| [10](10-time-server-reset.md) | Blocked: retry details shared with 04 are missing. |
| [11](11-database-ready-gate.md) | Isolate the network-session readiness/identity invariant with one number snapshot and an unanswered pre-ready close. Exact vendor transport refusal, existing-session teardown and reacquisition remain unproved. |
| [12](12-wired-original-cdj-eligibility.md) | Explicit mode and macOS SystemConfiguration MAC classifier; original-model eligibility and outer filtering distinguish constructor, classified and rejected runtime states. Other-platform detection and full old-device compatibility remain unproved. |
| [13](13-track-specific-vbr-reply.md) | Correct VBR names/comments and preserve the complete existing compatibility envelope. Track-specific retrieval, unavailable-data reply sequence and client validation remain blocked. |
| [14](14-legacy-cue-read.md) | Blocked: full serializer/setup variants, tie ordering and error boundary need reference fixtures. |
| [15](15-key-information-read.md) | Blocked: PKEY reference fixture, failed-load boundary and client consumption remain unresolved. |
| [16](16-browse-type-context.md) | Blocked: RX3 fallback compatibility must be established before changing the supported reply. |
| [17](17-played-state-value.md) | Return scalar 2 only for played tracks in context byte 4; preserve internal boolean state and menus. |
| [18](18-load-ack-status.md) | Isolate neutral parsing/reporting of the established response fields; remove the unsupported acceptance log. Status enum and actual load-success behavior remain blocked. |
| [19](19-rpc-cache-socket-identity.md) | Include a fresh receiving-loop token in the RPC cache key; verify mutation replay on two real sockets. |
| [20](20-network-loss-recovery.md) | Blocked: complete network-loss detector and recovery callbacks remain unavailable. |
| [21](21-core-discovery-reply-validation.md) | Evidence gate: matching source-visible vendor/client traces and current socket comparison still required. |
| [22](22-core-browse-reply-validation.md) | Evidence gate: paired browse sequence/fixture comparison and prerequisite 16 remain open. |
| [23](23-core-load-data-validation.md) | Evidence gate: load comparison depends on unresolved 13–15. |
| [24](24-basic-parity-acceptance.md) | Evidence gate: fresh versioned lifecycle acceptance requires 21–23 and actual device/firmware traces. |
| [25](25-complete-request-inventory.md) | Evidence gate: exhaustive reachability and missing callback/emitter closure remain open. |
| [26](26-logical-multideck-members.md) | Add the established running-state logical identities after first-session coexistence and shared sender guards, preserving raw flags and pending slot timers across removal/recreation. Apply only evidenced paired expiry/removal; complete OPUS group lifetime and notification equivalence remain unresolved. |
| [27](27-compatibility-reset-guard.md) | Isolate the source-backed idle no-op guard; downstream teardown and compatibility-mode follow-ups remain gated. |
| [28](28-service-startup-retry.md) | Blocked: visible retry code does not establish a reachable bind-failure retry. |
| [29](29-mounted-host-lifecycle.md) | Isolate malformed UMNT no-removal behavior with a void success reply. Automatic Link cleanup and final-host handle invalidation remain gated. |
| [30](30-beat-master-handover.md) | Blocked: master arbitration and reply callbacks are absent. |
| [31](31-monitor-requests.md) | Blocked: monitor response and notification consumers are absent. |
| [32](32-system-manager-tcp.md) | Blocked: complete stream/reply and client-reachability contract is missing. |
| [33](33-device-connect-streams.md) | Blocked: application callbacks, writes and authorization contract are missing. |
| [34](34-my-settings-read-write.md) | Blocked: complete settings replies, storage and write callbacks are missing. |
| [35](35-library-sync-and-device-search.md) | Blocked: initiation, reply and library-sync callbacks remain unresolved. |
| [36](36-advanced-browse-menus.md) | Blocked: menu backend, hierarchy/buckets and complete row replies are missing. |
| [37](37-taglist-playlist-create.md) | Blocked: persistent playlist identity, ordering and rollback contract is missing. |
| [38](38-filter-my-tag-and-persistence.md) | Isolate the established invalid-property scalar 0x32; MyTag operations, ownership and persistence remain blocked. |
| [39](39-database-scalar-and-notice-replies.md) | Isolate established scalar/silent routes, including the exact 3c03 argument mask, and preserve active menus. Complete callback matrix remains blocked. |
| [40](40-bpm-and-rating-writes.md) | Blocked: the boolean edit adapter cannot preserve unknown backend result codes. Changing success to 1 while retaining failure 1 would falsely acknowledge failures. BPM packing and notification delivery are also unresolved. |
| [41](41-analysis-and-cue-writes.md) | Return the proven early 2705/2805/2905 refusals without storage calls; retain explicit unsupported-command responses for other writes. Durable formats, authorization, transaction effects and update consumers remain unresolved. |
| [42](42-account-user-info.md) | Blocked: empty-result client handling and identity lifecycle/consent remain unresolved. |
| [43](43-rpc-portmap-and-mount.md) | Isolate GETPORT version matching, remote SET/UNSET false, mount DUMP refusal and bounded EXPORTALL. Local dynamic maps and lifetime remain blocked. |
| [44](44-nfs-file-and-directory-semantics.md) | Match the established zero-byte READ IO result, including count zero; test full replies and malformed/stale boundaries. Read-size policy, freshness/cookies and client impact remain gated; export confinement is unchanged. |
| [45](45-multi-platform-device-matrix.md) | Evidence gate: advertised cells and fresh device/OS/mode traces remain required. |
| [46](46-full-parity-claim-review.md) | Evidence gate: unresolved reachable behavior and acceptance results prevent a full-parity claim. |

## Step 2 requirement audit

- **Established contracts only:** the table accounts for every numbered issue,
  including blocked dependencies and acceptance tasks. Source checks constrain
  the implemented portions; unknown layouts, callbacks, identity policy and
  device behavior are not filled in with guesses. An explicit unsupported
  response is not treated as a successful write.
- **Implementation model:** bounded execution used `gpt-6.1-sol` at `high`.
  Additional Astra source checks resolve implementation prerequisites only;
  they do not constitute the separate Step 3 diff review.
- **Focused tests:** packet/session tests exercise complete encoded replies
  or intentional silence, relevant malformed inputs, wrong-state and foreign
  context, and preserved menu/membership state. Socket tests cover readiness,
  destinations and receiving-socket cache identity. Each changed issue records
  its particular cases and limits; no vendor malformed-input policy is inferred
  from an RBX defensive check.
- **Architecture and data safety:** no frontend or Tauri command changes are
  made; the IPC boundary and `rbl-index` sorting/filtering stay intact. Tests use
  isolated fixtures and `RB_LITE_TEST=1`, not an installed rekordbox library.
- **Evidence boundary:** these are source-backed implementations with fixture
  and socket regressions. They do not satisfy the dated vendor/device traces,
  advertised OS/mode matrix or full-parity acceptance gates. Those requirements
  remain explicitly blocked in their issue records.

## Validation

Focused Rust results are recorded with the changed issues. The final
affected-crate run for prolink, DBserver and Link passed 202 tests, with two
existing Link timing ignores. The completeness audit added established
portions in 13, 26, 39, 41 and 44, and rechecked rejection prerequisites before
implementing 09. There are 22 dated implementation sections, not 22 claims
of complete issue or device parity.

### Step 2 consolidated Rust validation

After the final code checkpoint, `cargo clippy --workspace --all-targets
-- -D warnings` and `RB_LITE_TEST=1 cargo test --workspace` both passed.
The workspace run includes all unit, integration and doc-test targets,
including the desktop package. Existing ignored timing/CPU and manual
benchmark tests remain ignored; they are not represented as passed.
Clippy reports the existing dependency future-incompatibility notice for
`block v0.1.6`, not a workspace lint failure.

The Step 2 established-contract execution is complete with the blocked
portions explicitly retained above. This is not completion of all 46 issue
acceptance criteria. No CI, vendor comparison or new hardware acceptance
result is claimed.

### Initial validation history

The following Rust totals cover the earlier 17-issue pass only; they are
retained as history, not the final checkout's consolidated results.

`cargo clippy --workspace --all-targets -- -D warnings` passed.
The first workspace test run caught an incorrect captured destination IP
in the new load-response socket fixture. The loopback barrier now names
localhost; the protocol implementation did not need changing.

The corrected Link crate rerun, `RB_LITE_TEST=1 cargo test -p rbl-link`,
passed 72 tests with 2 existing ignores, including all integration targets
and doc tests. The rest of the workspace passed separately with
`RB_LITE_TEST=1 cargo test --workspace --exclude rbl-link`: 1,061 harness
tests passed, with 3 existing ignores. Together the reruns cover the whole
workspace. The retained ignores are environment-sensitive timing/CPU checks
and a manual copy benchmark; they were not enabled or represented as passed.

`git diff --check` passed. The initial documentation audit verified all 46
ordered ledger entries and the 17 implementation sections then present.

### Step 2 frontend and documentation validation

The final documentation audit verified all 46 ordered ledger entries, one
dated Step 2 section in each of the 22 changed issues, and the local
README/ledger links. `git diff --check` passed. The subsequent Step 3 Astra
review is recorded above. At that Step 2 checkpoint no commit or push had
been made; the later per-issue commits are recorded in [commits.md](commits.md).

Frontend files and IPC boundaries are unchanged by these backend passes.
Frontend checks completed successfully:

- `pnpm lint`
- `pnpm build`
- `pnpm test`: 917 passed across 80 test files.
- `pnpm budget`: initial 132.4 / 150 KB gzip; total 289.8 / 320 KB gzip.
- `E2E_PORT=15461 pnpm e2e`: 618 passed, 8 existing skips.

The browser checks use the mock backend. They do not compare the changed
Link packets with a vendor or device oracle. All write fixtures must use
`RB_LITE_TEST=1` and isolated temporary data, as required by AGENTS.md.

### Post-review remediation validation

Sol high corrected F1's missing first-session coexistence history, F2's
missing shared receive-context guard and F3's discarded pending slot timers.
Astra xhigh re-reviewed the final code fingerprint and closed all three.
The re-review also exposed and resolved a wall-clock-dependent fixture;
the corrected regression passed 100 consecutive runs.

The affected all-target run passed 130 tests with two existing ignores.
Affected and full-workspace strict Clippy passed. The final-checkpoint
`RB_LITE_TEST=1 cargo test --workspace` passed 1,166 tests with five existing
ignores and no failures, including desktop and doc-test targets.
[review.md](review.md) records the final fingerprint, documentation audit
and independently rerun checks. Frontend files are unchanged, so the Step 2
frontend results above were not repeated for remediation.

The remediation preserves the existing Player activity and six-second
timeout policies, Unknown-mode boundaries and open vendor/device evidence
tasks. No installed library or host network configuration was changed.

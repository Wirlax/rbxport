# Step 3 review record

Date: 2026-10-05. Current review verdict: **bounded patch approved — F1–F3 resolved**.

The initial review required three P2 corrections. They are implemented and
the final Astra re-review closes all three; see the remediation record below.
The initial findings and source evidence are retained as history. Review
approval does not close device acceptance gates or establish full parity.

This completes the review requested by step 3 of [todo.md](todo.md).
Two reviewers used gpt-6-astra at xhigh: one reviewed Link/Prolink lifecycle
and interface handling; the other reviewed database/catalogue, RPC/NFS and
the complete 46-issue scope ledger. No max escalation was needed. The
firmware-coverage checklist guided the source-to-code and full-reply checks.

The initial review covered the then-uncommitted Step 2 diff against
8b0489463072a2c025de7e11c30e749f6027a84e, including all 21 changed code,
test and dependency files and all 24 changed implementation documents.
The initial 45-file snapshot SHA-256 was
d4d242fee544294a1c8d7094e660ca588ced741a648190383d54a7df45bffc6b.
The code/test/dependency subset SHA-256 was
88481c1e72e3426b80f11b1102f0cd9d7b05fff60ffcc40f4b8f81390117a930.
Each digest hashes sorted relative paths, a NUL, file contents, and a NUL
for each file. The reviewed implementation was unchanged during the initial review;
only this report and its documentation links were added afterward.

The source references use the [investigation evidence key](investigation.md):
V1–V7 are the local macOS rekordbox 7.2.11.0342 exports; F1 is the documented
RX3 1.20 client reference. Installed executable hashes were not refreshed.
Source comparison and the local Rust reproductions below are distinct from
vendor runtime, firmware or physical-device validation.

## Initial findings

The descriptions and line numbers below refer to the initial reviewed
snapshot. Current correction and validation status follows in the remediation
record; these historical reproductions are not results from the corrected code.

### F1 — P2: Apply the coexistence guard before synthesizing members

Location: [beacon.rs](../../crates/rbl-link/src/beacon.rs), lines 249–268,
called at line 942. Issue 26.

The new synthesis requires Running and device type 1–9, then creates the
paired identities. It does not retain the session classification that
controls whether those member numbers may coexist.

[V1 readConfigNotify](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c)
sets runtime byte +0x1aa from the first successful LinkUp trigger: type 7
sets it to 1; other eligible types set it to 0 (7261–7271). The running
branch rejects numbers 9–12 for a non-type-7 session and numbers 1–4 for a
type-7 session (7319–7334), before reaching synthesis (7436–7527).
Issue 26's established contract explicitly includes these coexistence checks.

The review diagnostic used the actual Beacon UDP loop and ordinary Join
acquisition, without injecting a Running state:

    CDJ-3000/type 1 keepalive -> Up17, players=[1]
    OPUS-QUAD/type 7, primary 9 -> Up17, players=[1,9,10,11,12]

The vendor branch rejects the second announcement. RBX instead creates
three additional members and includes them in player/member counts.
The existing test at beacon.rs:1794 seeds members 1/2 and injects Running;
it does not establish the required session history.

Required correction and regression: preserve the first successful session
classification and apply its guard before membership/synthesis. Establish
regular-CDJ and type-7 sessions through Join, then test rejected mixed
membership and permitted 9/11 pairs separately.

### F2 — P2: Apply the initialized-interface sender guard before dispatch

Location: [beacon.rs](../../crates/rbl-link/src/beacon.rs), lines 808–825,
828–831 and 882–896. The sender check currently appears only at line 858
inside the rejection branch. Issues 02, 03, 05, 06, 07 and 26 are affected.

[V5 messageReceived](../../../rbxport-private/verification/link/rekordbox-re/link-delta-network-20261004.c),
1354–1367, rejects the selected own address and, once NetIF has a nonzero
first octet, off-subnet senders before any SysMgr dispatch. This outer guard
does not require a disconnect sender to own the payload-number member.

The new numbered disconnect can remove an on-subnet member in response to
an off-subnet datagram. New discovery can similarly remove entries matching
its MAC. New block replies and occupancy/address changes also need this
receive context. Existing unrelated broad packet acceptance is not itself
the finding; these newly implemented operations bypass the established guard.

A local diagnostic used simulated selected NetIF 192.168.50.2/24, with all
actual traffic confined to loopback, and completed real acquisition:

    initialized_netif_offsubnet_09: state=Up { number: 17 } players=[1]
    initialized_netif_same_offsubnet_07: state=Up { number: 17 } players=[]

Thus the initialized context is enforced for 09 but not for numbered removal.

Required correction and regressions: enforce the shared receive guard on
the new operations for initialized known-mode interfaces. Own-address and
off-subnet 00, 02/02, 07 and 08 must be inert. An unrelated same-subnet
sender may still disconnect the payload-number member. Also cover the new
occupancy/address mutations. Preserve fresh/uninitialized behavior: the
source does not impose that subnet restriction before NetIF initialization.
Unknown classification remains a separately documented boundary.

### F3 — P2: Preserve an armed slot timer across removal and recreation

Location: [rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs),
lines 1235–1238, especially unconditional received_at removal at 1237;
called by [beacon.rs](../../crates/rbl-link/src/beacon.rs):189. Issues 05,
07 and 26, and their interaction with periodic expiry in 08.

[V2 readDisconnect](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c),
589–608, marks the member inactive without stopping its timer. V1
readDiscoveryRequest (6849–7049) similarly removes membership without
stopping those timers. V2 notifyMessage (314–383) only constructs/sends IPC;
it does not cancel a timer. V2 timerFunc (921–923) dispatches ageing, and
[V3 timerFuncAging](../../../rbxport-private/verification/link/rekordbox-re/rb_bystring.c),
3610–3637, stops the indexed timer and checks the slot's current active flag.
V1 synthesis (7436–7553) neither stops nor refreshes a secondary's timer.

Consequently, removing a directly received secondary and recreating it
synthetically before its deadline does not cancel the pending callback.
It can still remove the recreated active slot. Current Rust behavior is:

    observe member 10 at 0
    remove_number(10) before expiry
    observe_synthetic member 10 at 1000
    expire_received(6001)
    -> expired=[] remaining=[10] synthetic=true

RBX has discarded the deadline. This is separate from the documented
unknowns about complete OPUS group teardown and notification consumers.

Required correction and regressions: separate pending slot timers from
active membership. Cover explicit disconnect and rediscovery before expiry,
synthetic recreation retaining the original deadline, direct keepalive
refresh, and consuming a deadline while its slot is inactive. Whole-session
teardown and an already fired deadline remain distinct cases. The existing
[packets.rs](../../crates/rbl-prolink/tests/packets.rs):674 recreation test
correctly follows an already consumed expiry; it does not cover this case.

## Initial coverage and dispositions

Pass below means the documented bounded change passed this source/diff
review. It does not close the entire issue or any device acceptance gate.
The 22 Step 2 sections count implementation records, not approved issues.
At the initial review, F1–F3 prevented approval of the combined patch.

| Issue | Review disposition and checked boundary |
| --- | --- |
| 01 | Pass: 4003 preserves transaction, both menus, filters and intended silence. V4 OnUnknownClientCmd 224–289; session tests 1753/1836 and malformed codec test. |
| 02 | Changes required, F2. Distinct masks, counter, 39-byte reply and payload-IP destination match V1 7059–7144 / V6 1702–1754. Full wireless acquisition remains unproved. |
| 03 | Changes required, F2. Probe occupancy and later candidate skip match V1 7237–7315, subject to the shared sender guard. |
| 04 | Blocked: collision retry timer/arithmetic and callbacks; no new implementation or approval. |
| 05 | Changes required, F2/F3. Numbered removal, state guards and 9→10 / 11→12 pairs otherwise match V2 578–611. |
| 06 | Changes required, F2. Matching payload/sender address correction and command destination otherwise match V1 7402–7466. Disagreement, same-IP MAC lifecycle and vendor greeting follow-up remain unresolved. |
| 07 | Changes required, F2/F3. Type/range/MAC branches otherwise match V1 6832–7056. |
| 08 | Periodic invocation passes its narrow scope; explicit-removal/synthesis interaction requires F3. Six-second policy, independent Player clocks and full vendor thresholds remain bounded. |
| 09 | Pass for known-mode terminal rejection: exact conditional output, runtime identity/model gates, state clearing and zero readiness. Unknown mode, vendor recovery and cross-service shutdown remain unproved. |
| 10 | Blocked by retry behavior shared with 04. |
| 11 | Pass for single-snapshot network readiness/identity. V3 notifyLinkConnect 4236–4296; session tests 2142/2163. Refusal timing, existing-session teardown and reacquisition remain unproved. |
| 12 | Pass for macOS classifier and runtime model eligibility. Complete getLinkIF/LinkUp/frameRead paths, target dependencies and fixtures checked. Other OS modes, full wireless timing and physical original-player compatibility remain unproved. |
| 13 | Pass for VBR naming and preservation only. V4 OnSongAnlzCmd and V6 GetVbrInf/MstLoadVBR checked. The existing 1604-byte zero placeholder is not track-specific VBR or a vendor oracle. |
| 14 | Blocked: cue serializer/fixtures/error boundaries. No 2104 implementation added. |
| 15 | Blocked: PKEY fixture/error/client consumption. No 2a04 implementation added. |
| 16 | Blocked: RX3 fallback compatibility. Existing 3303 reply is unchanged. |
| 17 | Pass: context bits 8–15 equal 4 and played scalar 2/0 match V4 OnOtherCmd 902–924 and RX3 scalar consumption. Device display remains unverified. |
| 18 | Pass for four raw load-response fields and neutral reporting. V5 setData 1379–1404 and dispatch 515–523 checked; no acceptance enum or immediate loaded event invented. |
| 19 | Pass: receiver token, endpoint and 24-byte cache prefix; real two-socket replay test. V7 transport/cache helpers 3360–3454. |
| 20 | Blocked: authoritative network-loss detector/recovery callbacks. |
| 21 | Evidence gate remains unpassed: matched discovery/source-visible trace. |
| 22 | Evidence gate remains unpassed: browse sequence comparison and dependency 16. |
| 23 | Evidence gate remains unpassed: load comparison and dependencies 13–15. |
| 24 | Lifecycle acceptance remains unpassed; unit/socket tests do not close it. |
| 25 | Exhaustive reachability/request inventory remains unproven. |
| 26 | Changes required, F1/F2/F3. Raw flags, ordinary timer promotion and paired removal were reviewed; complete OPUS lifecycle/notification equivalence remains separately unresolved. |
| 27 | Pass for absent/Waiting idle guard against V2 readCompatiRes 749–848. Active compatibility/reset effects remain incomplete; assigned number still clears on the existing next-loop boundary. |
| 28 | Blocked: reachable startup-failure/retry path. |
| 29 | Pass for malformed specific UMNT void-success/no-removal and valid path/host scope. V7 3801–3858. Cross-service cleanup/final-host invalidation remain blocked. |
| 30 | Blocked: master handover arbitration/replies. |
| 31 | Blocked: monitor consumers/replies. |
| 32 | Blocked: robust system-manager TCP framing/reply lifecycle. |
| 33 | Blocked: DeviceConnect callbacks, authorization and storage. |
| 34 | Blocked: settings writes/storage. 3c03 is not implemented as a settings writer. |
| 35 | Blocked: initiation, sync and disconnect callbacks. |
| 36 | Blocked: play-count/MyTag browse backend and complete row contract. |
| 37 | Blocked: playlist identity/persistence/rollback. No success route added. |
| 38 | Pass for typed property/reserved/length guards and scalar 0x32 against V4 731–802 / 6020–6143. MyTag, ownership and persistence remain blocked. |
| 39 | Pass for selected scalar/silent routes, foreign 3401 silence and exact 3c03 mask. V4 OnHistoryCmd/OnPrepareCmd/OnOtherCmd 507–943 checked. Full callback/result matrix remains blocked. |
| 40 | Blocked: backend results/BPM/notifications. Existing rating boolean adapter unchanged. |
| 41 | Pass for early 2705/2805/2905 refusals and complete envelopes. V4 write dispatcher/helpers and V6 RetNewCueToClient checked. No storage/read callbacks; other shapes retain explicit 4003. Durable writes remain blocked. |
| 42 | Blocked: identity availability, lifecycle and consent. Existing fields/policy unchanged. |
| 43 | Pass for selected GETPORT/version, remote false registration, DUMP refusal and EXPORTALL behavior against V7 2836–3096 / 3641–3931. Dynamic maps/lifetime remain blocked. |
| 44 | Pass for zero-byte READ IO, stale/short-read/malformed boundaries and complete replies. V7 401–445 / 2266–2329. Other file/directory policy differences remain blocked. |
| 45 | No new OS/device/firmware/network-mode acceptance cells validated. |
| 46 | Unresolved reachable behavior and acceptance gaps still prohibit full-parity publication. |

The changed database routes introduce no unsupported storage operation
acknowledged as successful. Scalar values, typed failure envelopes, empty
blob encoding, transaction routing and retained menus were compared with
V4/V6 and F1 scalar/extended-cue consumers. Guard-passing unimplemented
writes still receive 4003. Existing VBR, rating, browse-type, user-info,
filter-ownership and NFS freshness differences remain disclosed limitations.

## Initial review validation and residual limits

Review reruns passed:

- DB analysis_write_early_refusals_preserve_menus_without_storage_calls: 1 test.
- RPC duplicate_mounts_are_scoped_to_the_actual_receiving_socket: 1 test.
- NFS read_edge: 5 tests.
- Scoped git diff --check.

The Link reviewer built the initial reviewed library with RB_LITE_TEST=1 and compiled
two small diagnostics against its public APIs. They used temporary binaries
and loopback sockets; neither changed repository source or user data:

    /tmp/rbx-link-formal-review.CoxzIg/probe
    /tmp/rbx-link-formal-review.CoxzIg/sender_probe

Both exited successfully and produced the reproduction outputs in F1–F3.
The temporary binaries are review-session artifacts, not committed tests.
Their state/action sequences above specify the regressions needed in the
repository. Existing Join, classifier, packet, lifecycle, changed-address,
load-response and rejection/readiness tests were inspected for coverage.

The full workspace/frontend gate results in [implementation.md](implementation.md)
were inspected and are unchanged; they were not repeated for this review.
Their passing result does not resolve the three demonstrated mismatches.
No new vendor capture, firmware/device test, installed-library access,
write-persistence test, CI run or release was performed. Frontend IPC and
rbl-index sorting/filtering are unchanged.

Two observations are recorded separately from the three introduced defects:

- The inherited database session module comment at session.rs:5 says every
  layout was captured from a CDJ-3000. That comment overstates provenance for
  source-derived routes; it is not a new protocol defect in this patch.
- The late player-status path correctly checks rejection under the Shared
  lock. An already-started generic status/greeting/reply iteration can still
  retain a stale identity across rejection (beacon.rs:994–1077). Universal
  post-rejection wire silence is neither established nor claimed here;
  full cross-service/session teardown remains a documented evidence gap.

The initial Step 3 review completed with a changes-required verdict: F1–F3
needed correction and focused re-review before the per-issue commit step.
No production code was fixed or committed during that initial review.

## Remediation — 2026-10-05

Implementation used gpt-6.1-sol at high, following the established contract
and firmware-coverage checklist. The original reproductions failed before
the fixes and pass afterward. The changes are limited to five code/test
files: Link's `join.rs`, `beacon.rs` and `tests/beacon.rs`, and Prolink's
`lib.rs` and `tests/packets.rs`. Database, RPC/NFS, dependencies, interface
classification, frontend IPC and sorting/filtering code are unchanged from
the initial reviewed snapshot.

The corrected 21-file code/test/dependency SHA-256 is
6b8db4312fee5c971f455d112fc54a7a65e26a2cce33c571007663d4b0233385,
using the same path/content digest procedure above.

| Finding | Correction and regression coverage |
| --- | --- |
| F1 | Save the triggering device type only on a successful Waiting→Discovery transition. Reject the other session's numbered deck range while Running, before membership/synthesis. Real Join acquisition tests cover ordinary/type-7 first peers, all forbidden numbers, permitted OPUS and 11→12 additions, raw flags, failed wireless original-player attempts, and reset/reclassification. |
| F2 | Move the cached-NetIF guard before all announcement dispatch. Own/off-subnet discovery, probes/blocks, disconnects, compatibility, keepalives and rejection are inert after initialization. Actual probing tests cover 03 replies and keepalive occupancy; same-subnet complete replies, payload-number disconnects and matching address changes remain accepted. Fresh/Unknown and cached-interface-after-reset boundaries remain explicit. |
| F3 | Retain pending slot deadlines independently of active membership. Synthetic recreation inherits an armed deadline; direct reception refreshes it. Consume inactive expiry without removing an unrelated active secondary. Tests cover disconnect, rediscovery, paired disconnect/expiry, timeout boundaries, metadata, recreation after a fired timer and full session clearing. |

An additional Astra source check confirmed F1's classification is independent
of interface mode: V1 `readConfigNotify` 7256–7271 and 7317–7334, and
`linkUpFunc` 6532–6543, 6597–6599 and 6633. Thus an actual successful
Unknown-mode acquisition establishes this history too. Unknown's conservative
original-model exclusion, rejection silence and uninitialized NetIF policy
remain separate and unchanged.

For portability, socket fixtures use actual 127.0.0.1 transport with a
simulated selected/cached 127.0.0.2 address. The pinned-interface fixture
retains its actually assigned address and uses its first pre-initialization
keepalive before status-port checks. No host address alias was added and no
production receive guard was weakened for tests.

Astra's focused rerun caught a nondeterministic timer fixture: wall-clock
receipt could be later than zero while the asserted deadline assumed zero.
The fixture now explicitly seeds its controlled receipt clock, starts from
a deliberately nonzero wall-clock offset, and anchors acquisition ticks
after receipt. The targeted regression passed 100 consecutive runs after
this test-only correction; production code was unchanged.

### Remediation validation

- `RB_LITE_TEST=1 cargo test -p rbl-link -p rbl-prolink --all-targets --quiet`:
  130 passed, two existing ignored. This comprises 78 Link unit, six Beacon
  socket, seven blob, three Link-client and 36 Prolink tests.
- `cargo clippy -p rbl-link -p rbl-prolink --all-targets -- -D warnings`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed, with only
  the existing `block v0.1.6` future-incompatibility notice.
- `RB_LITE_TEST=1 cargo test --workspace` on the final checkpoint: 1,166
  passed, five existing ignored, no failures. This includes unit, integration
  and doc-test targets, including the desktop package.
- Final documentation audit: all 46 ordered implementation/review entries,
  22 Step 2 sections and local links in 25 documents passed. `git diff
  --check` passed, and all 21 reviewed code/test/dependency file hashes
  remained unchanged after re-review.

### Final Astra re-review

The gpt-6-astra xhigh reviewer verified the final digest above and closed
F1, F2 and F3 with no remaining production findings. No max escalation was
needed. Independently rerun checks passed: the corrected timer regression
(one test), all 33 `all_in_one_tests`, the pending-slot Prolink regression
(one test), and Beacon socket integration (six passed, two existing ignored).
Scoped `git diff --check` also passed.

Current disposition: the reviewed bounded portions of issues 02, 03, 05,
06, 07, 08 and 26 now pass after remediation. All other entries in the
initial 46-issue matrix retain their dispositions and limitations; no
blocked behavior or acceptance gate was promoted to complete. The database,
RPC/NFS and other code reviewed earlier is unchanged, as verified against
the initial per-file hashes. The later [Step 4 commit ledger](commits.md)
records the per-issue commits and validation.

The independent Player activity policy, six-second RBX timeout, complete
OPUS lifecycle/notification equivalence and all device/OS/network-mode
acceptance gaps remain unchanged. These fixes are not evidence of full
vendor parity or universal post-rejection wire silence. No installed library,
host networking, CI, release, commit or push was changed by remediation.

# Step 1 investigation record

Investigation date: 2026-10-05. Scope: step 1 of [todo.md](todo.md), for
issues 01–46. Implementation, implementation commits, and parity acceptance
are separate work. Each issue's investigation records the established
contract and the bounded evidence attempt for unresolved behavior.

RBX baseline: `6c34065b9ba031fd1d4d016bc3324caba6136497`, initially clean.
The vendor exports identify the previously inspected macOS rekordbox
7.2.11.0342 build. The executable hashes in [the original report](link-delta.md)
are historical provenance, not a fresh installed-binary verification.
Function names, addresses, and line references below identify the actual
local exports read. `[OBS]` means observed in the cited source unless a
capture or test is explicitly named. `[ASSUME]` identifies an RBX design
proposal, not vendor behavior. `[UNKNOWN]` is never permission to invent a
wire format or acknowledge an unverified write.

## Source key

| Key | Artifact |
| --- | --- |
| V1 | [rb_named.c](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c) |
| V2 | [rb_sysmgr2.c](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c) |
| V3 | [rb_bystring.c](../../../rbxport-private/verification/link/rekordbox-re/rb_bystring.c) |
| V4 | [link-delta-20261004.c](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c) |
| V5 | [link-delta-network-20261004.c](../../../rbxport-private/verification/link/rekordbox-re/link-delta-network-20261004.c) |
| V6 | [link-delta-analysis-20261004.c](../../../rbxport-private/verification/link/rekordbox-re/link-delta-analysis-20261004.c) |
| V7 | [filsine.c](../../../rbxport-private/verification/link/rekordbox-re/filsine.c) |
| C1 | [client-versions.txt](../../../rbxport-private/verification/link/interaction-audit-20260920/client-versions.txt) |
| F1 | [RX3 client readable source, chunk_0026.c](../../../alphatheta-docs/devices/xdj-rx3/application/assets/decompiled-readable/_unattributed/chunk_0026.c): `dbcl_WaitCue`, `dbcl_GetBrowseType`, `dbcl_GetTrackPlayState`; RX3 1.20 reference per the current testing strategy |
| F2 | [RX3 device-server readable source, chunk_0015.c](../../../alphatheta-docs/devices/xdj-rx3/application/assets/decompiled-readable/_unattributed/chunk_0015.c): `DBSMain_OnOtherClientCmd`; not the desktop rekordbox server |
| R1 | [Link](../../crates/rbl-link/src/lib.rs), [app lifecycle](../../src-tauri/src/link.rs) |
| R2 | [join](../../crates/rbl-link/src/join.rs), [beacon](../../crates/rbl-link/src/beacon.rs), [packet codec and peer table](../../crates/rbl-prolink/src/lib.rs) |
| R3 | [session](../../crates/rbl-dbserver/src/session.rs), [wire codec](../../crates/rbl-dbserver/src/lib.rs), [networking](../../crates/rbl-dbserver/src/net.rs), [filters](../../crates/rbl-dbserver/src/filter.rs) |
| R4 | [catalog](../../crates/rbl-link/src/catalog.rs), [blobs](../../crates/rbl-link/src/blobs.rs), [exports](../../crates/rbl-link/src/files.rs) |
| R5 | [RPC](../../crates/rbl-nfs/src/lib.rs), [VFS](../../crates/rbl-nfs/src/vfs.rs), [UDP services](../../crates/rbl-nfs/src/net.rs) |

These links are relative to this directory; private artifacts require the
sibling private repository. An issue cites a key plus the inspected
function/line range rather than treating the old comparison as primary proof.

Additional inspected artifacts: [decoded database capture](../../../rbxport-private/verification/link/dbserver-decoded.txt),
[KUVO exchange](../../../rbxport-private/verification/link/kuvo-delivery-20260919.txt),
[interaction request list](../../../rbxport-private/verification/link/interaction-audit-20260920/rekordbox-requests.txt),
[filter-property probe](../../../rbxport-private/verification/link/interaction-audit-20260920/filter-properties.txt),
[testing strategy](../testing-strategy.md), and
[private firmware harness](../../../rbxport-private/scripts/e2e-link/README.md).

## Shared contract boundaries

Argument indices are zero-based after RemoteDB decoding. Offsets described
as `frame` are from the beginning of that wire payload, not a C object or
local message. Vendor local queue IDs/callback return values are not wire
opcodes/statuses unless the serializer establishes that mapping. V4
`Ret4ByteToClient`, `RetBinToClient` and V6 cue serializers identify the
response route and transaction behavior; malformed-wire acceptance is not
proven merely by reading their post-decoding callbacks.

For announcement issues 02–10/26–27, V1 `frameRead` (6670–6832) first
checks enable state, takes a distinct compatibility path when +0xcde is
set, and applies the wireless original-CDJ exclusion before dispatch.
Per-issue guards are additional to these outer gates. Safe bounds checks
are required in RBX; reproducing unchecked native reads is not a parity
requirement. No complete claim is made about all vendor malformed inputs.

All referenced future write tests require `RB_LITE_TEST=1` and an isolated
`rbl_db::fixture::build` library or temporary VFS/profile files. A proposed
fixture or validation command is not a newly executed test. Source contracts
and captured cases may be ready for bounded implementation while other
branches remain explicitly blocked; do not interpret that as permission to
implement guessed branches or replace an entire issue with a narrower goal.

## Shared announcement capture check

On 2026-10-05, `tshark` enumerated every UDP-50000 payload in the following
available captures. Kind/subtype are bytes 10/11, counted without packet
deduplication. Read-only command, with `CAPTURE` replaced by each path:

```sh
tshark -r CAPTURE -Y 'udp.port == 50000' -T fields -e udp.payload \
  | awk '{print substr($0,21,4)}' | sort | uniq -c
```

| Capture, relative to `rbxport-private/verification/link/` | Result |
| --- | --- |
| `rekordbox-7.2.11-cdj3000-link-export-20260912.pcap` | 664 packets, all `06/00` |
| `rekordbox-7.2.11-cdj3000-browse-tour-20260912.pcap` | 642 packets, all `06/00` |
| `push-load-cdj3000-emu-20260913.pcap` | No UDP-50000 packets |
| `kuvo-delivery-20260919.pcap` | No UDP-50000 packets |
| `interaction-audit-20260920/rekordbox.pcap` | No UDP-50000 packets |

Example full reference keepalive: browse-tour frame 325, source
`192.168.1.14`, destination `192.168.1.255`, payload:

```text
5173707431576d4a4f4c060072656b6f7264626f78000000000000000000000001030036110100e04ccf632ec0a8010e020100000408
```

The captures therefore cannot establish collision, probe-counter, rejection,
rediscovery, time-server-reset, compatibility-reset, or explicit-disconnect
behavior. They do not prove those operations never occur. No new device
trace, app instrumentation, or installed-library write was performed.

## Other capture checks

The same read-only inventory on UDP 50002 (kind at byte 10) found:

| Capture | Kind → packet count |
| --- | --- |
| [link-export](../../../rbxport-private/verification/link/rekordbox-7.2.11-cdj3000-link-export-20260912.pcap) | 05→1, 06→1, 0a→2164, 16→2, 29→3906, 46→1, 47→1 |
| [browse-tour](../../../rbxport-private/verification/link/rekordbox-7.2.11-cdj3000-browse-tour-20260912.pcap) | 0a→2900, 29→2806 |
| [push-load](../../../rbxport-private/verification/link/push-load-cdj3000-emu-20260913.pcap) | 0a→175, 19→1, 1a→1 |

Reproduction:

```sh
tshark -r CAPTURE -Y 'udp.port == 50002' -T fields -e udp.payload \
  | awk '{print substr($0,21,2)}' | sort | uniq -c
```

In link-export, media request/reply are frames 17599/17604, device-settings
read/reply 17731/17742. The latter complete payloads are:

```text
17731: 5173707431576d4a4f4c4643444a2d333030300000000000000000000000000100010004010400e4
17742: 5173707431576d4a4f4c4772656b6f7264626f7800000000000000000000000101110024110400001234567800000001010104010101000002000000000000000000000000000000
```

Both pairs return to the player's UDP 50002 rather than its ephemeral source
port. They do not establish the other settings families or writes.
Link-export's UDP-50001 traffic contains 4042 kind-0b and 72 kind-28
packets, no 26/27 handover; no UDP-50004 or TCP-50000 traffic was found.
This only bounds these captures, not protocol reachability.

Link-export RPC call enumeration found GETPORT (NFS v2/UDP once, mount
v1/UDP three times), MNT once, UMNT once and EXPORT twice. No registration,
DUMP, EXPORTALL or wrong-version probe is established. NFS READ requested
counts were 4096 once, 12288 once, 16384 thirteen times, 32768 679 times;
none requested zero. These are request counts, not proof of matching reply
lengths or complete file reconstruction. Reproduction:

```sh
tshark -r CAPTURE -Y 'rpc.msgtyp == 0 && (rpc.program == 100000 || rpc.program == 100005)' \
  -T fields -e rpc.program -e rpc.procedure -e portmap.prog -e portmap.version -e portmap.proto
tshark -r CAPTURE -Y 'nfs.read.count' -T fields -e nfs.read.count | sort -n | uniq -c
```

Packet counts include retransmissions. The older text request list contains
no hit for the four play-count/MyTag menu opcodes, 3102, 2107/2507 or the
six write opcodes in 41. That bounded search is not a claim about every
packet in every available capture.

## Verification performed in this pass

`RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session
unsupported_command_keeps_the_active_menu_and_echoes_its_kind_in_4003`:
passed, 1 test, 2026-10-05. This proves the existing model-level regression
case only, not all of issue 01's acceptance requirements.

The affected-crate baseline also completed successfully on 2026-10-05:

```sh
RB_LITE_TEST=1 cargo test -p rbl-dbserver -p rbl-prolink -p rbl-link -p rbl-nfs
```

| Crate | Passed | Ignored |
| --- | ---: | ---: |
| `rbl-dbserver` | 70 | 0 |
| `rbl-prolink` | 30 | 0 |
| `rbl-link` | 53 | 2 |
| `rbl-nfs` | 52 | 0 |
| Total | 205 | 2 |

Zero failures; command exit status 0. The two pre-existing ignored beacon
tests are `nothing_is_said_until_a_player_is_heard_and_the_join_settles_on_seventeen`
and `the_status_beacon_runs_at_five_hertz_with_no_tempo_until_a_master_reports`;
both rely on wall-clock timing. They were not forced to run. The single
focused test above is included in the 205, not an additional distinct test.

These tests check current behavior, including some documented differences
such as the rating scalar, IP-wide bye handling and 8192-byte READ cap.
Passing them does not resolve those differences or establish vendor parity.
No new socket comparison, booted-firmware run, physical-device run, CI run,
implementation change or release was performed in this investigation.

## Per-issue disposition

All rows below complete investigation step 1 only. “Established” applies to
the particular contract named, not the entire issue's implementation or
acceptance. Open evidence tasks and dependencies remain in each issue.

| Issue | Investigation result |
| --- | --- |
| [01 — Unsupported-command errors](01-unsupported-command-errors.md) | Existing core fix; validation contract established. |
| [02 — ID-block probes](02-id-block-probe-replies.md) | Packet contract established; mode plumbing required. |
| [03 — Occupied number during probing](03-occupied-number-during-probing.md) | State contract established; depends on 02. |
| [04 — Running-number collision](04-running-number-collision.md) | Implementation blocked on retry-timer evidence. |
| [05 — Numbered disconnect](05-numbered-player-disconnect.md) | Numbered removal contract established. |
| [06 — Peer-address change](06-peer-address-change.md) | Membership replacement established; greeting wire policy unresolved. |
| [07 — Rediscovery reset](07-rediscovery-reset.md) | Type-scoped MAC removal contract established. |
| [08 — Periodic peer expiry](08-periodic-peer-expiry.md) | Periodic expiry established; broader timer parity gated. |
| [09 — Rejection handling](09-rejection-handling.md) | Rejection contract established; do not reuse collision retry. |
| [10 — Time-server reset](10-time-server-reset.md) | Reset branch established; retry details block implementation. |
| [11 — Database-ready gate](11-database-ready-gate.md) | Direct-session gap confirmed; refusal/reacquisition blocked. |
| [12 — Wired original-CDJ eligibility](12-wired-original-cdj-eligibility.md) | macOS mode/eligibility established; other OS modes unproved. |
| [13 — Track-specific VBR](13-track-specific-vbr-reply.md) | Layout recovered; unavailable-sequence and client validation blocked. |
| [14 — Legacy cue read](14-legacy-cue-read.md) | Layout established; golden fixtures and error boundary pending. |
| [15 — Key-information read](15-key-information-read.md) | Data source/layout established; golden validation pending. |
| [16 — Browse-type context](16-browse-type-context.md) | Desktop scalar established; RX3 compatibility evidence required. |
| [17 — Played-state value](17-played-state-value.md) | Wire value/context contract established. |
| [18 — Load ACK](18-load-ack-status.md) | Field extraction established; status enum meaning blocked. |
| [19 — RPC cache socket identity](19-rpc-cache-socket-identity.md) | Cache-key contract established. |
| [20 — Network-loss recovery](20-network-loss-recovery.md) | Vendor transitions recovered; network detector blocked. |
| [21 — Discovery-reply validation](21-core-discovery-reply-validation.md) | Comparison contract defined; acceptance evidence incomplete. |
| [22 — Browse-reply validation](22-core-browse-reply-validation.md) | Sequence/fixture contract defined; paired comparison pending. |
| [23 — Load-data validation](23-core-load-data-validation.md) | Load comparison defined; acceptance blocked by 13–15. |
| [24 — Basic parity acceptance](24-basic-parity-acceptance.md) | Acceptance contract established; no fresh acceptance run. |
| [25 — Complete request inventory](25-complete-request-inventory.md) | Inventory specification established; exhaustive coverage unproven. |
| [26 — Logical multideck members](26-logical-multideck-members.md) | Addition rules established; complete group removal blocked. |
| [27 — Compatibility reset](27-compatibility-reset-guard.md) | Static guard/state contract established; wire follow-up unverified. |
| [28 — Service startup retry](28-service-startup-retry.md) | Original premise narrowed; failure reachability blocks implementation. |
| [29 — Mounted-host lifecycle](29-mounted-host-lifecycle.md) | Explicit unmount mechanics established; cross-service lifecycle blocked. |
| [30 — Beat/master handover](30-beat-master-handover.md) | Receive parsing established; arbitration/reply contract blocked. |
| [31 — Monitor requests](31-monitor-requests.md) | Receiver transitions established; monitor response blocked. |
| [32 — System-manager TCP](32-system-manager-tcp.md) | TCP ingress established; robust stream/reply contract blocked. |
| [33 — DeviceConnect streams](33-device-connect-streams.md) | Two receive state machines mapped; application/write contract blocked. |
| [34 — My Settings](34-my-settings-read-write.md) | Request families and one captured read established; writes blocked. |
| [35 — Library sync/device search](35-library-sync-and-device-search.md) | Receive families mapped; initiation/reply/sync contract blocked. |
| [36 — Advanced browse menus](36-advanced-browse-menus.md) | Opcodes/argument routing established; complete menu contract blocked. |
| [37 — Tag List playlist creation](37-taglist-playlist-create.md) | Callback sequence/reply established; persistent playlist contract blocked. |
| [38 — Filter/MyTag persistence](38-filter-my-tag-and-persistence.md) | Dispatch/guards partly established; ownership/persistence blocked. |
| [39 — Scalars/notices](39-database-scalar-and-notice-replies.md) | Listed reply distinctions established; complete callback matrix blocked. |
| [40 — BPM/rating writes](40-bpm-and-rating-writes.md) | Dispatch/reply/update ordering established; backend validation blocked. |
| [41 — Analysis/cue writes](41-analysis-and-cue-writes.md) | Per-operation guards/refusals mapped; durable write formats blocked. |
| [42 — Account/user information](42-account-user-info.md) | Availability/envelope/provenance established; identity semantics blocked. |
| [43 — Portmap/mount](43-rpc-portmap-and-mount.md) | Core differences established; map lifetime/malformed edges need evidence. |
| [44 — NFS files/directories](44-nfs-file-and-directory-semantics.md) | Differences established; parity versus compatibility policy unresolved. |
| [45 — Platform/device matrix](45-multi-platform-device-matrix.md) | Matrix acceptance contract established; no new cells validated. |
| [46 — Full-parity claim review](46-full-parity-claim-review.md) | Review contract established; broad parity claim currently prohibited. |

## Documentation audit

On 2026-10-05, all 46 consecutively numbered issues were checked for one
dated investigation section, scoped completion status, disposition, contract,
evidence reference, bounded attempt, explicit unknowns, affected files,
fixtures, dependencies and a validation command. All 46 are linked above.
Local Markdown link targets were checked across all 50 Markdown files in
this directory: zero missing targets in this workspace. Private evidence
links still require the named sibling repositories.

`git diff --check -- docs/link-delta` passed. The working-tree change set
contains documentation only. These structural checks supplement the source
and capture investigation; they do not validate protocol assertions by
themselves. The firmware-coverage-review checklist informed the contract
fields and the separation of static, test, firmware and physical evidence.

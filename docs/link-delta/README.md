# Link parity worklist

These are proposed tasks, not implemented fixes. Numbered filenames are recommended execution priority, based on connection/state correctness first, load-data correctness next, and end-to-end proof last. The ordering is an engineering judgment—not evidence that every difference has caused a player failure.

Source: [link-delta.md](link-delta.md). The historical source report is stored alongside this worklist; the dated investigations below recheck its findings against the current checkout.

Basic parity here means a device can discover/select the desktop source, connect with a coherent identity, browse playlists/tracks, obtain load data, and disconnect/reconnect coherently. It does not mean reproducing every rekordbox feature.

## Investigation status — 2026-10-05

Step 1 of [todo.md](todo.md) is recorded for all 46 numbered issues.
Each issue has a dated contract, current-code comparison, bounded evidence
attempt, unresolved evidence tasks, dependencies, fixture requirements and
smallest useful validation command. See the [investigation index](investigation.md)
for the source/capture baseline and per-issue dispositions.

“Step 1 complete” means the investigation is documented, including explicit
blocks where evidence is missing. It does not mean an implementation or an
acceptance gate has passed. The original problem statements are historical;
the dated section takes precedence where current code or deeper source
inspection contradicts them. No production code or installed library was
changed in this pass.

## Tasks

| Priority | One specific outcome |
|---|---|
| 01 | [Return an error without destroying the active menu](01-unsupported-command-errors.md) |
| 02 | [Decode and answer occupied-number bitmasks correctly](02-id-block-probe-replies.md) |
| 03 | [Honor keepalives while selecting a number](03-occupied-number-during-probing.md) |
| 04 | [Reacquire a number when a joined peer claims ours](04-running-number-collision.md) |
| 05 | [Disconnect the correct logical deck and its peer entry](05-numbered-player-disconnect.md) |
| 06 | [Update a known player's address consistently](06-peer-address-change.md) |
| 07 | [Clear obsolete membership when a device rediscovers](07-rediscovery-reset.md) |
| 08 | [Expire silent peers without requiring more incoming traffic](08-periodic-peer-expiry.md) |
| 09 | [Respond to an announcement rejection](09-rejection-handling.md) |
| 10 | [Honor targeted time-server reset requests](10-time-server-reset.md) |
| 11 | [Do not serve sessions using an unnegotiated fallback number](11-database-ready-gate.md) |
| 12 | [Allow original CDJ announcements on the evidenced wired path](12-wired-original-cdj-eligibility.md) |
| 13 | [Serve real VBR data for request 2504](13-track-specific-vbr-reply.md) |
| 14 | [Answer cue request 2104 with the expected cue envelope](14-legacy-cue-read.md) |
| 15 | [Answer key-information request 2a04](15-key-information-read.md) |
| 16 | [Return browse type for the requested context](16-browse-type-context.md) |
| 17 | [Return the evidenced played-state value and context gate](17-played-state-value.md) |
| 18 | [Distinguish load-command response statuses](18-load-ack-status.md) |
| 19 | [Keep duplicate RPC replies separate by receiving socket](19-rpc-cache-socket-identity.md) |
| 20 | [Reset Link when its selected network actually disconnects](20-network-loss-recovery.md) |
| 21 | [Prove discovery-to-source-visible reply parity](21-core-discovery-reply-validation.md) |
| 22 | [Prove the basic playlist-to-track browse sequence](22-core-browse-reply-validation.md) |
| 23 | [Prove the basic track-load data sequence](23-core-load-data-validation.md) |
| 24 | [Verify the complete basic Link lifecycle end to end](24-basic-parity-acceptance.md) |

## Scope and sequencing

- P0 tasks close demonstrated reply/state gaps in the basic connection path. P1 tasks close focused read/diagnostic gaps or supply missing parity evidence.
- Explicit dependencies are stated inside tasks. Checks and evidence collection should accompany each change; tasks 21–24 are focused comparison/acceptance work, not a reason to postpone all testing.
- The firmware coverage checklist shaped the acceptance criteria: requests, full envelopes, state, follow-ups, and client interpretation must be covered. Static, socket-test, firmware, and physical-device evidence remain separate.
- These files authorize no implementation, deployment, commit, or change to a real user library.

## Outside the basic-parity worklist

Settings transfer, mobile DeviceConnect, master/beat handover, monitor/TCP packet-plane features, library sync, play-count/MyTag browsing, Tag List playlist creation, rating/BPM/cue writes, complete filter persistence, and account-data parity are covered by the broader tasks below. Missing receivers are not automatically required for ordinary desktop export.

Do not copy implementation-only differences without demonstrating a requirement: wildcard binding, shutdown order, caches, larger NFS reads, random port selection, buffer sizes, file-handle allocation, and directory ordering are not automatically basic-parity defects. Portmapper/mount edge cases and file/directory freshness remain documented gaps, not erased claims of parity.

Logical-member synthesis, compatibility-reset guards, startup retries, and full mounted-host cleanup are covered by tasks 26–29. The basic acceptance sequence should identify whether a tested device requires any of them.

Completion of tasks 01–24 supports only the recorded basic scenarios and devices. The tasks below address the additional behavior and evidence needed for a broader, versioned claim.

## Further work for a broader behavior claim

These follow the basic tasks in recommended priority order. A task to trace a callback or validate a client flow precedes implementation where the current evidence does not establish its effect. Finishing every implementation task without completing the evidence gates still does not justify a broad claim.

| Priority | One specific outcome |
|---|---|
| 25 | [Inventory every Link request and reply before a full parity claim](25-complete-request-inventory.md) |
| 26 | [Synthesize and remove the vendor's paired logical members](26-logical-multideck-members.md) |
| 27 | [Match the compatibility reset state guard](27-compatibility-reset-guard.md) |
| 28 | [Recover from transient Link service startup failures](28-service-startup-retry.md) |
| 29 | [Reconcile mounted-host state with device disconnect and stop](29-mounted-host-lifecycle.md) |
| 30 | [Handle incoming beat and master handover traffic](30-beat-master-handover.md) |
| 31 | [Handle Link monitor and reset messages](31-monitor-requests.md) |
| 32 | [Implement the system-manager TCP request path](32-system-manager-tcp.md) |
| 33 | [Support the DeviceConnect and DeviceConnect2 stream lifecycle](33-device-connect-streams.md) |
| 34 | [Serve My Settings reads and writes](34-my-settings-read-write.md) |
| 35 | [Handle device search, disconnect, and library sync messages](35-library-sync-and-device-search.md) |
| 36 | [Implement play-count and MyTag browse families](36-advanced-browse-menus.md) |
| 37 | [Create a playlist from the Tag List on request 3102](37-taglist-playlist-create.md) |
| 38 | [Match filter conditions and session persistence](38-filter-my-tag-and-persistence.md) |
| 39 | [Match remaining database scalar, notice, and silent replies](39-database-scalar-and-notice-replies.md) |
| 40 | [Persist BPM and rating changes with update delivery](40-bpm-and-rating-writes.md) |
| 41 | [Implement or correctly refuse analysis and cue save requests](41-analysis-and-cue-writes.md) |
| 42 | [Match account and user-info response behavior](42-account-user-info.md) |
| 43 | [Match portmapper and mount procedure behavior](43-rpc-portmap-and-mount.md) |
| 44 | [Resolve file read and directory edge behavior](44-nfs-file-and-directory-semantics.md) |
| 45 | [Validate each claimed OS, device, firmware, and network mode](45-multi-platform-device-matrix.md) |
| 46 | [Publish a bounded full behavior parity claim](46-full-parity-claim-review.md) |

The current source report compares one inspected macOS rekordbox build. Any statement that RBX “matches rekordbox” must name the features, builds, devices, OS, and network modes that were actually compared, plus remaining exceptions.

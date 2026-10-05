# Step 4 validation and commit record

Date: 2026-10-05. Baseline: `8b0489463072a2c025de7e11c30e749f6027a84e`.
Target branch: `dev`.

The 22 established Step 2 portions approved by the [Step 3 review](review.md)
each have one bounded Conventional Commit with an explanatory body. A
separate documentation commit records the consolidated evidence and ledger.
The remaining parts of those issues, blocked issues and acceptance gates
retain the [implementation record](implementation.md)'s dispositions.

The sequence follows dependencies: issue 12 supplies selected-interface
classification to 02; 11 supplies readiness to 09; and 29 is recorded before
26, following its documented dependency order. Each intermediate commit passed its affected
all-target Rust tests and strict Clippy before creation. The temporary
worktree kept the complete reviewed working copy intact during partitioning.

## Issue commits

| Issue | Commit | Change |
| --- | --- | --- |
| [01](01-unsupported-command-errors.md) | `6a08bbd` | test: preserve LINK menus across unsupported requests |
| [12](12-wired-original-cdj-eligibility.md) | `f19a2a2` | fix: classify LINK interfaces for original players |
| [02](02-id-block-probe-replies.md) | `a20614d` | fix: answer LINK occupancy masks in interface context |
| [03](03-occupied-number-during-probing.md) | `f61005f` | fix: honor keepalives during LINK number probing |
| [05](05-numbered-player-disconnect.md) | `4218aa2` | fix: disconnect the numbered LINK member |
| [06](06-peer-address-change.md) | `247dcd7` | fix: update LINK peer addresses consistently |
| [07](07-rediscovery-reset.md) | `1bfa8fd` | fix: clear stale LINK members on rediscovery |
| [08](08-periodic-peer-expiry.md) | `98d665f` | fix: expire LINK peers without incoming traffic |
| [11](11-database-ready-gate.md) | `345cd4d` | fix: refuse database sessions before LINK readiness |
| [09](09-rejection-handling.md) | `8da475a` | fix: handle known-mode LINK announcement rejection |
| [13](13-track-specific-vbr-reply.md) | `9ef4808` | refactor: name LINK VBR responses accurately |
| [17](17-played-state-value.md) | `d72bc1d` | fix: return the evidenced LINK played-state scalar |
| [18](18-load-ack-status.md) | `959cc96` | fix: report raw LINK load-response fields |
| [19](19-rpc-cache-socket-identity.md) | `2ef8826` | fix: scope RPC replay cache to receiving sockets |
| [29](29-mounted-host-lifecycle.md) | `9bdd1ac` | fix: preserve mount hosts on malformed unmounts |
| [26](26-logical-multideck-members.md) | `52a4613` | feat: retain paired logical LINK identities |
| [27](27-compatibility-reset-guard.md) | `931e73e` | fix: ignore idle LINK compatibility responses |
| [38](38-filter-my-tag-and-persistence.md) | `ac1c133` | fix: return native LINK filter refusal codes |
| [39](39-database-scalar-and-notice-replies.md) | `4a79014` | fix: preserve LINK menus for scalar and silent requests |
| [41](41-analysis-and-cue-writes.md) | `864d5d5` | fix: refuse unsupported LINK analysis writes precisely |
| [43](43-rpc-portmap-and-mount.md) | `dc10795` | fix: match bounded RPC portmap and mount replies |
| [44](44-nfs-file-and-directory-semantics.md) | `201d8d7` | fix: return NFS IO for zero-byte reads |

## Validation

Every issue was checked with `RB_LITE_TEST=1 cargo test` and `cargo clippy`
using `--all-targets` for its affected packages; Clippy used `-- -D warnings`.
The package scopes were:

- DBserver: 01, 11, 17, 38, 39, 41.
- DBserver and Link: 13.
- Link and Prolink: 02, 03, 05, 06, 07, 08, 09, 12, 18, 26, 27.
- NFS/RPC: 19, 29, 43, 44.

`git diff --check` passed before every commit. No installed rekordbox library
was used for writes; the tests used isolated data and test-mode guards.

The assembled 21 code/test/dependency files match the final Astra-reviewed
snapshot byte for byte. Their sorted path/content SHA-256 is
`6b8db4312fee5c971f455d112fc54a7a65e26a2cce33c571007663d4b0233385`.
Partitioning the work did not alter the approved final implementation.

The consolidated checks on the assembled implementation passed:

- `RB_LITE_TEST=1 cargo test --workspace`: 1,166 passed, five existing
  ignored, no failures; includes desktop and doc-test targets.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed. The
  existing `block v0.1.6` future-incompatibility notice is unchanged.

The history audit verified 22 unique issue commits, one issue document per
commit, Conventional Commit titles and bodies, and a single-parent chain
from the baseline. The final documentation audit checks all 46 ledger
entries, the 22 Step 2 sections and the local Markdown links.

Frontend, design-token and icon sources are unchanged. The successful Step 2
frontend gates in [implementation.md](implementation.md) remain the recorded
frontend evidence; they were not repeated for this commit partition.

The recorded static source and fixture/socket evidence does not close the
vendor/firmware/device comparison gates or the full-parity publication gate.

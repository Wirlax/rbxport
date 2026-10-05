# 12. Allow original CDJ announcements on the evidenced wired path

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] RBX unconditionally excludes minor-0 CDJ-2000/CDJ-900 announcements; vendor exclusion is wireless-only.

Direct consequence and boundary: Actual compatibility of every old device is not established by removing this one gate.

## Do this one thing

Represent the evidenced connection mode and apply the model restriction only under the vendor condition. Trace how mode is determined before adding a boolean shortcut.

## Evidence and code

V1 linkUpFunc/readFrame; R2 brings_link_up. Primary artifacts: [V1](../../../rbxport-private/verification/link/rekordbox-re/rb_named.c).

Implementation locations:

- [crates/rbl-prolink/src/lib.rs](../../crates/rbl-prolink/src/lib.rs)
- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Test the same model/version in wired versus wireless mode, unaffected newer models, and unknown mode. Add a packet/integration fixture establishing that the wired eligible announcement advances Link.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **macOS mode/eligibility contract established; other OS modes unproved**.
[Evidence key and capture check](investigation.md).

### Contract and current coverage

- [OBS] V1 `PSvLinkSysMgrNetworkAccess::getLinkIF` (6145–6258,
  `100a3e554`) enumerates SystemConfiguration services of type
  `kSCNetworkInterfaceTypeIEEE80211`, compares their hardware address
  with the selected MAC, returns 0 for a matching wireless interface and
  1 otherwise. `linkUpFunc` (6532–6608) stores that mode, chooses the
  wired/wireless candidate branch, and applies the original-model exclusion
  only when mode is 0. `frameRead` (6734–6746) repeats that condition.
- For valid keepalives from types 1/2/3/7, original `CDJ-2000` and
  `CDJ-900` at header minor version `0` may start the wired path;
  they are excluded on the evidenced wireless branch. Newer versions/models
  remain eligible under the other guards. No immediate reply is implied by
  eligibility: accepted input advances the discovery state and its output.
- R2 `brings_link_up` (473–479) has no mode and excludes those models
  unconditionally. The current `Join` regression enshrines that
  overbroad exclusion. R2 `BeaconConfig` needs explicit selected-interface
  mode passed to eligibility; do not infer wireless from peer number or name.

### Unknowns and bounded evidence attempt

Read the full macOS mode detector, both exclusion sites, and the current
eligibility test. Existing captures feature CDJ-3000, not the two original
models. [UNKNOWN] original-player end-to-end compatibility and corresponding
Windows/Linux interface classification. The vendor error branch uses
`0xff` as unknown and retains special candidate handling; it is not
evidence that unknown mode should be treated as wired. Evidence task:
fixture the macOS detector mapping and obtain a model-specific wired trace;
trace platform mode detection before extending the claim to other OSes.

### Implementation handoff

Affected: `rbl-prolink::brings_link_up`, `rbl-link/src/join.rs`,
`beacon.rs`/`lib.rs` configuration and interface discovery as needed.
Dependencies: none for the eligibility contract; supplies mode evidence to
02 and 08. Fixtures: each original model at minor 0 on wired/wireless/unknown,
newer minor versions, nonplayer types, self-origin and malformed packets,
and a wired accepted keepalive that starts discovery. Do not silently alter
all candidate timing in this issue.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-prolink`, then
`RB_LITE_TEST=1 cargo test -p rbl-link join::tests`.

## Step 2 — implementation (2026-10-05)

Implemented explicit Wired/Wireless/Unknown mode plumbing through Beacon
configuration and Join. On macOS the read-only detector follows the inspected
current SystemConfiguration set/services path and compares IEEE80211 hardware
MACs with the selected MAC. A proved match is wireless; a completed nonmatching
enumeration is wired; unavailable/invalid data before a match is unknown.
Other platforms remain Unknown rather than inferring mode from names/numbers.

Original minor-0 CDJ-2000/CDJ-900 announcements may start the wired path;
known wireless excludes them after classification, while Unknown keeps RBX's
conservative original-model restriction. Newer models/versions retain existing eligibility.
Pure detector fixtures and complete encoded announcement/controlled Join
tests cover mode branches, malformed detector data and the wired first claim.
`RB_LITE_TEST=1 cargo test -p rbl-link --lib` passed.
Full wireless candidate timing, other-platform vendor classification and
physical original-player compatibility remain unverified.

The bounded issue 09 implementation clarified the runtime outer gate. [OBS]
V2's constructor (1–94) initializes runtime mode `+0x1a1` to `0xff`;
V1 `frameRead` (6737–6745) excludes original minor-0 models only when that
runtime byte is zero, independently of selected interface mode. A known-mode
fresh object therefore permits their first LinkUp attempt. V1 `linkUpFunc`
(6499–6593) sets NetIF/runtime IP and configured candidate 17 (wired) or 41
(wireless) before its inner model gate. The first wireless original-player
attempt remains Waiting with that identity, but does not join, add members,
or greet. Subsequent original announcements are excluded before dispatch.

V2 `readReject` clears the runtime mode byte to zero, so this outer exclusion
also becomes active after a wired rejection. Selected mode and configured
candidate are retained separately: an original-named wired rejection can
be processed once but its identical repeat is excluded; newer-model repeats
still use the cached candidate and zero runtime IP. A new object starts with
the runtime exclusion inactive. Encoded tests cover both original names,
fresh/classified/rejected gates, the failed wireless first-attempt identity,
and complete rejection frames. This does not enable original-model wireless
negotiation, change its timer/candidate ladder, or map Unknown to vendor's
defensive `0xff` classification behavior.

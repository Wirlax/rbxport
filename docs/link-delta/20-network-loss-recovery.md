# 20. Reset Link when its selected network actually disconnects

Priority: P1. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] Vendor monitor checks network disconnect/IP/BSSID; RBX only checks interface name/address presence.

Direct consequence and boundary: No permission to widen binding to all interfaces or automatically connect on a different interface is implied.

## Do this one thing

Trace vendor transitions, then add evidence-backed detection/recovery for the selected interface when name/address alone remain present. Keep wireless detection platform-scoped rather than guessing portable APIs.

## Evidence and code

V2 timerCallback; R1 app reporter; R2 monitor. Primary artifacts: [V2](../../../rbxport-private/verification/link/rekordbox-re/rb_sysmgr2.c).

Implementation locations:

- [crates/rbl-link/src/beacon.rs](../../crates/rbl-link/src/beacon.rs)
- [src-tauri/src/link.rs](../../src-tauri/src/link.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Test a retained-address interface that becomes disconnected, IP change, and restart. Record actual wired unplug/replug and applicable wireless change behavior; report supported OS/device boundaries.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **vendor transitions recovered; network-detector implementation blocked**.
[Evidence key](investigation.md).

### Established contract and current coverage

[OBS] The relevant V2 function is `timerFuncNetMonitor`
(979–1208, `100f4dba8`), not just a generic timer callback.
It stops its timer on entry; idle state re-arms at 1000 ms. In wireless mode,
BSSID matching either the original BSSID or a second stored BSSID is allowed.
A different BSSID triggers link-down. Otherwise it checks IP change and
`isNetDisconnect`. A healthy check resets a disconnect budget to 1 wired
or 180 wireless; a disconnected check decrements it and resets when exhausted.
The initial wireless budget is 20 in V1 `linkUpFunc` (6568–6571),
so 180 seconds is not a universal initial-disconnect rule.

On down: remove/notify all members, stop all timers, set state 0, clear
AP state, notify local link state, reset defaults and mode to unknown.
No outgoing goodbye packet is present in this monitor body. Subsequent
link-up follows fresh discovery; this does not authorize selecting another
interface or widening socket binds.

R2 `interface_lost` only checks selected name/address existence.
`announce_loop` marks down and clears the assigned number. R1 app reporter
(`src-tauri/src/link.rs`, 451–468) removes/drops the Link session on down;
it does not automatically reconnect elsewhere.

### Unknowns and bounded evidence attempt

Read V2's full monitor, V1 initial budgets and R1/R2 down handling. Searched
all cited vendor exports for definitions of `isIpChanged/isNetDisconnect`;
only calls were present. [UNKNOWN] authoritative selected-interface link
status checks, second-BSSID meaning, and platform-specific detection APIs.
Existing steady-state captures contain no controlled cable/BSSID event.
Evidence task: export those detector helpers and trace retained-address
unplug/replug and Wi-Fi roaming against the exact build. Do not replace
them with global reachability or assume an interface with an IP is connected.

### Implementation handoff

Affected: `beacon.rs` monitor abstraction and mode/budget state, app
`link.rs` lifecycle tests; platform detector module after evidence.
Dependencies: 11 readiness and 12 mode, with 04/09/10 reset distinctions
preserved. Fixtures: retained IPv4 but disconnected, IP loss/change,
healthy recovery before budget expiry, initial versus steady Wi-Fi budget,
both allowed BSSIDs, changed BSSID, and explicit restart on selected interface.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-link --test beacon`.
Physical unplug/replug and OS-specific Wi-Fi traces remain acceptance tasks.

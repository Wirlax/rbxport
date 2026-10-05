# 22. Prove the basic playlist-to-track browse sequence

Priority: P1 evidence gate. Step 1: complete (2026-10-05). Implementation/acceptance: see disposition below. Source: [comparison report](link-delta.md).

## Why this task exists

[OBS] Both have browse routes, but menu contents, sorting, pagination, and setup parity are not completely established.

Direct consequence and boundary: Depends on 01 and 16. New play-count/MyTag browse features are outside this basic sequence.

## Do this one thing

Compare one controlled root-to-playlist-to-track-to-metadata sequence, including repeated render and multiple menu locations; fix only evidenced differences.

## Evidence and code

V4 list/song-info dispatch; C1 setup probes; R3 menus/render; R4 index adapter. Primary artifacts: [V4](../../../rbxport-private/verification/link/rekordbox-re/link-delta-20261004.c).

Implementation locations:

- [crates/rbl-dbserver/src/session.rs](../../crates/rbl-dbserver/src/session.rs)
- [crates/rbl-link/src/catalog.rs](../../crates/rbl-link/src/catalog.rs)

Recheck the current code and trace the exact reference branch before editing. The audit baseline is dated; a report finding is not proof that the code still has the same gap.

## Done when

Store complete request/reply fixtures for empty/populated playlists, explicit track order, pagination boundaries, and both observed setup shapes. Keep sorting/filtering in rbl-index.

Record complete request/reply and state assertions, not just handler presence. If the required payload or client interpretation is still [UNKNOWN], obtain evidence rather than inventing it.

Use the smallest affected crate checks first; apply the repository's validation gates to substantial implementation changes. Write tests must use `RB_LITE_TEST=1` with temporary fixture libraries, never an installed rekordbox library. Preserve device-specific behavior unless evidence proves a change is needed.

## Step 1 — investigation (2026-10-05)

Disposition: **sequence/fixture contract defined; paired comparison pending**.
[Evidence key](investigation.md).

### Required comparison and current coverage

[OBS] R3 menu storage/render and R4 query adapter are exercised by
`rbl-link/tests/link.rs::a_player_browses_loads_and_reads_a_track_as_the_capture_shows`
and `a_second_player_only_sees_its_own_list`. The former creates a
three-track fixture, browses root→tracks→playlist→members→track-info,
and checks a playlist position. It is an RBX socket-client test, not an
independent vendor comparison.

The required contract is a controlled root→playlist→track→metadata
sequence: numeric context and transaction preserved; initial `4000`
contains request/count; render gives `4001`, ordered `4101` rows,
then `4201`. Preserve count, strings, IDs, row flags, explicit playlist
order, render window and independent menu locations. No database write.
Commands must route through the same session; repeats must not silently
change the remembered menu.

[OBS] V4 `OnListClientCmd`, `OnPlaylistListCmd` (2206 onward),
`OnSongInfCmd` (3548 onward) are the vendor entry points; C1 supplies
one-argument and `[player,20]` setup shapes. Historical
`dbserver-decoded.txt` transactions `17f/180` show a nine-row root;
`183/184` show a 25-item artist window. These are useful references,
but neither supplies every controlled playlist boundary case.

### Unknowns and bounded evidence attempt

Read C1, existing captured session start/windows and current integration
fixture. [UNKNOWN] complete matched bytes for empty/populated playlists,
nonalphabetic stored order, repeated renders, independent locations, and
both setup versions over the same controlled library. Evidence task:
replay the same fixture requests against the named vendor build and RBX,
retain full streams and compare each field, permitting only documented
identity/path normalization. Existing decoder output abbreviates large
blobs and must not replace raw-stream retention.

### Implementation/evidence handoff

Affected: R3 session and R4 catalog only for demonstrated differences;
ordering/filtering/search remains in `rbl-index`. Store golden requests,
replies and a dated fixture/build/hash manifest with row/state assertions.
Dependencies: 01 and 16; a blocked RX3 compatibility case stays blocked.
Fixture cases: empty list, nested/populated playlist with explicit track
order, zero/last/out-of-range page, repeated render, independent sessions
and locations, both setup shapes, unknown command between browse/render.

Smallest validation: `RB_LITE_TEST=1 cargo test -p rbl-dbserver --test session`,
then `RB_LITE_TEST=1 cargo test -p rbl-link --test link`.
No fresh vendor or device acceptance result is claimed.

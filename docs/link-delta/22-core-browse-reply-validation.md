# 22. Prove the basic playlist-to-track browse sequence

Priority: P1 evidence gate. Status: planned. Source: [comparison report](../../link-delta.md).

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


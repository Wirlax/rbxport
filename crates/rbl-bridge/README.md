# rbl-bridge (`tempo-bridge`)

This fork's own: a small **read-only** HTTP server that lets the Tempo app
(iPhone) see the rekordbox playlists of this Mac and hear their tracks. It
reads `master.db` through `rbl-db` in read-only mode (rekordbox may be open)
and never writes the library. It listens on `127.0.0.1` only; the Tempo
container reaches it through `host.containers.internal`.

## Environment

| Variable | Meaning |
| --- | --- |
| `TEMPO_BRIDGE_KEY` | Required. Value of the `X-Bridge-Key` header every `/v1` call must carry. The server refuses to start without it. |
| `TEMPO_BRIDGE_PORT` | Optional, default `4107`. |
| `RBXPORT_OPTIONS` | Optional, read by `rbl_db::detect()`: names another `options.json` (tests). |

## Build and run

```sh
MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --profile fork -p rbl-bridge
TEMPO_BRIDGE_KEY=secret TEMPO_BRIDGE_PORT=4107 target/fork/tempo-bridge
```

`MACOSX_DEPLOYMENT_TARGET` must match the one `scripts/install-app.mjs` sets,
or SQLCipher is rebuilt from scratch. Tests: `RBXPORT_TEST=1 RB_LITE_TEST=1
cargo test -p rbl-bridge`.

## Endpoints

`GET /health` answers `{"ok":true}` without a key. Everything under `/v1`
needs `X-Bridge-Key` (compared in constant time), else `401`. Errors are
`{"error":{"code","message"}}`.

- `/v1/status`: `{bridge, rekordboxRunning, libraryUpdatedAt, trackCount, playlistCount}`
- `/v1/playlists`: every playlist and folder in rekordbox order, with
  `trackCount` (separators not counted) and `miniSets {blocks, emptySlots}`
  (`null` without separators).
- `/v1/playlists/:id`: `{id, name, trackCount, durationSec, items}`, items
  being `{kind:"track", position, track}` or `{kind:"separator", position, label}`;
  `position` counts from 1 over all rows, separators included.
- `/v1/tracks?query=&limit=50&offset=0`: `{total, tracks}`; the query matches
  title, artist and remixer, ignoring case, accents and dots (`amc` finds
  `A.M.C`), every word required; sorted by title; `limit` is capped at 200.
- `/v1/tracks/:id`: one track with its `cues` (`loopMs` is the loop's length).
- `/v1/tracks/:id/audio`: the file, with `Range` (206), `HEAD`, `ETag` and
  `If-None-Match`; `404 file_missing` when it is not on the disk. The path
  comes from the library, never from the client.

Tracks titled `SEPARATORBREMSEN…` are separators: never in `/v1/tracks`, not
counted, and `separator` items in a playlist.

The library is read once into memory and read again when `master.db` or its
`-wal` file changes (checked at most once a second).

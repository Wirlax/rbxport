# rekordbox-lite

A Rekordbox 7 clone that does **export mode only**: manage the shared Rekordbox
library, analyze tracks, write USB exports, and (in progress) serve the library
to CDJs over Pro DJ Link.

macOS and Windows, built with Tauri 2 — Rust backend, React frontend.

Design: [`docs/pre-release/PLAN.md`](docs/pre-release/PLAN.md) · Working notes: [`TODO.md`](TODO.md)

## Principles, in priority order

1. **Performance.** Budgets in `perf-budgets.json` are enforced, not aspirational.
2. **Stability.** No panics across the IPC boundary; edge cases degrade rather than crash.
3. **Correctness.** Byte-compatible with Rekordbox's formats, and the interface matches the real app.

## Where it stands

Measured against a real 38,681-track library, read-only, with Rekordbox running.

| | |
|---|---|
| Open the library | **523 ms** (budget 1 s) |
| Sort any column | **<1 ms** (budget 50 ms) |
| Search | **≤2 ms** (budget 30 ms) |
| Packaged app at idle | **77.5 MB, 0.0% CPU** |
| Analysis files parsed | **97,954 / 97,954** |
| Analysis files re-emitted byte-identically | **99,032 / 99,032** |
| BPM vs Rekordbox's own stamps | median error **0.025 BPM**, 78% within 0.05 |
| Tracks landing on the wrong beat multiple | 5 / 40 (**12%**) |
| Key vs Rekordbox | **49%** exact, 84% harmonically compatible |
| Real USB export written and verified | 25 tracks, 319 MB |

Working: browsing the real library with waveforms, sorting, search, playlists,
track analysis, USB export, and reading Rekordbox's own `export.pdb`. The Pro DJ
Link protocols are implemented and tested end to end over loopback — a stand-in
player mounts our NFS export and pulls a track back byte-identically — but the
database server's *menus* wait on a capture of what Rekordbox actually answers.

Not yet: writing to the library from the interface beyond playlists and track
metadata (see below), and browsing from a real deck.

Known weak: key detection agrees with Rekordbox on 49% of tracks exactly, and
84% once a relative key or a neighbour on the Camelot wheel counts — up from
25% and 52%, though those constants are provisional (see `TODO.md`). Tempo is
accurate where it locks onto the right beat family; **12% of tracks land on a
wrong multiple**, and an attempt to fix that made it worse and was reverted.
Both numbers are measured, not estimated.

## Safety around your library

This application opens your real Rekordbox database. Three rules are enforced in
code rather than left to callers:

- Reads are **read-only** unless write access is asked for explicitly.
- Writes are **refused while Rekordbox is running**, re-checked before every
  transaction.
- With `RB_LITE_TEST=1`, read-write against the *detected* library is refused
  outright, so a test can never touch it.

Nothing is written to the library yet. Several fields are undocumented — the
`Analysed` bitfield, `rb_data_status`, `BeatLoopSize`, the hot-cue palette — and
writing a guess into a collection is how libraries end up in
`~/Library/Pioneer/rekordbox/Corrupt/`. `rbl-difftool` records what Rekordbox
itself writes for a single action; those recordings come first. See
[`recordings/README.md`](recordings/README.md).

## Layout

```
crates/
  rbl-core        ids, timestamps, fourcc
  rbl-db          master.db: detect, key derivation, SQLCipher, guarded writes
  rbl-index       columnar library: views, sort ranks, folded search
  rbl-audio       decoding for analysis
  rbl-analysis    tempo, beat grid, key, waveforms (phrase/vocal are stubs)
  rbl-anlz        ANLZ read/write, byte-exact
  rbl-pdb         DeviceSQL read/write for USB exports
  rbl-export      the export pipeline
  rbl-onelibrary  exportLibrary.db, which rekordbox reads a stick back from
  rbl-difftool    records what Rekordbox writes, so no field is guessed
  rbl-prolink     Pro DJ Link announce, keep-alive, device table
  rbl-dbserver    the remote-database protocol a CDJ browses over
  rbl-nfs         SUN-RPC/XDR, portmap/mount/NFSv2, read-only virtual FS
  rbl-fakecdj     a stand-in player, for tests and for recording Rekordbox
src-tauri/        the shell: commands, panic boundary, state
src/              React frontend
design/           reference captures, measurements, tokens
```

## Commands

```bash
pnpm dev            # the app
pnpm dev:mock       # frontend only, mock backend, plain browser
pnpm test           # vitest
pnpm e2e            # playwright (chromium + webkit), incl. the geometry gate
pnpm tokens         # regenerate CSS tokens from measurements
pnpm measure        # re-measure the reference captures
pnpm icons          # regenerate src/components/icons.tsx from design/icons/ui/
./scripts/build-icons.sh   # regenerate app icons from design/icons/app-icon.svg
cargo test --workspace
cargo clippy --workspace --all-targets

# read-only diagnostics against the installed library
cargo run --release -p rbl-db       --example probe
cargo run --release -p rbl-index    --example bench
cargo run --release -p rbl-anlz     --example rebuild -- 3000
cargo run --release -p rbl-analysis --example golden  -- 60
cargo run --release -p rbl-export   --example real    -- x 25
```

## The interface

Colours, sizes and fonts come from measurements of the real app, not estimates.
`design/measure/measure.ts` re-derives them from screenshots and reports drift
against the committed tokens; `pnpm tokens` regenerates the CSS. The font was
identified as Arial at 16.5 px by fitting ink extents of known strings
(`design/measure/font-report.json`).

Pioneer's own icons are reference for geometry only and are never shipped.

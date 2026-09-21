# Database crash-safety audit — 2026-09-21

Scope: production database mutations in `rbl-db`, `rbl-onelibrary`,
`rbl-export`, `rbl-devices`, and the Tauri callers; database backups/restores;
analysis/artwork dependencies; the disposable `rbl-index` snapshot cache.
No installed library or physical USB device was modified during verification.

## Commit boundaries

| Path | Atomicity and recovery |
| --- | --- |
| `rbl-db::Writer`: playlist/folder create, rename, move, delete, smart rules, membership/reorder | One immediate SQLite transaction per action; dependent rows, sequence changes and update counter commit together. |
| Track import, delete, metadata, ratings, comments, references, My Tags | One immediate transaction per action, including lookup rows and update counter. Read failures now propagate rather than manufacturing a counter or silently omitting rows being renumbered. |
| Relocate | FolderPath and FileNameL now commit in the same transaction. Previously each had its own commit. |
| Reload Tag | All reloaded fields, new lookup rows and counters now commit together. Previously each field committed separately. |
| Cues, cue moves/deletes, tag-list membership, play history, USB cues/history | Existing per-action transactions cover the related rows. USB cue/grid file changes additionally use the app's durable file journal. |
| Analysis registration, clearing analysis, analysis metadata | One transaction per track; callers must durably publish referenced files before registration. |
| Low-level BPM override | Writes a new immutable analysis directory, flushes its files/directories, then commits BPM and AnalysisDataPath together. The original files remain intact if the SQL update fails. Companion analysis files have the same grid. |
| Artwork import/replacement | New unique paths; originals and thumbnails are now durable before the database publishes ImagePath. Old referenced files are never overwritten. |
| App analysis, grid edits and USB cue import | Existing durable before/after file journal. On restart the committed database row determines replay or rollback. Later ordinary row edits recover abandoned journals before changing the decision evidence. Errors after a database call reconcile against the persisted row, including an error reported after commit. |
| OneLibrary Builder | Builds schema and all rows in one transaction in a unique temporary file; only finish publishes a closed, flushed database. Abandoning a builder never publishes a partial database. |
| OneLibrary settings | One immediate transaction. Explicit durable rollback-journal mode also ensures that players ignoring WAL files see the committed result. Mode changes/checkpoint failures are no longer discarded. |
| USB export, initial device library, device settings, sync records | Serialize publishers using an OS file lock. Stage changed files before publishing a durable commit-intent directory. Retain every staged image until the entire set is installed. An interrupted publication rolls forward before the app inspects, imports, verifies, or changes the device again. |
| USB removed/renamed tracks | Remove obsolete audio/analysis only after database and manifest publication succeeds. A crash during cleanup leaves unreferenced files, not missing referenced files. |
| Writer backups | SQLite VACUUM INTO produces a consistent encrypted snapshot including WAL pages. Flush and atomically publish it; incomplete staging is not listed as a backup. |
| App ZIP backups | Also use a SQLite snapshot rather than sequential copying of live main/WAL files. Existing archive staging, validation and durable final rename remain. |
| Low-level restore | Validate a private copy and fold its WAL into the main file; checkpoint/close the old live database; atomically replace it. A corrupt backup never removes the live WAL. Callers must close other handles before restore. |
| App full-library restore | Existing durable swap journal covers database, WAL, analysis, artwork and library settings. Startup rolls back an uncommitted restore or cleans up a committed one before opening the library. |
| Index snapshot cache | Unique temporary file, flush, atomic replacement. A cache miss or invalid fingerprint rebuilds from the authoritative database. |

All writable master-library connections explicitly select synchronous=EXTRA,
fullfsync and checkpoint_fullfsync, reject unsafe journal modes and use committed
reads. SQLite rolls back uncommitted SQL mutations after a process crash. A
read-only library opener that encounters a hot journal replays it through the
normal guarded writable opener and then returns a read-only handle; it does not
bypass the installed-library/test/process gates.

Batch imports intentionally remain collections of committed actions: a crash
may leave a valid imported prefix. They do not claim all-or-nothing behavior for
an entire folder or XML file. Fixture builders, test corruption/injection code
and read-only diagnostic examples are not production mutation paths.

## Verification

Regression coverage includes:

- Child processes exit without destructors before and after a transaction's
  commit, with cache spilling, in DELETE and WAL modes. Reopening read-only
  must recover old or new rows and counters together and pass integrity_check.
- SQL triggers abort relocation, tag reload and BPM updates; earlier changes
  and lookup insertions roll back, and the original grid remains byte-identical.
- A database call reporting failure after commit keeps the matching committed
  file images rather than rolling them back.
- WAL-backed backups reopen without a sidecar and contain committed WAL data.
- Invalid restore input preserves the current main database and live WAL.
- A publication fails after its first file, fails again during recovery, then
  successfully replays the retained images and intended deletion.
- Abandoned publication staging and unfinished OneLibrary builders never expose
  partial results.
- A USB export fails after staging changed audio; previous audio, database and
  manifest remain usable.
- Existing library, export, device-settings, cache, backup, restore, file-journal,
  analysis and grid tests exercise their normal and error paths.

## Limits and operational consequences

A filesystem cannot atomically replace a set of independent filenames. The
app's file journals make those operations recoverable, not instantly atomic to
an unrelated program. After an interrupted USB publication, reopen the device
in this app to finish recovery before using it in rekordbox or a player. After
an interrupted full-library restore or in-place analysis edit, restart this app
before opening that library in another application.

No software can guarantee durability when storage loses acknowledged flushes,
media is physically damaged, or another application writes outside the protocol.
These tests simulate process termination and filesystem/SQL errors; they are
not physical power-cut testing. Unix directory metadata is explicitly flushed;
the Windows directory-flush behavior and physical FAT/exFAT media require native
platform/power-loss validation before making an equivalent power-loss claim.

Snapshots now pay SQLite's VACUUM cost to obtain a consistent view. USB staging
needs space for changed files plus publication copies and adds I/O. Interrupted
staging/cleanup or a failed immutable artwork/grid edit can leave unreferenced
files; those are harmless to database integrity and can be reclaimed separately.

SQLite references: [atomic commit](https://www.sqlite.org/atomiccommit.html),
[synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous),
[fullfsync](https://www.sqlite.org/pragma.html#pragma_fullfsync).

## Results

- Database/core/export/device/cache/OneLibrary suite: 320 tests passed.
- Tauri application suite: 119 passed, one existing test ignored.
- Final targeted writer/export regressions: 111 passed; durable helper tests:
  13 passed (reruns included in the suite counts above).
- `git diff --check`: passed.
- The broader all-target Clippy check remains blocked by pre-existing denied
  lints in Tauri backup commands, `backup_sizes`, `backup_copy` tests and a
  `state` test. Existing warnings also occur in database import, device and
  index code. These are separate from crash-recovery verification.

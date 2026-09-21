# rbxport

rbxport is a GPL-2.0-or-later desktop application for managing a DJ library,
analysing local audio files, creating USB exports, and serving a local library
to supported players over a local network.

It is built with Tauri 2, Rust, and React for macOS and Windows.

## Safety

The application reads a local library by default. Writes require an explicit
action and are refused while a conflicting library process is running. Before
using write features, make an independent backup of your library.

## USB export

Preferences → USB Export includes **Delete music outside playlists**, off by
default. When enabled, playlist export and Sync Manager remove previously
RBXport-exported music that is outside the playlists being synced, including
tracks exported individually. Tracks in any selected playlist are kept.
Cleanup only removes recorded copies on the USB, after the updated export is
published; original music and unrelated USB files stay untouched. With this
option off, individually exported tracks are kept during playlist sync.

**Maximum CDJ compatibility** automatically converts incompatible export copies
to **WAV** (default) or **320 kbps MP3**. Compatible files are copied as-is, and
your originals stay untouched. Conversion is built in; no FFmpeg installation
is needed. See [USB export preferences](docs/usb-export.md) for format details.

## Backups

The Rekordbox Data bar refreshes automatically when its saved estimate is a week
old, on opening Backups or while the pane remains open. **Refresh** updates it
immediately. The last successful estimate is saved across app restarts.

Preferences → Backups lists each saved backup’s date, time, and size. Create a backup
there, or restore or delete an existing one. New backups include the library
database (including playlists, tags, ratings and history), the complete
`PIONEER/USBANLZ` analysis folder (cues, grids, waveforms, phrases and vocals),
and `PIONEER/Artwork` images and thumbnails. Playlist sync selections and the
Automix playlist are included when present. Music files are not backed up.
New backups are single ZIP files using maximum Deflate compression. The
archives are named `rbxport-backup-YYYYMMDD-HHMM.zip` using local, 24-hour time.
An existing backup from the same minute is never overwritten. Compression
runs in parallel based on available CPU capacity. Backups are kept until you delete them. Older database-only backups remain
available and are labelled accordingly.

Use **Change folder…** under **Default backup folder** to choose where future
backups are saved. The setting persists across app restarts. Existing backups
stay in their original folders; the saved-backup list shows the selected folder.
Use **Restore from ZIP…** under **Restore RBXport backup file** at the bottom
to choose a backup anywhere, including one you moved
or renamed. The app checks the archive and shows its date and location before
asking you to confirm the restore. Recovery journals remain in the app’s local
data directory, so changing the destination does not relocate recovery state.

Quit rekordbox before creating or restoring a backup, and turn off PRO DJ LINK
before restoring. Library Protection must be off to restore. A restore replaces
the saved library and analysis, unloads the decks, and refreshes the collection.

## Development

Waveform dragging uses a velocity-controlled low-pass filter: slow scrubs sound
darker, and faster scrubs open the high frequencies. See
[waveform scrubbing](docs/waveform-scrubbing.md) for the cutoff curve and behavior.

```sh
pnpm install
pnpm dev
```

```sh
pnpm test
cargo test
```

### Logs

The app logs to stdout and to a daily file under
`~/Library/Application Support/rbxport/logs` (macOS) or
`%APPDATA%\rbxport\logs` (Windows), the last seven days kept. `LOG_LEVEL`
sets the level for the app's own crates — `error`, `warn`, `info`, `debug`
(the default) or `trace`, which adds LINK's packet-by-packet lines.
`RUST_LOG` replaces the whole filter when a per-crate mix is wanted
(`RUST_LOG=rbl_link=trace,rbxport=info`); `RBXPORT_LOG_DIR` moves the file.

## License

rbxport is licensed under GPL-2.0-or-later. See [LICENSE](LICENSE) and
[LICENSING.md](LICENSING.md).

## Trademark notice

All third-party product names are the property of their respective owners.
rbxport is an independent project and is not affiliated with, endorsed by, or
sponsored by any third-party product or trademark owner.

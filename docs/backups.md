# Backups

The Rekordbox Data bar refreshes automatically when its saved estimate is a week
old, on opening Backups or while the pane remains open. **Refresh** updates it
immediately. The last successful estimate is saved across app restarts.

Preferences → Backups lists each saved backup’s date, time, and size. Create a
backup there, or delete an existing one. New backups include the library
database (including playlists, tags, ratings and history), the complete
`PIONEER/USBANLZ` analysis folder (cues, grids, waveforms, phrases and vocals),
and `PIONEER/Artwork` images and thumbnails. Playlist sync selections and the
Automix playlist are included when present. Music files are not backed up.
New backups are single ZIP files using maximum Deflate compression. The
archives are named `rbexport-YYYYMMDD-HHMM.zip` using local, 24-hour time.
An existing backup from the same minute is never overwritten. Compression
runs in parallel based on available CPU capacity. Backups are kept until you delete them.

New backups also save a `summary.json` inside the ZIP. It records the number
of tracks, playlists, hot cues and memory cues, and the size of each part of
the backup.

Use **Change folder…** under **Default backup folder** to choose where future
backups are saved. The setting persists across app restarts. Existing backups
stay in their original folders; the saved-backup list shows the selected folder.

Quit rekordbox before creating a backup.

## Restoring a backup

Backups are restored with the separate RBXport Restore app, not from RBXport.
Quit RBXport and rekordbox first. RBXport Restore lists your backups and shows
what each one holds, using `summary.json` when the backup has one. It can
restore the whole backup or only some parts: the library database, the
analysis files, the artwork, or the Sync Manager and Automix selections.
If a restore is interrupted, the next start of RBXport or RBXport Restore
rolls it back, or finishes it if the new files were already in place, before
the library is opened.

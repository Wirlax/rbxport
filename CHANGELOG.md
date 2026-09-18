# Changelog

What changed for the person using the app, release by release. Versions are
the git tags; a tag is what the release workflow builds and publishes. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
the version numbers [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Analyze Track writes its result to the library: the beat grid and every
  waveform go into the analysis files beside rekordbox's own, and the BPM,
  key, length and analysis path onto the track. It is back in the track and
  player menus and on `A`; a track rekordbox analysed keeps its phrases.
- Intelligent playlists open, sort, search and export as the tracks their
  rule admits, with their own icon in the tree.
- A stick carries the tracks' artwork and the library's My Tags, and is
  dated the day it was made where the machine is.
- Loops on the player: AU loops the chosen number of beats from the head,
  snapped to the grid when Q is on; MA takes IN and OUT by hand, and
  RELOOP/EXIT either way. The seam is the audio's own join.
- File › Import rekordbox xml… brings a rekordbox XML collection in — the
  files, the playlist tree, and each new track's rating, comment and cues —
  and Export Collection in xml format… writes one.
- The track menu's Reset DJ Play Count, Remove from Collection (which asks
  first) and Convert Memory Cues to Hot Cues are live, and the tree menu's
  Export a playlist to a file writes an m3u8 or a tab-separated txt.
- Preferences › Advanced › Database backs the library up on request, lists
  the backups, and puts one back; and finds duplicates, tracks that share a
  title and an artist, with a copy removed at a time.
- Loops are drawn on both waveforms, and a memory loop called plays as a
  loop.
- An export says which track it is on, and a stick plugged in or pulled out
  is noticed within two seconds.
- The BPM is typed over in the list and the Info tab, and the beat grid is
  retimed to it so the CDJ agrees with the column.
- Add Artwork and Delete Artwork on the Info tab's Artwork page.
- A track played for a minute goes on today's history session and its DJ
  Play Count goes up, as rekordbox records it; Remove from History takes a
  play off again. Preferences › Advanced › Browse turns the recording off.

### Fixed
- A cloud-synced track whose file is not where the library says is exported
  from its local copy rather than skipped.
- The LINK strip stays hidden while nothing is on the network, even when
  LINK cannot be turned on, and the Connect button keeps its space.
- A click that selects a row no longer opens the cell's editor as well.
- A stick's DEVSETTING.DAT is written when the device panel opens on it, as
  rekordbox does, rather than on every export.

## [0.7.0] — 2026-09-16

### Added
- Pro DJ Link (EXPORT). Turn LINK on and the app appears to the CDJs and
  mixer on the network as a rekordbox source: players browse the library
  and load tracks — artwork, waveforms, cues, key and BPM — and play the
  audio itself, exactly as they do from rekordbox. Nothing is written to the
  library.
- The LINK strip along the bottom shows every player and the mixer on the
  network, what each has loaded from the app, and its CUE / PLAY / MASTER /
  SYNC state. Drag a track from the library onto a player to load it there.
- Tempo master. The app can be the network's tempo master: set the BPM with
  the −/+ buttons or take it from whichever player is master, then press
  MASTER, and every player set to SYNC follows the app's tempo and downbeat.
- DJ System › PRO DJ LINK settings. Turn LINK on and off, see the players on
  the network, and choose which network interface LINK runs on — Automatic
  (the one the players are reached through) or a specific one by name.
- The arrow keys move the highlighted track up and down the list; Enter loads
  the highlighted track onto Player 1, and Left / Right beat-jump it.
- Drag a playlist or folder to a new place in the tree — between two others,
  or into a folder — and it stays there in rekordbox too. Rename Playlist and
  Rename Folder in the tree's right-click menu type the name over in place.
- Drag tracks within a playlist to reorder it. The drag is offered only while
  the playlist is shown in its own order — not sorted by a column, searched
  or filtered.
- Artist, Album, Genre and Label are edited in the list: double-click the
  cell (or click it on a selected row, with Edit Library › Double-click to
  edit off), type, and press Enter; Escape abandons.
- The status bar shows the version beside the app's name, and Preferences ›
  About says who the app is by.
- The device panel warns that USB export is still in development before any
  tab is opened.

### Changed
- The Preferences window has the app's own title bar on every OS, and its
  Keyboard pane opens with every group closed.
- DJ System › PRO DJ LINK reads "Connect to PRO DJ LINK" / "Disconnect", with
  the network interface on its own row.
- Cloud Library Sync and Auto Upload are gone from the right-click menus
  rather than greyed — there is no cloud library behind the app. Analyze
  Track is greyed everywhere until analysis is worth offering.
- Scrolling the list quickly no longer shows blank rows: the rows either side
  of the screen are fetched ahead, a row still loading draws a placeholder,
  and waveforms start loading before their row scrolls into view.

### Fixed
- No stray blue border appears around the Preferences or Update window.
- The LINK button says why it cannot turn on — usually rekordbox already
  running and holding the network ports — instead of doing nothing.
- The CPU figure in the title bar shows the app's real load. It read 0%
  whatever the app was doing, and now keeps a decimal below 10% so an idle
  app reads as 0.1% rather than 0%.
- The window reopens where it was left — on a second display, say — even
  after a crash or a force-quit. Its position was saved only on a clean quit.

## [0.6.0] — 2026-09-11

### Fixed
- A rating set in the app is stored as the number of stars, which is how
  rekordbox stores it. Ratings were written on the wrong scale, so a
  four-star track showed five stars in the browser, and the information
  panel read every rating as none.
- The Histories section is there on every start. It was missing whenever
  the library came from the app's own snapshot, which is most starts.
- A folder that has nothing in it yet is drawn and treated as a folder: a
  playlist made from its menu goes inside it, and its menu offers Delete
  Folder.
- Create New Playlist and Create New Folder work from a playlist's menu. They
  did nothing but show an error, because the tree asked for a parent the
  library does not know.
- Dragging a selection onto a playlist adds every selected track, not only
  the one under the hand; dropped on a deck, the first of the selection
  loads, as in rekordbox.
- Right-clicking a selected row keeps the selection, so Remove from
  Playlist takes every track that was chosen.
- A rating or comment set after a playlist edit stays set once the library
  has been re-read, instead of lighting for a moment and going out.

### Changed
- The library is backed up before the first edit of a session, not before
  every edit. Each rating click and each drop on a playlist copied the whole
  database.

## [0.5.3] — 2026-09-10

### Fixed
- On Windows, the Preferences window opens with its contents instead of a
  blank white frame, without the application menu bar on it, and closing the
  main window with Preferences open quits the app rather than leaving it
  running with only Preferences.
- On Windows, the menu's keyboard shortcuts — Ctrl+, for Preferences,
  Ctrl+O, Ctrl+I, Ctrl+B, Ctrl+7 to Ctrl+0 for the layouts — work while the
  app has keyboard focus.
- A window that could not open the library shows an empty list, not the
  previous session's tracks.
- The app idles within its processor budget: the cost readout in the title
  bar cost more than the budget it reports, and now reads every five seconds
  and re-renders only itself.

## [0.5.2] — 2026-09-10

Everything in 0.5.0 and 0.5.1, neither of which published — 0.5.0's macOS
build made a disk image but no update the app could take, and 0.5.1 built on
both platforms but its publish step deleted its own installers before
uploading them. This is the release that ships the self-updating app.

### Fixed
- Escape no longer closes the Update Manager while a download or install
  is running.

## [0.5.1] — 2026-09-10 — not published

## [0.5.0] — 2026-09-10 — not published

### Added
- The app keeps itself up to date. Check for Updates… in the application
  menu, or Preferences › Advanced › Others, opens the Update Manager: it
  shows the version running and the latest one, what changed between them,
  and downloads the new version with a progress bar, installs it and
  restarts. A check runs on its own shortly after launch and only opens the
  window when there is something new; that can be switched off in the same
  Preferences pane. Every download is checked against a signing key built
  into the app before it is installed.
- On Windows the update installs behind a small progress window rather
  than the full installer.

## [0.4.0] — 2026-09-10

Everything tagged as 0.3.0 plus the work below. 0.3.0's build was cancelled
before it published — its tag missed fourteen commits that had not reached
the remote — so this is the release that ships both.

### Added
- The settings gear opens rekordbox's Preferences — View, Audio, Analysis,
  DJ System, Keyboard and Advanced — as a window of its own that can be
  moved, and its choices change the app and are kept between runs. The master
  limiter lives in Audio.
- BEAT SYNC holds a deck to the master's tempo until RST or MASTER ends it;
  it can take a double or half BPM as a match, or match the tempo alone. With
  Q on, play starts a deck on the master's beat, and a synced deck waits for
  the master's next beat before it sounds.
- A quantized cue can snap to a half, quarter or eighth of a beat.
- The Traffic Light lights the keys that go with the loaded track's, and the
  # column sorts a playlist by its own order.
- The 2 PLAYER details are half waveforms that meet at the line between the
  decks.
- A fresh stick takes the DJ System defaults on export; missing tracks can be
  relocated from search folders; an import says which tracks landed.

### Fixed
- A track dragged to a player carries a faded copy of its row, every time,
  in the shell as well as the browser; the sleeve no longer gets a dashed
  border.
- Dragging the waveform of a freshly loaded track sounds right without
  pressing play first.

## [0.3.0] — 2026-09-10 — not published

The build was cancelled before it reached the bucket; see 0.4.0.

### Added
- A master limiter on the mix bus, so two decks at full level no longer
  distort. Settings › Audio output has the switch, a ceiling (−12…0 dBFS,
  −0.3 by default) and a release (10…1000 ms, 100 by default), and shows how
  far the sum is being turned down. The setting is remembered between
  sessions.
- The 2 PLAYER layout is the one rekordbox draws — two full decks meeting in
  the middle — rather than the single deck at half height.
- Every empty sleeve shows the record the track list draws.
- A hot cue on the detail waveform is its lettered square in its colour, under
  the memory cue's red triangle, the way rekordbox draws it.
- The right-click menus in the tree and the track list list what rekordbox's
  do, in its order, at its size and colours.
- The MEMORY list draws ten boxes, so a track with no memory cues shows an
  empty grid rather than nothing.
- The icon column beside the browser offers Information and Sub-Browser only.
- The top bar no longer shows the info button or the Professional badge.

### Fixed
- A track dragged onto a deck loads in the real app, not only in the browser,
  and the simple player's sleeve shows it will take one.
- Dragging a track lights only the playlist under the pointer, not every
  playlist in the tree.
- The beat grid's heads sit at their measured height with the line under
  them.
- The title bar, status bar, search field, scrollbars, icon column and the
  rows above the list are the greys, faces and sizes rekordbox's window has.
- The track list's horizontal scrollbar sits at the panel's foot, not under
  the last row.
- A track without artwork shows rekordbox's record in the list, not a
  coloured tint.

## [0.2.0] — 2026-09-09

### Added
- The deck's INFO tab shows the loaded track's rating, colour, comment and
  file the way rekordbox's manual lists them.
- Hot cues: set from an empty pad, called from a set one, cleared from the
  HOT CUE list; the browser row's CUE mark follows the edit without a reload.
- Memory cues are set, called and deleted from the deck; M, B, N and X are
  bound as rekordbox binds them. Memory cues, hot cues and loops can be added,
  moved and deleted.
- Hot cues show on every waveform in the colours rekordbox paints them.
- The information panel has rekordbox's Summary, Info and Artwork tabs, and
  the Info tab edits what the writer can safely take.
- An Explorer section in the tree opens the disk's folders as track lists; a
  folder with more subfolders than the tree can show says how many were left
  out.
- A selected device opens the six settings tabs rekordbox draws for it, and a
  stick's settings can be read and written back.
- The Track Filter drops down from the browser header and narrows the list by
  BPM, key, rating and colour, all in Rust.
- The sub-browser is a second browser beside the first, opened from the icon
  column, with its own selection.
- The simple player is one strip, the way rekordbox draws it.

### Changed
- Reading a file's tags no longer reads its cover art; a page of loose
  Explorer files reads its tags eight at a time, within a budget.
- End-to-end tests run at the capture's 1800×1130 rather than a device
  preset.

### Fixed
- Cue markers and the HOT CUE list no longer vanish on a start that hits the
  snapshot.
- A loop in the memory list is labelled as rekordbox labels a cue.
- A waveform whose element is swapped by a layout switch is drawn at its new
  size.

## [0.1.0] — 2026-09-09

The first tagged build: an export-mode rekordbox clone that reads the real
library, plays it through its own engine, analyses tracks, serves CDJs over
Pro DJ Link and writes USB exports. Everything below landed between
2026-09-07 and the tag.

### Library
- Reads the real rekordbox `master.db` read-only, within every performance
  budget; starts from a cached library (666 ms down to 75 ms) and draws the
  last screen before the library is read.
- Rust owns sorting, filtering and search; the list fills the window, columns
  can be chosen, reordered and resized, and each kind of table remembers its
  own; sort goes ascending, descending, then off; rows are numbered.
- Playlists can be created, renamed, moved and deleted; tracks dragged onto
  them; ratings, comments and colours edited in the list. Every write is a
  soft delete with a USN bump in one transaction, refused while rekordbox is
  running, with a backup taken before the first write of a session.
- Tracks whose files have gone are found and can be relocated; music files
  can be added to the library; a selection can be analysed with progress that
  can be stopped.
- The Histories section shows the sessions rekordbox recorded and what was
  played in each.
- Artwork and the three-band waveforms are drawn in the browser through the
  `rbl://` scheme; a missing sleeve shows a cached one or an empty square, not
  a broken-image mark.

### Player
- A real audio engine: two decks, one clock, play/pause/seek without clicks,
  a smooth playhead, and a drag-scrub that sounds like a record — slow drags
  turn it evenly, a fast drag plays a burst and stops.
- A deck can be played faster or slower with MASTER TEMPO holding the pitch
  accurately enough to shift a key; beat sync pulls one deck onto the other's
  tempo and bar.
- A channel strip per deck — trim, three bands, kills — and a crossfader; a
  master level whose moves do not crackle, with meters that fall in time.
- The deck rebuilt as the one rekordbox shows: measured geometry and palette,
  CDJ controls, pad and panel tabs that switch, the beat grid, cue markers,
  the bar count, and hot cues drawn in their colours on both waveforms.
- 1 PLAYER, SIMPLE PLAYER and 2 PLAYER layouts; a playing deck carries on
  through a switch. Deck B reads bottom-up so the two waveforms meet.
- A track reaches a deck three ways: dropped on it, from the menu, or by
  clicking an empty one.
- Audio can be sent to any output the machine has.
- rekordbox's own keyboard shortcuts and right-click menus.

### Analysis and devices
- Tracks are analysed for tempo, key and waveforms; analysis files reproduce
  rekordbox's byte for byte; tempo matches rekordbox's value on 95% of tracks
  and to a hundredth of a BPM; key agrees on far more tracks; phrases and
  vocal placement are read and drawn.
- USB exports a CDJ can browse, with every table type rekordbox writes, and
  only what changed copied on a second export to the same stick; rekordbox
  reads a stick this app exported.
- The app announces itself on a Pro DJ Link network, shows which players and
  mixers are on it, serves the remote-database protocol CDJs browse over and
  lets a player mount the library over NFS.

### App
- Native macOS menu bar, the app's own icon, a Content-Security-Policy, and
  the window reopens where it was left — on the screen, not half off it.
- The interface stays responsive while artwork and audio load; the window no
  longer re-renders six hundred times a second.
- Signed builds for macOS (opens without Gatekeeper refusing it) and Windows,
  published with readable download URLs.

[0.5.3]: https://github.com/chrisle/rbxport/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/chrisle/rbxport/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/chrisle/rbxport/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/chrisle/rbxport/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/chrisle/rbxport/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/chrisle/rbxport/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/chrisle/rbxport/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/chrisle/rbxport/releases/tag/v0.1.0

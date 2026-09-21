//! Retain device-only playlists/tracks and history while replacing the selected
//! master-library portion. Device identities are never inferred from titles.
use crate::{
    snapshot::Snapshot, DeviceTrack, ExportError, Manifest, Result, SourcePlaylist, SourceTrack,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
pub(crate) const DEVICE_PLAYLIST_ID_BASE: u64 = 1 << 63;
fn sql(e: impl std::fmt::Display) -> ExportError {
    ExportError::OneLibrary(e.to_string())
}

pub fn prepare(
    root: &Path,
    before: &Snapshot,
    previous: Option<&Manifest>,
    tracks: &[SourceTrack],
    playlists: &[SourcePlaylist],
    db_id: u64,
) -> Result<(Vec<SourceTrack>, Vec<SourcePlaylist>)> {
    let mut tracks = tracks.to_vec();
    let mut playlists = playlists.to_vec();
    let Some(current) = before.merged_library() else {
        return Ok((tracks, playlists));
    };
    let existing_sources = sources(root, before)?;
    for track in &mut tracks {
        if track.device.is_some() {
            continue;
        }
        let identity = before
            .identity
            .iter()
            .find(|(_, (db, content))| *db == db_id && *content == track.id && track.id != 0)
            .map(|(id, _)| *id)
            .or_else(|| {
                previous
                    .filter(|m| m.db_id == db_id)
                    .and_then(|m| {
                        m.tracks.iter().find(|t| {
                            t.key()
                                == crate::track_key(track.id, &track.source_path.to_string_lossy())
                        })
                    })
                    .map(|t| t.export_id)
            });
        if let Some(id) = identity {
            if let Some(source) = existing_sources.get(&id) {
                let mut device = source.device.clone().unwrap_or_default();
                device.preserve = false;
                device.master_db_id = db_id;
                device.master_content_id = track.id;
                track.device = Some(device);
            }
        }
    }
    let record = crate::sync_record::read(root).filter(|r| r.db_id == db_id && db_id != 0);
    for p in &mut playlists {
        if p.device_id != 0 {
            continue;
        }
        p.device_id = previous
            .and_then(|m| {
                m.playlists
                    .iter()
                    .find(|old| old.library_id == p.id && p.id != 0 && !old.device_only)
            })
            .map(|p| p.export_id)
            .or_else(|| {
                record
                    .as_ref()
                    .and_then(|r| r.device_ids.get(&p.id).copied())
            })
            .unwrap_or(0);
    }
    let owned: BTreeSet<u32> = previous
        .map(|m| {
            m.playlists
                .iter()
                .filter(|p| !p.device_only)
                .map(|p| p.export_id)
                .filter(|id| *id != 0)
                .collect()
        })
        .unwrap_or_else(|| {
            record
                .as_ref()
                .map(|r| r.device_ids.values().copied().collect())
                .unwrap_or_default()
        });
    let retained: Vec<_> = current
        .playlists
        .iter()
        .filter(|p| !owned.contains(&p.id) && !playlists.iter().any(|n| n.device_id == p.id))
        .collect();
    let mut needed: BTreeSet<u32> = retained
        .iter()
        .flat_map(|p| &p.tracks)
        .copied()
        .chain(before.history.iter().flat_map(|h| &h.tracks).copied())
        .collect();
    let previously_owned: BTreeSet<u32> = previous
        .map(|m| m.tracks.iter().map(|t| t.export_id).collect())
        .unwrap_or_default();
    // Tracks that arrived through another writer belong to the USB, not to our
    // previous selection. Retain them even when no playlist references them.
    needed.extend(
        current
            .tracks
            .iter()
            .filter(|t| !previously_owned.contains(&t.id))
            .map(|t| t.id),
    );
    let mut positions: BTreeMap<u32, usize> = tracks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.device.as_ref().map(|d| (d.id, i)))
        .collect();
    for id in needed {
        if positions.contains_key(&id) {
            continue;
        }
        let track = existing_sources
            .get(&id)
            .ok_or_else(|| {
                ExportError::Conflict(format!(
                    "Device playlist/history references missing track {id}"
                ))
            })?
            .clone();
        positions.insert(id, tracks.len());
        tracks.push(track);
    }
    // Preserve ancestor folders of device-only playlists too, including folders
    // shared with master playlists that have just been deselected.
    let mut retained_ids: BTreeSet<_> = retained.iter().map(|p| p.id).collect();
    let mut parents: Vec<_> = retained.iter().map(|p| p.parent).collect();
    while let Some(id) = parents.pop() {
        if id == 0 || playlists.iter().any(|p| p.device_id == id) || !retained_ids.insert(id) {
            continue;
        }
        let p = current
            .playlists
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| ExportError::Conflict("Missing device playlist ancestor".into()))?;
        parents.push(p.parent);
    }
    for p in current
        .playlists
        .iter()
        .filter(|p| retained_ids.contains(&p.id))
    {
        let parent_id = if p.parent == 0 {
            0
        } else {
            playlists
                .iter()
                .find(|n| n.device_id == p.parent)
                .map_or(DEVICE_PLAYLIST_ID_BASE + u64::from(p.parent), |n| n.id)
        };
        playlists.push(SourcePlaylist {
            id: DEVICE_PLAYLIST_ID_BASE + u64::from(p.id),
            device_id: p.id,
            device_only: true,
            name: p.name.clone(),
            parent_id,
            folder: p.folder,
            track_indices: p
                .tracks
                .iter()
                .map(|id| {
                    positions.get(id).copied().ok_or_else(|| {
                        ExportError::Conflict("Unresolved device playlist track".into())
                    })
                })
                .collect::<Result<_>>()?,
        });
    }
    Ok((tracks, playlists))
}

/// Read complete track metadata from whichever database exists. Both formats
/// share the audio and analysis files; only the explicit identity map is trusted.
fn sources(root: &Path, before: &Snapshot) -> Result<BTreeMap<u32, SourceTrack>> {
    let mut out = BTreeMap::new();
    let manifest = Manifest::load(root);
    let dir = crate::export_root(root).join("rekordbox");
    if before.one.is_some() {
        let db = rbl_onelibrary::ExportLibrary::open_read_only(&dir.join("exportLibrary.db"))
            .map_err(sql)?;
        let mut q = db.connection().prepare("SELECT c.content_id, COALESCE(c.title,''), COALESCE(a.name,''), COALESCE(al.name,''), COALESCE(g.name,''), COALESCE(l.name,''), COALESCE(k.name,''), COALESCE(c.djComment,''), COALESCE(c.dateAdded,''), COALESCE(c.releaseDate,''), COALESCE(c.bpmx100,0), COALESCE(c.length,0), COALESCE(c.rating,0), COALESCE(c.color_id,0), COALESCE(c.releaseYear,0), COALESCE(c.bitrate,0), COALESCE(c.samplingRate,0), COALESCE(c.fileSize,0), COALESCE(i.path,'') FROM content c LEFT JOIN artist a ON a.artist_id=c.artist_id_artist LEFT JOIN album al ON al.album_id=c.album_id LEFT JOIN genre g ON g.genre_id=c.genre_id LEFT JOIN label l ON l.label_id=c.label_id LEFT JOIN key k ON k.key_id=c.key_id LEFT JOIN image i ON i.image_id=c.image_id").map_err(sql)?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, u32>(0)?,
                    SourceTrack {
                        title: r.get(1)?,
                        artist: r.get(2)?,
                        album: r.get(3)?,
                        genre: r.get(4)?,
                        label: r.get(5)?,
                        key: r.get(6)?,
                        comment: r.get(7)?,
                        date_added: r.get(8)?,
                        release_date: r.get(9)?,
                        bpm_x100: r.get(10)?,
                        duration_sec: r.get(11)?,
                        rating: r.get::<_, u8>(12)? / 51,
                        color_id: r.get(13)?,
                        year: r.get(14)?,
                        bitrate: r.get(15)?,
                        sample_rate: r.get(16)?,
                        file_size: u64::try_from(r.get::<_, i64>(17)?).unwrap_or(0),
                        artwork: None,
                        ..SourceTrack::default()
                    },
                    r.get::<_, String>(18)?,
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (id, mut track, artwork) = row.map_err(sql)?;
            if !artwork.is_empty() {
                track.artwork = Some(crate::checked_under(root, &artwork)?);
            }
            out.insert(id, track);
        }
    }
    if before.legacy.is_some() {
        let bytes = std::fs::read(dir.join("export.pdb"))?;
        let pdb = rbl_pdb::Pdb::parse(&bytes).map_err(sql)?;
        use rbl_pdb::PageType;
        let names = |kind| -> BTreeMap<u32, String> {
            pdb.table(kind)
                .map(|t| {
                    pdb.named_rows(t)
                        .into_iter()
                        .map(|n| (n.id, n.name))
                        .collect()
                })
                .unwrap_or_default()
        };
        let artists = names(PageType::Artists);
        let albums = names(PageType::Albums);
        let genres = names(PageType::Genres);
        let labels = names(PageType::Labels);
        let keys = names(PageType::Keys);
        let art = names(PageType::Artwork);
        for t in pdb
            .table(PageType::Tracks)
            .map(|t| pdb.track_rows(t))
            .unwrap_or_default()
        {
            out.entry(t.id).or_insert(SourceTrack {
                title: t.title,
                artist: artists.get(&t.artist_id).cloned().unwrap_or_default(),
                album: albums.get(&t.album_id).cloned().unwrap_or_default(),
                genre: genres.get(&t.genre_id).cloned().unwrap_or_default(),
                label: labels.get(&t.label_id).cloned().unwrap_or_default(),
                key: keys.get(&t.key_id).cloned().unwrap_or_default(),
                comment: t.comment,
                date_added: t.date_added,
                release_date: t.release_date,
                bpm_x100: t.tempo_x100,
                duration_sec: t.duration_sec,
                rating: t.rating,
                color_id: t.color_id,
                year: t.year,
                bitrate: t.bitrate,
                sample_rate: t.sample_rate,
                file_size: u64::from(t.file_size),
                artwork: art
                    .get(&t.artwork_id)
                    .map(|p| crate::checked_under(root, p))
                    .transpose()?,
                ..SourceTrack::default()
            });
        }
    }
    let current = before.merged_library();
    for t in current.as_ref().into_iter().flat_map(|l| &l.tracks) {
        let track = out
            .get_mut(&t.id)
            .ok_or_else(|| ExportError::Conflict("Missing USB track metadata".into()))?;
        track.source_path = crate::checked_under(root, &t.path)?;
        track.my_tags = before
            .tag_memberships
            .get(&t.id)
            .cloned()
            .unwrap_or_default();
        let analysis_dir = Path::new(&t.analysis)
            .parent()
            .map_or(String::new(), |p| p.to_string_lossy().into_owned());
        if !t.analysis.is_empty() {
            let base = crate::checked_under(root, &t.analysis)?;
            for extension in ["DAT", "EXT", "2EX"] {
                let path = base.with_extension(extension);
                match std::fs::read(&path) {
                    Ok(bytes) => track.analysis.push((extension.into(), bytes)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        let (master_db_id, master_content_id) = before
            .identity
            .get(&t.id)
            .copied()
            .or_else(|| {
                manifest.as_ref().and_then(|m| {
                    m.tracks
                        .iter()
                        .find(|old| old.export_id == t.id)
                        .map(|old| (m.db_id, old.library_id))
                })
            })
            .unwrap_or_default();
        track.device = Some(DeviceTrack {
            id: t.id,
            master_db_id,
            master_content_id,
            audio: t.path.clone(),
            analysis_dir,
            preserve: true,
        });
    }
    Ok(out)
}

/// Convert a missing sibling database without changing the device selection.
pub fn all(
    root: &Path,
    before: &Snapshot,
    manifest: Option<&Manifest>,
) -> Result<(Vec<SourceTrack>, Vec<SourcePlaylist>)> {
    let current = before.merged_library();
    let sources = sources(root, before)?;
    let positions: BTreeMap<u32, usize> =
        sources.keys().enumerate().map(|(i, id)| (*id, i)).collect();
    let tracks = sources.into_values().collect();
    let ids: BTreeMap<u32, u64> = current
        .as_ref()
        .into_iter()
        .flat_map(|l| &l.playlists)
        .map(|p| {
            let id = manifest
                .and_then(|m| m.playlists.iter().find(|n| n.export_id == p.id))
                .map_or(DEVICE_PLAYLIST_ID_BASE + u64::from(p.id), |p| p.library_id);
            (p.id, id)
        })
        .collect();
    let playlists = current
        .as_ref()
        .into_iter()
        .flat_map(|l| &l.playlists)
        .map(|p| {
            Ok(SourcePlaylist {
                id: ids[&p.id],
                device_id: p.id,
                device_only: manifest
                    .and_then(|m| m.playlists.iter().find(|n| n.export_id == p.id))
                    .is_none_or(|p| p.device_only),
                parent_id: ids.get(&p.parent).copied().unwrap_or(0),
                folder: p.folder,
                name: p.name.clone(),
                track_indices: p
                    .tracks
                    .iter()
                    .map(|id| {
                        positions.get(id).copied().ok_or_else(|| {
                            ExportError::Conflict("Missing track in existing playlist".into())
                        })
                    })
                    .collect::<Result<_>>()?,
            })
        })
        .collect::<Result<_>>()?;
    Ok((tracks, playlists))
}

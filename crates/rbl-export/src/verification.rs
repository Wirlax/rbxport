use std::{collections::BTreeSet, path::Path};
use crate::{Result, snapshot::Snapshot};
#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub parsed: bool, pub tracks: usize, pub playlists: usize, pub playlist_entries: usize,
    pub audio_present: usize, pub analysis_present: usize, pub missing_audio: Vec<String>,
    pub errors: Vec<String>,
}
impl VerifyReport {
    pub fn is_ok(&self) -> bool { self.parsed && self.missing_audio.is_empty() && self.errors.is_empty() }
}
pub fn verify(root: &Path) -> Result<VerifyReport> {
    let snapshot = Snapshot::read(root)?;
    verify_staged(root, root, &snapshot)
}
pub(crate) fn verify_staged(root: &Path, existing: &Path, snapshot: &Snapshot) -> Result<VerifyReport> {
    let mut report = VerifyReport::default();
    let resolve = |relative: &str| -> Result<std::path::PathBuf> {
        let path=crate::checked_under(root,relative)?;
        if path.is_file() { Ok(path) } else { crate::checked_under(existing,relative) }
    };
    let (Some(legacy), Some(one)) = (&snapshot.legacy, &snapshot.one) else {
        report.errors.push("Both Device Library and OneLibrary must be present".into()); return Ok(report);
    };
    report.parsed = true;
    if legacy != one { report.errors.push("Device Library and OneLibrary disagree".into()); }
    report.tracks = legacy.tracks.len();
    report.playlists = legacy.playlists.len();
    let ids: BTreeSet<_> = legacy.tracks.iter().map(|t| t.id).collect();
    if ids.len() != legacy.tracks.len() { report.errors.push("Duplicate track IDs".into()); }
    let mut paths = BTreeSet::new();
    for track in &legacy.tracks {
        if !paths.insert(crate::path_key(&track.path)) { report.errors.push(format!("Audio path collision: {}", track.path)); }
        let path = resolve(&track.path)?;
        if path.is_file() { report.audio_present += 1; } else { report.missing_audio.push(track.path.clone()); }
        if !track.analysis.is_empty() {
            let path = resolve(&track.analysis)?;
            if path.is_file() && rbl_anlz::Anlz::read(&path).is_ok() { report.analysis_present += 1; }
            else { report.errors.push(format!("Missing or invalid analysis: {}", track.analysis)); }
        }
    }
    let playlists: BTreeSet<_> = legacy.playlists.iter().map(|p| p.id).collect();
    for p in &legacy.playlists {
        if p.parent != 0 && !legacy.playlists.iter().any(|n| n.id == p.parent && n.folder) { report.errors.push(format!("Missing parent folder for {}", p.name)); }
        let mut seen = BTreeSet::new(); let mut parent = p.parent;
        while parent != 0 {
            if !seen.insert(parent) { report.errors.push("Playlist folder cycle".into()); break; }
            parent = legacy.playlists.iter().find(|n| n.id == parent).map_or(0, |n| n.parent);
        }
        if p.folder && !p.tracks.is_empty() { report.errors.push("Folder contains direct track entries".into()); }
        report.playlist_entries += p.tracks.len();
        if p.tracks.iter().any(|id| !ids.contains(id)) { report.errors.push(format!("Dangling playlist entry in {}", p.name)); }
    }
    if playlists.len() != legacy.playlists.len() { report.errors.push("Duplicate playlist IDs".into()); }
    snapshot.check_retained_history(snapshot)?;
    Ok(report)
}

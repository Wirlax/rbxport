# Track analysis settings

Choosing **Analyze Track** from a track menu or a player's menu, or using
the analyse-selection shortcut, opens **Analysis Setting** before work is
queued. The dialog captures the selected track IDs and shows the count.
Cancel or Escape closes it without starting analysis. OK queues that
captured selection; changing the browser selection does not change the batch.

| Control | Behavior |
|---|---|
| BPM / Grid | Replaces BPM and beat-grid analysis and regenerates waveforms. Turning it off disables the nested timing controls. |
| High precision analysis | Uses the attack detector for timing. Turning it off places beats using the onset envelope. Enabled by default. |
| Analysis Mode | Displays Normal (`rekordbox`) and RBXport (`rbxport`), but both currently run RBXport settings. A distinct Normal implementation is missing. Initially follows Preferences. |
| BPM Range | Constrains the tempo search. Choices are 70–180 (default), 98–195, 118–236 and 58–115. |
| KEY | Updates the detected key. An unchecked KEY preserves the existing key. |

At least BPM / Grid or KEY must be checked. Phrase, vocal and automatic
cue analysis are not yet available.
The normal analysis author preserves existing cue and other supported
sections through its existing-file inputs.

A key-only request runs key detection and updates only the key metadata.
It does not regenerate files, change BPM, move beats, or mark an
unanalysed track as grid-analysed. If no key is detected, the old key stays.
Both the app's local analysis lock and the library's analysis-lock flag
prevent analysis; refusals appear in the queue's failure count and do not
stop other tracks. Locks are checked before decoding and again before writing.

Settings belong to each queued track. Later preference changes or another
batch cannot change pending work. Automatic imports keep their direct
queueing behavior, with the preferred preset, BPM/grid and key enabled,
high precision enabled, and a 70–180 range. Manual dialog choices apply to
that batch and do not overwrite global preferences.

The dialog is a native modal: it contains keyboard focus and makes the
background inert. Focus returns to the previous control when it closes.

## Implementation and verification

`src/views/analysis/AnalysisDialog.tsx` owns the draft choices;
`src/app/App.tsx` captures the selection and submits it through
`src/store/useAnalysis.ts`. `QueueItem.analysis` carries a copy of the
settings, and `analyseTrack` sends them to `src-tauri/src/analysis.rs`.
The backend validates the range and stage selection. Omitted settings
retain the previous full-analysis defaults.

The existing preset names currently select the same underlying analysis
options; the explicit precision and range choices are applied afterwards.
The transient fallback described in
[the beat guide](../crates/rbl-analysis/docs/beat.md#6-grid-the-change)
is automatic during eligible tempo transitions, with either preset.

Regression coverage includes the dialog's cancellation and stage controls
in Chromium and WebKit (`e2e/analysis.spec.ts`), existing progress and failure
handling (`e2e/app.spec.ts`), saved per-batch settings
(`src/store/useAnalysis.test.tsx`), and backend preservation of unchecked
results, lock refusal and option validation (`src-tauri/src/analysis.rs`).

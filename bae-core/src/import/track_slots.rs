//! The file↔release mapping, as a value.
//!
//! A **track slot** is one row of the release's track table: the audio on disk
//! and the track the source names for it, laid alongside each other. The
//! mapping is computed when a release is picked, so it is something to look at
//! and correct before anything is written, and the commit consumes what the
//! user saw rather than re-deriving it.
//!
//! Slots are **additive**: a bound track sheet carves one slot per track it
//! describes out of one container, a standalone audio file makes one, and they
//! land in the same ordered list. A folder holding a disc image plus loose
//! bonus tracks maps both — neither set is dropped for the other.
//!
//! A disagreement between the two sides is a slot, never an error.
//! [`TrackSlot::FileOnly`] is audio the source's tracklist does not account
//! for; [`TrackSlot::TrackOnly`] is a track no audio backs. The only refusal
//! left here is audio that will not decode.

use crate::audio_codec::ProbeResult;
use crate::db::DbTrack;
use crate::import::folder_scanner::{BoundTrackSheet, CategorizedFiles, ScannedFile};
use crate::import::types::{AudioFile, CueAnalyzedAudioFile, CueFlacAnalysis, TrackFile};
use crate::import::{ImportError, TrackUserEdit};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, warn};

/// One row of the file↔release mapping.
///
/// Every variant carries the editable track row, whose
/// [`file`](TrackUserEdit::file) names the audio bound to it. That binding is
/// the part that survives: it rides the edit to the commit, so a pairing the
/// user corrected is the one that gets written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackSlot {
    /// The source names this track and audio on disk backs it.
    Paired(TrackUserEdit),
    /// Audio on disk the source's tracklist does not account for. Its title is
    /// blank until someone names it.
    FileOnly(TrackUserEdit),
    /// A track the source names with no audio bound to it.
    TrackOnly(TrackUserEdit),
}

impl TrackSlot {
    pub fn track(&self) -> &TrackUserEdit {
        match self {
            Self::Paired(track) | Self::FileOnly(track) | Self::TrackOnly(track) => track,
        }
    }

    pub fn into_track(self) -> TrackUserEdit {
        match self {
            Self::Paired(track) | Self::FileOnly(track) | Self::TrackOnly(track) => track,
        }
    }

    /// The audio this slot is bound to — `None` exactly for
    /// [`TrackOnly`](Self::TrackOnly).
    pub fn file(&self) -> Option<&AudioFile> {
        self.track().file.as_ref()
    }
}

/// The audio the folder offers, one entry per track it can produce, in the
/// order it sits on disk.
///
/// A track sheet that is bound and carves at least one track contributes one
/// [`AudioFile::SheetSlice`] per track it describes, at its container's
/// position; the audio it speaks for contributes nothing of its own. Every
/// other audio file contributes one [`AudioFile::Standalone`]. That is what
/// makes the mapping additive rather than one shape winning.
///
/// The order is the scan's own, which is what "disk order" means everywhere
/// else in the import — the same order the Unknown path reads embedded tags in,
/// so the two cannot pair a file's tags with another file's samples.
pub(crate) fn audio_units(files: &CategorizedFiles) -> Vec<AudioFile> {
    let bound = files.bound_sheets();

    // A sheet that describes no playable track carves nothing, so it neither
    // leads with its audio nor speaks for it: the container stays a track of
    // its own instead of vanishing with the sheet.
    let carving: Vec<&BoundTrackSheet<'_>> = bound
        .iter()
        .filter(|sheet| sheet.sheet.playable_track_count() > 0)
        .collect();

    let mut leads: HashMap<&str, Vec<&BoundTrackSheet<'_>>> = HashMap::new();
    let mut spoken_for: HashSet<&str> = HashSet::new();
    for sheet in &carving {
        for file_id in sheet_audio_ids(files, sheet) {
            spoken_for.insert(file_id);
        }
        leads
            .entry(sheet.audio.relative_path.as_str())
            .or_default()
            .push(sheet);
    }

    let mut units = Vec::new();
    for file in files.audio() {
        match leads.get(file.relative_path.as_str()) {
            Some(sheets) => {
                for sheet in sheets {
                    for index in 0..sheet.sheet.playable_track_count() {
                        units.push(AudioFile::SheetSlice {
                            file_id: file.relative_path.clone(),
                            sheet_id: sheet.file.relative_path.clone(),
                            index: index as u32,
                        });
                    }
                }
            }
            None if spoken_for.contains(file.relative_path.as_str()) => {}
            None => units.push(AudioFile::Standalone {
                file_id: file.relative_path.clone(),
            }),
        }
    }
    units
}

/// Which of the folder's audio one bound sheet speaks for.
///
/// A sheet that names one file for the whole disc speaks for the audio it is
/// *bound* to, which is not necessarily what its `FILE` directive spells — the
/// two differ exactly when the binding is the user's. A sheet that names one
/// file per track has no single binding to stand in for its references, so
/// those resolve as written, inside the sheet's own directory.
fn sheet_audio_ids<'a>(files: &'a CategorizedFiles, bound: &BoundTrackSheet<'a>) -> Vec<&'a str> {
    if bound.sheet.single_file().is_some() {
        return vec![bound.audio.relative_path.as_str()];
    }
    let Some(cue_dir) = bound.file.path.parent() else {
        return vec![bound.audio.relative_path.as_str()];
    };
    bound
        .sheet
        .audio_file_references()
        .iter()
        .filter_map(|reference| {
            let path = cue_dir.join(reference);
            files
                .audio()
                .find(|audio| audio.path == path)
                .map(|audio| audio.relative_path.as_str())
        })
        .collect()
}

/// The file↔release mapping for one folder and one tracklist.
///
/// The folder's audio (in disk order) and `source_tracks` (in the source's own
/// order) are laid alongside each other. Where both sides have a row the slot
/// is [`Paired`](TrackSlot::Paired); audio the tracklist runs out for is
/// [`FileOnly`](TrackSlot::FileOnly); tracklist the audio runs out for is
/// [`TrackOnly`](TrackSlot::TrackOnly).
///
/// Two consequences the callers rely on. Slot `i` stands for source track `i`
/// for every `i` the source names, so the row the commit writes and the track
/// the source described stay together without either side carrying an index.
/// And the side that runs out first puts its leftover rows at the tail, next to
/// the rows they follow on disk rather than in a footer of their own.
pub(crate) fn compute_track_slots(
    source_tracks: &[TrackUserEdit],
    files: &CategorizedFiles,
) -> Vec<TrackSlot> {
    let units = audio_units(files);
    let mut slots = Vec::with_capacity(units.len().max(source_tracks.len()));

    for (index, unit) in units.iter().enumerate() {
        match source_tracks.get(index) {
            Some(track) => {
                let mut track = track.clone();
                track.file = Some(unit.clone());
                slots.push(TrackSlot::Paired(track));
            }
            None => {
                // The source says nothing about this audio, so the row starts
                // blank and continues the numbering of the row above it.
                let (side, track_number) = match slots.last().map(TrackSlot::track) {
                    Some(previous) => (previous.side, previous.track_number.map(|n| n + 1)),
                    None => (1, Some(1)),
                };
                slots.push(TrackSlot::FileOnly(TrackUserEdit {
                    title: String::new(),
                    side,
                    track_number,
                    artist_names: Vec::new(),
                    file: Some(unit.clone()),
                }));
            }
        }
    }

    for track in source_tracks.iter().skip(units.len()) {
        let mut track = track.clone();
        track.file = None;
        slots.push(TrackSlot::TrackOnly(track));
    }

    slots
}

/// Bind each track to the audio holding its samples and yield the
/// [`TrackFile`]s the run pass consumes.
///
/// `rows` is the mapping the commit settled: one `(track, audio)` pair per row
/// that will be written, in track order. Every `DbTrack` moves into a
/// `TrackFile` variant with its `duration_ms` filled in — from the sheet's own
/// timing for a slice, from a probe for a standalone file — and every slice of
/// one sheet shares that sheet's single parsed analysis.
///
/// A row whose title is blank is titled after its audio file's name. An empty
/// title is a track nobody can find again: it renders as a blank row in every
/// list, sorts nowhere, and matches no search. The file's own name is the one
/// fact about that track that is certainly true, and it is what the slot table
/// showed on that very row — so it is what gets written. Reading the file's
/// embedded tag instead would let a second metadata authority into an import
/// whose authority the user already chose, and would write something the slot
/// table never displayed.
pub(crate) fn resolve_track_files(
    rows: Vec<(DbTrack, AudioFile)>,
    files: &CategorizedFiles,
) -> Result<Vec<TrackFile>, ImportError> {
    debug!("Binding {} tracks to the folder's audio", rows.len());
    let mut analyses: HashMap<String, Arc<CueFlacAnalysis>> = HashMap::new();
    let mut track_files = Vec::with_capacity(rows.len());

    for (mut db_track, audio) in rows {
        let file = audio_file(files, &audio)?;
        if db_track.title.trim().is_empty() {
            db_track.title = file_title(file);
        }
        match &audio {
            AudioFile::Standalone { .. } => {
                db_track.duration_ms = probe_duration_ms(&file.path);
                track_files.push(TrackFile::Standalone {
                    db_track,
                    file_path: file.path.clone(),
                });
            }
            AudioFile::SheetSlice {
                sheet_id, index, ..
            } => {
                let analysis = match analyses.get(sheet_id) {
                    Some(analysis) => Arc::clone(analysis),
                    None => {
                        let analysis = Arc::new(sheet_analysis(files, sheet_id)?);
                        analyses.insert(sheet_id.clone(), Arc::clone(&analysis));
                        analysis
                    }
                };
                let cue_index = *index as usize;
                let cue_track = analysis
                    .cue_sheet
                    .playable_tracks()
                    .nth(cue_index)
                    .ok_or_else(|| ImportError::UnusableFile {
                        detail: format!("{sheet_id} no longer describes a track {}", cue_index + 1),
                    })?;
                // The sheet gives exact per-track timing, but the final track
                // has no next-track boundary in it, so its duration comes from
                // the container's total duration minus its INDEX 01 start.
                db_track.duration_ms =
                    cue_track.track_duration_ms().map(|d| d as i64).or_else(|| {
                        analysis
                            .audio_files
                            .iter()
                            .find(|file| file.file_reference == cue_track.file_reference)
                            .map(|file| file.probe.duration.as_millis() as i64)
                            .map(|total| total - cue_track.start_time_ms() as i64)
                    });
                track_files.push(TrackFile::CueBacked {
                    db_track,
                    file_path: file.path.clone(),
                    cue_pair: analysis,
                    cue_index,
                });
            }
        }
    }
    Ok(track_files)
}

/// The scanned audio a binding names, as the commit's own walk of the folder
/// found it. Audio that is no longer there is a refusal: the mapping named
/// samples this import cannot read, and the folder changed under the choice.
fn audio_file<'a>(
    files: &'a CategorizedFiles,
    audio: &AudioFile,
) -> Result<&'a ScannedFile, ImportError> {
    files
        .audio()
        .find(|file| file.relative_path == audio.file_id())
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("{} is no longer in the folder", audio.file_id()),
        })
}

/// A file's name without its extension — the title an unnamed slot writes.
fn file_title(file: &ScannedFile) -> String {
    std::path::Path::new(&file.file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&file.file_name)
        .to_string()
}

/// Parse-and-probe one bound track sheet, once, for every slice carved out of
/// it. The audio a sheet naming a single file is analyzed against is the audio
/// it is *bound* to, not what its `FILE` directive spells: a sheet written for
/// a WAV that was later encoded to FLAC still says `WAV`, and re-resolving the
/// directive here would refuse the very pairing the user corrected. A sheet
/// naming one file per track has no single binding to stand in, so its
/// references resolve as written.
fn sheet_analysis(
    files: &CategorizedFiles,
    sheet_id: &str,
) -> Result<CueFlacAnalysis, ImportError> {
    let bound = files.bound_sheets();
    let sheet = bound
        .iter()
        .find(|bound| bound.file.relative_path == sheet_id)
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("{sheet_id} no longer describes any of the folder's audio"),
        })?;

    let cue_dir = sheet
        .file
        .path
        .parent()
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("{sheet_id} has no parent directory"),
        })?;
    let single_file = sheet.sheet.single_file().is_some();

    let mut audio_files = Vec::new();
    for file_reference in sheet.sheet.audio_file_references() {
        let path = if single_file {
            sheet.audio.path.clone()
        } else {
            cue_dir.join(file_reference)
        };
        let probe = analyze_cue_audio(&path)?;
        audio_files.push(CueAnalyzedAudioFile {
            file_reference: file_reference.to_string(),
            path,
            probe,
        });
    }

    Ok(CueFlacAnalysis {
        cue_sheet: sheet.sheet.clone(),
        audio_files,
    })
}

/// A standalone audio file's duration. `None` just means the track lands
/// without one — `probe_audio_from_path` logs its own failure reason.
fn probe_duration_ms(file_path: &std::path::Path) -> Option<i64> {
    let Some(path_str) = file_path.to_str() else {
        warn!(
            "Cannot probe duration for non-UTF-8 path: {}",
            file_path.display()
        );
        return None;
    };
    let probe = crate::audio_codec::probe_audio_from_path(path_str)?;
    Some(probe.duration.as_millis() as i64)
}

/// Probe a container a track sheet carves tracks out of.
///
/// The codec is not re-judged here: the scan probes the audio a sheet names and
/// refuses the binding outright when bae cannot carve that container, so a
/// sheet that is still bound after the commit's own walk names a container that
/// already passed. What is left is the file being readable at all.
pub(crate) fn analyze_cue_audio(audio_path: &std::path::Path) -> Result<ProbeResult, ImportError> {
    let path_str = audio_path
        .to_str()
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("non-UTF-8 audio path: {:?}", audio_path),
        })?;
    crate::audio_codec::probe_audio_from_path(path_str).ok_or_else(|| ImportError::UnusableFile {
        detail: format!("audio file could not be read: {:?}", audio_path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::folder_scanner::{collect_release_candidate_files, StoredSheetBindings};
    use std::fs;
    use std::path::Path;

    /// Synthetic FLAC bytes valid enough to round-trip through the scan's
    /// audio validation and the CUE container probe.
    ///
    /// 44.1 kHz / 2-channel / 16-bit STREAMINFO declaring 1 second of audio.
    fn synthetic_flac_bytes() -> Vec<u8> {
        let sample_rate: u32 = 44_100;
        let channels: u32 = 2;
        let bps: u32 = 16;
        let total_samples: u64 = 44_100;

        let mut buf = Vec::new();
        buf.extend_from_slice(b"fLaC");

        // STREAMINFO block header: last-block=1, type=0, length=34.
        buf.extend_from_slice(&[0x80, 0x00, 0x00, 34]);

        // STREAMINFO data: 34 bytes laid out as
        //   [0..2]   min block size
        //   [2..4]   max block size
        //   [4..7]   min frame size
        //   [7..10]  max frame size
        //   [10..13] sample rate (20 bits) | channels-1 (3) | bps-1 high bit
        //   [13]     bps-1 low 4 bits | total_samples high 4 bits
        //   [14..18] total_samples low 32 bits
        //   [18..34] MD5 signature
        buf.extend_from_slice(&[0x10, 0x00, 0x10, 0x00]);
        buf.extend_from_slice(&[0u8; 6]);

        let ch_minus_1 = (channels - 1) & 0x07;
        let bps_minus_1 = (bps - 1) & 0x1F;
        let ts_high = ((total_samples >> 32) & 0x0F) as u8;

        buf.push((sample_rate >> 12) as u8);
        buf.push(((sample_rate >> 4) & 0xFF) as u8);
        buf.push(
            (((sample_rate & 0x0F) as u8) << 4)
                | ((ch_minus_1 as u8) << 1)
                | ((bps_minus_1 >> 4) as u8),
        );
        buf.push((((bps_minus_1 & 0x0F) as u8) << 4) | ts_high);
        buf.extend_from_slice(&((total_samples & 0xFFFF_FFFF) as u32).to_be_bytes());
        buf.extend_from_slice(&[0u8; 16]);

        debug_assert_eq!(buf.len(), 42);
        buf.resize(18_000, 0);
        buf
    }

    /// A sheet naming one container for the whole disc, with `count` playable
    /// tracks three minutes apart.
    fn cue_sheet_text(audio_file_name: &str, count: usize) -> String {
        let mut text = String::from("PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n");
        text.push_str(&format!("FILE \"{audio_file_name}\" WAVE\n"));
        for index in 0..count {
            text.push_str(&format!("  TRACK {:02} AUDIO\n", index + 1));
            text.push_str(&format!("    TITLE \"Track {}\"\n", index + 1));
            text.push_str(&format!("    INDEX 01 {:02}:00:00\n", index * 3));
        }
        text
    }

    fn write_flac(path: &Path) {
        fs::write(path, synthetic_flac_bytes()).expect("write flac");
    }

    fn source_tracks(count: usize) -> Vec<TrackUserEdit> {
        (0..count)
            .map(|index| TrackUserEdit {
                title: format!("Track Title {}", index + 1),
                side: 1,
                track_number: Some(index as i32 + 1),
                artist_names: Vec::new(),
                file: None,
            })
            .collect()
    }

    fn scan(root: &Path) -> CategorizedFiles {
        collect_release_candidate_files(root, &StoredSheetBindings::none()).expect("scan succeeds")
    }

    fn file_ids(slots: &[TrackSlot]) -> Vec<Option<&str>> {
        slots
            .iter()
            .map(|slot| slot.file().map(AudioFile::file_id))
            .collect()
    }

    /// Thirteen files against a twelve-track source: twelve rows agree and the
    /// thirteenth file gets a slot of its own, at the end of the folder's own
    /// order rather than in a footer. Nothing fails.
    #[test]
    fn extra_audio_becomes_a_file_only_slot_in_disk_order() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        for index in 1..=13 {
            write_flac(&tmp.path().join(format!("{index:02}.flac")));
        }

        let slots = compute_track_slots(&source_tracks(12), &scan(tmp.path()));

        assert_eq!(slots.len(), 13);
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot, TrackSlot::Paired(_)))
                .count(),
            12,
        );
        assert!(matches!(slots[12], TrackSlot::FileOnly(_)));
        assert_eq!(
            file_ids(&slots),
            (1..=13)
                .map(|index| format!("{index:02}.flac"))
                .collect::<Vec<_>>()
                .iter()
                .map(|id| Some(id.as_str()))
                .collect::<Vec<_>>(),
        );
        // The unnamed slot keeps a place in the numbering rather than
        // restarting it.
        let unnamed = slots[12].track();
        assert_eq!(unnamed.title, "");
        assert_eq!(unnamed.side, 1);
        assert_eq!(unnamed.track_number, Some(13));
    }

    /// Fourteen source tracks against thirteen files: the fourteenth is a slot
    /// with no audio, and every other row still pairs.
    #[test]
    fn a_track_with_no_audio_becomes_a_track_only_slot() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        for index in 1..=13 {
            write_flac(&tmp.path().join(format!("{index:02}.flac")));
        }

        let slots = compute_track_slots(&source_tracks(14), &scan(tmp.path()));

        assert_eq!(slots.len(), 14);
        assert_eq!(
            slots
                .iter()
                .filter(|slot| matches!(slot, TrackSlot::Paired(_)))
                .count(),
            13,
        );
        match &slots[13] {
            TrackSlot::TrackOnly(track) => {
                assert_eq!(track.title, "Track Title 14");
                assert!(track.file.is_none());
            }
            other => panic!("expected a TrackOnly slot, got {other:?}"),
        }
    }

    /// A disc image plus two loose bonus tracks. The sheet's slices and the two
    /// standalone files coexist, in disk order — neither set is dropped for the
    /// other, which is the additive property.
    #[test]
    fn a_disc_image_and_loose_audio_produce_slots_for_both() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("CDImage.flac"));
        fs::write(
            tmp.path().join("CDImage.cue"),
            cue_sheet_text("CDImage.flac", 11),
        )
        .expect("write cue");
        write_flac(&tmp.path().join("bonus-1.flac"));
        write_flac(&tmp.path().join("bonus-2.flac"));

        let files = scan(tmp.path());
        let slots = compute_track_slots(&source_tracks(13), &files);

        assert_eq!(slots.len(), 13);
        // Disk order: the bonus files sort before the disc image, so their
        // slots lead. The image's eleven slices follow in sheet order.
        assert_eq!(
            file_ids(&slots),
            vec![
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("CDImage.flac"),
                Some("bonus-1.flac"),
                Some("bonus-2.flac"),
            ],
        );
        let slice_indices: Vec<u32> = slots
            .iter()
            .filter_map(|slot| match slot.file() {
                Some(AudioFile::SheetSlice { index, .. }) => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(slice_indices, (0..11).collect::<Vec<_>>());
        assert!(slots
            .iter()
            .all(|slot| matches!(slot, TrackSlot::Paired(_))));
        // The loose files are standalone slots, not slices of the image.
        assert!(matches!(
            slots[11].file(),
            Some(AudioFile::Standalone { .. })
        ));
    }

    /// A sheet describing more tracks than the source names lands in the same
    /// table as any other disagreement: extra slices become `FileOnly` slots,
    /// and nothing errors on the way.
    #[test]
    fn a_sheet_disagreeing_with_the_source_lands_in_the_same_table() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("CDImage.flac"));
        fs::write(
            tmp.path().join("CDImage.cue"),
            cue_sheet_text("CDImage.flac", 12),
        )
        .expect("write cue");

        let files = scan(tmp.path());

        let more_slices_than_tracks = compute_track_slots(&source_tracks(10), &files);
        assert_eq!(more_slices_than_tracks.len(), 12);
        assert_eq!(
            more_slices_than_tracks
                .iter()
                .filter(|slot| matches!(slot, TrackSlot::FileOnly(_)))
                .count(),
            2,
        );

        let fewer_slices_than_tracks = compute_track_slots(&source_tracks(14), &files);
        assert_eq!(fewer_slices_than_tracks.len(), 14);
        assert_eq!(
            fewer_slices_than_tracks
                .iter()
                .filter(|slot| matches!(slot, TrackSlot::TrackOnly(_)))
                .count(),
            2,
        );
    }

    /// Multi-disc rips lay their discs down in the order a person reads them:
    /// `CD10` after `CD9`, not after `CD1`.
    #[test]
    fn discs_are_laid_down_in_natural_order() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        for disc in 1..=10 {
            let dir = tmp.path().join(format!("CD{disc}"));
            fs::create_dir_all(&dir).expect("mkdir");
            write_flac(&dir.join("CDImage.flac"));
            fs::write(
                dir.join("CDImage.cue"),
                cue_sheet_text("CDImage.flac", disc),
            )
            .expect("write cue");
        }

        let total: usize = (1..=10).sum();
        let slots = compute_track_slots(&source_tracks(total), &scan(tmp.path()));

        assert_eq!(slots.len(), total);
        let mut expected = Vec::new();
        for disc in 1..=10 {
            for _ in 0..disc {
                expected.push(Some(format!("CD{disc}/CDImage.flac")));
            }
        }
        assert_eq!(
            file_ids(&slots),
            expected.iter().map(|id| id.as_deref()).collect::<Vec<_>>(),
        );
    }

    /// Re-pairing two slots is a swap of their bindings, and the swapped rows
    /// are what `resolve_track_files` binds — the correction is not re-derived
    /// away by position.
    #[test]
    fn a_corrected_pairing_is_what_gets_bound() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        for index in 1..=3 {
            write_flac(&tmp.path().join(format!("{index:02}.flac")));
        }
        let files = scan(tmp.path());
        let mut slots = compute_track_slots(&source_tracks(3), &files);

        // The rip named its files in the wrong order: track 1 is really 02.flac
        // and track 2 is really 01.flac.
        let first = slots[0].track().file.clone();
        let second = slots[1].track().file.clone();
        match &mut slots[0] {
            TrackSlot::Paired(track) => track.file = second,
            other => panic!("expected a Paired slot, got {other:?}"),
        }
        match &mut slots[1] {
            TrackSlot::Paired(track) => track.file = first,
            other => panic!("expected a Paired slot, got {other:?}"),
        }

        let now = chrono::Utc::now();
        let rows: Vec<(DbTrack, AudioFile)> = slots
            .into_iter()
            .enumerate()
            .map(|(index, slot)| {
                let track = slot.into_track();
                (
                    DbTrack {
                        id: format!("track-{index}"),
                        release_id: "release-1".to_string(),
                        title: track.title.clone(),
                        side: track.side,
                        track_number: track.track_number,
                        duration_ms: None,
                        discogs_position: None,
                        created_at: now,
                    },
                    track.file.expect("every row is paired"),
                )
            })
            .collect();

        let track_files = resolve_track_files(rows, &files).expect("binding succeeds");
        let bound: Vec<(&str, String)> = track_files
            .iter()
            .map(|track_file| {
                (
                    track_file.db_track().id.as_str(),
                    track_file
                        .file_path()
                        .file_name()
                        .expect("file name")
                        .to_string_lossy()
                        .into_owned(),
                )
            })
            .collect();
        assert_eq!(
            bound,
            vec![
                ("track-0", "02.flac".to_string()),
                ("track-1", "01.flac".to_string()),
                ("track-2", "03.flac".to_string()),
            ],
        );
    }

    /// A slot nobody named commits under its file's own name. An empty title is
    /// a track that cannot be found again, and the file name is what the slot
    /// table showed on that row.
    #[test]
    fn an_unnamed_slot_is_titled_after_its_file() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("hidden track.flac"));
        let files = scan(tmp.path());

        let track_files = resolve_track_files(
            vec![(
                DbTrack {
                    id: "track-0".to_string(),
                    release_id: "release-1".to_string(),
                    title: "   ".to_string(),
                    side: 1,
                    track_number: Some(1),
                    duration_ms: None,
                    discogs_position: None,
                    created_at: chrono::Utc::now(),
                },
                AudioFile::Standalone {
                    file_id: "hidden track.flac".to_string(),
                },
            )],
            &files,
        )
        .expect("binding succeeds");

        assert_eq!(track_files[0].db_track().title, "hidden track");
    }

    /// Every slice of a disc image binds to the container, carries its own
    /// index into the sheet, and shares one parsed analysis.
    #[test]
    fn sheet_slices_bind_to_their_container_and_share_one_analysis() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("CDImage.flac"));
        fs::write(
            tmp.path().join("CDImage.cue"),
            cue_sheet_text("CDImage.flac", 4),
        )
        .expect("write cue");
        let files = scan(tmp.path());
        let slots = compute_track_slots(&source_tracks(4), &files);

        let now = chrono::Utc::now();
        let rows: Vec<(DbTrack, AudioFile)> = slots
            .into_iter()
            .enumerate()
            .map(|(index, slot)| {
                let track = slot.into_track();
                (
                    DbTrack {
                        id: format!("track-{index}"),
                        release_id: "release-1".to_string(),
                        title: track.title.clone(),
                        side: track.side,
                        track_number: track.track_number,
                        duration_ms: None,
                        discogs_position: None,
                        created_at: now,
                    },
                    track.file.expect("every row is paired"),
                )
            })
            .collect();

        let track_files = resolve_track_files(rows, &files).expect("binding succeeds");
        assert_eq!(track_files.len(), 4);

        let mut analyses = Vec::new();
        for (position, track_file) in track_files.iter().enumerate() {
            match track_file {
                TrackFile::CueBacked {
                    cue_index,
                    cue_pair,
                    file_path,
                    db_track,
                } => {
                    assert_eq!(*cue_index, position);
                    assert_eq!(file_path.file_name().unwrap(), "CDImage.flac");
                    assert!(
                        db_track.duration_ms.is_some(),
                        "every slice gets a duration",
                    );
                    analyses.push(Arc::as_ptr(cue_pair));
                }
                other => panic!("expected a CueBacked track file, got {other:?}"),
            }
        }
        assert!(
            analyses.windows(2).all(|pair| pair[0] == pair[1]),
            "one sheet is parsed and probed once for all its slices",
        );
    }

    /// Audio a binding names that is no longer in the folder is the one thing
    /// that still refuses: there are no samples to write.
    #[test]
    fn audio_that_left_the_folder_refuses() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("01.flac"));
        let files = scan(tmp.path());

        let err = resolve_track_files(
            vec![(
                DbTrack {
                    id: "track-0".to_string(),
                    release_id: "release-1".to_string(),
                    title: "Track Title".to_string(),
                    side: 1,
                    track_number: Some(1),
                    duration_ms: None,
                    discogs_position: None,
                    created_at: chrono::Utc::now(),
                },
                AudioFile::Standalone {
                    file_id: "02.flac".to_string(),
                },
            )],
            &files,
        )
        .expect_err("audio that is gone cannot be bound");
        assert!(
            matches!(&err, ImportError::UnusableFile { detail } if detail.contains("02.flac")),
            "got: {err}",
        );
    }
}

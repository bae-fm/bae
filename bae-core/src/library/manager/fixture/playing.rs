//! The album a fixture's library is playing: a CUE image the test writes,
//! laid out in the library as an import lays out the image's tracks, and the
//! resume row playback starts from.

use super::*;
use crate::db::{DbFile, SeededAudio};
use crate::import::{CueAnalyzedAudioFile, CueFlacAnalysis, TrackAudio, TrackFile};

/// The album a [`LibraryFixture`]'s library is playing, added after its
/// albums.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FixturePlaying {
    pub title: String,
    /// Who the album is credited to, in credit order: at least one.
    pub artists: Vec<String>,
    /// A CUE sheet titling each track with a `TITLE` line and naming its
    /// audio with `FILE` lines, beside the sheet.
    pub cue_sheet: PathBuf,
    /// The sheet's number of the track playback is on.
    pub track: u32,
    /// How far into that track's stream playback is: its pregap, where it
    /// has one, then the track.
    pub position_ms: u64,
}

impl LibraryManager {
    /// `playing` as the album its write inserts, added at `added`, and the
    /// id of the track playback is on.
    pub(super) async fn seeded_playing_album(
        &self,
        playing: &FixturePlaying,
        added: chrono::DateTime<chrono::Utc>,
        artists: &mut Vec<DbArtist>,
    ) -> Result<(SeededAlbum, String), LibraryFixtureError> {
        let invalid = |detail: String| {
            LibraryFixtureError::Invalid(format!(
                "the playing album's sheet {}: {detail}",
                playing.cue_sheet.display()
            ))
        };
        let sheet = crate::cue_flac::parse_cue_sheet(&playing.cue_sheet)
            .map_err(|error| invalid(error.to_string()))?;
        let folder = playing
            .cue_sheet
            .parent()
            .ok_or_else(|| invalid("names no folder".to_string()))?;
        // Each file the sheet names, probed, with what the scan reads of it.
        let mut audio_files: Vec<CueAnalyzedAudioFile> = Vec::new();
        let mut scanned_audio = Vec::new();
        for track in sheet.playable_tracks() {
            if audio_files
                .iter()
                .any(|file| file.file_reference == track.file_reference)
            {
                continue;
            }
            let path = folder.join(&track.file_reference);
            let unplayable = || invalid(format!("{} is no audio bae plays", path.display()));
            let probe = path
                .to_str()
                .and_then(crate::audio_codec::probe_audio_from_path)
                .ok_or_else(unplayable)?;
            scanned_audio
                .push(crate::import::folder_scanner::scanned_audio(&probe).ok_or_else(unplayable)?);
            audio_files.push(CueAnalyzedAudioFile {
                file_reference: track.file_reference.clone(),
                path,
                probe,
            });
        }
        let analysis = Arc::new(CueFlacAnalysis {
            cue_sheet: sheet,
            audio_files,
        });

        let mut seeded = self
            .seeded_album(
                &FixtureAlbum {
                    title: playing.title.clone(),
                    artists: playing.artists.clone(),
                    tracks: Vec::new(),
                    cover: None,
                },
                added,
                artists,
            )
            .await?;
        let release_id = seeded.release.id.clone();
        let sheet_name = playing.cue_sheet.to_string_lossy();
        let mut tracks_to_files = Vec::new();
        let mut current = None;
        for (index, track) in analysis.cue_sheet.playable_tracks().enumerate() {
            let title = track
                .title
                .clone()
                .ok_or_else(|| invalid(format!("track {} has no TITLE", track.number)))?;
            let duration_ms =
                crate::import::probe::sheet_track_duration_ms(&analysis, index, &sheet_name)
                    .map_err(|error| invalid(error.to_string()))?;
            let db_track = DbTrack {
                id: self.ids.new_id(),
                release_id: release_id.clone(),
                title,
                side: Some(1),
                track_number: Some(track.number as i32),
                duration_ms: Some(duration_ms as i64),
                discogs_position: None,
                created_at: added,
            };
            if track.number == playing.track {
                current = Some(db_track.id.clone());
            }
            tracks_to_files.push(TrackFile {
                db_track,
                audio: TrackAudio::CueBacked {
                    cue_pair: analysis.clone(),
                    cue_index: index,
                },
            });
        }
        let current =
            current.ok_or_else(|| invalid(format!("holds no track {}", playing.track)))?;

        let mut files = Vec::new();
        let mut file_ids = std::collections::HashMap::new();
        for (audio, scanned) in analysis.audio_files.iter().zip(scanned_audio) {
            let size = std::fs::metadata(&audio.path)
                .map_err(|error| invalid(format!("{}: {error}", audio.path.display())))?
                .len();
            let mut file = DbFile::new(
                &release_id,
                &audio.file_reference,
                size as i64,
                audio.probe.content_type.clone(),
                self.ids.new_id(),
                added,
            );
            // What the scan read of it, as an import records it.
            file.source_audio = Some(crate::album_detail::SourceAudioFile {
                layout: Some(crate::album_detail::SourceAudioLayout::Cue),
                format: scanned.format,
                content_type: scanned.content_type,
                duration_ms: scanned.duration_ms as i64,
            });
            let blob = coven::prepare_external_blob(&audio.path, |_| {})
                .await
                .map_err(|error| invalid(format!("{}: {error}", audio.path.display())))?;
            file_ids.insert(audio.path.clone(), file.id.clone());
            files.push((file, blob));
        }

        let built = {
            let clock = self.clock.clone();
            let ids = self.ids.clone();
            let tracks = tracks_to_files.clone();
            tokio::task::spawn_blocking(move || {
                crate::import::ImportService::build_audio_formats(
                    &tracks,
                    &file_ids,
                    clock.as_ref(),
                    ids.as_ref(),
                )
            })
            .await
            .map_err(|error| invalid(error.to_string()))?
            .map_err(|error| invalid(error.to_string()))?
        };
        seeded.tracks = tracks_to_files
            .into_iter()
            .map(|track| track.db_track)
            .collect();
        seeded.audio = Some(SeededAudio {
            files,
            audio_formats: built.audio_formats,
            audio_segments: built.audio_segments,
        });
        Ok((seeded, current))
    }

    /// Have playback resume `release_id` at `track_id`, `position_ms` into
    /// its stream, as playback leaves its resume row when it pauses there.
    pub(super) async fn write_fixture_resume_row(
        &self,
        release_id: &str,
        track_id: String,
        position_ms: u64,
    ) -> Result<(), LibraryFixtureError> {
        let row = crate::db::DbPlaybackState {
            context: Some(crate::db::DbPlaybackContext {
                source: crate::playback::source_to_str(&crate::playback::ContextSource::Release(
                    release_id.to_string(),
                )),
                shuffled: false,
            }),
            manual: "[]".to_string(),
            repeat: crate::playback::repeat_to_str(crate::playback::RepeatMode::Off),
            current_track_id: Some(track_id),
            position_ms: Some(position_ms as i64),
            volume: 1.0,
            is_muted: false,
        };
        self.save_playback_state(&row).await?;
        Ok(())
    }
}

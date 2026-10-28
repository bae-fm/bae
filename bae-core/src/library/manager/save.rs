//! The save arm: rendered standalone output files — decode then encode to the
//! preset codec, tags from bae's metadata, optional embedded cover, and a
//! token-pattern filename. The output queue/staging/marker/replace live in
//! [`super::output`]; the verbatim export arm in [`super::export`].

use super::*;
use crate::library::SaveTrackPlan;
use crate::playback::stream_pipeline::{SegmentDecodeParams, StreamDecodeParams};
use crate::playback::track_sources::SourceStream;
use std::num::NonZeroUsize;

impl LibraryManager {
    /// Resolve one track's tag data from the database alone — the tag fields, its
    /// track number, the release's track total, and whether the media is digital.
    /// Reads no audio and no cover, so both the filename-suggestion path (which
    /// must not download a whole file) and the full export plan share it.
    ///
    /// No cover id rides along: the art a track export embeds is its own release's,
    /// which the caller already holds.
    pub(crate) async fn resolve_save_tags(
        &self,
        release: &DbRelease,
        track: &DbTrack,
    ) -> Result<ResolvedSaveTags, LibraryError> {
        let album = self.database.get_album_for_release(release).await?;

        let album_artists = self.database.get_artists_for_album(&album.id).await?;
        let album_artist = crate::credits::library_line(&album_artists);
        let track_artists = self.database.get_artists_for_track(&track.id).await?;
        let artist = if track_artists.is_empty() {
            album_artist.clone()
        } else {
            crate::credits::library_line(&track_artists)
        };

        let total_tracks = self.database.count_tracks_for_release(&release.id).await?;
        let disc = self
            .database
            .get_release_mediums(&release.id)
            .await?
            .disc_number(track);

        let year = release.pressing.year.or(album.year);

        let tags = SaveTags {
            title: track.title.clone(),
            artist,
            album_artist,
            album: album.title,
            year,
            disc,
        };

        Ok(ResolvedSaveTags {
            tags,
            track_number: track.track_number.get(),
            total_tracks,
        })
    }

    /// One track's save plan under `window`: its tag data and stored audio,
    /// read from the database alone. No source is opened here; the save opens
    /// the files the plan reads while it saves that track.
    async fn save_track_plan(
        &self,
        meta: TrackAudioMeta,
        window: SaveWindow,
    ) -> Result<SaveTrackPlan, LibraryError> {
        let resolved = self.resolve_save_tags(&meta.release, &meta.track).await?;
        Ok(SaveTrackPlan {
            resolved,
            audio_meta: meta,
            window,
        })
    }

    /// The cover bytes a save of `release_id` embeds: the art of the release
    /// the tracks are on, the same rule playback applies. `None` when the
    /// preset does not embed — the file is then never read — or the release
    /// has no art.
    pub async fn save_cover_image(
        &self,
        release_id: &str,
        embed_cover: bool,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        if !embed_cover {
            return Ok(None);
        }
        match self.cover_ref(release_id).await? {
            Some(image) => self.read_image_bytes(&image).await,
            None => Ok(None),
        }
    }

    /// Open a streaming read of one release file for the save decoder: a
    /// sparse buffer sized to the stored file size, filled on demand through
    /// coven's locality-aware ranged read (the user's own file, the local
    /// store, the cache, or the cloud with decrypt). A read failure — or a
    /// file shorter than the stored size — fails the buffer, and the decode
    /// fails loudly with that read's error.
    fn open_save_source(&self, file: &crate::db::DbFile) -> SourceStream {
        use crate::playback::data_source::{create_audio_reader, FetchArbiter};

        // A fresh arbiter per file: save has no foreground track to prioritize,
        // so every fetch runs ungated.
        let reader = create_audio_reader(self, &file.id, FetchArbiter::new(), None, false);
        let file_id = file.id.clone();
        SourceStream::start(
            reader,
            file.file_size as u64,
            Box::new(move |error| {
                tracing::warn!("save: streaming release file {file_id} failed: {error}");
            }),
        )
    }

    /// Every release file `plans` read, by id, for opening them as the save
    /// reaches the tracks reading them.
    fn save_source_files<'a>(
        plans: impl IntoIterator<Item = &'a SaveTrackPlan>,
    ) -> HashMap<String, crate::db::DbFile> {
        plans
            .into_iter()
            .flat_map(|plan| {
                let next = match &plan.window {
                    SaveWindow::TrackFile { next, .. } => next.as_ref(),
                    SaveWindow::ImageTrack => None,
                };
                plan.audio_meta
                    .audio_files
                    .iter()
                    .chain(next.into_iter().flat_map(|next| next.audio_files.iter()))
            })
            .map(|file| (file.id.clone(), file.clone()))
            .collect()
    }

    /// The default filename (stem, no extension) a single-track "Save As…"
    /// suggests for `track_id` under the preset named by `preset_id` (must exist
    /// and apply to track saves), rendered from that preset's token pattern and
    /// the track's tag data. Reads no audio and no cover — only the database — so
    /// seeding a save panel never touches a whole file or the cloud.
    pub async fn save_track_suggested_name(
        &self,
        track_id: &str,
        preset_id: &str,
    ) -> Result<String, LibraryError> {
        let preset = self
            .save_presets()
            .into_iter()
            .find(|preset| preset.id == preset_id && preset.applies_to_track)
            .ok_or_else(|| {
                LibraryError::Save(format!(
                    "export preset {preset_id} is not available for track save"
                ))
            })?;
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let resolved = self.resolve_save_tags(&meta.release, &meta.track).await?;
        Ok(crate::library::save::render_save_filename(
            &preset.filename_tokens,
            &resolved,
        ))
    }

    /// Save one track to `output_path` under the preset named by `preset_id`
    /// (must exist and apply to track saves). The source audio is decoded and
    /// re-encoded to the preset's codec, tagged from bae's metadata, and cover
    /// art embedded — always a constructed file, never a verbatim copy.
    pub async fn save_track(
        &self,
        track_id: &str,
        output_path: &Path,
        preset_id: &str,
    ) -> Result<(), LibraryError> {
        let preset = self
            .save_presets()
            .into_iter()
            .find(|preset| preset.id == preset_id && preset.applies_to_track)
            .ok_or_else(|| {
                LibraryError::Save(format!(
                    "export preset {preset_id} is not available for track save"
                ))
            })?;
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let neighbors = self
            .database
            .track_neighbors(&meta.track.id)
            .await?
            .ok_or_else(|| {
                LibraryError::Save(format!(
                    "track {} is not ordered in release {}",
                    meta.track.id, meta.release.id
                ))
            })?;
        let next = match &neighbors.next {
            Some(next) => Some(TrackAudioMeta::resolve(&self.database, next).await?),
            None => None,
        };
        let cover = self
            .save_cover_image(&meta.release.id, preset.embed_cover)
            .await?;
        let plan = self
            .save_track_plan(
                meta,
                SaveWindow::TrackFile {
                    next,
                    placement: preset.pregap_placement,
                    is_first_track: neighbors.is_first,
                },
            )
            .await?;
        let sources = Self::save_source_files([&plan]);
        SaveService::save_tracks(
            vec![(plan, output_path.to_path_buf())],
            cover,
            preset.codec,
            NonZeroUsize::MIN,
            |file_id: &String| self.open_save_source(&sources[file_id]),
            Arc::new(|| {}),
        )
        .await
        .map_err(LibraryError::Save)
    }

    pub(super) async fn save_release_tracks_to_dir(
        &self,
        release_id: &str,
        preset: crate::config::SavePreset,
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let tracks = self.database.get_tracks_for_release(release_id).await?;
        let total = tracks.len();
        if preset.pregap_placement == crate::config::SavePregapPlacement::SingleFileWithCue {
            self.save_release_image_with_cue_to_dir(release_id, preset, &tracks, staging_dir)
                .await?;
            self.set_output_progress(release_id, 100);
            return Ok(());
        }
        let cover = self
            .save_cover_image(release_id, preset.embed_cover)
            .await?;
        let mut metas = Vec::with_capacity(tracks.len());
        for track in &tracks {
            metas.push(TrackAudioMeta::resolve(&self.database, &track.id).await?);
        }
        // Each track's plan carries the track after it, whose audio pregap a
        // placement may append to this one's file.
        let nexts: Vec<Option<TrackAudioMeta>> = metas
            .iter()
            .skip(1)
            .cloned()
            .map(Some)
            .chain(std::iter::once(None))
            .collect();
        let mut used_paths = std::collections::HashSet::new();
        let mut saves = Vec::with_capacity(tracks.len());
        for (index, (meta, next)) in metas.into_iter().zip(nexts).enumerate() {
            let plan = self
                .save_track_plan(
                    meta,
                    SaveWindow::TrackFile {
                        next,
                        placement: preset.pregap_placement,
                        is_first_track: index == 0,
                    },
                )
                .await?;
            let stem =
                crate::library::save::render_save_filename(&preset.filename_tokens, &plan.resolved);
            let output_path = unique_output_path(
                staging_dir,
                &stem,
                preset.codec.extension(),
                &mut used_paths,
            );
            saves.push((plan, output_path));
        }

        let sources = Self::save_source_files(saves.iter().map(|(plan, _)| plan));
        let saved = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let on_saved = {
            let manager = self.clone();
            let release_id = release_id.to_string();
            Arc::new(move || {
                let saved = saved.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                manager.set_output_progress(&release_id, ((saved * 100) / total.max(1)) as u8);
            })
        };
        // Decoding and encoding are CPU work: save as many tracks at once as
        // there are cores, which also bounds the source files open at once.
        let parallelism = std::thread::available_parallelism().unwrap_or_else(|error| {
            tracing::warn!(
                "could not read the available parallelism ({error}); saving one track at a time"
            );
            NonZeroUsize::MIN
        });
        SaveService::save_tracks(
            saves,
            cover,
            preset.codec,
            parallelism,
            |file_id: &String| self.open_save_source(&sources[file_id]),
            on_saved,
        )
        .await
        .map_err(LibraryError::Save)
    }

    async fn save_release_image_with_cue_to_dir(
        &self,
        release_id: &str,
        preset: crate::config::SavePreset,
        tracks: &[DbTrack],
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let mut plans = Vec::with_capacity(tracks.len());
        for track in tracks {
            let meta = TrackAudioMeta::resolve(&self.database, &track.id).await?;
            plans.push(self.save_track_plan(meta, SaveWindow::ImageTrack).await?);
        }
        let cover = self
            .save_cover_image(release_id, preset.embed_cover)
            .await?;

        let release = self
            .get_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Save(format!("release not found: {release_id}")))?;
        let folder = release.source_folder_name.ok_or_else(|| {
            LibraryError::Save(format!(
                "release {release_id} has no source folder name; cannot name its CUE image"
            ))
        })?;
        let stem = crate::library::save::sanitize_filename_stem(&folder);
        if stem.is_empty() {
            return Err(LibraryError::Save(format!(
                "release {release_id} source folder name has no usable filename characters"
            )));
        }
        let output_audio_path = staging_dir.join(format!("{stem}.{}", preset.codec.extension()));
        let output_cue_path = staging_dir.join(format!("{stem}.cue"));

        let sources = Self::save_source_files(&plans);
        SaveService::save_release_image_with_cue(
            plans,
            cover,
            &output_audio_path,
            &output_cue_path,
            media_catalog_number(&release.pressing.barcodes),
            preset,
            |file_id: &String| self.open_save_source(&sources[file_id]),
        )
        .await
        .map_err(LibraryError::Save)
    }
}

/// Which of a track's stored audio a saved file carries.
pub(crate) enum SaveWindow {
    /// One track of a release image: every segment as stored (audio pregap
    /// included), led by its generated pregap as silence so the image's CUE
    /// indexes line up.
    ImageTrack,
    /// A standalone track file under `placement`: the CUE audio pregap can be
    /// excluded, kept (HTOA on the first track), or relocated — the `next`
    /// track's pregap segments appended here, with its generated pregap as
    /// trailing silence.
    TrackFile {
        next: Option<TrackAudioMeta>,
        placement: crate::config::SavePregapPlacement,
        is_first_track: bool,
    },
}

/// The segments a saved track file decodes, in order, and the silence around
/// them.
struct SaveSegments<'a> {
    segments: Vec<&'a crate::db::DbAudioSegment>,
    leading_silence_frames: u64,
    trailing_silence_frames: u64,
}

impl SaveTrackPlan {
    fn segments(&self) -> SaveSegments<'_> {
        let meta = &self.audio_meta;
        match &self.window {
            SaveWindow::ImageTrack => SaveSegments {
                segments: meta
                    .audio_segments
                    .iter()
                    .map(|entry| &entry.segment)
                    .collect(),
                leading_silence_frames: pregap_frames(meta.audio_format.generated_pregap_samples),
                trailing_silence_frames: 0,
            },
            SaveWindow::TrackFile {
                next,
                placement,
                is_first_track,
            } => {
                use crate::config::SavePregapPlacement;

                let own_audio_pregap = pregap_frames(meta.audio_format.pregap_samples);
                let includes_htoa = *is_first_track
                    && *placement == SavePregapPlacement::AppendToPreviousIncludingHtoa;
                let include_own_pregap = includes_htoa || own_audio_pregap == 0;
                let mut segments: Vec<_> = meta
                    .audio_segments
                    .iter()
                    .map(|entry| &entry.segment)
                    .filter(|segment| {
                        include_own_pregap || segment.role == crate::db::DbAudioSegmentRole::Main
                    })
                    .collect();
                let leading_silence_frames = if includes_htoa {
                    pregap_frames(meta.audio_format.generated_pregap_samples)
                } else {
                    0
                };
                let mut trailing_silence_frames = 0;
                if matches!(
                    placement,
                    SavePregapPlacement::AppendToPreviousExceptHtoa
                        | SavePregapPlacement::AppendToPreviousIncludingHtoa
                ) {
                    if let Some(next) = next {
                        if pregap_frames(next.audio_format.pregap_samples) > 0 {
                            segments.extend(
                                next.audio_segments
                                    .iter()
                                    .map(|entry| &entry.segment)
                                    .filter(|segment| {
                                        segment.role == crate::db::DbAudioSegmentRole::AudioPregap
                                    }),
                            );
                        }
                        trailing_silence_frames =
                            pregap_frames(next.audio_format.generated_pregap_samples);
                    }
                }
                SaveSegments {
                    segments,
                    leading_silence_frames,
                    trailing_silence_frames,
                }
            }
        }
    }

    /// The release files this track's saved audio reads, by id.
    pub(crate) fn source_files(&self) -> Vec<String> {
        self.segments()
            .segments
            .iter()
            .map(|segment| segment.file_id.clone())
            .collect()
    }

    /// The decode of this track's saved audio from its files' open `streams`.
    pub(crate) fn decode(
        &self,
        streams: &HashMap<String, bae_audio::sparse_buffer::SharedSparseBuffer>,
    ) -> Result<StreamDecodeParams, LibraryError> {
        let window = self.segments();
        let segments = window
            .segments
            .iter()
            .map(|segment| {
                let stream = streams.get(&segment.file_id).ok_or_else(|| {
                    LibraryError::Save(format!(
                        "no audio stream opened for file {}",
                        segment.file_id
                    ))
                })?;
                Ok(SegmentDecodeParams::new(stream.clone(), segment.span(), 0))
            })
            .collect::<Result<Vec<_>, LibraryError>>()?;
        Ok(StreamDecodeParams::new(
            segments,
            byte_seekable(&self.audio_meta),
            window.leading_silence_frames,
            window.trailing_silence_frames,
        ))
    }
}

/// Whether this track's codec keeps its place on a by-byte jump to a recorded
/// landing, as playback decides it.
fn byte_seekable(meta: &TrackAudioMeta) -> bool {
    bae_audio::byte_jump_keeps_place(&meta.audio_format.content_type)
}

/// A format with no recorded pregap has none.
fn pregap_frames(samples: Option<u64>) -> u64 {
    samples.unwrap_or(0)
}

#[cfg(desktop)]
fn unique_output_path(
    dir: &std::path::Path,
    stem: &str,
    extension: &str,
    used_paths: &mut std::collections::HashSet<std::path::PathBuf>,
) -> std::path::PathBuf {
    let mut index = 1usize;
    loop {
        let candidate_stem = if index == 1 {
            stem.to_string()
        } else {
            format!("{stem} ({index})")
        };
        let path = dir.join(format!("{candidate_stem}.{extension}"));
        if used_paths.insert(path.clone()) {
            return path;
        }
        index += 1;
    }
}

/// The code a saved CUE sheet's `CATALOG` line states: the one product every
/// barcode of the pressing names, in the one spelling codes are compared by
/// (see [`bae_types::barcode::comparison_key`]). None where the pressing states
/// none, or names two products — a sheet has room for one, and neither is the
/// disc's more than the other.
fn media_catalog_number(barcodes: &[String]) -> Option<String> {
    let mut codes: Vec<String> = Vec::new();
    for key in barcodes
        .iter()
        .filter_map(|barcode| bae_types::barcode::comparison_key(barcode).ok())
    {
        if !codes.contains(&key) {
            codes.push(key);
        }
    }
    match <[String; 1]>::try_from(codes) {
        Ok([code]) => Some(code),
        Err(_) => None,
    }
}

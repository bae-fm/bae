//! The save arm: rendered standalone output files — decode then encode to the
//! preset codec, tags from bae's metadata, optional embedded cover, and a
//! token-pattern filename. The output queue/staging/marker/replace live in
//! [`super::output`]; the verbatim export arm in [`super::export`].

use super::*;

impl LibraryManager {
    /// Resolve one track's tag data from the database alone — the tag fields, its
    /// track number, the release's track total, and whether the media is digital.
    /// Reads no audio and no cover, so both the filename-suggestion path (which
    /// must not download a whole file) and the full export plan share it.
    ///
    /// No cover id rides along: the art a track export embeds is its own release's,
    /// which the caller already holds.
    async fn resolve_save_tags(
        &self,
        meta: &TrackAudioMeta,
    ) -> Result<ResolvedSaveTags, LibraryError> {
        let album = self.database.get_album_for_release(&meta.release).await?;

        let album_artists = self.database.get_artists_for_album(&album.id).await?;
        let artist = join_artist_names(&album_artists);

        let release_tracks = self
            .database
            .get_tracks_for_release(&meta.release.id)
            .await?;
        let total_tracks = release_tracks.len();
        let has_multiple_sides = release_tracks
            .iter()
            .map(|t| t.side)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;
        let disc = if has_multiple_sides {
            Some(meta.track.side)
        } else {
            None
        };

        let year = meta.release.pressing.year.or(album.year);
        let is_digital =
            crate::util::format::is_digital_format(meta.release.pressing.format.as_deref());

        let tags = SaveTags {
            title: meta.track.title.clone(),
            artist,
            album: album.title,
            year,
            disc,
        };

        Ok(ResolvedSaveTags {
            tags,
            track_number: meta.track.track_number,
            total_tracks,
            is_digital,
        })
    }

    /// Assemble everything `SaveService::save_track` needs for a
    /// track in one pass: source audio bytes, tag fields, cover image bytes,
    /// neighbour counts, and the raw audio-format aggregate for decoding.
    /// Cloud-only tracks download + decrypt here — export never requires a
    /// local copy.
    ///
    /// `embed_cover` is the preset's choice: when false the cover blob is never
    /// read (no wasted download/decrypt), so `cover_image_bytes` is `Some` only
    /// when the preset embeds *and* the release has art.
    pub async fn get_save_track_plan(
        &self,
        track_id: &str,
        embed_cover: bool,
    ) -> Result<SaveTrackPlan, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let resolved = self.resolve_save_tags(&meta).await?;

        let mut audio_bytes = Vec::new();
        for audio_file in &meta.audio_files {
            let bytes = crate::storage::transfer::read_release_file_bytes(audio_file, self)
                .await
                .map_err(|e| {
                    LibraryError::TrackMapping(format!(
                        "Couldn't read audio file {} for track {track_id}: {e}",
                        audio_file.id
                    ))
                })?;
            audio_bytes.push(super::SaveAudioBytes {
                file_id: audio_file.id.clone(),
                bytes,
            });
        }

        // The exported file embeds the art of the release the track is actually on,
        // not the album's primary release's — the same rule playback applies. When
        // the preset doesn't embed, skip the blob read entirely.
        let cover_image_bytes = if embed_cover {
            match self.cover_ref(&meta.release.id).await? {
                Some(image) => self.read_image_blob(&image).await?,
                None => None,
            }
        } else {
            None
        };

        Ok(SaveTrackPlan {
            audio_bytes,
            resolved,
            cover_image_bytes,
            audio_window: save_window_from_meta(&meta),
            audio_meta: meta,
        })
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
        let resolved = self.resolve_save_tags(&meta).await?;
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
        let mut plan = self
            .get_save_track_plan(track_id, preset.embed_cover)
            .await?;
        let release_tracks = self
            .database
            .get_tracks_for_release(&plan.audio_meta.release.id)
            .await?;
        let track_index = release_tracks
            .iter()
            .position(|track| track.id == plan.audio_meta.track.id)
            .ok_or_else(|| {
                LibraryError::Save(format!(
                    "track {} is not ordered in release {}",
                    plan.audio_meta.track.id, plan.audio_meta.release.id
                ))
            })?;
        let next_meta = if track_index + 1 < release_tracks.len() {
            Some(
                TrackAudioMeta::resolve(&self.database, &release_tracks[track_index + 1].id)
                    .await?,
            )
        } else {
            None
        };
        plan.audio_window = save_window_for_track_file(
            &plan.audio_meta,
            next_meta.as_ref(),
            preset.pregap_placement,
            track_index == 0,
        );
        SaveService::save_track(plan, output_path, preset)
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
        let mut used_paths = std::collections::HashSet::new();

        for (index, track) in tracks.iter().enumerate() {
            let mut plan = self
                .get_save_track_plan(&track.id, preset.embed_cover)
                .await?;
            let next_meta = if index + 1 < tracks.len() {
                Some(TrackAudioMeta::resolve(&self.database, &tracks[index + 1].id).await?)
            } else {
                None
            };
            plan.audio_window = save_window_for_track_file(
                &plan.audio_meta,
                next_meta.as_ref(),
                preset.pregap_placement,
                index == 0,
            );

            let stem =
                crate::library::save::render_save_filename(&preset.filename_tokens, &plan.resolved);
            let output_path = unique_output_path(
                staging_dir,
                &stem,
                preset.codec.extension(),
                &mut used_paths,
            );
            SaveService::save_track(plan, &output_path, preset.clone())
                .await
                .map_err(LibraryError::Save)?;
            let percent = (((index + 1) * 100) / total.max(1)) as u8;
            self.set_output_progress(release_id, percent);
        }

        Ok(())
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
            let mut plan = self
                .get_save_track_plan(&track.id, preset.embed_cover)
                .await?;
            plan.audio_window = save_window_from_meta(&plan.audio_meta);
            plan.audio_window.leading_silence_samples =
                non_negative_samples(plan.audio_meta.audio_format.generated_pregap_samples);
            plans.push(plan);
        }

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

        SaveService::save_release_image_with_cue(
            plans,
            &output_audio_path,
            &output_cue_path,
            release.pressing.barcode,
            preset,
        )
        .await
        .map_err(LibraryError::Save)
    }
}

fn save_window_from_meta(meta: &TrackAudioMeta) -> super::SaveAudioWindow {
    super::SaveAudioWindow {
        segments: save_segment_windows(meta, true),
        leading_silence_samples: 0,
        trailing_silence_samples: 0,
    }
}

fn save_window_for_track_file(
    meta: &TrackAudioMeta,
    next_meta: Option<&TrackAudioMeta>,
    placement: crate::config::SavePregapPlacement,
    is_first_track: bool,
) -> super::SaveAudioWindow {
    let own_audio_pregap = non_negative_samples(meta.audio_format.pregap_samples);
    let own_generated_pregap = non_negative_samples(meta.audio_format.generated_pregap_samples);
    let includes_htoa = is_first_track
        && placement == crate::config::SavePregapPlacement::AppendToPreviousIncludingHtoa;
    let mut segments = save_segment_windows(meta, includes_htoa || own_audio_pregap == 0);
    let leading_silence_samples = if includes_htoa {
        own_generated_pregap
    } else {
        0
    };

    let mut trailing_silence_samples = 0;
    if matches!(
        placement,
        crate::config::SavePregapPlacement::AppendToPreviousExceptHtoa
            | crate::config::SavePregapPlacement::AppendToPreviousIncludingHtoa
    ) {
        if let Some(next) = next_meta {
            let next_audio_pregap = non_negative_samples(next.audio_format.pregap_samples);
            if next_audio_pregap > 0 {
                segments.extend(save_segment_windows_for_role(
                    next,
                    crate::db::DbAudioSegmentRole::AudioPregap,
                ));
            }
            trailing_silence_samples =
                non_negative_samples(next.audio_format.generated_pregap_samples);
        }
    }

    super::SaveAudioWindow {
        segments,
        leading_silence_samples,
        trailing_silence_samples,
    }
}

fn save_segment_windows(
    meta: &TrackAudioMeta,
    include_audio_pregap: bool,
) -> Vec<super::SaveAudioSegmentWindow> {
    meta.audio_segments
        .iter()
        .filter(|segment| {
            include_audio_pregap || segment.role == crate::db::DbAudioSegmentRole::Main
        })
        .map(save_segment_window)
        .collect()
}

fn save_segment_windows_for_role(
    meta: &TrackAudioMeta,
    role: crate::db::DbAudioSegmentRole,
) -> Vec<super::SaveAudioSegmentWindow> {
    meta.audio_segments
        .iter()
        .filter(|segment| segment.role == role)
        .map(save_segment_window)
        .collect()
}

fn save_segment_window(segment: &crate::db::DbAudioSegment) -> super::SaveAudioSegmentWindow {
    super::SaveAudioSegmentWindow {
        file_id: segment.file_id.clone(),
        source_start_sample: u64::try_from(segment.start_sample)
            .expect("audio segment start_sample is non-negative"),
        source_end_sample: segment
            .end_sample
            .map(|sample| u64::try_from(sample).expect("audio segment end_sample is non-negative")),
    }
}

fn non_negative_samples(samples: Option<i64>) -> u64 {
    samples.map_or(0, |sample| {
        u64::try_from(sample).expect("audio_format pregap samples are non-negative")
    })
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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

//! The save arm: rendered standalone output files — decode then encode to the
//! preset codec, tags from bae's metadata, optional embedded cover, and a
//! token-pattern filename. The output queue/staging/marker/replace live in
//! [`super::output`]; the verbatim export arm in [`super::export`].

use super::output::output_percent;
use super::*;
use crate::SaveTrackPlan;
use bae_records::{ResolvedTrackAudio, TrackForSave};
use bae_track_audio::track_sources::SourceStream;
use bae_track_audio::{SegmentDecodeParams, StreamDecodeParams};
use std::collections::HashMap;
use std::num::NonZeroUsize;

impl LibraryManager {
    /// One track's save plan under `window`: its tags and stored audio as
    /// the library's read gave them. No source is opened here; the save opens
    /// the files the plan reads while it saves that track.
    fn save_track_plan(
        track: TrackForSave,
        window: SaveWindow,
        loudness_tags: bool,
    ) -> SaveTrackPlan {
        let replay_gain = loudness_tags.then(|| crate::save::ReplayGainTags::of(&track.audio));
        SaveTrackPlan {
            resolved: ResolvedSaveTags::of(track.tags),
            audio: track.audio,
            window,
            replay_gain,
        }
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
        self.read_cover_copy(release_id, CoverCopy::Large).await
    }

    /// Open a streaming read of one release file for the save decoder: a
    /// sparse buffer sized to the stored file size, filled on demand through
    /// coven's locality-aware ranged read (the user's own file, the local
    /// store, the cache, or the cloud with decrypt). A read failure — or a
    /// file shorter than the stored size — fails the buffer, and the decode
    /// fails loudly with that read's error.
    fn open_save_source(
        &self,
        file_id: &str,
        file_size: u64,
        lease: &coven::FileLease,
    ) -> SourceStream {
        SourceStream::of_release_file(
            self.runtime_handle.clone(),
            self.records.clone(),
            file_id.to_string(),
            file_size,
            self.diagnostics.clone(),
            lease.clone(),
            "save",
        )
    }

    /// The size of every release file `plans` read, by id, for opening them
    /// as the save reaches the tracks reading them.
    fn save_source_sizes<'a>(
        plans: impl IntoIterator<Item = &'a SaveTrackPlan>,
    ) -> HashMap<String, u64> {
        plans
            .into_iter()
            .flat_map(|plan| {
                plan.audio
                    .segments
                    .iter()
                    .chain(plan.window.next.iter().flat_map(|next| next.segments.iter()))
            })
            .map(|segment| (segment.file_id.clone(), segment.file_size))
            .collect()
    }

    /// The default filename (stem, no extension) a single-track "Save as…"
    /// suggests for `track_id`, rendered from the library's one filename
    /// pattern and the track's tag data, and written as a release save writes
    /// it (`save_file_name`). The name is the same in every format. Reads no
    /// audio and no cover — only the database — so seeding a save panel never
    /// touches a whole file or the cloud.
    pub async fn save_track_suggested_name(&self, track_id: &str) -> Result<String, LibraryError> {
        let pattern = self.preferences().await?.save_filename_pattern;
        let resolved = ResolvedSaveTags::of(self.records.track_tags(track_id).await?);
        let stem = crate::save::render_save_filename(&pattern, &resolved)
            .map_err(LibraryError::FilenamePattern)?;
        // coven's name for a file depends on its extension only when a name
        // is cut to fit; `flac` and `aiff` are the longest, so the stem fits
        // under every format's extension.
        let name = crate::save::save_file_name(&stem, "flac");
        Ok(name
            .to_str()
            .and_then(|name| name.strip_suffix(".flac"))
            .expect("coven writes a name made of text as text, keeping its extension")
            .to_string())
    }

    /// Save one track to `output_path` under the preset named by `preset_id`
    /// (must exist). The source audio is decoded and re-encoded to the
    /// preset's codec, or copied when the track is already in it; either way
    /// the file is tagged from bae's metadata and has the cover art embedded.
    pub async fn save_track(
        &self,
        track_id: &str,
        output_path: &Path,
        preset_id: &str,
    ) -> Result<(), LibraryError> {
        let preset = self.save_preset_for(preset_id).await?;
        self.set_last_save_preset(preset.id.clone()).await?;
        let read = self.records.track_for_save(track_id).await?;
        let extras = self.preferences().await?.save_extras;
        let cover = self.save_cover_image(&read.track.audio.release_id, extras.embed_cover).await?;
        let plan = Self::save_track_plan(
            read.track,
            SaveWindow { next: read.next, is_first_track: read.is_first },
            extras.loudness_tags,
        );
        let sources = Self::save_source_sizes([&plan]);
        SaveService::save_tracks(
            vec![(plan, output_path.to_path_buf())],
            cover,
            preset.codec,
            &self.files,
            NonZeroUsize::MIN,
            crate::save::ExistingOutput::Replace,
            |file_id: &String, lease: &coven::FileLease| {
                self.open_save_source(file_id, sources[file_id], lease)
            },
            Arc::new(|| {}),
            self.bugs(),
        )
        .await
        .map_err(|error| self.save_failed(error))
    }

    /// Save the release `release_id` into `staging_dir` under `preset`, each
    /// file named by `filename_pattern`.
    pub(super) async fn save_release_tracks_to_dir(
        &self,
        release_id: &str,
        preset: bae_preferences::SavePreset,
        filename_pattern: &bae_preferences::SaveFilenamePattern,
        extras: bae_preferences::SaveExtras,
        staging_dir: &std::path::Path,
    ) -> Result<(), LibraryError> {
        let tracks = self.records.release_tracks_for_save(release_id).await?;
        let total = tracks.len();
        let cover = self.save_cover_image(release_id, extras.embed_cover).await?;
        // Each track's plan carries the track after it, whose gap goes at the
        // end of this one's file.
        let nexts: Vec<Option<ResolvedTrackAudio>> = tracks
            .iter()
            .skip(1)
            .map(|track| Some(track.audio.clone()))
            .chain(std::iter::once(None))
            .collect();
        let mut used_paths = std::collections::HashSet::new();
        let mut saves = Vec::with_capacity(total);
        for (index, (track, next)) in tracks.into_iter().zip(nexts).enumerate() {
            let plan = Self::save_track_plan(
                track,
                SaveWindow { next, is_first_track: index == 0 },
                extras.loudness_tags,
            );
            let stem = crate::save::render_save_filename(filename_pattern, &plan.resolved)
                .map_err(LibraryError::FilenamePattern)?;
            let output_path =
                unique_output_path(staging_dir, &stem, preset.codec.extension(), &mut used_paths);
            saves.push((plan, output_path));
        }

        let sources = Self::save_source_sizes(saves.iter().map(|(plan, _)| plan));
        let saved = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let on_saved = {
            let manager = self.clone();
            let release_id = release_id.to_string();
            Arc::new(move || {
                let saved = saved.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                manager.set_output_progress(&release_id, output_percent(saved, total));
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
            &self.files,
            parallelism,
            crate::save::ExistingOutput::Refuse,
            |file_id: &String, lease: &coven::FileLease| {
                self.open_save_source(file_id, sources[file_id], lease)
            },
            on_saved,
            self.bugs(),
        )
        .await
        .map_err(|error| self.save_failed(error))
    }

    /// A save's failure as the library reports it: a save that could not be
    /// written fails as a save, and work in it that panicked is reported as
    /// a bug.
    fn save_failed(&self, error: crate::save::SaveError) -> LibraryError {
        match error {
            crate::save::SaveError::Failed(detail) => LibraryError::Save(detail),
            crate::save::SaveError::Unfinished(failed) => self.bug(failed).into(),
        }
    }
}

/// Which of a track's stored audio its saved file carries. Every save writes
/// one file per track by one rule: the track's gap goes at the end of the
/// track before it (the `next` track's pregap segments appended here, with its
/// generated pregap as trailing silence), and the hidden audio before track 1
/// stays at the start of track 1.
pub(crate) struct SaveWindow {
    pub(crate) next: Option<ResolvedTrackAudio>,
    pub(crate) is_first_track: bool,
}

/// The segments a saved track file decodes, in order, and the silence around
/// them.
struct SaveSegments<'a> {
    segments: Vec<&'a bae_records::ResolvedTrackAudioSegment>,
    leading_silence_frames: u64,
    trailing_silence_frames: u64,
}

impl SaveTrackPlan {
    fn segments(&self) -> SaveSegments<'_> {
        let audio = &self.audio;
        let SaveWindow { next, is_first_track } = &self.window;

        let own_audio_pregap = pregap_frames(audio.pregap_samples);
        let include_own_pregap = *is_first_track || own_audio_pregap == 0;
        let mut segments: Vec<_> = audio
            .segments
            .iter()
            .filter(|segment| {
                include_own_pregap || segment.role == bae_records::DbAudioSegmentRole::Main
            })
            .collect();
        let leading_silence_frames =
            if *is_first_track { pregap_frames(audio.generated_pregap_samples) } else { 0 };
        let mut trailing_silence_frames = 0;
        if let Some(next) = next {
            if pregap_frames(next.pregap_samples) > 0 {
                segments.extend(next.segments.iter().filter(|segment| {
                    segment.role == bae_records::DbAudioSegmentRole::AudioPregap
                }));
            }
            trailing_silence_frames = pregap_frames(next.generated_pregap_samples);
        }
        SaveSegments { segments, leading_silence_frames, trailing_silence_frames }
    }

    /// The release file whose bytes are already what `codec` writes for this
    /// track, which a save copies instead of decoding and encoding it: the
    /// track is all of one file, at the depth the codec writes, in the codec's
    /// own container. Only lossless codecs qualify, since a lossy one writes
    /// the bitrate the format names, which the stored file does not state. A
    /// track cut from an image, or given a gap or hidden audio from the track
    /// after or before it, is not one file's bytes.
    pub(crate) fn copied_file(&self, codec: &bae_preferences::SaveCodec) -> Option<&str> {
        use bae_preferences::SaveCodec;
        use bae_util::audio_container::AudioContainer;
        use bae_util::content_type::ContentType;

        let window = self.segments();
        let [segment] = window.segments[..] else {
            return None;
        };
        if window.leading_silence_frames != 0
            || window.trailing_silence_frames != 0
            || segment.span != bae_records::SegmentSpan::whole_file()
        {
            return None;
        }
        let container = &segment.container;
        let audio = &self.audio;
        let depth = audio.bits_per_sample;
        // What the file states of PCM does not say whether its samples are
        // signed integers, which float, unsigned and companded PCM share the
        // type with; WAV and AIFF files at 16 or 24 bits always are.
        let integer_pcm = audio.content_type == ContentType::Pcm
            && matches!(depth.map(std::num::NonZeroU32::get), Some(16 | 24));
        let already = match codec {
            SaveCodec::Flac { bit_depth } => {
                *container == AudioContainer::Flac
                    && audio.content_type == ContentType::Flac
                    && depth.is_some_and(|depth| bit_depth.bits(Some(depth)) == depth.get())
            }
            SaveCodec::Wav { bit_depth } => {
                *container == AudioContainer::Wav
                    && integer_pcm
                    && depth.is_some_and(|depth| bit_depth.bits(Some(depth)) == depth.get())
            }
            SaveCodec::Aiff { bit_depth } => {
                *container == AudioContainer::Aiff
                    && integer_pcm
                    && depth.is_some_and(|depth| bit_depth.bits(Some(depth)) == depth.get())
            }
            SaveCodec::Mp3 { .. } | SaveCodec::Aac { .. } | SaveCodec::OpusOgg { .. } => false,
        };
        already.then_some(segment.file_id.as_str())
    }

    /// The release files this track's saved audio reads, by id.
    pub(crate) fn source_files(&self) -> Vec<String> {
        self.segments().segments.iter().map(|segment| segment.file_id.clone()).collect()
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
                Ok(SegmentDecodeParams::new(stream.clone(), segment.span, 0))
            })
            .collect::<Result<Vec<_>, LibraryError>>()?;
        Ok(StreamDecodeParams::new(
            segments,
            byte_seekable(&self.audio),
            window.leading_silence_frames,
            window.trailing_silence_frames,
        ))
    }
}

/// Whether this track's codec keeps its place on a by-byte jump to a recorded
/// landing, as playback decides it.
fn byte_seekable(audio: &ResolvedTrackAudio) -> bool {
    bae_audio::byte_jump_keeps_place(&audio.content_type)
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
        let candidate_stem =
            if index == 1 { stem.to_string() } else { format!("{stem} ({index})") };
        let path = dir.join(crate::save::save_file_name(&candidate_stem, extension));
        if used_paths.insert(path.clone()) {
            return path;
        }
        index += 1;
    }
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;

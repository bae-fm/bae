use crate::audio_codec::{EncodeFormat, StreamingEncoder};
use crate::library::SaveTrackPlan;
use crate::playback::track_sources::{run_tracks_over_sources, SourceStream};
use crate::playback::SharedSparseBuffer;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, warn};

/// Render a single-track export's suggested filename stem (no extension) from
/// the ordered token list and the track's tag data. Each token substitutes its
/// value; an absent value (no year, say) drops out, and the non-empty values
/// join with single spaces. The result goes through `sanitize_filename_stem`;
/// if that leaves it empty, fall back to the sanitized title, then to "track".
pub fn render_save_filename(
    tokens: &[crate::config::SaveFilenameToken],
    resolved: &crate::library::manager::ResolvedSaveTags,
) -> String {
    use crate::config::SaveFilenameToken;

    let tags = &resolved.tags;
    let rendered = tokens
        .iter()
        .map(|token| match token {
            SaveFilenameToken::Title => tags.title.clone(),
            SaveFilenameToken::Artist => tags.artist.clone(),
            SaveFilenameToken::Album => tags.album.clone(),
            // An absent year / disc / track number renders empty and drops out
            // of the join. That is a legitimate state for a filename pattern,
            // not an error.
            SaveFilenameToken::Year => tags.year.map(|y| y.to_string()).unwrap_or_default(),
            // Zero-padded to two digits so tracks sort lexically; empty when the
            // track carries no number.
            SaveFilenameToken::TrackNumber => resolved
                .track_number
                .map(|n| format!("{n:02}"))
                .unwrap_or_default(),
            SaveFilenameToken::DiscNumber => tags.disc.map(|d| d.to_string()).unwrap_or_default(),
            SaveFilenameToken::TrackTotal => resolved.total_tracks.to_string(),
        })
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let stem = sanitize_filename_stem(&rendered);
    if !stem.is_empty() {
        return stem;
    }
    // The template rendered nothing usable (every value it referenced was empty).
    // Log rather than silently invent a name, then fall back to the track title,
    // and finally to a fixed stem.
    let title_stem = sanitize_filename_stem(&tags.title);
    if !title_stem.is_empty() {
        debug!(
            track_title = %tags.title,
            "export filename template rendered empty; using track title"
        );
        return title_stem;
    }
    debug!(
        track_title = %tags.title,
        "export filename template and track title both empty; using \"track\""
    );
    "track".to_string()
}

/// Sanitize a rendered filename stem: control characters and the ones illegal on
/// macOS/Windows become '-', whitespace runs collapse to one space, leading and
/// trailing spaces and dashes are trimmed, and a leading '.' is stripped so the
/// file isn't hidden. Replacing the separators is also what makes a `../` escape
/// impossible.
pub(crate) fn sanitize_filename_stem(input: &str) -> String {
    let replaced: String = input
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let collapsed = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_matches(|c: char| c == ' ' || c == '-')
        .trim_start_matches('.')
        .to_string()
}

pub(crate) struct SaveService;

struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

struct OutputPathsGuard {
    paths: Vec<std::path::PathBuf>,
    committed: bool,
}

impl OutputPathsGuard {
    fn new(paths: Vec<std::path::PathBuf>) -> Self {
        Self {
            paths,
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for OutputPathsGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for path in &self.paths {
            if let Err(error) = std::fs::remove_file(path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    warn!(
                        path = %path.display(),
                        "failed to remove incomplete export output: {error}"
                    );
                }
            }
        }
    }
}

/// One track handed to a release image's encoder thread: its decode, and
/// where to send back its CUE entry.
struct ImageTrack {
    index: usize,
    track_id: String,
    title: String,
    performer: String,
    audio_pregap_samples: Option<i64>,
    decode: crate::playback::stream_pipeline::StreamDecodeParams,
    reply: tokio::sync::oneshot::Sender<Result<CueTrack, String>>,
}

impl SaveService {
    /// Save each track to its own output path, up to `parallelism` at once.
    ///
    /// For one-file-per-track: decodes and re-encodes to the target format.
    /// For CUE/FLAC: extracts, decodes, and re-encodes as a standalone file.
    /// Embeds metadata (title, artist, album, year, track number, cover art).
    /// A source file is open only while a track reading it is being saved
    /// (see [`run_tracks_over_sources`]); `open` opens one by release file id.
    /// `on_saved` runs as each track's file is written.
    ///
    /// On future-drop the cancel flag flips, the encoder loop exits between
    /// frames, and any partially-written output file is removed.
    pub(crate) async fn save_tracks(
        tracks: Vec<(SaveTrackPlan, PathBuf)>,
        cover_image_bytes: Option<Vec<u8>>,
        codec: crate::config::SaveCodec,
        parallelism: NonZeroUsize,
        open: impl Fn(&String) -> SourceStream,
        on_saved: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), String> {
        let cover_image_bytes = Arc::new(cover_image_bytes);
        run_tracks_over_sources(
            tracks,
            parallelism,
            |(plan, _): &(SaveTrackPlan, PathBuf)| plan.source_files(),
            open,
            |(plan, output_path), streams| {
                let cover_image_bytes = cover_image_bytes.clone();
                let codec = codec.clone();
                let on_saved = on_saved.clone();
                async move {
                    Self::save_track_with_codec(
                        plan,
                        streams,
                        cover_image_bytes,
                        output_path,
                        codec,
                    )
                    .await?;
                    on_saved();
                    Ok(())
                }
            },
        )
        .await
        .map(|_| ())
    }

    /// Save every track, in order, into one audio image with a CUE sheet
    /// indexing them. Tracks stream into the image one at a time; a source
    /// file is open only while a track reading it is being encoded.
    pub(crate) async fn save_release_image_with_cue(
        plans: Vec<SaveTrackPlan>,
        cover_image_bytes: Option<Vec<u8>>,
        output_audio_path: &Path,
        output_cue_path: &Path,
        catalog: Option<String>,
        preset: crate::config::SavePreset,
        open: impl Fn(&String) -> SourceStream,
    ) -> Result<(), String> {
        if plans.is_empty() {
            return Err("release image export requires at least one track".to_string());
        }
        if plans.len() > 99 {
            return Err("CUE sheets support at most 99 audio tracks".to_string());
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let _cancel_guard = CancelOnDrop(Arc::clone(&cancel));

        let source_bits_per_sample = plans[0].audio_meta.audio_format.bits_per_sample;
        let sample_rate = stored_sample_rate(&plans[0])?;
        let channels = stored_channels(&plans[0])?;
        let release_title = plans[0].resolved.tags.album.clone();
        let release_performer = plans[0].resolved.tags.artist.clone();
        let is_digital = plans[0].resolved.is_digital;
        let year = plans[0].resolved.tags.year;
        let cue_file_type = cue_file_type(&preset.codec)?;
        let audio_filename = output_audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "audio image path has no UTF-8 filename: {}",
                    output_audio_path.display()
                )
            })?
            .to_string();

        let format = encode_format(&preset.codec, source_bits_per_sample);
        let tag_type = codec_tag_type(&preset.codec);
        let tags = crate::library::manager::SaveTags {
            title: release_title.clone(),
            artist: release_performer.clone(),
            album: release_title.clone(),
            year,
            disc: None,
        };
        let total_tracks = plans.len() as u32;

        let output_audio_path = output_audio_path.to_path_buf();
        let output_cue_path = output_cue_path.to_path_buf();
        // Declared before the encode starts, so every exit below removes the
        // partial image and sheet unless the save commits them.
        let mut output_guard =
            OutputPathsGuard::new(vec![output_audio_path.clone(), output_cue_path.clone()]);

        // The encoder lives on one thread for the whole image (FFmpeg's
        // contexts cannot move between threads); tracks reach it in order,
        // one at a time, each while its sources are open.
        let (tracks_tx, tracks_rx) = std::sync::mpsc::channel::<ImageTrack>();
        let encode = tokio::task::spawn_blocking({
            let output_audio_path = output_audio_path.clone();
            let cancel = cancel.clone();
            move || -> Result<(), String> {
                let file = std::fs::File::create(&output_audio_path)
                    .map_err(|e| format!("Failed to create release image: {e}"))?;
                let mut encoder =
                    StreamingEncoder::seekable(format, Box::new(file), cancel.clone());
                let mut current_sample_frame = 0u64;
                let mut failed = false;
                for track in tracks_rx {
                    let entry = encode_image_track(
                        &track,
                        &mut encoder,
                        &mut current_sample_frame,
                        sample_rate,
                        channels,
                        cancel.clone(),
                    );
                    failed |= entry.is_err();
                    // The track's save is waiting on this answer; one that
                    // stopped waiting was cancelled with the whole save.
                    let _ = track.reply.send(entry);
                }
                if failed {
                    return Ok(());
                }
                encoder.finish()
            }
        });

        let tracks = run_tracks_over_sources(
            plans.into_iter().enumerate().collect(),
            NonZeroUsize::MIN,
            |(_, plan): &(usize, SaveTrackPlan)| plan.source_files(),
            open,
            |(index, plan), streams| {
                let tracks_tx = tracks_tx.clone();
                async move {
                    let decode = plan.decode(&streams).map_err(|e| e.to_string())?;
                    drop(streams);
                    let (reply, entry) = tokio::sync::oneshot::channel();
                    tracks_tx
                        .send(ImageTrack {
                            index,
                            track_id: plan.audio_meta.track.id.clone(),
                            title: plan.resolved.tags.title.clone(),
                            performer: plan.resolved.tags.artist.clone(),
                            audio_pregap_samples: plan.audio_meta.audio_format.pregap_samples,
                            decode,
                            reply,
                        })
                        .map_err(|_| "the release image encoder stopped".to_string())?;
                    entry
                        .await
                        .map_err(|_| "the release image encoder stopped".to_string())?
                }
            },
        )
        .await;
        drop(tracks_tx);
        // The encoder's own failure (it could not create the image, or finish
        // it) explains a track's "stopped" better than the track does.
        encode
            .await
            .map_err(|e| format!("encode task join error: {e}"))??;
        let cue_tracks = tracks?;

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            if cancel.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            let cue = render_cue_sheet(
                &release_title,
                &release_performer,
                catalog
                    .as_deref()
                    .map(str::trim)
                    .filter(|catalog| !catalog.is_empty()),
                year,
                &audio_filename,
                cue_file_type,
                sample_rate,
                &cue_tracks,
            );
            std::fs::write(&output_cue_path, cue)
                .map_err(|e| format!("Failed to write CUE sheet: {e}"))?;

            if cancel.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            write_tags(
                &output_audio_path,
                tag_type,
                &tags,
                None,
                total_tracks,
                is_digital,
                cover_image_bytes.as_deref(),
            )?;

            if cancel.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }
            Ok(())
        })
        .await
        .map_err(|e| format!("encode task join error: {e}"))??;

        output_guard.commit();
        Ok(())
    }

    async fn save_track_with_codec(
        plan: SaveTrackPlan,
        streams: HashMap<String, SharedSparseBuffer>,
        cover_image_bytes: Arc<Option<Vec<u8>>>,
        output_path: PathBuf,
        codec: crate::config::SaveCodec,
    ) -> Result<(), String> {
        let track_id = plan.audio_meta.track.id.clone();
        debug!("Exporting track {} to {}", track_id, output_path.display());

        let cancel = Arc::new(AtomicBool::new(false));
        let _cancel_guard = CancelOnDrop(Arc::clone(&cancel));

        let format = encode_format(&codec, plan.audio_meta.audio_format.bits_per_sample);
        let tag_type = codec_tag_type(&codec);
        let sample_rate = stored_sample_rate(&plan)?;
        let channels = stored_channels(&plan)?;
        let decode = plan.decode(&streams).map_err(|e| e.to_string())?;
        drop(streams);

        let cancel_for_blocking = Arc::clone(&cancel);
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let mut output_guard = OutputPathsGuard::new(vec![output_path.clone()]);

            let file = std::fs::File::create(&output_path)
                .map_err(|e| format!("Failed to create track file: {e}"))?;
            let mut encoder =
                StreamingEncoder::seekable(format, Box::new(file), cancel_for_blocking.clone());
            decode.run_to_sink(
                sample_rate,
                channels,
                &mut encoder,
                cancel_for_blocking.clone(),
            )?;
            // `finish` closes the muxer and drops the file handle, so the tag
            // writer below reads a fully-flushed file.
            encoder.finish()?;

            // Cancel check at every blocking boundary. write_tags can run for a
            // while on slow disks; checking around it keeps the
            // cancel-to-cleanup window tight. On bail-out OutputPathsGuard
            // removes whatever was written.
            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            write_tags(
                &output_path,
                tag_type,
                &plan.resolved.tags,
                plan.resolved.track_number.map(|n| n as u32),
                plan.resolved.total_tracks as u32,
                plan.resolved.is_digital,
                cover_image_bytes.as_deref(),
            )?;

            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            output_guard.commit();
            Ok(())
        })
        .await
        .map_err(|e| format!("encode task join error: {e}"))??;

        debug!("Successfully exported track {}", track_id);
        Ok(())
    }
}

/// Stream one track of a release image into its encoder and index it: its
/// CUE track entry, with INDEX 00 where its pregap starts and INDEX 01 where
/// its audio does.
fn encode_image_track(
    track: &ImageTrack,
    encoder: &mut StreamingEncoder,
    current_sample_frame: &mut u64,
    sample_rate: u32,
    channels: u32,
    cancel: Arc<AtomicBool>,
) -> Result<CueTrack, String> {
    let pregap_sample_frames = track
        .decode
        .leading_silence_frames()
        .checked_add(non_negative_samples(track.audio_pregap_samples)?)
        .ok_or_else(|| "CUE pregap sample count overflow".to_string())?;

    let frames_before = encoder.frames_accepted();
    track
        .decode
        .run_to_sink(sample_rate, channels, encoder, cancel)?;
    if let Some(error) = encoder.error() {
        return Err(error.to_string());
    }
    let segment_sample_frames = encoder.frames_accepted() - frames_before;
    if pregap_sample_frames > segment_sample_frames {
        return Err(format!(
            "track {} pregap exceeds decoded segment length",
            track.track_id
        ));
    }

    let start = *current_sample_frame;
    *current_sample_frame = start
        .checked_add(segment_sample_frames)
        .ok_or_else(|| "release image sample count overflow".to_string())?;
    Ok(CueTrack {
        number: u8::try_from(track.index + 1)
            .map_err(|_| "CUE track number exceeds 99".to_string())?,
        title: track.title.clone(),
        performer: track.performer.clone(),
        index_00_sample_frame: (pregap_sample_frames > 0).then_some(start),
        index_01_sample_frame: start
            .checked_add(pregap_sample_frames)
            .ok_or_else(|| "CUE index sample count overflow".to_string())?,
    })
}

/// The stored sample rate for this plan's track, validated usable. The decode
/// re-announces the probed rate, so a stored-vs-probed mismatch still fails
/// loud at the encoder.
fn stored_sample_rate(plan: &SaveTrackPlan) -> Result<u32, String> {
    u32::try_from(plan.audio_meta.audio_format.sample_rate)
        .ok()
        .filter(|&rate| rate > 0)
        .ok_or_else(|| {
            format!(
                "stored sample rate {} is unusable",
                plan.audio_meta.audio_format.sample_rate
            )
        })
}

/// The stored channel count for this plan's track, validated non-zero.
fn stored_channels(plan: &SaveTrackPlan) -> Result<u32, String> {
    u32::try_from(plan.audio_meta.audio_format.channels)
        .ok()
        .filter(|&channels| channels > 0)
        .ok_or_else(|| {
            format!(
                "stored channel count {} is unusable",
                plan.audio_meta.audio_format.channels
            )
        })
}

/// The encoder format for a save codec, with bit depths resolved against the
/// source.
fn encode_format(
    codec: &crate::config::SaveCodec,
    source_bits_per_sample: Option<i64>,
) -> EncodeFormat {
    use crate::config::SaveCodec;
    match codec {
        SaveCodec::Flac { bit_depth } => EncodeFormat::Flac {
            bits_per_sample: bit_depth.resolve(source_bits_per_sample),
        },
        SaveCodec::Mp3 { bitrate_kbps } => EncodeFormat::Mp3 {
            bitrate_kbps: *bitrate_kbps,
        },
        SaveCodec::Aac { bitrate_kbps } => EncodeFormat::Aac {
            bitrate_kbps: *bitrate_kbps,
        },
        SaveCodec::OpusOgg { bitrate_kbps } => EncodeFormat::OpusOgg {
            bitrate_kbps: *bitrate_kbps,
        },
        SaveCodec::Wav { bit_depth } => EncodeFormat::PcmWav {
            bits_per_sample: bit_depth.resolve(source_bits_per_sample),
        },
        SaveCodec::Aiff { bit_depth } => EncodeFormat::PcmAiff {
            bits_per_sample: bit_depth.resolve(source_bits_per_sample),
        },
    }
}

/// The tag container lofty writes for a save codec's output file.
fn codec_tag_type(codec: &crate::config::SaveCodec) -> lofty::tag::TagType {
    use crate::config::SaveCodec;
    match codec {
        SaveCodec::Flac { .. } => lofty::tag::TagType::VorbisComments,
        SaveCodec::Mp3 { .. } => lofty::tag::TagType::Id3v2,
        SaveCodec::Aac { .. } => lofty::tag::TagType::Mp4Ilst,
        SaveCodec::OpusOgg { .. } => lofty::tag::TagType::VorbisComments,
        SaveCodec::Wav { .. } => lofty::tag::TagType::RiffInfo,
        SaveCodec::Aiff { .. } => lofty::tag::TagType::AiffText,
    }
}

struct CueTrack {
    number: u8,
    title: String,
    performer: String,
    index_00_sample_frame: Option<u64>,
    index_01_sample_frame: u64,
}

fn render_cue_sheet(
    release_title: &str,
    release_performer: &str,
    catalog: Option<&str>,
    year: Option<i32>,
    audio_filename: &str,
    cue_file_type: &'static str,
    sample_rate: u32,
    tracks: &[CueTrack],
) -> String {
    let mut cue = String::new();
    if let Some(catalog) = catalog {
        cue.push_str(&format!("CATALOG {}\n", catalog));
    }
    if let Some(year) = year {
        cue.push_str(&format!("REM DATE {year}\n"));
    }
    cue.push_str(&format!(
        "PERFORMER \"{}\"\n",
        cue_string(release_performer)
    ));
    cue.push_str(&format!("TITLE \"{}\"\n", cue_string(release_title)));
    cue.push_str(&format!(
        "FILE \"{}\" {}\n",
        cue_string(audio_filename),
        cue_file_type
    ));
    for track in tracks {
        cue.push_str(&format!("  TRACK {:02} AUDIO\n", track.number));
        cue.push_str(&format!("    TITLE \"{}\"\n", cue_string(&track.title)));
        cue.push_str(&format!(
            "    PERFORMER \"{}\"\n",
            cue_string(&track.performer)
        ));
        if let Some(index_00) = track.index_00_sample_frame {
            cue.push_str(&format!(
                "    INDEX 00 {}\n",
                cue_time(index_00, sample_rate)
            ));
        }
        cue.push_str(&format!(
            "    INDEX 01 {}\n",
            cue_time(track.index_01_sample_frame, sample_rate)
        ));
    }
    cue
}

fn cue_file_type(codec: &crate::config::SaveCodec) -> Result<&'static str, String> {
    match codec {
        crate::config::SaveCodec::Mp3 { .. } => Ok("MP3"),
        crate::config::SaveCodec::Aiff { .. } => Ok("AIFF"),
        crate::config::SaveCodec::Flac { .. }
        | crate::config::SaveCodec::Wav { .. } => Ok("WAVE"),
        crate::config::SaveCodec::Aac { .. } => Err(
            "single-file CUE export does not support AAC because CUE has no AAC file type"
                .to_string(),
        ),
        crate::config::SaveCodec::OpusOgg { .. } => Err(
            "single-file CUE export does not support Opus/Ogg because CUE has no Opus/Ogg file type"
                .to_string(),
        ),
    }
}

fn cue_time(sample_frame: u64, sample_rate: u32) -> String {
    let cue_frames = sample_frame.saturating_mul(75) / u64::from(sample_rate);
    let minutes = cue_frames / (75 * 60);
    let seconds = (cue_frames / 75) % 60;
    let frames = cue_frames % 75;
    format!("{minutes:02}:{seconds:02}:{frames:02}")
}

fn cue_string(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() {
                ' '
            } else if c == '"' {
                '\''
            } else {
                c
            }
        })
        .collect()
}

fn non_negative_samples(samples: Option<i64>) -> Result<u64, String> {
    match samples {
        Some(sample) => {
            u64::try_from(sample).map_err(|_| "audio pregap sample count is negative".to_string())
        }
        None => Ok(0),
    }
}

/// Write every known metadata tag that describes this exported file. Cover art
/// is written when the export plan found cover bytes; disc number is only
/// written for digital media because side-based media do not map to ID3 discs.
fn write_tags(
    path: &Path,
    tag_type: lofty::tag::TagType,
    tags: &crate::library::manager::SaveTags,
    track_number: Option<u32>,
    total_tracks: u32,
    is_digital: bool,
    cover_data: Option<&[u8]>,
) -> Result<(), String> {
    use lofty::config::WriteOptions;
    use lofty::picture::{Picture, PictureType};
    use lofty::prelude::*;
    use lofty::tag::{items::Timestamp, Tag};

    let mut tagged_file = lofty::read_from_path(path)
        .map_err(|e| format!("Failed to read file for tagging: {}", e))?;

    let mut tag = Tag::new(tag_type);
    tag.set_title(tags.title.clone());
    tag.set_artist(tags.artist.clone());
    tag.set_album(tags.album.clone());

    if let Some(year) = tags.year {
        tag.set_date(Timestamp {
            year: year as u16,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        });
    }

    if let Some(n) = track_number {
        tag.set_track(n);
        tag.set_track_total(total_tracks);
    }

    // Skip the disc tag on vinyl / cassette: ID3 "disc number" doesn't
    // describe a physical side, and writing it mislabels side B as disc 2.
    if is_digital {
        if let Some(disc) = tags.disc {
            tag.set_disk(disc as u32);
        }
    }

    if let Some(data) = cover_data {
        // The stored cover is already a ≤600 JPEG (resized at store time), so
        // embed its bytes directly — no second resize/JPEG pass here.
        tag.push_picture(
            Picture::unchecked(data.to_vec())
                .pic_type(PictureType::CoverFront)
                .build(),
        );
    }

    tagged_file.insert_tag(tag);

    tagged_file
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| format!("Failed to write tags: {}", e))?;

    debug!("Wrote metadata tags to {}", path.display());
    Ok(())
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;

use crate::library::manager::ExportTrackPlan;
use crate::playback::{DecodedPcm, PlaybackError};
use crate::util::content_type::ContentType;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Render a single-track export's suggested filename stem (no extension) from a
/// template and the track's tag data. Supported tokens:
/// `{title} {artist} {album} {year} {track_number} {disc_number} {track_total}`.
/// Unknown `{...}` sequences are left literal. Absent values (e.g. no year)
/// substitute empty. The result is sanitized (path separators and characters
/// illegal on macOS/Windows → '-'), whitespace-collapsed, and trimmed of
/// leading/trailing spaces and dashes. If it renders empty, fall back to the
/// sanitized title, else "track".
pub fn render_export_filename(
    template: &str,
    resolved: &crate::library::manager::ResolvedExportTags,
) -> String {
    let tags = &resolved.tags;
    // Absent optional values (year, disc, track number) render as an empty token
    // — a legitimate domain state for a filename template, not an error; the
    // sanitize step below collapses any separator gap they leave.
    let substitute = |token: &str| -> Option<String> {
        Some(match token {
            "title" => tags.title.clone(),
            "artist" => tags.artist.clone(),
            "album" => tags.album.clone(),
            "year" => tags.year.map(|y| y.to_string()).unwrap_or_default(),
            // Zero-padded to two digits so tracks sort lexically; empty when the
            // track carries no number.
            "track_number" => resolved
                .track_number
                .map(|n| format!("{n:02}"))
                .unwrap_or_default(),
            "disc_number" => tags.disc.map(|d| d.to_string()).unwrap_or_default(),
            "track_total" => resolved.total_tracks.to_string(),
            _ => return None,
        })
    };

    // Single left-to-right scan: substituted values are appended to `rendered`
    // and never re-scanned, so a tag value that itself contains `{title}` is
    // emitted literally rather than substituted again.
    let mut rendered = String::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '{' {
            rendered.push(ch);
            continue;
        }
        let mut token = String::new();
        let mut closed = false;
        while let Some(&c) = chars.peek() {
            chars.next();
            if c == '}' {
                closed = true;
                break;
            }
            token.push(c);
        }
        match (closed, substitute(&token)) {
            (true, Some(value)) => rendered.push_str(&value),
            // Unknown token or an unterminated `{` — emit verbatim.
            (true, None) => {
                rendered.push('{');
                rendered.push_str(&token);
                rendered.push('}');
            }
            (false, _) => {
                rendered.push('{');
                rendered.push_str(&token);
            }
        }
    }

    let stem = sanitize_filename_stem(&rendered);
    if !stem.is_empty() {
        return stem;
    }
    // The template rendered to nothing usable (e.g. every referenced value was
    // empty). Log the skip rather than silently produce a name, then fall back
    // to the track title, and finally a fixed stem.
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

/// Sanitize a rendered filename stem: replace the macOS+Windows-illegal
/// characters and control characters with '-', collapse whitespace runs to one
/// space, trim leading/trailing spaces and dashes, then strip a leading '.' so
/// the file isn't hidden. Replacing the separators is also what makes a `../`
/// escape impossible.
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

/// Export service for exporting individual tracks.
pub struct ExportService;

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

impl ExportService {
    pub async fn export_original_track(
        mut plan: ExportTrackPlan,
        output_path: &Path,
    ) -> Result<(), String> {
        let format = &plan.audio_meta.audio_format;
        let is_whole_source = format.start_sample == 0
            && format.end_sample.is_none()
            && format.start_byte.is_none()
            && format.end_byte.is_none();
        let output_path_owned = output_path.to_path_buf();
        if is_whole_source {
            let audio_bytes = std::mem::take(&mut plan.audio_bytes);
            tokio::task::spawn_blocking(move || -> Result<(), String> {
                let mut output_guard = OutputPathsGuard::new(vec![output_path_owned.clone()]);
                std::fs::write(&output_path_owned, &audio_bytes)
                    .map_err(|e| format!("Failed to write original track file: {e}"))?;
                output_guard.commit();
                Ok(())
            })
            .await
            .map_err(|e| format!("original export task join error: {e}"))??;
            return Ok(());
        }

        if !is_lossless_original_source(&format.content_type) {
            return Err(format!(
                "Original export cannot split {} without transcoding",
                format.content_type.display_name()
            ));
        }

        Self::export_track_with_codec(
            plan,
            output_path,
            crate::config::ExportPresetCodec::Flac {
                bit_depth: crate::config::ExportBitDepth::Source,
            },
        )
        .await
    }

    /// Export a single track to the given format.
    ///
    /// For one-file-per-track: decodes and re-encodes to the target format.
    /// For CUE/FLAC: extracts, decodes, and re-encodes as a standalone file.
    /// Embeds metadata (title, artist, album, year, track number, cover art).
    ///
    /// On future-drop the cancel flag flips, the encoder loop exits between
    /// frames, and any partially-written output file is removed.
    pub async fn export_track(
        plan: ExportTrackPlan,
        output_path: &Path,
        preset: crate::config::ExportPreset,
    ) -> Result<(), String> {
        Self::export_track_with_codec(plan, output_path, preset.codec).await
    }

    pub async fn export_release_image_with_cue(
        mut plans: Vec<ExportTrackPlan>,
        output_audio_path: &Path,
        output_cue_path: &Path,
        catalog: Option<String>,
        preset: crate::config::ExportPreset,
    ) -> Result<(), String> {
        if plans.is_empty() {
            return Err("release image export requires at least one track".to_string());
        }
        if plans.len() > 99 {
            return Err("CUE sheets support at most 99 audio tracks".to_string());
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let _cancel_guard = CancelOnDrop(Arc::clone(&cancel));
        let mut combined_samples = Vec::new();
        let mut cue_tracks = Vec::with_capacity(plans.len());
        let mut sample_rate = None;
        let mut channels = None;
        let source_bits_per_sample = plans[0].audio_meta.audio_format.bits_per_sample;
        let release_title = plans[0].tags.album.clone();
        let release_performer = plans[0].tags.artist.clone();
        let cover_image_bytes = plans[0].cover_image_bytes.clone();
        let is_digital = plans[0].is_digital;
        let mut current_sample_frame = 0u64;

        for (index, plan) in plans.iter_mut().enumerate() {
            let pregap_sample_frames = plan
                .audio_window
                .leading_silence_samples
                .checked_add(non_negative_samples(
                    plan.audio_meta.audio_format.pregap_samples,
                )?)
                .ok_or_else(|| "CUE pregap sample count overflow".to_string())?;
            let decoded_pcm = load_track_audio(plan).await.map_err(|e| e.to_string())?;
            match (sample_rate, channels) {
                (None, None) => {
                    sample_rate = Some(decoded_pcm.sample_rate());
                    channels = Some(decoded_pcm.channels());
                }
                (Some(rate), Some(channel_count))
                    if rate == decoded_pcm.sample_rate()
                        && channel_count == decoded_pcm.channels() => {}
                _ => {
                    return Err(format!(
                        "release image export requires matching PCM shape; track {} is {}Hz/{}ch",
                        plan.audio_meta.track.id,
                        decoded_pcm.sample_rate(),
                        decoded_pcm.channels()
                    ));
                }
            }

            let channel_count = usize::try_from(decoded_pcm.channels())
                .map_err(|_| "channel count exceeds addressable memory".to_string())?;
            if channel_count == 0 {
                return Err("decoded audio has zero channels".to_string());
            }
            if decoded_pcm.raw_samples().len() % channel_count != 0 {
                return Err(format!(
                    "decoded sample count is not divisible by channel count for track {}",
                    plan.audio_meta.track.id
                ));
            }
            let segment_sample_frames =
                u64::try_from(decoded_pcm.raw_samples().len() / channel_count)
                    .map_err(|_| "decoded sample count exceeds CUE addressable range")?;
            if pregap_sample_frames > segment_sample_frames {
                return Err(format!(
                    "track {} pregap exceeds decoded segment length",
                    plan.audio_meta.track.id
                ));
            }

            cue_tracks.push(CueTrack {
                number: u8::try_from(index + 1)
                    .map_err(|_| "CUE track number exceeds 99".to_string())?,
                title: plan.tags.title.clone(),
                performer: plan.tags.artist.clone(),
                index_00_sample_frame: (pregap_sample_frames > 0).then_some(current_sample_frame),
                index_01_sample_frame: current_sample_frame
                    .checked_add(pregap_sample_frames)
                    .ok_or_else(|| "CUE index sample count overflow".to_string())?,
            });
            current_sample_frame = current_sample_frame
                .checked_add(segment_sample_frames)
                .ok_or_else(|| "release image sample count overflow".to_string())?;
            combined_samples.extend_from_slice(decoded_pcm.raw_samples());
        }

        let sample_rate =
            sample_rate.ok_or_else(|| "release image export requires audio".to_string())?;
        if sample_rate == 0 {
            return Err("release image export requires a non-zero sample rate".to_string());
        }
        let channels = channels.ok_or_else(|| "release image export requires audio".to_string())?;
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
        let cue = render_cue_sheet(
            &release_title,
            &release_performer,
            catalog
                .as_deref()
                .map(str::trim)
                .filter(|catalog| !catalog.is_empty()),
            plans[0].tags.year,
            &audio_filename,
            cue_file_type(&preset.codec)?,
            sample_rate,
            &cue_tracks,
        );

        let decoded_pcm = DecodedPcm::new(combined_samples, sample_rate, channels);
        let output_audio_path_owned = output_audio_path.to_path_buf();
        let output_cue_path_owned = output_cue_path.to_path_buf();
        let cancel_for_blocking = Arc::clone(&cancel);
        let codec = preset.codec;
        let tags = crate::library::manager::ExportTags {
            title: release_title.clone(),
            artist: release_performer,
            album: release_title,
            year: plans[0].tags.year,
            disc: None,
        };
        let total_tracks = plans.len() as u32;

        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let mut output_guard = OutputPathsGuard::new(vec![
                output_audio_path_owned.clone(),
                output_cue_path_owned.clone(),
            ]);
            let (encoded_data, tag_type) = encode_pcm_with_codec(
                &decoded_pcm,
                &codec,
                source_bits_per_sample,
                &cancel_for_blocking,
            )?;

            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            std::fs::write(&output_audio_path_owned, &encoded_data)
                .map_err(|e| format!("Failed to write release image: {e}"))?;

            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            std::fs::write(&output_cue_path_owned, cue)
                .map_err(|e| format!("Failed to write CUE sheet: {e}"))?;

            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            write_tags(
                &output_audio_path_owned,
                tag_type,
                &tags,
                None,
                total_tracks,
                is_digital,
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

        Ok(())
    }

    async fn export_track_with_codec(
        mut plan: ExportTrackPlan,
        output_path: &Path,
        codec: crate::config::ExportPresetCodec,
    ) -> Result<(), String> {
        let track_id = plan.audio_meta.track.id.clone();
        info!("Exporting track {} to {}", track_id, output_path.display());

        let cancel = Arc::new(AtomicBool::new(false));
        let _cancel_guard = CancelOnDrop(Arc::clone(&cancel));

        let decoded_pcm = load_track_audio(&mut plan)
            .await
            .map_err(|e| e.to_string())?;

        let output_path_owned = output_path.to_path_buf();
        let cancel_for_blocking = Arc::clone(&cancel);
        let encoded_len = tokio::task::spawn_blocking(move || -> Result<usize, String> {
            let mut output_guard = OutputPathsGuard::new(vec![output_path_owned.clone()]);

            let (encoded_data, tag_type) = encode_pcm_with_codec(
                &decoded_pcm,
                &codec,
                plan.audio_meta.audio_format.bits_per_sample,
                &cancel_for_blocking,
            )?;

            // Cancel check at every blocking boundary. Each fs::write /
            // write_tags call can run for a while on slow disks; checking
            // around them keeps the cancel-to-cleanup window tight. On
            // bail-out OutputFileGuard removes whatever was written.
            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            std::fs::write(&output_path_owned, &encoded_data)
                .map_err(|e| format!("Failed to write track file: {e}"))?;

            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            let cover_data = plan.cover_image_bytes.as_deref();

            write_tags(
                &output_path_owned,
                tag_type,
                &plan.tags,
                plan.track_number.map(|n| n as u32),
                plan.total_tracks as u32,
                plan.is_digital,
                cover_data,
            )?;

            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            output_guard.commit();
            Ok(encoded_data.len())
        })
        .await
        .map_err(|e| format!("encode task join error: {e}"))??;

        info!(
            "Successfully exported track {} ({} bytes)",
            track_id, encoded_len
        );
        Ok(())
    }
}

fn encode_pcm_with_codec(
    decoded_pcm: &DecodedPcm,
    codec: &crate::config::ExportPresetCodec,
    source_bits_per_sample: Option<i64>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(Vec<u8>, lofty::tag::TagType), String> {
    let encoded_data = match codec {
        crate::config::ExportPresetCodec::Flac { bit_depth } => crate::audio_codec::encode_to_flac(
            decoded_pcm.raw_samples(),
            decoded_pcm.sample_rate(),
            decoded_pcm.channels(),
            bit_depth.resolve(source_bits_per_sample),
            cancel,
        )
        .map_err(|e| format!("Failed to encode FLAC: {e}"))?,
        crate::config::ExportPresetCodec::Mp3 { bitrate_kbps } => {
            crate::audio_codec::encode_to_mp3_with_bitrate(
                decoded_pcm.raw_samples(),
                decoded_pcm.sample_rate(),
                decoded_pcm.channels(),
                *bitrate_kbps,
                cancel,
            )
            .map_err(|e| format!("Failed to encode MP3: {e}"))?
        }
        crate::config::ExportPresetCodec::OpusOgg { bitrate_kbps } => {
            crate::audio_codec::encode_to_opus_ogg(
                decoded_pcm.raw_samples(),
                decoded_pcm.sample_rate(),
                decoded_pcm.channels(),
                *bitrate_kbps,
                cancel,
            )
            .map_err(|e| format!("Failed to encode Opus/Ogg: {e}"))?
        }
        crate::config::ExportPresetCodec::Wav { bit_depth } => crate::audio_codec::encode_to_wav(
            decoded_pcm.raw_samples(),
            decoded_pcm.sample_rate(),
            decoded_pcm.channels(),
            bit_depth.resolve(source_bits_per_sample),
            cancel,
        )
        .map_err(|e| format!("Failed to encode WAV: {e}"))?,
        crate::config::ExportPresetCodec::Aiff { bit_depth } => crate::audio_codec::encode_to_aiff(
            decoded_pcm.raw_samples(),
            decoded_pcm.sample_rate(),
            decoded_pcm.channels(),
            bit_depth.resolve(source_bits_per_sample),
            cancel,
        )
        .map_err(|e| format!("Failed to encode AIFF: {e}"))?,
    };

    let tag_type = match codec {
        crate::config::ExportPresetCodec::Flac { .. } => lofty::tag::TagType::VorbisComments,
        crate::config::ExportPresetCodec::Mp3 { .. } => lofty::tag::TagType::Id3v2,
        crate::config::ExportPresetCodec::OpusOgg { .. } => lofty::tag::TagType::VorbisComments,
        crate::config::ExportPresetCodec::Wav { .. } => lofty::tag::TagType::RiffInfo,
        crate::config::ExportPresetCodec::Aiff { .. } => lofty::tag::TagType::AiffText,
    };

    Ok((encoded_data, tag_type))
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

fn cue_file_type(codec: &crate::config::ExportPresetCodec) -> Result<&'static str, String> {
    match codec {
        crate::config::ExportPresetCodec::Mp3 { .. } => Ok("MP3"),
        crate::config::ExportPresetCodec::Aiff { .. } => Ok("AIFF"),
        crate::config::ExportPresetCodec::Flac { .. }
        | crate::config::ExportPresetCodec::Wav { .. } => Ok("WAVE"),
        crate::config::ExportPresetCodec::OpusOgg { .. } => Err(
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

fn is_lossless_original_source(content_type: &ContentType) -> bool {
    matches!(
        content_type,
        ContentType::Flac
            | ContentType::Ape
            | ContentType::Alac
            | ContentType::Pcm
            | ContentType::WavPack
            | ContentType::Dsd
    )
}

/// Write every known metadata tag that describes this exported file. Cover art
/// is written when the export plan found cover bytes; disc number is only
/// written for digital media because side-based media do not map to ID3 discs.
fn write_tags(
    path: &Path,
    tag_type: lofty::tag::TagType,
    tags: &crate::library::manager::ExportTags,
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

/// Decode a track's source audio (already read into the plan) to PCM.
///
/// Used by export to decode entire tracks into memory for re-encoding. Every
/// track decodes its whole backing file and trims to its sample window.
async fn load_track_audio(plan: &mut ExportTrackPlan) -> Result<Arc<DecodedPcm>, PlaybackError> {
    let track_id = plan.audio_meta.track.id.clone();

    let audio_data_owned = std::mem::take(&mut plan.audio_bytes);
    debug!(
        "Loading audio for track {} ({} bytes)",
        track_id,
        audio_data_owned.len()
    );

    // Export uses its own window so CUE pregaps can be excluded or appended to
    // the previous track without changing playback's raw track window.
    let window = plan.audio_window;
    let start_sample = (window.source_start_sample > 0).then_some(window.source_start_sample);
    let end_sample = window.source_end_sample;

    debug!(
        "Decoding {} bytes of audio data to PCM",
        audio_data_owned.len()
    );
    let mut decoded = tokio::task::spawn_blocking(move || {
        crate::audio_codec::decode_audio(&audio_data_owned, start_sample, end_sample)
    })
    .await
    .map_err(PlaybackError::task)?
    .map_err(PlaybackError::flac)?;

    info!(
        "Successfully decoded track {}: {} samples, {}Hz, {} channels",
        track_id,
        decoded.samples.len(),
        decoded.sample_rate,
        decoded.channels
    );

    if window.leading_silence_samples > 0 || window.trailing_silence_samples > 0 {
        let channels = decoded.channels as usize;
        let leading = usize::try_from(window.leading_silence_samples)
            .map_err(|_| PlaybackError::flac("leading silence exceeds addressable memory"))?
            .checked_mul(channels)
            .ok_or_else(|| PlaybackError::flac("leading silence sample count overflow"))?;
        let trailing = usize::try_from(window.trailing_silence_samples)
            .map_err(|_| PlaybackError::flac("trailing silence exceeds addressable memory"))?
            .checked_mul(channels)
            .ok_or_else(|| PlaybackError::flac("trailing silence sample count overflow"))?;
        let mut samples = Vec::with_capacity(leading + decoded.samples.len() + trailing);
        samples.resize(leading, 0);
        samples.extend_from_slice(&decoded.samples);
        samples.resize(samples.len() + trailing, 0);
        decoded.samples = samples;
    }

    Ok(Arc::new(DecodedPcm::new(
        decoded.samples,
        decoded.sample_rate,
        decoded.channels,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::manager::{ExportTags, ResolvedExportTags};

    fn resolved(
        title: &str,
        track_number: Option<i32>,
        total_tracks: usize,
        disc: Option<i32>,
        year: Option<i32>,
    ) -> ResolvedExportTags {
        ResolvedExportTags {
            tags: ExportTags {
                title: title.to_string(),
                artist: "Artist Name".to_string(),
                album: "Album Title".to_string(),
                year,
                disc,
            },
            track_number,
            total_tracks,
            is_digital: true,
            primary_release_id: None,
        }
    }

    #[test]
    fn default_template_all_present_pads_track_number() {
        let r = resolved("Track Title", Some(3), 10, None, Some(2001));
        assert_eq!(
            render_export_filename("{track_number} - {title}", &r),
            "03 - Track Title"
        );
    }

    #[test]
    fn absent_track_number_trims_leading_separator() {
        let r = resolved("Track Title", None, 10, None, Some(2001));
        assert_eq!(
            render_export_filename("{track_number} - {title}", &r),
            "Track Title"
        );
    }

    #[test]
    fn full_template_substitutes_every_token() {
        let r = resolved("Track Title", Some(3), 10, Some(2), Some(2001));
        assert_eq!(
            render_export_filename("{artist} - {album} - {track_number} - {title}", &r),
            "Artist Name - Album Title - 03 - Track Title"
        );
    }

    #[test]
    fn slash_and_colon_become_dashes() {
        let r = resolved("Some/Weird:Title", None, 1, None, None);
        assert_eq!(render_export_filename("{title}", &r), "Some-Weird-Title");
    }

    #[test]
    fn path_escape_leaves_no_separator() {
        let r = resolved("../secret", None, 1, None, None);
        let name = render_export_filename("{title}", &r);
        assert!(!name.contains('/'), "no forward slash in {name}");
        assert!(!name.contains('\\'), "no backslash in {name}");
    }

    #[test]
    fn unknown_token_is_left_literal() {
        let r = resolved("Track Title", None, 1, None, None);
        assert_eq!(render_export_filename("{foo}", &r), "{foo}");
    }

    #[test]
    fn tag_value_containing_a_token_is_not_re_substituted() {
        // The title itself is the string "{title}"; a single left-to-right scan
        // emits it verbatim rather than treating it as another token.
        let r = resolved("{title}", None, 1, None, None);
        assert_eq!(render_export_filename("{title}", &r), "{title}");
    }

    #[test]
    fn empty_render_falls_back_to_title() {
        let r = resolved("Fallback Title", None, 1, None, None);
        assert_eq!(
            render_export_filename("{track_number}", &r),
            "Fallback Title"
        );
    }

    #[test]
    fn release_image_cue_places_indexes_from_track_windows() {
        let cue = render_cue_sheet(
            "Album Title",
            "Artist Name",
            None,
            None,
            "Album.flac",
            "WAVE",
            44_100,
            &[
                CueTrack {
                    number: 1,
                    title: "Opening".to_string(),
                    performer: "Artist Name".to_string(),
                    index_00_sample_frame: Some(0),
                    index_01_sample_frame: 44_100 * 2,
                },
                CueTrack {
                    number: 2,
                    title: "Second".to_string(),
                    performer: "Artist Name".to_string(),
                    index_00_sample_frame: Some(44_100 * 10),
                    index_01_sample_frame: 44_100 * 12,
                },
            ],
        );

        assert!(cue.contains("FILE \"Album.flac\" WAVE"));
        assert!(cue.contains("  TRACK 01 AUDIO\n"));
        assert!(cue.contains("    INDEX 00 00:00:00\n"));
        assert!(cue.contains("    INDEX 01 00:02:00\n"));
        assert!(cue.contains("  TRACK 02 AUDIO\n"));
        assert!(cue.contains("    INDEX 00 00:10:00\n"));
        assert!(cue.contains("    INDEX 01 00:12:00\n"));
    }

    #[test]
    fn release_image_cue_writes_catalog_and_date() {
        let cue = render_cue_sheet(
            "Album Title",
            "Artist Name",
            Some("0123456789012"),
            Some(2024),
            "Album.flac",
            "WAVE",
            44_100,
            &[CueTrack {
                number: 1,
                title: "Opening".to_string(),
                performer: "Artist Name".to_string(),
                index_00_sample_frame: None,
                index_01_sample_frame: 0,
            }],
        );

        assert!(cue.contains("CATALOG 0123456789012\n"));
        assert!(cue.contains("REM DATE 2024\n"));
    }

    /// Exercises the real `write_tags` against an encoded FLAC: every known tag
    /// and the cover image are embedded.
    #[test]
    fn write_tags_writes_every_known_field() {
        use lofty::prelude::*;
        use lofty::tag::TagType;

        crate::audio_codec::init();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let samples: Vec<i32> = (0..4410)
            .map(|i| ((i as f64 * 0.02).sin() * 0.5 * i32::MAX as f64) as i32)
            .collect();
        let flac = crate::audio_codec::encode_to_flac(&samples, 44100, 1, 16, &cancel).unwrap();

        let cover_bytes = {
            let img = image::RgbImage::from_pixel(8, 8, image::Rgb([120, 40, 200]));
            let mut buf = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut buf, image::ImageFormat::Png)
                .unwrap();
            buf.into_inner()
        };

        let tags = ExportTags {
            title: "Track Title".to_string(),
            artist: "Artist Name".to_string(),
            album: "Album Title".to_string(),
            year: Some(2001),
            disc: Some(1),
        };

        let dir = tempfile::TempDir::new().unwrap();

        let path = dir.path().join("tagged.flac");
        std::fs::write(&path, &flac).unwrap();
        write_tags(
            &path,
            TagType::VorbisComments,
            &tags,
            Some(3),
            10,
            true,
            Some(&cover_bytes),
        )
        .unwrap();

        let tagged = lofty::read_from_path(&path).unwrap();
        let tag = tagged
            .tag(TagType::VorbisComments)
            .expect("VorbisComments tag present");
        assert_eq!(tag.title().as_deref(), Some("Track Title"));
        assert_eq!(tag.artist().as_deref(), Some("Artist Name"));
        assert_eq!(tag.album().as_deref(), Some("Album Title"));
        assert_eq!(tag.track(), Some(3));
        assert_eq!(tag.track_total(), Some(10));
        assert_eq!(tag.disk(), Some(1));
        assert!(!tag.pictures().is_empty(), "cover embedded");
    }
}

use crate::library::manager::ExportTrackPlan;
use crate::playback::{DecodedPcm, PlaybackError};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

/// Output format for track export.
pub enum ExportFormat {
    Flac,
    Mp3 { bitrate: u32 },
}

/// Standard bitrate for MP3 track export, in bits per second (320 kbit/s).
pub const MP3_EXPORT_BITRATE: u32 = 320_000;

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
fn sanitize_filename_stem(input: &str) -> String {
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

impl ExportService {
    /// Export a single track to the given format.
    ///
    /// For one-file-per-track: decodes and re-encodes to the target format.
    /// For CUE/FLAC: extracts, decodes, and re-encodes as a standalone file.
    /// Embeds metadata (title, artist, album, year, track number, cover art).
    ///
    /// On future-drop the cancel flag flips, the encoder loop exits between
    /// frames, and any partially-written output file is removed.
    pub async fn export_track(
        mut plan: ExportTrackPlan,
        output_path: &Path,
        format: ExportFormat,
    ) -> Result<(), String> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        struct CancelOnDrop(Arc<AtomicBool>);
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }

        struct OutputFileGuard {
            path: std::path::PathBuf,
            committed: bool,
        }
        impl Drop for OutputFileGuard {
            fn drop(&mut self) {
                if !self.committed {
                    let _ = std::fs::remove_file(&self.path);
                }
            }
        }

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
            let mut output_guard = OutputFileGuard {
                path: output_path_owned.clone(),
                committed: false,
            };

            let encoded_data = match format {
                ExportFormat::Flac => crate::audio_codec::encode_to_flac(
                    decoded_pcm.raw_samples(),
                    decoded_pcm.sample_rate(),
                    decoded_pcm.channels(),
                    plan.audio_meta
                        .audio_format
                        .bits_per_sample
                        .map(|bits| bits as u32)
                        .unwrap_or(32),
                    &cancel_for_blocking,
                )
                .map_err(|e| format!("Failed to encode FLAC: {e}"))?,
                ExportFormat::Mp3 { bitrate } => crate::audio_codec::encode_to_mp3(
                    decoded_pcm.raw_samples(),
                    decoded_pcm.sample_rate(),
                    decoded_pcm.channels(),
                    bitrate,
                    &cancel_for_blocking,
                )
                .map_err(|e| format!("Failed to encode MP3: {e}"))?,
            };

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

            let tag_type = match format {
                ExportFormat::Flac => lofty::tag::TagType::VorbisComments,
                ExportFormat::Mp3 { .. } => lofty::tag::TagType::Id3v2,
            };

            write_tags(
                &output_path_owned,
                tag_type,
                &plan.tags,
                plan.track_number.map(|n| n as u32),
                plan.total_tracks as u32,
                plan.is_digital,
                cover_data,
                &plan.metadata,
            )?;

            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err("export cancelled".to_string());
            }

            output_guard.committed = true;
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

/// Maximum dimension for embedded cover art.
const COVER_MAX_SIZE: u32 = 600;

/// Write metadata tags to an encoded audio file. Each tag is written only when
/// the user's `metadata` selection includes it; cover art is governed by
/// `cover_data` being `Some` (the plan omits the bytes when it's deselected),
/// so presence *is* the selection and there is no separate cover guard here.
fn write_tags(
    path: &Path,
    tag_type: lofty::tag::TagType,
    tags: &crate::library::manager::ExportTags,
    track_number: Option<u32>,
    total_tracks: u32,
    is_digital: bool,
    cover_data: Option<&[u8]>,
    metadata: &crate::config::ExportMetadata,
) -> Result<(), String> {
    use lofty::config::WriteOptions;
    use lofty::picture::{Picture, PictureType};
    use lofty::prelude::*;
    use lofty::tag::{items::Timestamp, Tag};

    let mut tagged_file = lofty::read_from_path(path)
        .map_err(|e| format!("Failed to read file for tagging: {}", e))?;

    let mut tag = Tag::new(tag_type);
    if metadata.title {
        tag.set_title(tags.title.clone());
    }
    if metadata.artist {
        tag.set_artist(tags.artist.clone());
    }
    if metadata.album {
        tag.set_album(tags.album.clone());
    }

    if metadata.year {
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
    }
    if metadata.track_number {
        if let Some(n) = track_number {
            tag.set_track(n);
        }
        tag.set_track_total(total_tracks);
    }

    // Skip the disc tag on vinyl / cassette: ID3 "disc number" doesn't
    // describe a physical side, and writing it mislabels side B as disc 2. The
    // digital gate stays an AND with the user's disc-number selection.
    if metadata.disc_number && is_digital {
        if let Some(disc) = tags.disc {
            tag.set_disk(disc as u32);
        }
    }

    if let Some(data) = cover_data {
        let resized = resize_cover(data)?;

        tag.push_picture(
            Picture::unchecked(resized)
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

/// Resize cover art to fit within COVER_MAX_SIZE, encoded as JPEG.
fn resize_cover(data: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(data)
        .map_err(|e| format!("Failed to decode cover image: {}", e))?;

    let (w, h) = (img.width(), img.height());

    let img = if w > COVER_MAX_SIZE || h > COVER_MAX_SIZE {
        debug!(
            "Resizing cover art from {}x{} to fit {}x{}",
            w, h, COVER_MAX_SIZE, COVER_MAX_SIZE
        );
        img.resize(
            COVER_MAX_SIZE,
            COVER_MAX_SIZE,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode cover as JPEG: {}", e))?;
    Ok(buf.into_inner())
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

    // Every track decodes its whole backing file; the sample window trims it to
    // just this track. A per-track source (start 0 / end None) decodes whole.
    // start_sample is a non-negative sample position; a negative one is corrupt
    // metadata, surfaced rather than silently decoded from the start. 0 means the
    // track begins at the file start, so no start trim is needed.
    let start_sample = u64::try_from(plan.audio_meta.audio_format.start_sample)
        .expect("audio_format.start_sample is a non-negative sample position");
    let start_sample = (start_sample > 0).then_some(start_sample);
    let end_sample = plan.audio_meta.audio_format.end_sample.map(|s| s as u64);

    debug!(
        "Decoding {} bytes of audio data to PCM",
        audio_data_owned.len()
    );
    let decoded = tokio::task::spawn_blocking(move || {
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

    Ok(Arc::new(DecodedPcm::new(
        decoded.samples,
        decoded.sample_rate,
        decoded.channels,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ExportMetadata;
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

    /// Exercises the real `write_tags` against an encoded FLAC: a selection that
    /// turns off artist and cover art must leave those absent, while everything
    /// on embeds them all.
    #[test]
    fn write_tags_honors_the_metadata_selection() {
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

        // Artist off suppresses the artist tag via the selection guard. Cover is
        // presence-driven in write_tags: passing None writes no picture — the
        // cover_art selection that withholds those bytes lives upstream in
        // get_export_track_plan, not here.
        let selection_off = ExportMetadata {
            title: true,
            artist: false,
            album: true,
            year: true,
            track_number: true,
            disc_number: true,
            cover_art: false,
        };
        let off_path = dir.path().join("off.flac");
        std::fs::write(&off_path, &flac).unwrap();
        write_tags(
            &off_path,
            TagType::VorbisComments,
            &tags,
            Some(3),
            10,
            true,
            None,
            &selection_off,
        )
        .unwrap();

        let tagged = lofty::read_from_path(&off_path).unwrap();
        let tag = tagged
            .tag(TagType::VorbisComments)
            .expect("VorbisComments tag present");
        assert_eq!(tag.title().as_deref(), Some("Track Title"));
        assert!(tag.artist().is_none(), "artist tag suppressed when off");
        assert!(tag.pictures().is_empty(), "no cover when off");

        // Everything on: title, artist, album, and an embedded cover.
        let selection_on = ExportMetadata {
            title: true,
            artist: true,
            album: true,
            year: true,
            track_number: true,
            disc_number: true,
            cover_art: true,
        };
        let on_path = dir.path().join("on.flac");
        std::fs::write(&on_path, &flac).unwrap();
        write_tags(
            &on_path,
            TagType::VorbisComments,
            &tags,
            Some(3),
            10,
            true,
            Some(&cover_bytes),
            &selection_on,
        )
        .unwrap();

        let tagged = lofty::read_from_path(&on_path).unwrap();
        let tag = tagged
            .tag(TagType::VorbisComments)
            .expect("VorbisComments tag present");
        assert_eq!(tag.title().as_deref(), Some("Track Title"));
        assert_eq!(tag.artist().as_deref(), Some("Artist Name"));
        assert_eq!(tag.album().as_deref(), Some("Album Title"));
        assert!(!tag.pictures().is_empty(), "cover embedded when on");
    }
}

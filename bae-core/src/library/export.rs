use crate::db::{DbAlbum, DbRelease};
use crate::library::manager::ExportTrackPlan;
use crate::library::LibraryManager;
use crate::playback::{DecodedPcm, PlaybackError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info};

/// Output format for track export.
pub enum ExportFormat {
    Flac,
    Mp3 { bitrate: u32 },
}

/// Standard bitrate for MP3 track export, in bits per second (320 kbit/s).
pub const MP3_EXPORT_BITRATE: u32 = 320_000;

/// Build a folder name from release metadata.
/// Format: "Artist - Title (Year) [Label CatNo]" with optional parts omitted when absent.
fn synthesize_folder_name(album: &DbAlbum, release: &DbRelease, artist_name: &str) -> String {
    let mut name = format!("{} - {}", artist_name, album.title);

    if let Some(year) = release.pressing.year.or(album.year) {
        name.push_str(&format!(" ({})", year));
    }

    match (&release.pressing.label, &release.pressing.catalog_number) {
        (Some(label), Some(cat)) => name.push_str(&format!(" [{} {}]", label, cat)),
        (Some(label), None) => name.push_str(&format!(" [{}]", label)),
        (None, Some(cat)) => name.push_str(&format!(" [{}]", cat)),
        (None, None) => {}
    }

    name
}

/// Export service for exporting files and tracks
pub struct ExportService;

impl ExportService {
    /// Resolve the output directory for a release export.
    /// Uses `source_folder_name` if stored, otherwise synthesizes from metadata.
    async fn resolve_release_dir(
        target_dir: &Path,
        release: &DbRelease,
        library_manager: &LibraryManager,
    ) -> Result<PathBuf, String> {
        let folder_name = if let Some(ref name) = release.source_folder_name {
            name.clone()
        } else {
            let album = library_manager
                .get_album_by_id(&release.album_id)
                .await
                .map_err(|e| format!("Failed to get album: {}", e))?
                .ok_or_else(|| "Album not found".to_string())?;

            let primary_artist = library_manager
                .get_artist_by_id(&album.artist_id)
                .await
                .map_err(|e| format!("Failed to get artist: {}", e))?
                .expect("album FK artist must exist");

            synthesize_folder_name(&album, release, &primary_artist.name)
        };

        Ok(target_dir.join(folder_name))
    }

    /// Export all files for a release to a directory
    ///
    /// Files are written into a subfolder of target_dir named after the
    /// source folder (or synthesized from metadata if not available).
    /// Each file is read from this device's local copy when one exists,
    /// otherwise downloaded from the cloud home and decrypted with the
    /// release's item key — cloud-only releases export without pinning.
    pub async fn export_release(
        release_id: &str,
        target_dir: &Path,
        library_manager: &LibraryManager,
    ) -> Result<(), String> {
        info!(
            "Exporting release {} to {}",
            release_id,
            target_dir.display()
        );

        let release = library_manager
            .get_release_by_id(release_id)
            .await
            .map_err(|e| format!("Failed to get release: {}", e))?
            .ok_or_else(|| "Release not found".to_string())?;

        let files = library_manager
            .get_files_for_release(release_id)
            .await
            .map_err(|e| format!("Failed to get files: {}", e))?;

        if files.is_empty() {
            return Err("No files found for release".to_string());
        }

        let output_dir = Self::resolve_release_dir(target_dir, &release, library_manager).await?;

        for file in &files {
            let file_data =
                crate::storage::local::transfer::read_release_file_bytes(file, library_manager)
                    .await
                    .map_err(|e| {
                        format!("Failed to read file {}: {}", file.original_filename, e)
                    })?;

            // Ensure subdirectories exist for nested filenames
            let file_path = output_dir.join(&file.original_filename);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            }

            std::fs::write(&file_path, &file_data)
                .map_err(|e| format!("Failed to write file {}: {}", file.original_filename, e))?;

            debug!(
                "Exported file {} ({} bytes)",
                file.original_filename,
                file_data.len()
            );
        }

        info!(
            "Successfully exported {} files to {}",
            files.len(),
            output_dir.display()
        );
        Ok(())
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

        let decoded_pcm = load_track_audio(&plan).await.map_err(|e| e.to_string())?;

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
                    decoded_pcm.bits_per_sample(),
                    &cancel_for_blocking,
                )
                .map_err(|e| format!("Failed to encode FLAC: {e}"))?,
                ExportFormat::Mp3 { bitrate } => crate::audio_codec::encode_to_mp3(
                    decoded_pcm.raw_samples(),
                    decoded_pcm.sample_rate(),
                    decoded_pcm.channels(),
                    decoded_pcm.bits_per_sample(),
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

            let cover_data =
                plan.cover_image_path
                    .as_ref()
                    .and_then(|path| match std::fs::read(path) {
                        Ok(data) => Some(data),
                        Err(e) => {
                            debug!("Could not read cover art at {}: {}", path.display(), e);
                            None
                        }
                    });

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
                cover_data.as_deref(),
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

/// Write metadata tags to an encoded audio file.
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
    }
    tag.set_track_total(total_tracks);

    // Skip the disc tag on vinyl / cassette: ID3 "disc number" doesn't
    // describe a physical side, and writing it mislabels side B as disc 2.
    if is_digital {
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
async fn load_track_audio(plan: &ExportTrackPlan) -> Result<Arc<DecodedPcm>, PlaybackError> {
    let track_id = plan.audio_meta.track.id.as_str();
    let audio_format = &plan.audio_meta.audio_format;

    let file_data = &plan.audio_bytes;
    debug!(
        "Loading audio for track {} ({} bytes)",
        track_id,
        file_data.len()
    );

    // Every track decodes its whole backing file; the sample window trims it to
    // just this track. A per-track source (start 0 / end None) decodes whole.
    let audio_data_owned: Vec<u8> = file_data.clone();
    // start_sample is a non-negative sample position; a negative one is corrupt
    // metadata, surfaced rather than silently decoded from the start. 0 means the
    // track begins at the file start, so no start trim is needed.
    let start_sample = u64::try_from(audio_format.start_sample)
        .expect("audio_format.start_sample is a non-negative sample position");
    let start_sample = (start_sample > 0).then_some(start_sample);
    let end_sample = audio_format.end_sample.map(|s| s as u64);

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
        decoded.bits_per_sample,
    )))
}

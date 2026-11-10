use crate::SaveTrackPlan;
use bae_audio::sparse_buffer::SharedSparseBuffer;
use bae_audio::{EncodeFormat, StreamingEncoder};
use bae_track_audio::track_sources::{run_tracks_over_sources, SourceStream};
use bae_util::task_failure::{Bug, TaskFailed};
use coven::task::{blocking_completing, BugSinkRef};
use std::collections::HashMap;
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, warn};

/// Render a saved track's filename stem (no extension) from the ordered token
/// list and the track's tag data. Each token substitutes its value; an absent
/// value (no year, say) drops out, and the non-empty values join with single
/// spaces. The result goes through `sanitize_filename_stem`; if that leaves it
/// empty, fall back to the sanitized title, then to "track". The stem is text:
/// a file takes it through `save_file_name`.
pub fn render_save_filename(
    tokens: &[bae_preferences::SaveFilenameToken],
    resolved: &crate::manager::ResolvedSaveTags,
) -> String {
    use bae_preferences::SaveFilenameToken;

    let tags = &resolved.tags;
    let rendered = tokens
        .iter()
        .map(|token| match token {
            SaveFilenameToken::Title => tags.title.clone(),
            SaveFilenameToken::Artist => tags.artist.clone(),
            SaveFilenameToken::Album => tags.album.clone(),
            // An absent year or disc renders empty and drops out of the join.
            // That is a legitimate state for a filename pattern, not an error.
            SaveFilenameToken::Year => tags.year.map(|y| y.to_string()).unwrap_or_default(),
            // Zero-padded to two digits so tracks sort lexically.
            SaveFilenameToken::TrackNumber => format!("{:02}", resolved.track_number),
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

/// A release's data a folder name pattern draws on, from the database alone.
pub struct SaveFolderTags {
    /// The album's line (ALBUMARTIST).
    pub artist: String,
    pub album: String,
    pub year: Option<i32>,
    /// The first label the pressing states a name for.
    pub label: Option<String>,
    /// The first catalog number the pressing states.
    pub catalog_number: Option<String>,
}

/// Render the folder name (one name, no extension) a release save writes into
/// from the ordered token list and the release's data: each token's value, the
/// non-empty values joined with single spaces, through `sanitize_filename_stem`.
/// A pattern that renders nothing usable falls back to the album, then to
/// "release". The name is text: the folder takes it through `coven`'s rule for
/// a stored name.
pub fn render_save_folder_name(
    tokens: &[bae_preferences::SaveFolderToken],
    tags: &SaveFolderTags,
) -> String {
    use bae_preferences::SaveFolderToken;

    let rendered = tokens
        .iter()
        .map(|token| match token {
            SaveFolderToken::Artist => tags.artist.clone(),
            SaveFolderToken::Album => tags.album.clone(),
            SaveFolderToken::Year => tags.year.map(|year| year.to_string()).unwrap_or_default(),
            SaveFolderToken::Label => tags.label.clone().unwrap_or_default(),
            SaveFolderToken::CatalogNumber => tags.catalog_number.clone().unwrap_or_default(),
        })
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let name = sanitize_filename_stem(&rendered);
    if !name.is_empty() {
        return name;
    }
    let album = sanitize_filename_stem(&tags.album);
    if !album.is_empty() {
        debug!(album = %tags.album, "folder name pattern rendered empty; using the album");
        return album;
    }
    debug!("folder name pattern and album both empty; using \"release\"");
    "release".to_string()
}

/// Tidy a rendered filename stem: whitespace runs collapse to one space,
/// leading and trailing spaces are trimmed, and a leading '.' is stripped so
/// the file isn't hidden. What a name may hold on a volume is coven's rule,
/// which [`save_file_name`] applies.
pub(crate) fn sanitize_filename_stem(input: &str) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_start_matches('.')
        .trim_start()
        .to_string()
}

/// The name a save writes for a file of `stem` and `extension`: coven's name
/// for it ([`coven::RelativePath::local_path`]), as coven names its own local
/// copies, so it is one valid name on every platform. `Some/Weird:Title` is
/// written as `Some／Weird：Title.flac`.
pub(crate) fn save_file_name(stem: &str, extension: &str) -> PathBuf {
    coven::RelativePath::new([format!("{stem}.{extension}")])
        .expect("a name with an extension is not empty")
        .local_path()
}

pub(crate) struct SaveService;

/// Why a save wrote nothing.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SaveError {
    /// A source, the encoder or the disk refused the save.
    #[error("{0}")]
    Failed(String),
    /// Work in the save gave no result: it panicked, on a blocking thread or
    /// in a decode's read of a source, which the caller reports as a bug; or
    /// its runtime is shutting down.
    #[error(transparent)]
    Unfinished(#[from] TaskFailed),
}

/// A save's work that panicked carries its bug: one its caller reports, or,
/// once no caller waits, the bug sink the work ran under.
impl coven::task::CarriesBug for SaveError {
    fn bug(&self) -> Option<&Bug> {
        match self {
            Self::Failed(_) => None,
            Self::Unfinished(failed) => coven::task::CarriesBug::bug(failed),
        }
    }
}

impl From<String> for SaveError {
    fn from(detail: String) -> Self {
        Self::Failed(detail)
    }
}

impl From<bae_audio::DecodeError> for SaveError {
    fn from(error: bae_audio::DecodeError) -> Self {
        match error {
            bae_audio::DecodeError::ReadPanicked(bug) => {
                Self::Unfinished(TaskFailed::Panicked(bug))
            }
            error => Self::Failed(error.to_string()),
        }
    }
}

/// What saving a track does with a file already at its output path.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ExistingOutput {
    /// Replace it: the person picked the path and confirmed replacing what is
    /// there.
    Replace,
    /// Fail the save: the path is in a release save's own folder, so a file
    /// there is another of its tracks, whose name coven writes as this one's
    /// or the drive holds as one.
    Refuse,
}

struct CancelSaveOnDrop(Arc<AtomicBool>);

impl Drop for CancelSaveOnDrop {
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
        Self { paths, committed: false }
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

impl SaveService {
    /// Save each track to its own output path, up to `parallelism` at once:
    /// decodes the track's audio (cut out of its image when it is stored in
    /// one) and re-encodes it to the target format.
    /// Embeds metadata (title, artist, album, year, track number, cover art).
    /// A source file is open only while a track reading it is being saved
    /// (see [`run_tracks_over_sources`]); `open` opens one by release file id.
    /// `on_saved` runs as each track's file is written. `existing` says what a
    /// file already at a track's output path does. A track whose save panics
    /// fails the save as unfinished, which the caller reports; once the
    /// caller stops waiting, an encode that panics goes to `bugs`.
    ///
    /// On future-drop the cancel flag flips, the encoder loop exits between
    /// frames, and any partially-written output file is removed.
    pub(crate) async fn save_tracks(
        tracks: Vec<(SaveTrackPlan, PathBuf)>,
        cover_image_bytes: Option<Vec<u8>>,
        codec: bae_preferences::SaveCodec,
        parallelism: NonZeroUsize,
        existing: ExistingOutput,
        open: impl Fn(&String) -> SourceStream,
        on_saved: Arc<dyn Fn() + Send + Sync>,
        bugs: BugSinkRef,
    ) -> Result<(), SaveError> {
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
                let bugs = bugs.clone();
                async move {
                    Self::save_track_with_codec(
                        plan,
                        streams,
                        cover_image_bytes,
                        output_path,
                        codec,
                        existing,
                        bugs,
                    )
                    .await?;
                    on_saved();
                    Ok(())
                }
            },
            SaveError::from,
        )
        .await
        .map(|_| ())
    }

    async fn save_track_with_codec(
        plan: SaveTrackPlan,
        streams: HashMap<String, SharedSparseBuffer>,
        cover_image_bytes: Arc<Option<Vec<u8>>>,
        output_path: PathBuf,
        codec: bae_preferences::SaveCodec,
        existing: ExistingOutput,
        bugs: BugSinkRef,
    ) -> Result<(), SaveError> {
        let track_id = plan.audio_meta.track.id.clone();
        debug!("Exporting track {} to {}", track_id, output_path.display());

        let cancel = Arc::new(AtomicBool::new(false));
        let _cancel_guard = CancelSaveOnDrop(Arc::clone(&cancel));

        let tag_type = codec_tag_type(&codec);
        let sample_rate = plan.audio_meta.audio_format.sample_rate.get();
        let channels = plan.audio_meta.audio_format.channels.get();
        let cancel_for_audio = Arc::clone(&cancel);
        // How the audio gets into the file: copied from the source file as
        // stored, or decoded and encoded to the codec.
        let write_audio: Box<dyn FnOnce(std::fs::File) -> Result<(), SaveError> + Send> = match plan
            .copied_file(&codec)?
        {
            Some(file_id) => {
                debug!("Track {track_id} is already in the saved codec: copying file {file_id}");
                let source = streams
                    .get(file_id)
                    .ok_or_else(|| format!("no audio stream opened for file {file_id}"))?
                    .clone();
                Box::new(move |file| copy_file_audio(&source, file, &cancel_for_audio))
            }
            None => {
                let format = encode_format(&codec, plan.audio_meta.audio_format.bits_per_sample);
                let decode = plan.decode(&streams).map_err(|e| e.to_string())?;
                Box::new(move |file| {
                    let mut encoder = StreamingEncoder::seekable(
                        format,
                        Box::new(file),
                        cancel_for_audio.clone(),
                    );
                    decode.run_to_sink(
                        sample_rate,
                        channels,
                        &mut encoder,
                        cancel_for_audio.clone(),
                    )?;
                    // `finish` closes the muxer and drops the file handle,
                    // so the tag writer reads a fully-flushed file.
                    encoder.finish()?;
                    Ok(())
                })
            }
        };
        drop(streams);

        let cancel_for_blocking = Arc::clone(&cancel);
        let subject = format!("track {track_id}");
        blocking_completing("encode a saved track", bugs, move || -> Result<(), SaveError> {
            let file = match existing {
                ExistingOutput::Replace => std::fs::File::create(&output_path)
                    .map_err(|e| format!("Failed to create track file: {e}"))?,
                ExistingOutput::Refuse => {
                    use crate::storage::output_file::{create_new_file, CreateFileError};
                    let name = Path::new(
                        output_path.file_name().expect("a track's output path names its file"),
                    );
                    create_new_file(&output_path, name, &subject).map_err(|error| {
                        SaveError::Failed(match error {
                            CreateFileError::Clash { .. } => error.to_string(),
                            CreateFileError::Io(error) => {
                                format!("Failed to create track file: {error}")
                            }
                        })
                    })?
                }
            };
            // Made once the file is this save's: a refused path holds another
            // track's file, which this save must not remove.
            let mut output_guard = OutputPathsGuard::new(vec![output_path.clone()]);
            write_audio(file)?;

            // Cancel check at every blocking boundary. write_tags can run for a
            // while on slow disks; checking around it keeps the
            // cancel-to-cleanup window tight. On bail-out OutputPathsGuard
            // removes whatever was written.
            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err(SaveError::Failed("export cancelled".to_string()));
            }

            write_tags(
                &output_path,
                tag_type,
                &plan.resolved.tags,
                Some(plan.resolved.track_number),
                plan.resolved.total_tracks as u32,
                cover_image_bytes.as_deref(),
            )?;

            if cancel_for_blocking.load(Ordering::Relaxed) {
                return Err(SaveError::Failed("export cancelled".to_string()));
            }

            output_guard.commit();
            Ok(())
        })
        .await??;

        debug!("Successfully exported track {}", track_id);
        Ok(())
    }
}

/// Write every byte of the source file behind `source` to `file`, then close
/// it: a save of a track that is already in the codec it writes.
fn copy_file_audio(
    source: &SharedSparseBuffer,
    mut file: std::fs::File,
    cancel: &AtomicBool,
) -> Result<(), SaveError> {
    use bae_audio::sparse_buffer::BufferStop;
    use std::io::Write;

    let mut reader = source.new_reader();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(SaveError::Failed("export cancelled".to_string()));
        }
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => file
                .write_all(&chunk[..read])
                .map_err(|e| format!("Failed to write track file: {e}"))?,
            Err(BufferStop::Cancelled) => {
                return Err(SaveError::Failed("export cancelled".to_string()))
            }
            Err(BufferStop::Failed(error)) => return Err(SaveError::Failed(error.to_string())),
        }
    }
    file.flush().map_err(|e| format!("Failed to write track file: {e}"))?;
    Ok(())
}

/// The encoder format for a save codec, for a track stored at `source_depth`
/// (see [`bae_preferences::SaveBitDepth::bits`]).
fn encode_format(
    codec: &bae_preferences::SaveCodec,
    source_depth: Option<NonZeroU32>,
) -> EncodeFormat {
    use bae_preferences::SaveCodec;
    match codec {
        SaveCodec::Flac { bit_depth } => {
            EncodeFormat::Flac { bits_per_sample: bit_depth.bits(source_depth) }
        }
        SaveCodec::Mp3 { bitrate_kbps } => EncodeFormat::Mp3 { bitrate_kbps: *bitrate_kbps },
        SaveCodec::Aac { bitrate_kbps } => EncodeFormat::Aac { bitrate_kbps: *bitrate_kbps },
        SaveCodec::OpusOgg { bitrate_kbps } => {
            EncodeFormat::OpusOgg { bitrate_kbps: *bitrate_kbps }
        }
        SaveCodec::Wav { bit_depth } => {
            EncodeFormat::PcmWav { bits_per_sample: bit_depth.bits(source_depth) }
        }
        SaveCodec::Aiff { bit_depth } => {
            EncodeFormat::PcmAiff { bits_per_sample: bit_depth.bits(source_depth) }
        }
    }
}

/// The tag container lofty writes for a save codec's output file.
fn codec_tag_type(codec: &bae_preferences::SaveCodec) -> lofty::tag::TagType {
    use bae_preferences::SaveCodec;
    match codec {
        SaveCodec::Flac { .. } => lofty::tag::TagType::VorbisComments,
        SaveCodec::Mp3 { .. } => lofty::tag::TagType::Id3v2,
        SaveCodec::Aac { .. } => lofty::tag::TagType::Mp4Ilst,
        SaveCodec::OpusOgg { .. } => lofty::tag::TagType::VorbisComments,
        SaveCodec::Wav { .. } => lofty::tag::TagType::RiffInfo,
        SaveCodec::Aiff { .. } => lofty::tag::TagType::AiffText,
    }
}

/// Write every known metadata tag that describes this exported file. Cover art
/// is written when the export plan found cover bytes; disc number is only
/// written for digital media because side-based media do not map to ID3 discs.
fn write_tags(
    path: &Path,
    tag_type: lofty::tag::TagType,
    tags: &crate::manager::SaveTags,
    track_number: Option<u32>,
    total_tracks: u32,
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
    tag.insert_text(ItemKey::AlbumArtist, tags.album_artist.clone());
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

    if let Some(disc) = tags.disc {
        tag.set_disk(disc);
    }

    if let Some(data) = cover_data {
        // The stored cover is already a ≤600 JPEG (resized at store time), so
        // embed its bytes directly — no second resize/JPEG pass here.
        tag.push_picture(
            Picture::unchecked(data.to_vec()).pic_type(PictureType::CoverFront).build(),
        );
    }

    // A copied file brings the tags it was stored with: the saved file carries
    // only these.
    tagged_file.clear();
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

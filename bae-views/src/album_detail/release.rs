//! Resolved release types (`ReleaseSummary`, `ReleaseDetail`) and the pure
//! projections that produce them from raw DB aggregates.

use super::*;
use crate::DbReleaseSummary;
use bae_records::DbReleaseDetail;

/// The per-release inputs the library's views read beside the release's rows and
/// threads into the release projections: whether the library syncs at all
/// (which gates `storage_actions`), and the release's own cover reference.
/// Bundled so the constructors take one context rather than a pile of parameters.
#[derive(Debug, Clone)]
pub(crate) struct ReleaseResolveCtx {
    pub(crate) syncs: bool,
    pub(crate) cover: Option<ImageRef>,
    /// Whether the owning album is a compilation. Decides `TrackDetail::display_artist`
    /// — the one album-level fact the per-track artist decision needs.
    pub(crate) is_compilation: bool,
}

/// The slim projection a list view (Storage Manager, release pickers) renders one
/// row from. Every field is pre-computed; nothing downstream derives anything.
/// Composed into [`ReleaseDetail`] for detail views.
///
/// Invariant: `album_id` names an album that exists — enforced by the
/// `releases.album_id` FK and by `delete_release`, which removes the album when its
/// last release goes.
#[derive(Debug, Clone)]
pub struct ReleaseSummary {
    pub id: String,
    pub album_id: String,
    /// What the release is made of, each carrier with its count; empty if
    /// unknown.
    pub media: Vec<bae_types::pressing::MediaCount>,
    /// Where its files are, and whether this device keeps them offline.
    pub storage_state: ReleaseStorageState,
    /// The transitions available right now, derived by the core from
    /// `storage_state`, whether this device holds the originals an upload reads,
    /// and whether the library syncs; empty without it, and while
    /// the release is `transitioning`. The UI renders these and never
    /// re-derives availability.
    pub storage_actions: Vec<ReleaseStorageAction>,
    pub transfer_action: Option<ReleaseStorageAction>,
    /// What of the release waits in this device's upload queue; `None` when
    /// nothing does.
    pub upload: Option<ReleaseUpload>,
    /// Whether a transition is under way: files waiting to upload, or a
    /// transfer running. It offers only cancelling; another storage action
    /// would race it.
    pub transitioning: bool,
    pub file_count: i64,
    pub total_size: i64,
    /// This release's *own* cover, or `None` when it has no cover row. Keyed on the
    /// release id, so two releases of one album each resolve to their own art rather
    /// than to the album's primary cover.
    pub cover: Option<ImageRef>,
}

impl ReleaseSummary {
    /// The one place `storage_actions` is derived: [`ReleaseDetail::from_raw`]
    /// projects its fat aggregate down to a slim row and routes through here too.
    pub(crate) fn from_raw(raw: DbReleaseSummary, ctx: &ReleaseResolveCtx) -> ReleaseSummary {
        let storage_state = storage_state(raw.remote, raw.pinned);
        let upload = raw.upload.map(ReleaseUpload::from_raw);
        let transitioning = upload.is_some() || raw.transfer_action.is_some();
        ReleaseSummary {
            id: raw.id,
            album_id: raw.album_id,
            media: raw.media,
            storage_state,
            storage_actions: if transitioning {
                Vec::new()
            } else {
                available_storage_actions(
                    storage_state,
                    &release_playability(&raw.unstored_audio),
                    ctx.syncs,
                )
            },
            transfer_action: raw.transfer_action,
            upload,
            transitioning,
            file_count: raw.file_count,
            total_size: raw.total_size,
            cover: ctx.cover.clone(),
        }
    }
}

/// A release's files in this device's upload queue: how many wait and how
/// many are being sent now, and its summary of those and of its stored
/// files. How far and how fast the files being sent go is the release's
/// transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUpload {
    pub progress: bae_records::UploadProgress,
    /// How many of the release's files are uploading, queued and stored,
    /// each left out at zero.
    pub summary: Vec<bae_transfers::TransferCount>,
}

impl ReleaseUpload {
    pub(crate) fn from_raw(raw: bae_records::DbReleaseUpload) -> Self {
        let progress = bae_records::UploadProgress::of_release(&raw);
        use bae_transfers::TransferStage;
        Self {
            summary: bae_transfers::transfer_summary([
                ("core.queue.uploading", progress.uploading, TransferStage::Moving),
                ("core.queue.queued", progress.queued, TransferStage::Waiting),
                ("core.queue.stored", raw.stored, TransferStage::Done),
            ]),
            progress,
        }
    }
}

/// The fat projection for the album detail view: a [`ReleaseSummary`] plus what only
/// a detail view needs (tracks, files, gallery). Split this way so a list consumer
/// can render a row without loading tracks.
#[derive(Debug, Clone)]
pub struct ReleaseDetail {
    pub summary: ReleaseSummary,
    /// The stored `release_name`, else its year and media, else its position
    /// within its album. The resolver picks that position, so no consumer needs
    /// the index.
    pub name: ReleaseName,
    pub year: Option<i32>,
    /// Every label the release is on, each with its catalog number.
    pub labels: Vec<bae_types::pressing::ReleaseLabel>,
    /// What the pressing is: its area, media, status, packaging and details.
    pub facts: bae_types::pressing::PressingFacts,
    /// Summed across all tracks, in milliseconds. The UI formats it.
    pub total_duration_ms: u64,
    pub tracks: Vec<TrackDetail>,
    pub track_groups: Vec<TrackGroup>,
    pub files: Vec<FileDetail>,
    pub source_audio: Option<SourceAudioSummary>,
    pub image_files: Vec<FileDetail>,
    /// Image files eligible for cover normalization; previews use `image_files`.
    pub cover_files: Vec<FileDetail>,
    /// The cover, then every image file the release has — including cloud-only ones,
    /// which the lightbox fetches on demand.
    pub gallery_items: Vec<GalleryItem>,
    /// Whether this device can play the release now, and why not.
    pub playability: ReleasePlayability,
    /// The download control's state for the release, from its entry in the
    /// download queue as it was read; `None` when there is no control to show.
    pub download_status: Option<ReleaseDownloadStatus>,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
}

impl ReleaseDetail {
    /// Joins each track's shown artist names, projects each
    /// file's stored scan facts, groups tracks by side, builds the gallery, derives
    /// `name`, and composes the slim [`ReleaseSummary`].
    ///
    /// Both ordinary reads and subscriptions route through here, so the resolve
    /// logic stays in one place.
    pub(crate) fn from_raw(
        raw: DbReleaseDetail,
        transfers: bae_transfers::ReleaseTransferState,
        release_index: usize,
        ctx: &ReleaseResolveCtx,
    ) -> ReleaseDetail {
        let release = raw.release;

        let tracks: Vec<TrackDetail> = raw
            .tracks
            .into_iter()
            .map(|entry| {
                let artist_names = bae_records::credits::library_line(&entry.shown_artists);
                let position = raw.mediums.position(&entry.track);
                let position_text = position.text();
                // On a compilation each row carries its own artist, because the
                // album header names no single one; on a single-artist album the
                // row would only repeat the header, so it shows nothing.
                let display_artist = ctx.is_compilation.then(|| artist_names.clone());
                TrackDetail {
                    id: entry.track.id,
                    title: entry.track.title,
                    duration_ms: entry.track.duration_ms,
                    artist_names,
                    display_artist,
                    position_text,
                    position,
                }
            })
            .collect();

        let track_groups: Vec<TrackGroup> = super::group_tracks_by_side(&tracks);

        let remote = raw.remote;
        let playability = release_playability(&raw.unstored_audio);
        let mut cover_files = Vec::new();
        let files: Vec<FileDetail> = raw
            .files
            .into_iter()
            .map(|row| {
                let supports_cover = row.content_type.is_supported_cover();
                let file = FileDetail::from_raw(row);
                if supports_cover {
                    cover_files.push(file.clone());
                }
                file
            })
            .collect();
        let image_files: Vec<FileDetail> = files.iter().filter(|f| f.is_image).cloned().collect();
        let source_audio = SourceAudioSummary::from_files(
            files.iter().filter_map(|file| file.source_audio.as_ref()?.summarized()),
        );

        let file_count = files.len() as i64;
        let total_size: i64 = files.iter().map(|f| f.file_size).sum();
        let total_duration_ms: u64 = tracks.iter().map(|t| t.duration_ms).sum();

        let mut gallery = Vec::new();
        // One resolve of the `covers` row feeds both the gallery's "Cover" slot and
        // the summary's `cover`. The lightbox fetches the bytes by image id and
        // caches them under `(id, version)`; coven owns where they sit on disk.
        if let Some(cover_ref) = &ctx.cover {
            gallery.push(GalleryItem {
                id: "cover".to_string(),
                label: "Cover".to_string(),
                source: GallerySource::Cover(cover_ref.clone()),
            });
        }
        // An image file likewise has no stable bae path: the lightbox fetches its
        // bytes by file id through `read_gallery_bytes`, and coven reads them from
        // wherever they are — the user's own file on the one device holding it,
        // the cache once it is in storage.
        for f in &image_files {
            gallery.push(GalleryItem {
                id: f.id.clone(),
                label: f.original_filename.clone(),
                source: GallerySource::ReleaseFile { file_id: f.id.clone() },
            });
        }

        let name = ReleaseName::of(
            release.release_name.as_deref(),
            release.pressing.year,
            &release.pressing.facts.media,
            (release_index + 1) as i64,
        );

        // Route the slim row through `ReleaseSummary::from_raw` rather than build it
        // here, so the `storage_actions` derivation stays in one place.
        let summary = ReleaseSummary::from_raw(
            DbReleaseSummary {
                id: release.id.clone(),
                album_id: release.album_id.clone(),
                media: release.pressing.facts.media.clone(),
                remote,
                pinned: raw.pinned,
                upload: raw.upload,
                transfer_action: transfers.action,
                file_count,
                total_size,
                unstored_audio: raw.unstored_audio,
            },
            ctx,
        );

        let download_status = release_download_status(
            summary.storage_state,
            &summary.storage_actions,
            transfers.download.as_ref(),
        );
        ReleaseDetail {
            download_status,
            summary,
            name,
            year: release.pressing.year,
            labels: release.pressing.labels,
            facts: release.pressing.facts,
            total_duration_ms,
            tracks,
            track_groups,
            files,
            source_audio,
            image_files,
            cover_files,
            gallery_items: gallery,
            playability,
            records: raw.records,
        }
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;

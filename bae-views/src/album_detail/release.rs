//! Resolved release types (`ReleaseSummary`, `ReleaseDetail`) and the pure
//! projections that produce them from raw DB aggregates.

use super::*;
use crate::DbReleaseSummary;
use bae_records::DbReleaseDetail;

/// The per-release inputs the library's views read beside the release's rows and
/// threads into the release projections: the release's own cover reference
/// and the album facts its tracks are shown against.
/// Bundled so the constructors take one context rather than a pile of parameters.
#[derive(Debug, Clone)]
pub(crate) struct ReleaseResolveCtx {
    pub(crate) cover: Option<ImageRef>,
    /// Whether the owning album is a compilation. Decides `TrackDetail::display_artist`
    /// — the one album-level fact the per-track artist decision needs.
    pub(crate) is_compilation: bool,
    /// The owning album's year, which [`ReleaseDetail::distinct_year`] is read against.
    pub(crate) album_year: Option<i32>,
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
    /// The transitions available right now, decided in SQL from
    /// `storage_state`, whether this device holds the originals an upload reads,
    /// and whether the library syncs; empty without it, and while
    /// the release is `transitioning`. The UI renders these and never
    /// re-derives availability.
    pub storage_actions: Vec<ReleaseStorageAction>,
    /// What of the release waits in this device's upload queue; `None` when
    /// nothing does.
    pub upload: Option<ReleaseUpload>,
    /// Whether a transition is under way: files waiting to upload, or a
    /// transfer running. It offers only cancelling; another storage action
    /// would race it.
    pub transitioning: bool,
    pub file_count: i64,
    pub total_size: u64,
    /// This release's *own* cover, or `None` when it has no cover row. Keyed on the
    /// release id, so two releases of one album each resolve to their own art rather
    /// than to the album's primary cover.
    pub cover: Option<ImageRef>,
}

impl ReleaseSummary {
    /// [`ReleaseDetail::from_raw`] projects its fat aggregate down to a slim
    /// row and routes through here too.
    pub(crate) fn from_raw(raw: DbReleaseSummary, ctx: &ReleaseResolveCtx) -> ReleaseSummary {
        let storage_state = storage_state(raw.remote, raw.pinned);
        let upload = raw.upload.map(ReleaseUpload::from_raw);
        ReleaseSummary {
            id: raw.id,
            album_id: raw.album_id,
            media: raw.media,
            storage_state,
            storage_actions: raw.actions.list(),
            upload,
            transitioning: raw.actions.transitioning,
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
    /// The name of the file whose missing original its move waits for,
    /// while none of its files is being sent.
    pub missing: Option<String>,
}

impl ReleaseUpload {
    pub(crate) fn from_raw(raw: bae_records::DbReleaseUpload) -> Self {
        let progress = bae_records::UploadProgress::of_release(&raw);
        use bae_transfers::TransferStage;
        Self {
            summary: bae_transfers::transfer_summary([
                ("core.queue.uploading", progress.uploading, TransferStage::Moving),
                ("core.queue.queued_to_upload", progress.queued, TransferStage::Waiting),
                ("core.queue.stored", raw.stored, TransferStage::Done),
            ]),
            progress,
            missing: raw.missing,
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
    /// The release's year when it differs from its album's, else `None`: what a
    /// line about the pressing names, since the album's year is already shown
    /// above it.
    pub distinct_year: Option<i32>,
    /// Every label the release is on, each with its catalog number.
    pub labels: Vec<bae_types::pressing::ReleaseLabel>,
    /// What the pressing is: its area, media, status, packaging and details.
    pub facts: bae_types::pressing::PressingFacts,
    /// Summed across all tracks, in milliseconds. The UI formats it.
    pub total_duration_ms: u64,
    pub tracks: Vec<TrackDetail>,
    pub track_groups: Vec<TrackGroup>,
    /// Who did what on the release and its tracks, every role: see
    /// [`CreditRow`].
    pub credits: Vec<CreditRow>,
    pub files: Vec<FileDetail>,
    /// The files no track plays from, in `files` order: the files the editor's
    /// Files section lists.
    pub other_files: Vec<FileDetail>,
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
    /// Where the release stands in being moved to this device, in the download queue;
    /// `None` when it is not being.
    pub move_to_this_device: Option<MoveToThisDeviceStatus>,
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
        download: Option<bae_transfers::DownloadEntry>,
        actions: crate::DbStorageActions,
        release_index: usize,
        ctx: &ReleaseResolveCtx,
    ) -> ReleaseDetail {
        let release = raw.release;

        let credits: Vec<CreditRow> = raw.credits.into_iter().map(CreditRow::from_raw).collect();
        let track_groups: Vec<TrackGroup> = raw
            .track_groups
            .into_iter()
            .map(|group| TrackGroup::from_raw(group, ctx.is_compilation))
            .collect();
        let tracks: Vec<TrackDetail> =
            track_groups.iter().flat_map(|group| group.tracks.iter().cloned()).collect();

        let remote = raw.remote;
        let playability = release_playability(&raw.unstored_audio);
        let files: Vec<FileDetail> =
            raw.files.files.into_iter().map(FileDetail::from_raw).collect();
        let image_files: Vec<FileDetail> =
            raw.files.images.into_iter().map(FileDetail::from_raw).collect();
        let cover_files: Vec<FileDetail> =
            raw.files.covers.into_iter().map(FileDetail::from_raw).collect();
        let other_files: Vec<FileDetail> =
            raw.files.others.into_iter().map(FileDetail::from_raw).collect();
        let source_audio = SourceAudioSummary::from_files(
            files.iter().filter_map(|file| file.source_audio.as_ref()?.summarized()),
        );

        let file_count = raw.files.count;
        let total_size = raw.files.total_size;
        let total_duration_ms = raw.total_duration_ms;

        let mut gallery = Vec::new();
        // One resolve of the `covers` row feeds both the gallery's "Cover" slot and
        // the summary's `cover`. The lightbox fetches the bytes by image id and
        // caches them under `(id, version)`; coven owns where they sit on disk.
        if let Some(cover_ref) = &ctx.cover {
            gallery.push(GalleryItem {
                id: "cover".to_string(),
                label: GalleryLabel::Cover,
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
                label: GalleryLabel::File { name: f.original_filename.clone() },
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
        // here.
        let summary = ReleaseSummary::from_raw(
            DbReleaseSummary {
                id: release.id.clone(),
                album_id: release.album_id.clone(),
                media: release.pressing.facts.media.clone(),
                remote,
                pinned: raw.pinned,
                upload: raw.upload,
                download: download.clone(),
                file_count,
                total_size,
                actions,
            },
            ctx,
        );

        let download_status = release_download_status(
            summary.storage_state,
            &summary.storage_actions,
            download.as_ref(),
        );
        let move_to_this_device = release_move_to_this_device_status(download.as_ref());
        ReleaseDetail {
            download_status,
            move_to_this_device,
            summary,
            name,
            year: release.pressing.year,
            distinct_year: release.pressing.year.filter(|year| Some(*year) != ctx.album_year),
            labels: release.pressing.labels,
            facts: release.pressing.facts,
            total_duration_ms,
            tracks,
            track_groups,
            credits,
            files,
            other_files,
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

use super::*;

impl BridgeFile {
    pub(super) fn from_core(f: bae_core::album_detail::FileDetail) -> Self {
        let bae_core::album_detail::FileDetail {
            id,
            original_filename,
            file_size,
            is_image,
            content_type,
            source_audio,
        } = f;
        BridgeFile {
            id,
            original_filename,
            file_size,
            is_image,
            content_type,
            audio_format: source_audio
                .map(|audio| crate::types::BridgeAudioFormat::from_core(audio.format)),
        }
    }
}

impl BridgeGalleryItem {
    pub(super) fn from_core(g: bae_core::album_detail::GalleryItem) -> Self {
        let bae_core::album_detail::GalleryItem { id, label, source } = g;
        BridgeGalleryItem {
            id,
            label,
            source: match source {
                bae_core::album_detail::GallerySource::Cover(image) => {
                    crate::types::BridgeGallerySource::Cover {
                        image: crate::types::BridgeImageRef::from_core(image),
                    }
                }
                bae_core::album_detail::GallerySource::ReleaseFile { file_id } => {
                    crate::types::BridgeGallerySource::ReleaseFile { file_id }
                }
            },
        }
    }
}

impl BridgeTrack {
    pub(super) fn from_core(t: bae_core::album_detail::TrackDetail) -> Self {
        let bae_core::album_detail::TrackDetail {
            id,
            title,
            side,
            track_number,
            duration_ms,
            artist_names,
            display_artist,
            position_text,
            // Structured position drives core-side grouping (`track_groups`); the
            // UI renders `position_text` and the group headers, not this.
            position: _,
        } = t;
        BridgeTrack {
            id,
            title,
            side,
            track_number,
            duration_clock: crate::types::BridgeDurationClock::from_millis(duration_ms),
            duration_ms,
            artist_names,
            display_artist,
            position_text,
        }
    }
}

impl BridgeTrackGroup {
    pub(super) fn from_core(g: bae_core::album_detail::TrackGroup) -> Self {
        let bae_core::album_detail::TrackGroup {
            side,
            tracks,
            total_duration_ms,
        } = g;
        let side = crate::types::BridgeTrackSide::from_core(side);
        BridgeTrackGroup {
            header_key: side.header_key().map(str::to_string),
            side,
            tracks: tracks.into_iter().map(BridgeTrack::from_core).collect(),
            total_duration: bae_core::util::duration::DurationUnits::from_millis(total_duration_ms)
                .map(crate::types::BridgeDurationUnits::from_core),
        }
    }
}

impl BridgeRelease {
    pub(super) fn from_core(rel: bae_core::album_detail::ReleaseDetail) -> Self {
        let bae_core::album_detail::ReleaseDetail {
            summary,
            display_name,
            year,
            label,
            catalog_number,
            country,
            total_duration_ms,
            tracks,
            track_groups,
            files,
            source_audio,
            image_files,
            gallery_items,
        } = rel;
        // Single-source the summary-derived fields through BridgeReleaseSummary,
        // then exhaustively destructure it into BridgeRelease's flat fields so the
        // storage/cover projection lives in one place.
        let BridgeReleaseSummary {
            id,
            album_id,
            format,
            storage_state,
            pinned,
            storage_actions,
            transfer_action,
            file_count,
            total_size,
            cover,
        } = BridgeReleaseSummary::from_core(summary);
        BridgeRelease {
            id,
            album_id,
            display_name,
            year,
            format,
            label,
            catalog_number,
            country,
            storage_state,
            pinned,
            storage_actions,
            transfer_action,
            total_duration: bae_core::util::duration::DurationUnits::from_millis(total_duration_ms)
                .map(crate::types::BridgeDurationUnits::from_core),
            tracks: tracks.into_iter().map(BridgeTrack::from_core).collect(),
            track_groups: track_groups
                .into_iter()
                .map(BridgeTrackGroup::from_core)
                .collect(),
            image_files: image_files.into_iter().map(BridgeFile::from_core).collect(),
            files: files.into_iter().map(BridgeFile::from_core).collect(),
            source_audio: source_audio.map(crate::types::BridgeSourceAudioSummary::from_core),
            gallery_items: gallery_items
                .into_iter()
                .map(BridgeGalleryItem::from_core)
                .collect(),
            file_count,
            total_size,
            cover,
        }
    }
}

impl BridgeAlbumSearchResult {
    pub(super) fn from_core(a: bae_core::album_detail::AlbumSearchResult) -> Self {
        let bae_core::album_detail::AlbumSearchResult {
            id,
            title,
            year,
            artist_name,
            cover,
        } = a;
        BridgeAlbumSearchResult {
            id,
            title,
            year,
            artist_name,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeTrackSearchResult {
    pub(super) fn from_core(t: bae_core::album_detail::TrackSearchResult) -> Self {
        let bae_core::album_detail::TrackSearchResult {
            id,
            title,
            duration_ms,
            album_id,
            album_title,
            artist_name,
            cover,
        } = t;
        BridgeTrackSearchResult {
            id,
            title,
            duration_clock: crate::types::BridgeDurationClock::from_millis(duration_ms),
            album_id,
            album_title,
            artist_name,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeStoragePage {
    pub(super) fn from_core(page: bae_core::album_detail::StoragePage) -> Self {
        let bae_core::album_detail::StoragePage { rows, total_count } = page;
        BridgeStoragePage {
            rows: rows.into_iter().map(BridgeStorageRow::from_core).collect(),
            total_count,
        }
    }
}

impl BridgeSearchResults {
    pub(crate) fn from_core(results: bae_core::album_detail::SearchResults) -> Self {
        Self {
            albums: results
                .albums
                .into_iter()
                .map(BridgeAlbumSearchResult::from_core)
                .collect(),
            artists: results
                .artists
                .into_iter()
                .map(BridgeArtistSummary::from_core)
                .collect(),
            tracks: results
                .tracks
                .into_iter()
                .map(BridgeTrackSearchResult::from_core)
                .collect(),
            composers: results
                .composers
                .into_iter()
                .map(BridgeComposerSummary::from_core)
                .collect(),
            works: results
                .works
                .into_iter()
                .map(BridgeWorkSummary::from_core)
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeCandidateSearchResults {
    /// Echoes back the `tab` and `source` the search ran against so the caller
    /// routes results into the matching slot even if the user changed either
    /// during the await.
    pub(super) fn from_core(
        grouped: bae_core::import::GroupedSearchResults,
        tab: crate::types::BridgeSearchQueryKind,
        source: crate::types::BridgeMetadataSource,
    ) -> Self {
        let bae_core::import::GroupedSearchResults { groups, statuses } = grouped;
        crate::types::BridgeCandidateSearchResults {
            tab,
            source,
            groups: groups
                .into_iter()
                .map(crate::types::BridgeReleaseGroup::from_core)
                .collect(),
            statuses: statuses
                .into_iter()
                .map(crate::types::BridgeLibraryStatus::from_core)
                .collect(),
        }
    }
}

impl BridgeStorageRow {
    pub(super) fn from_core(raw: bae_core::album_detail::StorageRow) -> Self {
        let bae_core::album_detail::StorageRow { release, album } = raw;
        BridgeStorageRow {
            release: BridgeReleaseSummary::from_core(release),
            album: BridgeAlbum::from_core(album),
        }
    }
}

impl BridgeReleaseSummary {
    pub(super) fn from_core(s: bae_core::album_detail::ReleaseSummary) -> Self {
        let bae_core::album_detail::ReleaseSummary {
            id,
            album_id,
            format,
            storage_state,
            pinned,
            storage_actions,
            transfer_action,
            file_count,
            total_size,
            cover,
        } = s;
        BridgeReleaseSummary {
            id,
            album_id,
            format,
            storage_state: crate::types::BridgeReleaseStorageState::from_core(storage_state),
            pinned,
            storage_actions: storage_actions
                .into_iter()
                .map(crate::types::BridgeReleaseStorageAction::from_core)
                .collect(),
            transfer_action: transfer_action
                .map(crate::types::BridgeReleaseStorageAction::from_core),
            file_count,
            total_size,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeAlbumDetail {
    pub(super) fn from_core(detail: bae_core::album_detail::AlbumDetail) -> Self {
        let bae_core::album_detail::AlbumDetail {
            album,
            artist_names,
            releases,
            primary_release_id,
            cover,
        } = detail;
        let release_ids: Vec<String> = releases.iter().map(|r| r.summary.id.clone()).collect();
        let bae_core::db::DbAlbum {
            id,
            title,
            year,
            is_compilation,
            // The album's own artist FK / created_at aren't surfaced, and the
            // primary release comes from the detail-level field below.
            artist_id: _,
            primary_release_id: _,
            created_at: _,
        } = album;
        // Reassemble the album's slim summary from the detail's DbAlbum plus the
        // detail-level artist_names/primary_release_id/cover, then reuse
        // BridgeAlbum::from_core so the BridgeAlbum projection lives in one place.
        let album = BridgeAlbum::from_core(bae_core::album_detail::AlbumSummary {
            id,
            title,
            year,
            is_compilation,
            artist_names,
            release_ids,
            primary_release_id,
            cover,
        });
        BridgeAlbumDetail {
            album,
            releases: releases.into_iter().map(BridgeRelease::from_core).collect(),
        }
    }
}

impl BridgeAlbum {
    pub(super) fn from_core(a: bae_core::album_detail::AlbumSummary) -> Self {
        let bae_core::album_detail::AlbumSummary {
            id,
            title,
            year,
            is_compilation,
            artist_names,
            release_ids,
            primary_release_id,
            cover,
        } = a;
        BridgeAlbum {
            id,
            title,
            year,
            is_compilation,
            artist_names,
            release_ids,
            primary_release_id,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeComposerSummary {
    pub(super) fn from_core(s: bae_core::album_detail::ComposerSummary) -> Self {
        let bae_core::album_detail::ComposerSummary { raw, image } = s;
        let bae_core::db::DbComposerSummary {
            artist,
            work_count,
            linked_release_count,
            unlinked_credit_count,
        } = raw;
        let bae_core::db::DbArtist {
            id: artist_id,
            name,
            sort_name,
            // Per-source dedup ids and the row timestamp don't cross to the UI.
            discogs_artist_id: _,
            musicbrainz_artist_id: _,
            created_at: _,
        } = artist;
        BridgeComposerSummary {
            artist_id,
            name,
            sort_name,
            work_count,
            linked_release_count,
            unlinked_credit_count,
            image: image.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeArtistSummary {
    pub(super) fn from_core(s: bae_core::album_detail::ArtistSummary) -> Self {
        let bae_core::album_detail::ArtistSummary { raw, image } = s;
        let bae_core::db::DbArtistSummary {
            artist,
            album_count,
        } = raw;
        let bae_core::db::DbArtist {
            id: artist_id,
            name,
            // The MB-only sort name, per-source dedup ids, and the row
            // timestamp don't cross to the UI.
            sort_name: _,
            discogs_artist_id: _,
            musicbrainz_artist_id: _,
            created_at: _,
        } = artist;
        BridgeArtistSummary {
            artist_id,
            name,
            album_count,
            image: image.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeArtistDetail {
    pub(super) fn from_core(d: bae_core::album_detail::ArtistDetail) -> Self {
        let bae_core::album_detail::ArtistDetail { artist, albums } = d;
        BridgeArtistDetail {
            artist: BridgeArtistSummary::from_core(artist),
            albums: albums.into_iter().map(BridgeAlbum::from_core).collect(),
        }
    }
}

impl BridgeWorkSummary {
    pub(super) fn from_core(s: bae_core::album_detail::WorkSummary) -> Self {
        let bae_core::album_detail::WorkSummary {
            raw,
            representative_cover,
        } = s;
        let bae_core::db::DbWorkSummary {
            work,
            parent_work_id,
            representative_release_id,
            composer_names,
            linked_release_count,
        } = raw;
        let bae_core::db::DbWork {
            id: work_id,
            title,
            disambiguation,
            work_type,
            // The source id the row was deduped on isn't surfaced.
            musicbrainz_work_id: _,
            // Row timestamp isn't surfaced.
            created_at: _,
        } = work;
        BridgeWorkSummary {
            work_id,
            title,
            disambiguation,
            work_type,
            parent_work_id,
            composer_names,
            linked_release_count,
            representative_release_id,
            representative_cover: representative_cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeReleaseRoleSummary {
    pub(super) fn from_core(s: bae_core::album_detail::ReleaseRoleSummary) -> Self {
        let bae_core::db::DbReleaseRoleSummary { role, album } = s;
        let bae_core::db::DbReleaseArtistRole {
            release_id,
            source,
            source_credit,
            id: _,
            artist_id: _,
            position: _,
            created_at: _,
        } = role;
        let bae_core::db::DbAlbum {
            id: album_id,
            title: album_title,
            artist_id: _,
            year: _,
            primary_release_id: _,
            is_compilation: _,
            created_at: _,
        } = album;
        BridgeReleaseRoleSummary {
            release_id,
            album_id,
            album_title,
            source: BridgeMetadataSource::from_core(source),
            source_credit,
        }
    }
}

impl BridgeTrackRoleSummary {
    pub(super) fn from_core(s: bae_core::album_detail::TrackRoleSummary) -> Self {
        let bae_core::db::DbTrackRoleSummary {
            role,
            track,
            album,
            artist,
        } = s;
        let bae_core::db::DbTrackArtistRole {
            track_id,
            artist_id,
            source,
            source_credit,
            id: _,
            position: _,
            created_at: _,
        } = role;
        let bae_core::db::DbTrack {
            title: track_title,
            release_id,
            id: _,
            side: _,
            track_number: _,
            duration_ms: _,
            discogs_position: _,
            created_at: _,
        } = track;
        let bae_core::db::DbAlbum {
            id: album_id,
            title: album_title,
            artist_id: _,
            year: _,
            primary_release_id: _,
            is_compilation: _,
            created_at: _,
        } = album;
        let bae_core::db::DbArtist {
            name: artist_name,
            id: _,
            sort_name: _,
            discogs_artist_id: _,
            musicbrainz_artist_id: _,
            created_at: _,
        } = artist;
        BridgeTrackRoleSummary {
            track_id,
            track_title,
            release_id,
            album_id,
            album_title,
            artist_id,
            artist_name,
            source: BridgeMetadataSource::from_core(source),
            source_credit,
        }
    }
}

impl BridgeWorkTrackSummary {
    pub(super) fn from_core(s: bae_core::album_detail::WorkTrackSummary) -> Self {
        let bae_core::db::DbWorkTrackSummary { link, track, album } = s;
        let bae_core::db::DbTrackWork {
            track_id,
            id: _,
            work_id: _,
            position: _,
            source: _,
            created_at: _,
        } = link;
        let bae_core::db::DbTrack {
            title: track_title,
            release_id,
            id: _,
            side: _,
            track_number: _,
            duration_ms: _,
            discogs_position: _,
            created_at: _,
        } = track;
        let bae_core::db::DbAlbum {
            id: album_id,
            title: album_title,
            artist_id: _,
            year: _,
            primary_release_id: _,
            is_compilation: _,
            created_at: _,
        } = album;
        BridgeWorkTrackSummary {
            track_id,
            track_title,
            release_id,
            album_id,
            album_title,
        }
    }
}

impl BridgeWorkReleaseSummary {
    pub(super) fn from_core(s: bae_core::album_detail::WorkReleaseSummary) -> Self {
        let bae_core::album_detail::WorkReleaseSummary {
            release_id,
            album_id,
            album_title,
            display_name,
            format,
            cover,
        } = s;
        BridgeWorkReleaseSummary {
            release_id,
            album_id,
            album_title,
            display_name,
            format,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeComposerWorkGroup {
    pub(super) fn from_core(group: bae_core::album_detail::ComposerWorkGroup) -> Self {
        let bae_core::album_detail::ComposerWorkGroup { id, parent, works } = group;
        BridgeComposerWorkGroup {
            id,
            parent: parent.map(BridgeWorkSummary::from_core),
            works: works
                .into_iter()
                .map(BridgeWorkSummary::from_core)
                .collect(),
        }
    }
}

impl BridgeComposerDetail {
    pub(super) fn from_core(d: bae_core::album_detail::ComposerDetail) -> Self {
        let bae_core::album_detail::ComposerDetail {
            composer,
            work_groups,
            unlinked_release_roles,
            unlinked_track_roles,
            default_work_id,
        } = d;
        BridgeComposerDetail {
            composer: BridgeComposerSummary::from_core(composer),
            work_groups: work_groups
                .into_iter()
                .map(BridgeComposerWorkGroup::from_core)
                .collect(),
            unlinked_release_roles: unlinked_release_roles
                .into_iter()
                .map(BridgeReleaseRoleSummary::from_core)
                .collect(),
            unlinked_track_roles: unlinked_track_roles
                .into_iter()
                .map(BridgeTrackRoleSummary::from_core)
                .collect(),
            default_work_id,
        }
    }
}

impl BridgeWorkDetail {
    pub(super) fn from_core(d: bae_core::album_detail::WorkDetail) -> Self {
        let bae_core::album_detail::WorkDetail {
            work,
            child_works,
            releases,
            tracks,
        } = d;
        BridgeWorkDetail {
            work: BridgeWorkSummary::from_core(work),
            child_works: child_works
                .into_iter()
                .map(BridgeWorkSummary::from_core)
                .collect(),
            releases: releases
                .into_iter()
                .map(BridgeWorkReleaseSummary::from_core)
                .collect(),
            tracks: tracks
                .into_iter()
                .map(BridgeWorkTrackSummary::from_core)
                .collect(),
        }
    }
}

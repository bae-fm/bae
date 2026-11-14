use super::*;

mirror_enum! {
    crate::types::BridgeFileKind = bae_views::album_detail::FileKind,
    from_core: fn,
    variants: {
        Audio { format },
        Image { format },
        DamagedImage,
        CueSheet,
        RipLog,
        AccurateRipReport,
        Playlist,
        Text,
        Pdf,
        Other,
    },
}

impl BridgeFile {
    pub(crate) fn from_core(f: bae_views::album_detail::FileDetail) -> Self {
        let bae_views::album_detail::FileDetail {
            id,
            original_filename,
            place,
            kind,
            file_size,
            is_image,
            content_type,
            source_audio,
        } = f;
        BridgeFile {
            id,
            original_filename,
            name: place.name,
            folder: place.folder,
            kind: crate::types::BridgeFileKind::from_core(kind),
            file_size,
            is_image,
            content_type,
            audio_format: source_audio
                .map(|audio| crate::types::BridgeAudioFormat::from_core(audio.format)),
        }
    }
}

mirror_struct! {
    BridgeGalleryItem = bae_views::album_detail::GalleryItem,
    from_core: pub(super) fn,
    fields: {
        id,
        label: (crate::types::BridgeGalleryLabel),
        source: (crate::types::BridgeGallerySource),
    },
}

impl BridgeTrack {
    pub(super) fn from_core(t: bae_views::album_detail::TrackDetail) -> Self {
        let bae_views::album_detail::TrackDetail {
            id,
            title,
            duration_ms,
            artist_line,
            display_artist,
            position_text,
            // Structured position drives core-side grouping (`track_groups`); the
            // UI renders `position_text` and the group headers, not this.
            position: _,
            works,
            my_stars,
        } = t;
        BridgeTrack {
            id,
            title,
            duration_clock: crate::types::BridgeDurationClock::of_length(duration_ms),
            duration_ms,
            artist_line: crate::types::BridgeCreditLine::from_core(artist_line),
            display_artist: display_artist.map(crate::types::BridgeCreditLine::from_core),
            position_text,
            works: works
                .into_iter()
                .map(|work| crate::types::BridgeTrackWork {
                    work_id: work.work_id,
                    title: work.title,
                    composer_names: work.composer_names,
                })
                .collect(),
            my_stars: my_stars.map(crate::types::BridgeStars::from_core),
        }
    }
}

impl BridgeTrackGroup {
    pub(super) fn from_core(g: bae_views::album_detail::TrackGroup) -> Self {
        let bae_views::album_detail::TrackGroup { side, tracks, total_duration_ms, shows_time } = g;
        let side = crate::types::BridgeTrackSide::from_core(side);
        BridgeTrackGroup {
            header_key: side.header_key().map(str::to_string),
            side,
            tracks: tracks.into_iter().map(BridgeTrack::from_core).collect(),
            total_duration: bae_util::duration::DurationUnits::from_millis(total_duration_ms)
                .map(crate::types::BridgeDurationUnits::from_core),
            shows_time,
        }
    }
}

impl BridgeRelease {
    pub(crate) fn from_core(rel: bae_views::album_detail::ReleaseDetail) -> Self {
        let bae_views::album_detail::ReleaseDetail {
            summary,
            name,
            year: _,
            distinct_year,
            labels,
            facts,
            total_duration_ms,
            tracks,
            track_groups,
            credits,
            // The Storage sheet reads the files in the order it sorts by
            // (`subscribe_release_files`), not the detail's.
            files: _,
            other_files,
            source_audio,
            image_files,
            cover_files,
            gallery_items,
            playability,
            download_status,
            move_to_this_device,
            records,
        } = rel;
        // Single-source the summary-derived fields through BridgeReleaseSummary,
        // then exhaustively destructure it into BridgeRelease's flat fields so the
        // storage/cover projection lives in one place.
        let BridgeReleaseSummary {
            id,
            album_id,
            media: _,
            media_terms,
            storage_state,
            storage_actions,
            upload,
            transitioning,
            file_count,
            total_size,
            cover,
        } = BridgeReleaseSummary::from_core(summary);
        // The summary's media are the facts' media, which the release carries
        // whole.
        let facts = crate::types::BridgePressingFacts::from_core(facts);
        BridgeRelease {
            id,
            album_id,
            name: crate::types::BridgeReleaseName::from_core(name),
            distinct_year,
            labels: bae_types::pressing::label_lines(&labels)
                .into_iter()
                .map(crate::types::BridgeLabelLine::from_core)
                .collect(),
            media_terms,
            pressing_summary: crate::types::bridge_pressing_summary(facts.clone()),
            pressing_details: crate::types::bridge_pressing_details(facts.clone()),
            facts,
            storage_state,
            storage_actions,
            upload,
            transitioning,
            total_duration: bae_util::duration::DurationUnits::from_millis(total_duration_ms)
                .map(crate::types::BridgeDurationUnits::from_core),
            tracks: tracks.into_iter().map(BridgeTrack::from_core).collect(),
            track_groups: track_groups.into_iter().map(BridgeTrackGroup::from_core).collect(),
            credits: credits.into_iter().map(crate::types::BridgeCreditRow::from_core).collect(),
            other_files: other_files.into_iter().map(BridgeFile::from_core).collect(),
            image_files: image_files.into_iter().map(BridgeFile::from_core).collect(),
            cover_files: cover_files.into_iter().map(BridgeFile::from_core).collect(),
            source_audio: source_audio.map(crate::types::BridgeSourceAudioSummary::from_core),
            gallery_items: gallery_items.into_iter().map(BridgeGalleryItem::from_core).collect(),
            playability: crate::types::BridgeReleasePlayability::from_core(playability),
            move_to_this_device: move_to_this_device
                .map(crate::types::BridgeMoveToThisDeviceStatus::from_core),
            download_status: download_status
                .map(crate::types::BridgeReleaseDownloadStatus::from_core),
            records: records
                .into_iter()
                .map(crate::types::BridgeReleaseRecord::from_core)
                .collect(),
            file_count,
            total_size,
            cover,
        }
    }
}

mirror_struct! {
    BridgeAlbumSearchResult = bae_views::album_detail::AlbumSearchResult,
    from_core: pub(super) fn,
    fields: {
        id,
        title,
        year,
        artist_line: (crate::types::BridgeCreditLine),
        cover: (opt crate::types::BridgeImageRef),
    },
}

impl BridgeTrackSearchResult {
    pub(super) fn from_core(t: bae_views::album_detail::TrackSearchResult) -> Self {
        let bae_views::album_detail::TrackSearchResult {
            id,
            title,
            duration_ms,
            release_id,
            album_id,
            album_title,
            artist_line,
            cover,
        } = t;
        BridgeTrackSearchResult {
            id,
            title,
            duration_clock: crate::types::BridgeDurationClock::of_length(duration_ms),
            release_id,
            album_id,
            album_title,
            artist_line: crate::types::BridgeCreditLine::from_core(artist_line),
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

mirror_struct! {
    BridgeSearchResults = bae_views::album_detail::SearchResults,
    from_core: pub(crate) fn,
    fields: {
        albums: (each BridgeAlbumSearchResult),
        artists: (each BridgeArtistSummary),
        tracks: (each BridgeTrackSearchResult),
        composers: (each BridgeComposerSummary),
        works: (each BridgeWorkSummary),
    },
}

mirror_struct! {
    BridgeStorageRow = bae_views::album_detail::StorageRow,
    from_core: pub(crate) fn,
    fields: {
        release: (BridgeReleaseSummary),
        album: (BridgeAlbum),
        status: (BridgeStorageRowStatus),
        date_added: (into),
    },
}

mirror_enum! {
    crate::types::BridgeDownloadEntryStep = bae_views::album_detail::DownloadEntryStep,
    from_core: fn,
    variants: {
        Queued,
        Running,
    },
}

mirror_enum! {
    BridgeStorageRowStatus = bae_views::album_detail::StorageRowStatus,
    from_core: fn,
    variants: {
        DownloadFailed { error },
        MoveFailed { file_name },
        MoveToThisDeviceFailed { error },
        Uploading,
        Pinning(step: (crate::types::BridgeDownloadEntryStep)),
        MovingToThisDevice(step: (crate::types::BridgeDownloadEntryStep)),
        OnOneDevice,
        Pinned,
        InStorage,
    },
}

/// Not a `mirror_struct`: the media cross worded as well as typed, so a
/// surface draws them without asking the bridge.
impl BridgeReleaseSummary {
    pub(super) fn from_core(summary: bae_views::album_detail::ReleaseSummary) -> Self {
        let bae_views::album_detail::ReleaseSummary {
            id,
            album_id,
            media,
            storage_state,
            storage_actions,
            upload,
            transitioning,
            file_count,
            total_size,
            cover,
        } = summary;
        let media: Vec<crate::types::BridgeMediaCount> =
            media.into_iter().map(crate::types::BridgeMediaCount::from_core).collect();
        BridgeReleaseSummary {
            id,
            album_id,
            media_terms: crate::types::bridge_media_terms(media.clone()),
            media,
            storage_state: crate::types::BridgeReleaseStorageState::from_core(storage_state),
            storage_actions: storage_actions
                .into_iter()
                .map(crate::types::BridgeReleaseStorageAction::from_core)
                .collect(),
            upload: upload.map(crate::types::BridgeReleaseUpload::from_core),
            transitioning,
            file_count,
            total_size,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeAlbumDetail {
    pub(crate) fn from_core(detail: bae_views::album_detail::AlbumDetail) -> Self {
        let bae_views::album_detail::AlbumDetail {
            album,
            // The credits' names arrive as the `artist_line`; no surface
            // links them from the album detail.
            artists: _,
            artist_line,
            releases,
            primary_release_id,
            cover,
            rating,
        } = detail;
        let release_ids: Vec<String> = releases.iter().map(|r| r.summary.id.clone()).collect();
        let bae_records::DbAlbum {
            id,
            title,
            year,
            is_compilation,
            // created_at isn't surfaced; the primary release comes from the
            // detail-level field below.
            created_at: _,
        } = album;
        // Reassemble the album's slim summary from the detail's DbAlbum plus the
        // detail-level artist_line/primary_release_id/cover, then reuse
        // BridgeAlbum::from_core so the BridgeAlbum projection lives in one place.
        let album = BridgeAlbum::from_core(bae_views::album_detail::AlbumSummary {
            id,
            title,
            year,
            is_compilation,
            artist_line,
            release_ids,
            primary_release_id,
            cover,
        });
        BridgeAlbumDetail {
            album,
            releases: releases.into_iter().map(BridgeRelease::from_core).collect(),
            rating: crate::types::BridgeAlbumRating::from_core(rating),
        }
    }
}

mirror_struct! {
    BridgeAlbum = bae_views::album_detail::AlbumSummary,
    from_core: pub(crate) fn,
    fields: {
        id,
        title,
        year,
        is_compilation,
        artist_line: (crate::types::BridgeCreditLine),
        release_ids,
        primary_release_id,
        cover: (opt crate::types::BridgeImageRef),
    },
}

mirror_struct! {
    crate::types::BridgeAlbumAppearance = bae_views::album_detail::AlbumAppearance,
    from_core: pub(crate) fn,
    fields: {
        album: (BridgeAlbum),
        credited_with: (opt crate::types::BridgeCreditLine),
    },
}

impl BridgeComposerSummary {
    pub(crate) fn from_core(s: bae_views::DbComposerSummary) -> Self {
        let count_line = crate::types::BridgeCountLabel::from_core(s.count_line());
        let bae_views::DbComposerSummary {
            artist,
            image,
            // Read as the count line above.
            work_count: _,
            linked_release_count,
            unlinked_credit_count,
        } = s;
        let bae_records::DbArtist {
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
            count_line,
            linked_release_count,
            unlinked_credit_count,
            image: image.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeArtistSummary {
    pub(crate) fn from_core(s: bae_views::DbArtistSummary) -> Self {
        let count_line = crate::types::BridgeCountLabel::from_core(s.count_line());
        // The album count is read as the count line above.
        let bae_views::DbArtistSummary { artist, image, album_count: _ } = s;
        let bae_records::DbArtist {
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
            count_line,
            image: image.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

mirror_struct! {
    BridgeArtistDetail = bae_views::album_detail::ArtistDetail,
    from_core: pub(crate) fn,
    fields: {
        artist: (BridgeArtistSummary),
        credit_names: (each crate::types::BridgeArtistCreditName),
        albums: (each BridgeAlbum),
    },
}

impl BridgeWorkSummary {
    pub(super) fn from_core(s: bae_views::DbWorkSummary) -> Self {
        let bae_views::DbWorkSummary {
            work,
            representative_release_id,
            representative_cover,
            composer_names,
            linked_release_count,
        } = s;
        let bae_records::DbWork {
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
            composer_names,
            linked_release_count,
            representative_release_id,
            representative_cover: representative_cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeReleaseRoleSummary {
    pub(super) fn from_core(s: bae_views::DbReleaseRoleSummary) -> Self {
        let bae_views::DbReleaseRoleSummary { role, album } = s;
        let bae_records::DbReleaseArtistRole {
            release_id,
            role,
            source: _,
            credit: _,
            id: _,
            artist_id: _,
            position: _,
            created_at: _,
        } = role;
        let bae_records::DbAlbum {
            id: album_id,
            title: album_title,
            year: _,
            is_compilation: _,
            created_at: _,
        } = album;
        BridgeReleaseRoleSummary {
            release_id,
            album_id,
            album_title,
            role: crate::types::BridgeCreditRole::from_core(role),
        }
    }
}

impl BridgeTrackRoleSummary {
    pub(super) fn from_core(s: bae_views::DbTrackRoleSummary) -> Self {
        let bae_views::DbTrackRoleSummary { role, track, album, artist } = s;
        let bae_records::DbTrackArtistRole {
            track_id,
            artist_id,
            role,
            source: _,
            credit: _,
            id: _,
            position: _,
            created_at: _,
        } = role;
        let bae_records::DbTrack {
            title: track_title,
            release_id,
            id: _,
            medium_id: _,
            side: _,
            track_number: _,
            duration_ms: _,
            created_at: _,
        } = track;
        let bae_records::DbAlbum {
            id: album_id,
            title: album_title,
            year: _,
            is_compilation: _,
            created_at: _,
        } = album;
        let bae_records::DbArtist {
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
            role: crate::types::BridgeCreditRole::from_core(role),
        }
    }
}

impl BridgeWorkTrackSummary {
    pub(super) fn from_core(s: bae_views::DbWorkTrackSummary) -> Self {
        let bae_views::DbWorkTrackSummary { link, track, album } = s;
        let bae_records::DbTrackWork {
            track_id,
            id: _,
            work_id: _,
            position: _,
            source: _,
            created_at: _,
        } = link;
        let bae_records::DbTrack {
            title: track_title,
            release_id,
            id: _,
            medium_id: _,
            side: _,
            track_number: _,
            duration_ms: _,
            created_at: _,
        } = track;
        let bae_records::DbAlbum {
            id: album_id,
            title: album_title,
            year: _,
            is_compilation: _,
            created_at: _,
        } = album;
        BridgeWorkTrackSummary { track_id, track_title, release_id, album_id, album_title }
    }
}

/// Not a `mirror_struct`: the media cross worded, which is all a surface
/// does with them.
impl BridgeWorkReleaseSummary {
    pub(super) fn from_core(summary: bae_views::album_detail::WorkReleaseSummary) -> Self {
        let bae_views::album_detail::WorkReleaseSummary {
            release_id,
            album_id,
            album_title,
            name,
            media,
            cover,
        } = summary;
        BridgeWorkReleaseSummary {
            release_id,
            album_id,
            album_title,
            name: crate::types::BridgeReleaseName::from_core(name),
            media: crate::types::bridge_media_terms(
                media.into_iter().map(crate::types::BridgeMediaCount::from_core).collect(),
            ),
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

mirror_struct! {
    BridgeComposerDetail = bae_views::album_detail::ComposerDetail,
    from_core: pub(crate) fn,
    fields: {
        composer: (BridgeComposerSummary),
        default_work_id,
    },
}

mirror_enum! {
    crate::types::BridgeComposerHeading = bae_views::album_detail::ComposerHeading,
    from_core: pub(super) fn,
    variants: { Works, Credits, Recordings },
}

impl crate::types::BridgeComposerDetailRow {
    pub(crate) fn from_core(row: bae_views::album_detail::ComposerDetailRow) -> Self {
        use bae_views::album_detail::ComposerDetailItem;
        let bae_views::album_detail::ComposerDetailRow { id, heading, item } = row;
        let item = match item {
            ComposerDetailItem::Work { work, part } => {
                crate::types::BridgeComposerDetailItem::Work {
                    work: BridgeWorkSummary::from_core(work),
                    part,
                }
            }
            ComposerDetailItem::ReleaseCredit(credit) => {
                crate::types::BridgeComposerDetailItem::ReleaseCredit {
                    credit: BridgeReleaseRoleSummary::from_core(credit),
                }
            }
            ComposerDetailItem::TrackCredit(credit) => {
                crate::types::BridgeComposerDetailItem::TrackCredit {
                    credit: BridgeTrackRoleSummary::from_core(credit),
                }
            }
        };
        Self { id, heading: heading.map(crate::types::BridgeComposerHeading::from_core), item }
    }
}

mirror_struct! {
    BridgeWorkDetail = bae_views::album_detail::WorkDetail,
    from_core: pub(crate) fn,
    fields: {
        work: (BridgeWorkSummary),
        child_works: (each BridgeWorkSummary),
        releases: (each BridgeWorkReleaseSummary),
        tracks: (each BridgeWorkTrackSummary),
    },
}

impl crate::types::BridgeReleaseUpload {
    pub(super) fn from_core(upload: bae_views::album_detail::ReleaseUpload) -> Self {
        Self {
            progress: crate::types::BridgeUploadProgress::from_core(upload.progress),
            summary: upload
                .summary
                .into_iter()
                .map(crate::types::BridgeTransferCount::from_core)
                .collect(),
            missing: upload.missing,
        }
    }
}

mirror_enum! {
    crate::types::BridgeUploadQueueWait = bae_records::UploadQueueWait,
    from_core: pub(crate) fn,
    variants: {
        Paused,
        Offline,
        Metered,
        Retrying { at_ms },
    },
}

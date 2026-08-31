use super::*;

pub(super) fn find_album_by_id_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Option<DbAlbum>, DbError> {
    sql.query_row(
        r#"
            SELECT
                id, title, artist_id, year, primary_release_id,
                is_compilation,
                created_at
            FROM albums
            WHERE id = ?
            "#,
        params![album_id],
        row_to_album,
    )
    .optional()
    .map_err(DbError::from)
}

pub(super) fn get_artists_for_album_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Vec<DbArtist>, DbError> {
    // Primary artist from FK (sort_key = -1 so it's first), then additional
    // artists from the junction table ordered by position.
    sql.query(
        r#"
            SELECT a.*, -1 AS sort_key FROM artists a
            JOIN albums alb ON alb.artist_id = a.id
            WHERE alb.id = ?
            UNION ALL
            SELECT a.*, aa.position AS sort_key FROM artists a
            JOIN album_artists aa ON a.id = aa.artist_id
            WHERE aa.album_id = ?
            ORDER BY sort_key
            "#,
        params![album_id, album_id],
        row_to_artist,
    )
    .map_err(DbError::from)
}

pub(super) fn find_release_by_id_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DbRelease>, DbError> {
    sql.query_row(
        "SELECT * FROM releases WHERE id = ?",
        params![release_id],
        row_to_release,
    )
    .optional()
    .map_err(DbError::from)
}

pub(super) fn get_releases_for_album_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Vec<DbRelease>, DbError> {
    sql.query(
        "SELECT * FROM releases WHERE album_id = ? ORDER BY created_at",
        params![album_id],
        row_to_release,
    )
    .map_err(DbError::from)
}

pub(super) fn build_release_detail_on(
    sql: &SqlReadContext<'_>,
    release: DbRelease,
) -> Result<DbReleaseDetail, DbError> {
    let tracks = get_tracks_with_artists_for_release_on(sql, &release.id)?;
    let files = get_files_for_release_on(sql, &release.id)?;
    let audio_formats = get_audio_formats_for_release_on(sql, &release.id)?;
    let audio_segments = get_audio_segments_for_release_on(sql, &release.id)?;
    let identities = get_release_identities_on(sql, &release.id)?;

    Ok(DbReleaseDetail {
        release,
        tracks,
        files,
        audio_formats,
        audio_segments,
        identities,
    })
}

/// One row per (track, artist) pair, so a track with several artists repeats.
/// The rows arrive grouped by track and ordered by artist position, and the fold
/// below rebuilds one entry per track from that run.
pub(super) fn get_tracks_with_artists_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbTrackWithArtists>, DbError> {
    let joined = sql.query(
        "SELECT
            track.id AS track_id,
            track.release_id AS track_release_id,
            track.title AS track_title,
            track.side AS track_side,
            track.track_number AS track_track_number,
            track.duration_ms AS track_duration_ms,
            track.discogs_position AS track_discogs_position,
            track.created_at AS track_created_at,
            artist.id AS artist_id,
            artist.name AS artist_name,
            artist.sort_name AS artist_sort_name,
            artist.discogs_artist_id AS artist_discogs_artist_id,
            artist.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
            artist.created_at AS artist_created_at
         FROM tracks track
         LEFT JOIN track_artists ta ON ta.track_id = track.id
         LEFT JOIN artists artist ON artist.id = ta.artist_id
         WHERE track.release_id = ?
         ORDER BY track.side, track.track_number, track.id, ta.position",
        params![release_id],
        |row| {
            let track = row_to_joined_track(row)?;
            let artist_id: Option<String> = row.get("artist_id")?;
            let artist = match artist_id {
                Some(_) => Some(row_to_joined_artist(row)?),
                None => None,
            };
            Ok((track, artist))
        },
    )?;

    let mut tracks: Vec<DbTrackWithArtists> = Vec::new();
    for (track, artist) in joined {
        if tracks.last().map(|last| last.track.id.as_str()) != Some(track.id.as_str()) {
            tracks.push(DbTrackWithArtists {
                track,
                artists: Vec::new(),
            });
        }
        if let Some(artist) = artist {
            tracks
                .last_mut()
                .expect("the row's track was just pushed")
                .artists
                .push(artist);
        }
    }

    Ok(tracks)
}

pub(super) fn get_files_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbFile>, DbError> {
    let mut files = sql.query(
        "SELECT * FROM release_files WHERE release_id = ?",
        params![release_id],
        row_to_file,
    )?;
    // Every file list a user sees (detail, gallery, storage, export) derives
    // from this read, so it is ordered here once, the same way the import
    // folder lists its files: natural order, case-insensitive. Id breaks
    // exact ties so the order is stable.
    files.sort_by(|a, b| {
        natord::compare_ignore_case(&a.original_filename, &b.original_filename)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(files)
}

/// Every audio-format row for a release, joined through its tracks — one row per
/// track. A single-file CUE rip yields many rows whose segments all point at the
/// same file; the resolver groups them by that file id to describe each audio
/// file's format.
pub(super) fn get_audio_formats_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbAudioFormat>, DbError> {
    sql.query(
        "SELECT af.* FROM audio_formats af \
             JOIN tracks t ON t.id = af.track_id \
             WHERE t.release_id = ?",
        params![release_id],
        row_to_audio_format,
    )
    .map_err(DbError::from)
}

pub(super) fn get_audio_segments_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbAudioSegment>, DbError> {
    sql.query(
        "SELECT s.* FROM audio_format_segments s \
             JOIN audio_formats af ON af.id = s.audio_format_id \
             JOIN tracks t ON t.id = af.track_id \
             WHERE t.release_id = ? \
             ORDER BY af.track_id, s.segment_index",
        params![release_id],
        row_to_audio_segment,
    )
    .map_err(DbError::from)
}

pub(super) fn get_release_identities_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<crate::import::ReleaseIdentity>, DbError> {
    let raw = sql.query(
        r#"
            SELECT source, source_group_id, source_release_id
            FROM release_identities
            WHERE release_id = ?
            "#,
        params![release_id],
        |row| {
            Ok((
                row.get::<_, String>("source")?,
                row.get::<_, String>("source_group_id")?,
                row.get::<_, String>("source_release_id")?,
            ))
        },
    )?;

    let mut identities = Vec::with_capacity(raw.len());
    for (source_str, source_group_id, source_release_id) in raw {
        let Ok(source) = crate::import::MetadataSource::from_str(&source_str) else {
            tracing::warn!(
                %release_id, source = %source_str,
                "skipping release_identities row with unknown source"
            );
            continue;
        };
        identities.push(crate::import::ReleaseIdentity {
            source,
            source_group_id,
            source_release_id,
        });
    }
    Ok(identities)
}

/// Build a column-conversion error for a named column whose stored text the
/// mapper could not turn into its typed value, so a corrupt column surfaces like
/// any other bad read instead of panicking or silently mis-defaulting.
pub(super) fn column_conversion_error(
    row: &Row,
    column: &str,
    message: String,
) -> coven::rusqlite::Error {
    // The column was just read, so its index resolves; if it somehow doesn't,
    // that lookup error is itself a faithful failure to return.
    match row.as_ref().column_index(column) {
        Ok(idx) => coven::rusqlite::Error::FromSqlConversionFailure(
            idx,
            coven::rusqlite::types::Type::Text,
            message.into(),
        ),
        Err(e) => e,
    }
}

/// Read a named rfc3339 timestamp column, surfacing a malformed value as a
/// column-conversion error rather than panicking on the parse.
pub(super) fn rfc3339_column(row: &Row, column: &str) -> coven::rusqlite::Result<DateTime<Utc>> {
    let raw: String = row.get(column)?;
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            column_conversion_error(
                row,
                column,
                format!("{column} {raw:?} is not a valid rfc3339 timestamp: {e}"),
            )
        })
}

pub(super) fn metadata_source_column(
    row: &Row,
    column: &str,
) -> coven::rusqlite::Result<MetadataSource> {
    let raw: String = row.get(column)?;
    raw.parse::<MetadataSource>()
        .map_err(|e| column_conversion_error(row, column, e))
}

pub(super) fn row_to_joined_artist(row: &Row) -> coven::rusqlite::Result<DbArtist> {
    Ok(DbArtist {
        id: row.get("artist_id")?,
        name: row.get("artist_name")?,
        sort_name: row.get("artist_sort_name")?,
        discogs_artist_id: row.get("artist_discogs_artist_id")?,
        musicbrainz_artist_id: row.get("artist_musicbrainz_artist_id")?,
        created_at: rfc3339_column(row, "artist_created_at")?,
    })
}

pub(super) fn row_to_joined_album(row: &Row) -> coven::rusqlite::Result<DbAlbum> {
    Ok(DbAlbum {
        id: row.get("album_id")?,
        title: row.get("album_title")?,
        artist_id: row.get("album_artist_id")?,
        year: row.get("album_year")?,
        primary_release_id: row.get("album_primary_release_id")?,
        is_compilation: row.get("album_is_compilation")?,
        created_at: rfc3339_column(row, "album_created_at")?,
    })
}

pub(super) fn row_to_joined_track(row: &Row) -> coven::rusqlite::Result<DbTrack> {
    Ok(DbTrack {
        id: row.get("track_id")?,
        release_id: row.get("track_release_id")?,
        title: row.get("track_title")?,
        side: row.get("track_side")?,
        track_number: row.get("track_track_number")?,
        duration_ms: row.get("track_duration_ms")?,
        discogs_position: row.get("track_discogs_position")?,
        created_at: rfc3339_column(row, "track_created_at")?,
    })
}

pub(super) fn row_to_release(row: &Row) -> coven::rusqlite::Result<DbRelease> {
    let metadata_source: String = row.get("metadata_source")?;
    let metadata_source_release_id: Option<String> = row.get("metadata_source_release_id")?;
    let metadata_provenance = match (metadata_source.as_str(), metadata_source_release_id) {
        ("none", None) => None,
        ("file_tags", None) => Some(crate::import::MetadataProvenance::FileTags),
        (source, Some(release_id)) => {
            let source = source.parse::<MetadataSource>().map_err(|e| {
                column_conversion_error(row, "metadata_source", format!("releases.{e}"))
            })?;
            Some(crate::import::MetadataProvenance::ExternalRelease { source, release_id })
        }
        (source, release_id) => {
            return Err(column_conversion_error(
                row,
                "metadata_source",
                format!(
                    "invalid releases metadata provenance columns: source={source:?}, release_id={release_id:?}"
                ),
            ));
        }
    };
    Ok(DbRelease {
        id: row.get("id")?,
        album_id: row.get("album_id")?,
        release_name: row.get("release_name")?,
        pressing: Pressing {
            year: row.get("year")?,
            format: row.get("format")?,
            label: row.get("label")?,
            catalog_number: row.get("catalog_number")?,
            country: row.get("country")?,
            barcode: row.get("barcode")?,
        },
        disc_id: row.get("disc_id")?,
        metadata_provenance,
        remote: row.get("remote")?,
        source_folder_name: row.get("source_folder_name")?,
        content_hash: row.get("content_hash")?,
        album_loudness_lufs: row.get("album_loudness_lufs")?,
        album_peak_linear: row.get("album_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_file(row: &Row) -> coven::rusqlite::Result<DbFile> {
    let layout = row
        .get::<_, Option<String>>("source_audio_layout")?
        .map(|layout| match layout.as_str() {
            "file" => Ok(crate::album_detail::SourceAudioLayout::File),
            "cue" => Ok(crate::album_detail::SourceAudioLayout::Cue),
            other => Err(coven::rusqlite::Error::FromSqlConversionFailure(
                0,
                coven::rusqlite::types::Type::Text,
                format!("invalid source_audio_layout {other:?}").into(),
            )),
        })
        .transpose()?;
    let source_audio = match (
        row.get::<_, Option<String>>("source_audio_content_type")?,
        row.get::<_, Option<i64>>("source_audio_duration_ms")?,
        row.get::<_, Option<i64>>("source_audio_sample_rate_hz")?,
        row.get::<_, Option<i64>>("source_audio_bits_per_sample")?,
        row.get::<_, Option<i64>>("source_audio_bitrate_kbps")?,
        row.get::<_, Option<i64>>("source_audio_channels")?,
    ) {
        (None, None, None, None, None, None) if layout.is_none() => None,
        (
            Some(content_type),
            Some(duration_ms),
            Some(sample_rate_hz),
            bits_per_sample,
            bitrate_kbps,
            Some(channels),
        ) => Some(crate::album_detail::SourceAudioFile {
            layout,
            content_type: ContentType::from_mime(&content_type),
            duration_ms,
            format: crate::album_detail::AudioFormat {
                codec: ContentType::from_mime(&content_type)
                    .display_name()
                    .to_string(),
                sample_rate_hz,
                bits_per_sample,
                bitrate_kbps,
                channels,
            },
        }),
        columns => {
            return Err(coven::rusqlite::Error::FromSqlConversionFailure(
                0,
                coven::rusqlite::types::Type::Text,
                format!("inconsistent release source-audio facts: {columns:?}").into(),
            ))
        }
    };
    Ok(DbFile {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        original_filename: row.get("original_filename")?,
        file_size: row.get("file_size")?,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        source_audio,
        cloud_path: row.get("cloud_path")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_artist(row: &Row) -> coven::rusqlite::Result<DbArtist> {
    Ok(DbArtist {
        id: row.get("id")?,
        name: row.get("name")?,
        sort_name: row.get("sort_name")?,
        discogs_artist_id: row.get("discogs_artist_id")?,
        musicbrainz_artist_id: row.get("musicbrainz_artist_id")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_album(row: &Row) -> coven::rusqlite::Result<DbAlbum> {
    Ok(DbAlbum {
        id: row.get("id")?,
        title: row.get("title")?,
        artist_id: row.get("artist_id")?,
        year: row.get("year")?,
        primary_release_id: row.get("primary_release_id")?,
        is_compilation: row.get("is_compilation")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_track(row: &Row) -> coven::rusqlite::Result<DbTrack> {
    Ok(DbTrack {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        title: row.get("title")?,
        side: row.get("side")?,
        track_number: row.get("track_number")?,
        duration_ms: row.get("duration_ms")?,
        discogs_position: row.get("discogs_position")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_audio_format(row: &Row) -> coven::rusqlite::Result<DbAudioFormat> {
    Ok(DbAudioFormat {
        id: row.get("id")?,
        track_id: row.get("track_id")?,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        pregap_ms: row.get("pregap_ms")?,
        generated_pregap_ms: row.get("generated_pregap_ms")?,
        pregap_samples: row.get("pregap_samples")?,
        generated_pregap_samples: row.get("generated_pregap_samples")?,
        sample_rate: row.get("sample_rate")?,
        bits_per_sample: row.get("bits_per_sample")?,
        channels: row.get("channels")?,
        track_loudness_lufs: row.get("track_loudness_lufs")?,
        track_peak_linear: row.get("track_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_audio_segment(row: &Row) -> coven::rusqlite::Result<DbAudioSegment> {
    let role_text: String = row.get("role")?;
    let role = DbAudioSegmentRole::from_db_value(&role_text).ok_or_else(|| {
        coven::rusqlite::Error::FromSqlConversionFailure(
            0,
            coven::rusqlite::types::Type::Text,
            format!("unknown audio segment role: {role_text}").into(),
        )
    })?;
    Ok(DbAudioSegment {
        id: row.get("id")?,
        audio_format_id: row.get("audio_format_id")?,
        segment_index: row.get("segment_index")?,
        role,
        file_id: row.get("file_id")?,
        start_sample: row.get("start_sample")?,
        end_sample: row.get("end_sample")?,
        start_byte: row.get("start_byte")?,
        end_byte: row.get("end_byte")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_release_storage_summary(
    row: &Row,
) -> coven::rusqlite::Result<DbReleaseStorageSummary> {
    Ok(DbReleaseStorageSummary {
        release_id: row.get("release_id")?,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        artist_names: row.get("artist_names")?,
        format: row.get("format")?,
        remote: row.get("remote")?,
        any_file_id: row.get("any_file_id")?,
        file_count: row.get("file_count")?,
        total_size: row.get("total_size")?,
    })
}

// ─── Synced-row INSERT helpers. Run inside `call_sql` — they take its
// `_updated_at` stamp — against a `&Connection` or a `&Transaction`, both of
// which deref to `&Connection`. ─────────────────────────────────────────────

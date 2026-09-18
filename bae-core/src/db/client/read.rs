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
) -> Result<ReleaseDetailRows, DbError> {
    let tracks = get_tracks_with_artists_for_release_on(sql, &release.id)?;
    let files = get_files_for_release_on(sql, &release.id)?;
    let audio_formats = get_audio_formats_for_release_on(sql, &release.id)?;
    let audio_segments = get_audio_segments_for_release_on(sql, &release.id)?;
    let records = get_release_records_on(sql, &release.id)?;
    let marks = get_release_marks_on(sql, &release.id)?;

    Ok(ReleaseDetailRows {
        release,
        tracks,
        files,
        audio_formats,
        audio_segments,
        records,
        marks,
    })
}

pub(super) struct ReleaseDetailRows {
    pub(super) release: DbRelease,
    tracks: Vec<(DbTrack, Option<DbArtist>)>,
    files: Vec<DbFile>,
    audio_formats: Vec<DbAudioFormat>,
    audio_segments: Vec<DbAudioSegment>,
    records: Vec<crate::import::ReleaseRecord>,
    marks: Vec<crate::import::ReleaseMark>,
}

impl ReleaseDetailRows {
    pub(super) fn process(self) -> DbReleaseDetail {
        DbReleaseDetail {
            release: self.release,
            tracks: process_tracks(self.tracks),
            files: process_files(self.files),
            audio_formats: self.audio_formats,
            audio_segments: self.audio_segments,
            records: self.records,
            marks: self.marks,
        }
    }
}

/// One row per (track, artist) pair, so a track with several artists repeats.
/// The rows arrive grouped by track and ordered by artist position, and the fold
/// below rebuilds one entry per track from that run.
pub(super) fn get_tracks_with_artists_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<(DbTrack, Option<DbArtist>)>, DbError> {
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
            let track = row_to_track_with_prefix(row, "track_")?;
            let artist_id: Option<String> = row.get("artist_id")?;
            let artist = match artist_id {
                Some(_) => Some(row_to_artist_with_prefix(row, "artist_")?),
                None => None,
            };
            Ok((track, artist))
        },
    )?;

    Ok(joined)
}

fn process_tracks(joined: Vec<(DbTrack, Option<DbArtist>)>) -> Vec<DbTrackWithArtists> {
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

    tracks
}

pub(super) fn get_files_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbFile>, DbError> {
    let files = sql.query(
        "SELECT * FROM release_files WHERE release_id = ?",
        params![release_id],
        row_to_file,
    )?;
    Ok(files)
}

pub(super) fn process_files(mut files: Vec<DbFile>) -> Vec<DbFile> {
    // Every file list a user sees (detail, gallery, storage, export) derives
    // from this read, so it is ordered here once, the same way the import
    // folder lists its files: natural order, case-insensitive. Id breaks
    // exact ties so the order is stable.
    files.sort_by(|a, b| {
        natord::compare_ignore_case(&a.original_filename, &b.original_filename)
            .then_with(|| a.id.cmp(&b.id))
    });
    files
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

/// Every catalog's description of a release, in the order surfaces list
/// catalogs.
pub(super) fn get_release_records_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<crate::import::ReleaseRecord>, DbError> {
    let mut records = sql.query(
        r#"
            SELECT catalog, key, group_key, url, reads_draft
            FROM release_records
            WHERE release_id = ?
            "#,
        params![release_id],
        |row| {
            Ok(crate::import::ReleaseRecord {
                catalog: parsed_column(row, "catalog")?,
                key: row.get("key")?,
                group_key: row.get("group_key")?,
                url: row.get("url")?,
                reads_draft: row.get("reads_draft")?,
            })
        },
    )?;
    records.sort_by_key(|record| {
        crate::import::Catalog::ALL
            .iter()
            .position(|catalog| *catalog == record.catalog)
            .expect("every stored catalog is one of the catalogs")
    });
    Ok(records)
}

/// Every name read off a release's own object, one row per sighting, in the
/// order extraction read them.
pub(super) fn get_release_marks_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<crate::import::ReleaseMark>, DbError> {
    sql.query(
        r#"
            SELECT kind, value, origin, origin_path,
                   region_x, region_y, region_width, region_height
            FROM release_marks
            WHERE release_id = ?
            ORDER BY position
            "#,
        params![release_id],
        |row| {
            let value: String = row.get("value")?;
            let region = stored_region(
                &value,
                [
                    row.get("region_x")?,
                    row.get("region_y")?,
                    row.get("region_width")?,
                    row.get("region_height")?,
                ],
            )
            .map_err(|e| column_conversion_error(row, "region_x", e.to_string()))?;
            Ok(crate::import::ReleaseMark {
                kind: parsed_column(row, "kind")?,
                sighting: crate::signals::SourcedValue {
                    value,
                    origin: parsed_column(row, "origin")?,
                    origin_path: row.get("origin_path")?,
                    region,
                },
            })
        },
    )
    .map_err(DbError::from)
}

/// The region a row stores, as the four columns every table that stores one
/// uses: all present and inside the image, or all absent. Anything else is a
/// row nothing here wrote.
pub(super) fn stored_region(
    value: &str,
    columns: [Option<f64>; 4],
) -> Result<Option<crate::signals::ImageRegion>, DbError> {
    match columns {
        [None, None, None, None] => Ok(None),
        [Some(x), Some(y), Some(width), Some(height)] => {
            crate::signals::ImageRegion::new(x as f32, y as f32, width as f32, height as f32)
                .map(Some)
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "the stored value {value:?} states a region outside its image"
                    ))
                })
        }
        _ => Err(DbError::Message(format!(
            "the stored value {value:?} states a partial region"
        ))),
    }
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

/// Read a named text column back into the type whose `FromStr` it was written
/// from, surfacing a word no variant answers to as a column-conversion error.
pub(super) fn parsed_column<T>(row: &Row, column: &str) -> coven::rusqlite::Result<T>
where
    T: std::str::FromStr<Err = String>,
{
    let raw: String = row.get(column)?;
    raw.parse::<T>()
        .map_err(|e| column_conversion_error(row, column, e))
}

// ─── Entity row readers ──────────────────────────────────────────────────────
//
// A query that joins several entity tables aliases each table's columns behind
// a per-entity prefix (`artist_name`, `album_title`); a query that selects one
// table leaves them bare. Same read either way, so each entity has one reader
// taking that prefix, plus a bare-column wrapper — which is also the `fn(&Row)`
// value the single-table queries hand to `query` / `query_row`.

pub(super) fn row_to_artist_with_prefix(
    row: &Row,
    prefix: &str,
) -> coven::rusqlite::Result<DbArtist> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbArtist {
        id: row.get(column("id").as_str())?,
        name: row.get(column("name").as_str())?,
        sort_name: row.get(column("sort_name").as_str())?,
        discogs_artist_id: row.get(column("discogs_artist_id").as_str())?,
        musicbrainz_artist_id: row.get(column("musicbrainz_artist_id").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

pub(super) fn row_to_album_with_prefix(
    row: &Row,
    prefix: &str,
) -> coven::rusqlite::Result<DbAlbum> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbAlbum {
        id: row.get(column("id").as_str())?,
        title: row.get(column("title").as_str())?,
        artist_id: row.get(column("artist_id").as_str())?,
        year: row.get(column("year").as_str())?,
        primary_release_id: row.get(column("primary_release_id").as_str())?,
        is_compilation: row.get(column("is_compilation").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

pub(super) fn row_to_track_with_prefix(
    row: &Row,
    prefix: &str,
) -> coven::rusqlite::Result<DbTrack> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbTrack {
        id: row.get(column("id").as_str())?,
        release_id: row.get(column("release_id").as_str())?,
        title: row.get(column("title").as_str())?,
        side: row.get(column("side").as_str())?,
        track_number: row.get(column("track_number").as_str())?,
        duration_ms: row.get(column("duration_ms").as_str())?,
        discogs_position: row.get(column("discogs_position").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

/// The origin columns beside a release's eight album-level values, read in the
/// order the fields are listed.
fn row_to_field_origins(row: &Row) -> coven::rusqlite::Result<crate::import::FieldOrigins> {
    const COLUMNS: [&str; 8] = [
        "album_title_origin",
        "album_year_origin",
        "year_origin",
        "format_origin",
        "label_origin",
        "catalog_number_origin",
        "country_origin",
        "barcode_origin",
    ];
    let mut origins = crate::import::FieldOrigins::default();
    for (field, column) in crate::import::CandidateEditField::ALL
        .into_iter()
        .zip(COLUMNS)
    {
        let Some(stored) = row.get::<_, Option<String>>(column)? else {
            continue;
        };
        let origin = stored.parse().map_err(|error: String| {
            coven::rusqlite::Error::FromSqlConversionFailure(
                0,
                coven::rusqlite::types::Type::Text,
                error.into(),
            )
        })?;
        origins.set(field, Some(origin));
    }
    Ok(origins)
}

pub(super) fn row_to_release(row: &Row) -> coven::rusqlite::Result<DbRelease> {
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
        draft_from_tags: row.get("draft_from_tags")?,
        field_origins: row_to_field_origins(row)?,
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
    row_to_artist_with_prefix(row, "")
}

pub(super) fn row_to_album(row: &Row) -> coven::rusqlite::Result<DbAlbum> {
    row_to_album_with_prefix(row, "")
}

pub(super) fn row_to_track(row: &Row) -> coven::rusqlite::Result<DbTrack> {
    row_to_track_with_prefix(row, "")
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

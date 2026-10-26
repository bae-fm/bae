use super::*;

pub(super) fn find_album_by_id_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Option<DbAlbum>, DbError> {
    sql.query_row(
        r#"
            SELECT
                id, title, year,
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
) -> Result<Vec<LibraryCredit>, DbError> {
    // Each credit in list order, showing the artist it resolves to after
    // merges; a list naming one artist twice gives it twice.
    sql.query(
        &format!(
            r#"
            SELECT {columns} FROM album_artists credit
            JOIN artists artist ON artist.id = {shown}
            LEFT JOIN artist_credits credit_name
                ON credit_name.artist_id = credit.artist_id
               AND credit_name.name_key = credit.credit_key
            WHERE credit.album_id = ?1
            ORDER BY credit.position
            "#,
            columns = LIBRARY_CREDIT_COLUMNS,
            shown = shown_artist_id("credit.artist_id"),
        ),
        params![album_id],
        row_to_library_credit,
    )
    .map_err(DbError::from)
}

/// What [`row_to_library_credit`] reads, from a query joining the credit row
/// as `credit`, its shown artist as `artist` and its credit name, if any, as
/// `credit_name`.
pub(super) const LIBRARY_CREDIT_COLUMNS: &str = "artist.id AS artist_id, \
     artist.name AS artist_name, artist.sort_name AS artist_sort_name, \
     artist.discogs_artist_id AS artist_discogs_artist_id, \
     artist.musicbrainz_artist_id AS artist_musicbrainz_artist_id, \
     artist.created_at AS artist_created_at, \
     credit_name.name_key AS credit_key, credit_name.name AS credit_name, \
     credit.join_phrase AS credit_join";

pub(super) fn row_to_library_credit(row: &Row) -> rusqlite::Result<LibraryCredit> {
    Ok(LibraryCredit {
        artist: row_to_artist_with_prefix(row, "artist_")?,
        credit: match row.get::<_, Option<String>>("credit_key")? {
            Some(key) => Some(crate::credits::CreditName {
                key,
                name: row.get("credit_name")?,
            }),
            None => None,
        },
        join: row.get("credit_join")?,
    })
}

/// One credit as [`super::query::album_credits_json_sql`] spells it.
#[derive(serde::Deserialize)]
struct CreditJson {
    id: String,
    name: String,
    sort_name: Option<String>,
    discogs_artist_id: Option<String>,
    musicbrainz_artist_id: Option<String>,
    created_at: String,
    credit_key: Option<String>,
    credit_name: Option<String>,
    join: Option<String>,
}

/// The credits a JSON column of [`super::query::album_credits_json_sql`] or
/// [`super::query::track_credits_json_sql`] holds, in list order.
pub(super) fn credits_from_json(json: &str) -> Result<Vec<LibraryCredit>, DbError> {
    let credits: Vec<CreditJson> = serde_json::from_str(json)
        .map_err(|error| DbError::Message(format!("credit list {json:?}: {error}")))?;
    credits
        .into_iter()
        .map(|credit| {
            let created_at = DateTime::parse_from_rfc3339(&credit.created_at)
                .map_err(|error| {
                    DbError::Message(format!(
                        "artist {} created_at {:?}: {error}",
                        credit.id, credit.created_at
                    ))
                })?
                .with_timezone(&Utc);
            Ok(LibraryCredit {
                artist: DbArtist {
                    id: credit.id,
                    name: credit.name,
                    sort_name: credit.sort_name,
                    discogs_artist_id: credit.discogs_artist_id,
                    musicbrainz_artist_id: credit.musicbrainz_artist_id,
                    created_at,
                },
                credit: match (credit.credit_key, credit.credit_name) {
                    (Some(key), Some(name)) => Some(crate::credits::CreditName { key, name }),
                    _ => None,
                },
                join: credit.join,
            })
        })
        .collect()
}

/// The line a credit-list JSON column prints, read in a row closure.
pub(super) fn credit_line_column(row: &Row, column: &str) -> rusqlite::Result<String> {
    let json: String = row.get(column)?;
    credits_from_json(&json)
        .map(|credits| crate::credits::library_line(&credits))
        .map_err(|error| column_conversion_error(row, column, error.to_string()))
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
        "SELECT * FROM releases WHERE album_id = ? ORDER BY album_order, id",
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
    let unstored_audio = unstored_audio_on(sql, &release.id)?;
    let mediums = release_mediums_on(sql, &release.id)?;

    Ok(ReleaseDetailRows {
        release,
        mediums,
        tracks,
        files,
        audio_formats,
        audio_segments,
        records,
        unstored_audio,
    })
}

/// What the library records this device holding for each of a release's audio
/// files whose row names no stored file: no original (another device imported
/// it), or the original's path with the size and modification time recorded at
/// import, uploading or not. The disk is not read here: a read holds one of the
/// database's readers, and the disk an original sits on can be slow to answer.
pub(super) fn unstored_audio_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbUnstoredAudio>, DbError> {
    let rows: Vec<(String, i64, Option<String>, Option<String>)> = sql.query(
        "SELECT f.content_type, f.file_size, l.path, l.modified_at \
         FROM release_files f LEFT JOIN local_release_files l ON l.file_id = f.id \
         WHERE f.release_id = ? AND f.hash IS NULL",
        params![release_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    rows.into_iter()
        .filter(|(content_type, ..)| ContentType::from_mime(content_type).is_audio())
        .map(|(_, size, path, modified_at)| {
            let (Some(path), Some(modified_at)) = (path, modified_at) else {
                return Ok(DbUnstoredAudio::Elsewhere);
            };
            let modified_at = DateTime::parse_from_rfc3339(&modified_at).map_err(|error| {
                DbError::Message(format!("invalid original modification time: {error}"))
            })?;
            Ok(DbUnstoredAudio::Original(RecordedOriginal {
                path: path.into(),
                size,
                modified_at: modified_at.into(),
            }))
        })
        .collect()
}

/// [`unstored_audio_on`] for a row mapper reading a release's summary, its
/// failure carried as the mapper's error.
pub(super) fn unstored_audio_in_row(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> rusqlite::Result<Vec<DbUnstoredAudio>> {
    unstored_audio_on(sql, release_id)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
}

#[derive(Clone, PartialEq)]
pub(super) struct ReleaseDetailRows {
    pub(super) release: DbRelease,
    mediums: DbReleaseMediums,
    tracks: Vec<(DbTrack, Option<LibraryCredit>)>,
    files: Vec<DbFile>,
    audio_formats: Vec<DbAudioFormat>,
    audio_segments: Vec<DbAudioSegment>,
    records: Vec<crate::import::ReleaseRecord>,
    unstored_audio: Vec<DbUnstoredAudio>,
}

impl ReleaseDetailRows {
    pub(super) fn process(self) -> DbReleaseDetail {
        DbReleaseDetail {
            release: self.release,
            mediums: self.mediums,
            tracks: process_tracks(self.tracks),
            files: process_files(self.files),
            audio_formats: self.audio_formats,
            audio_segments: self.audio_segments,
            records: self.records,
            unstored_audio: self.unstored_audio,
        }
    }
}

/// One row per (track, artist) pair, so a track with several artists repeats.
/// The rows arrive grouped by track and ordered by artist position, and the fold
/// below rebuilds one entry per track from that run.
pub(super) fn get_tracks_with_artists_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<(DbTrack, Option<LibraryCredit>)>, DbError> {
    let query = format!(
        "SELECT
            track.id AS track_id,
            track.release_id AS track_release_id,
            track.title AS track_title,
            track.medium_id AS track_medium_id,
            track.side AS track_side,
            track.track_number AS track_track_number,
            track.duration_ms AS track_duration_ms,
            track.created_at AS track_created_at,
            {LIBRARY_CREDIT_COLUMNS}
         FROM tracks track
         LEFT JOIN track_artists credit ON credit.track_id = track.id
         LEFT JOIN artists artist ON artist.id = {}
         LEFT JOIN artist_credits credit_name
             ON credit_name.artist_id = credit.artist_id
            AND credit_name.name_key = credit.credit_key
         WHERE track.release_id = ?
         ORDER BY track.position, track.id, credit.position",
        shown_artist_id("credit.artist_id")
    );
    let joined = sql.query(&query, params![release_id], |row| {
        let track = row_to_track_with_prefix(row, "track_")?;
        let artist_id: Option<String> = row.get("artist_id")?;
        let credit = match artist_id {
            Some(_) => Some(row_to_library_credit(row)?),
            None => None,
        };
        Ok((track, credit))
    })?;

    Ok(joined)
}

fn process_tracks(joined: Vec<(DbTrack, Option<LibraryCredit>)>) -> Vec<DbTrackWithArtists> {
    let mut tracks: Vec<DbTrackWithArtists> = Vec::new();
    for (track, artist) in joined {
        if tracks.last().map(|last| last.track.id.as_str()) != Some(track.id.as_str()) {
            tracks.push(DbTrackWithArtists {
                track,
                artists: Vec::new(),
            });
        }
        if let Some(credit) = artist {
            tracks
                .last_mut()
                .expect("the row's track was just pushed")
                .artists
                .push(credit);
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
            SELECT rr.catalog, rr.kind, rr.key, rr.album_key
            FROM release_records rr
            WHERE rr.release_id = ?
            "#,
        params![release_id],
        |row| {
            let identity = crate::import::MetadataRef::new(
                parsed_column(row, "catalog")?,
                row.get::<_, String>("key")?,
            );
            match row.get::<_, String>("kind")?.as_str() {
                "pressing" => Ok(crate::import::ReleaseRecord::new(
                    &identity,
                    row.get("album_key")?,
                )),
                "album" => Ok(crate::import::ReleaseRecord::album(&identity)),
                kind => Err(rusqlite::Error::FromSqlConversionFailure(
                    row.as_ref().column_index("kind")?,
                    rusqlite::types::Type::Text,
                    format!("unknown record kind: {kind}").into(),
                )),
            }
        },
    )?;
    records.sort_by_key(|record| {
        crate::catalog::Catalog::ALL
            .iter()
            .position(|catalog| *catalog == record.catalog())
            .expect("every stored catalog is one of the catalogs")
    });
    Ok(records)
}

/// Build a column-conversion error for a named column whose stored text the
/// mapper could not turn into its typed value, so a corrupt column surfaces like
/// any other bad read instead of panicking or silently mis-defaulting.
pub(super) fn column_conversion_error(row: &Row, column: &str, message: String) -> rusqlite::Error {
    // The column was just read, so its index resolves; if it somehow doesn't,
    // that lookup error is itself a faithful failure to return.
    match row.as_ref().column_index(column) {
        Ok(idx) => rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            message.into(),
        ),
        Err(e) => e,
    }
}

/// Read a named rfc3339 timestamp column, surfacing a malformed value as a
/// column-conversion error rather than panicking on the parse.
pub(super) fn rfc3339_column(row: &Row, column: &str) -> rusqlite::Result<DateTime<Utc>> {
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

/// Read a named integer column that holds a count or a position, which the
/// schema keeps non-negative, surfacing a negative one as a column-conversion
/// error.
pub(super) fn count_column(row: &Row, column: &str) -> rusqlite::Result<u64> {
    let raw: i64 = row.get(column)?;
    u64::try_from(raw).map_err(|error| {
        column_conversion_error(row, column, format!("{column} {raw} is negative: {error}"))
    })
}

/// [`count_column`] for a nullable column.
pub(super) fn optional_count_column(row: &Row, column: &str) -> rusqlite::Result<Option<u64>> {
    match row.get::<_, Option<i64>>(column)? {
        None => Ok(None),
        Some(_) => count_column(row, column).map(Some),
    }
}

/// Read a named text column back into the type whose `FromStr` it was written
/// from, surfacing a word no variant answers to as a column-conversion error.
pub(super) fn parsed_column<T>(row: &Row, column: &str) -> rusqlite::Result<T>
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

pub(super) fn row_to_artist_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbArtist> {
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

pub(super) fn row_to_album_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbAlbum> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbAlbum {
        id: row.get(column("id").as_str())?,
        title: row.get(column("title").as_str())?,
        year: row.get(column("year").as_str())?,
        is_compilation: row.get(column("is_compilation").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

pub(super) fn row_to_track_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbTrack> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbTrack {
        id: row.get(column("id").as_str())?,
        release_id: row.get(column("release_id").as_str())?,
        title: row.get(column("title").as_str())?,
        medium_id: row.get(column("medium_id").as_str())?,
        side: row.get(column("side").as_str())?,
        track_number: row.get(column("track_number").as_str())?,
        duration_ms: row.get(column("duration_ms").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

pub(super) fn row_to_release_medium(row: &Row) -> rusqlite::Result<DbReleaseMedium> {
    let medium: Option<String> = row.get("medium")?;
    let carrier = medium
        .map(|key| {
            crate::pressing::Medium::from_key(&key).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    format!("unknown release medium {key:?}").into(),
                )
            })
        })
        .transpose()?;
    Ok(DbReleaseMedium {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        position: row.get("position")?,
        medium: crate::mediums::ReleaseMedium {
            carrier,
            sides: row.get("side_count")?,
        },
        created_at: rfc3339_column(row, "created_at")?,
    })
}

/// A release's mediums, in order.
pub(super) fn release_mediums_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<DbReleaseMediums, DbError> {
    Ok(DbReleaseMediums(sql.query(
        "SELECT * FROM release_mediums WHERE release_id = ? ORDER BY position",
        params![release_id],
        row_to_release_medium,
    )?))
}

pub(super) fn row_to_release(row: &Row) -> rusqlite::Result<DbRelease> {
    Ok(DbRelease {
        id: row.get("id")?,
        album_id: row.get("album_id")?,
        release_name: row.get("release_name")?,
        pressing: crate::pressing::Pressing {
            year: row.get("year")?,
            labels: super::pressing_columns::read_labels(row, "labels")?,
            barcodes: super::pressing_columns::read_barcodes(row, "barcodes")?,
            facts: super::pressing_columns::read_facts(row, "")?,
        },
        source_folder_name: row.get("source_folder_name")?,
        album_loudness_lufs: row.get("album_loudness_lufs")?,
        album_peak_linear: row.get("album_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_file(row: &Row) -> rusqlite::Result<DbFile> {
    let layout = row
        .get::<_, Option<String>>("source_audio_layout")?
        .map(|layout| match layout.as_str() {
            "file" => Ok(crate::album_detail::SourceAudioLayout::File),
            "cue" => Ok(crate::album_detail::SourceAudioLayout::Cue),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid source_audio_layout {other:?}").into(),
            )),
        })
        .transpose()?;
    let source_audio = match (
        row.get::<_, Option<String>>("source_audio_content_type")?,
        row.get::<_, Option<u64>>("source_audio_duration_ms")?,
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
            return Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
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
        stored: row.get::<_, Option<Vec<u8>>>("hash")?.is_some(),
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_artist(row: &Row) -> rusqlite::Result<DbArtist> {
    row_to_artist_with_prefix(row, "")
}

pub(super) fn row_to_album(row: &Row) -> rusqlite::Result<DbAlbum> {
    row_to_album_with_prefix(row, "")
}

pub(super) fn row_to_track(row: &Row) -> rusqlite::Result<DbTrack> {
    row_to_track_with_prefix(row, "")
}

/// A track's stored audio value that must be above zero and fit a `u32`: its
/// sample rate or channel count. A value that doesn't is an error naming the
/// track, never a number wrapped into a different one.
fn track_audio_count_column(
    row: &Row,
    column: &str,
    track_id: &str,
) -> rusqlite::Result<std::num::NonZeroU32> {
    let raw: i64 = row.get(column)?;
    u32::try_from(raw)
        .ok()
        .and_then(std::num::NonZeroU32::new)
        .ok_or_else(|| {
            column_conversion_error(
                row,
                column,
                format!("track {track_id} has {column} {raw}, not a count above zero"),
            )
        })
}

/// A track's stored bit depth: absent for lossy audio, else 1 to 32.
fn track_bit_depth_column(
    row: &Row,
    track_id: &str,
) -> rusqlite::Result<Option<std::num::NonZeroU32>> {
    const COLUMN: &str = "bits_per_sample";
    let Some(raw) = row.get::<_, Option<i64>>(COLUMN)? else {
        return Ok(None);
    };
    u32::try_from(raw)
        .ok()
        .filter(|bits| (1..=32).contains(bits))
        .and_then(std::num::NonZeroU32::new)
        .map(Some)
        .ok_or_else(|| {
            column_conversion_error(
                row,
                COLUMN,
                format!("track {track_id} has {COLUMN} {raw}, not a depth of 1 to 32"),
            )
        })
}

pub(super) fn row_to_audio_format(row: &Row) -> rusqlite::Result<DbAudioFormat> {
    let track_id: String = row.get("track_id")?;
    Ok(DbAudioFormat {
        id: row.get("id")?,
        content_type: ContentType::from_mime(&row.get::<_, String>("content_type")?),
        pregap_samples: optional_count_column(row, "pregap_samples")?,
        generated_pregap_samples: optional_count_column(row, "generated_pregap_samples")?,
        sample_rate: track_audio_count_column(row, "sample_rate", &track_id)?,
        bits_per_sample: track_bit_depth_column(row, &track_id)?,
        channels: track_audio_count_column(row, "channels", &track_id)?,
        track_loudness_lufs: row.get("track_loudness_lufs")?,
        track_peak_linear: row.get("track_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
        track_id,
    })
}

pub(super) fn row_to_audio_segment(row: &Row) -> rusqlite::Result<DbAudioSegment> {
    let role_text: String = row.get("role")?;
    let role = DbAudioSegmentRole::from_db_value(&role_text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown audio segment role: {role_text}").into(),
        )
    })?;
    Ok(DbAudioSegment {
        id: row.get("id")?,
        audio_format_id: row.get("audio_format_id")?,
        segment_index: row.get("segment_index")?,
        role,
        file_id: row.get("file_id")?,
        start_sample: count_column(row, "start_sample")?,
        end_sample: optional_count_column(row, "end_sample")?,
        start_byte: optional_count_column(row, "start_byte")?,
        end_byte: optional_count_column(row, "end_byte")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub(super) fn row_to_scrub_point(row: &Row) -> rusqlite::Result<DbScrubPoint> {
    Ok(DbScrubPoint {
        audio_segment_id: row.get("audio_format_segment_id")?,
        point: ScrubPoint {
            sample: count_column(row, "sample")?,
            byte: count_column(row, "byte")?,
        },
    })
}

pub(super) fn row_to_release_storage_summary(
    sql: &SqlReadContext<'_>,
    row: &Row,
) -> rusqlite::Result<DbReleaseStorageSummary> {
    let release_id: String = row.get("release_id")?;
    Ok(DbReleaseStorageSummary {
        unstored_audio: unstored_audio_in_row(sql, &release_id)?,
        release_id,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        artist_names: credit_line_column(row, "artist_names")?,
        media: super::pressing_columns::read_media(row, "media")?,
        remote: row.get("remote")?,
        any_file_id: row.get("any_file_id")?,
        file_count: row.get("file_count")?,
        total_size: row.get("total_size")?,
    })
}

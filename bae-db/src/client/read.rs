use super::*;

pub fn find_album_by_id_on(
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

pub fn get_artists_for_album_on(
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
pub const LIBRARY_CREDIT_COLUMNS: &str = "artist.id AS artist_id, \
     artist.name AS artist_name, artist.sort_name AS artist_sort_name, \
     artist.discogs_artist_id AS artist_discogs_artist_id, \
     artist.musicbrainz_artist_id AS artist_musicbrainz_artist_id, \
     artist.created_at AS artist_created_at, \
     credit_name.name_key AS credit_key, credit_name.name AS credit_name, \
     credit.join_phrase AS credit_join";

pub fn row_to_library_credit(row: &Row) -> rusqlite::Result<LibraryCredit> {
    Ok(LibraryCredit {
        artist: row_to_artist_with_prefix(row, "artist_")?,
        credit: match row.get::<_, Option<String>>("credit_key")? {
            Some(key) => Some(crate::credits::CreditName { key, name: row.get("credit_name")? }),
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

/// The credits a credit-list JSON column holds, read in a row closure.
pub(super) fn credits_column(row: &Row, column: &str) -> rusqlite::Result<Vec<LibraryCredit>> {
    let json: String = row.get(column)?;
    credits_from_json(&json)
        .map_err(|error| column_conversion_error(row, column, error.to_string()))
}

/// The line a credit-list JSON column prints, read in a row closure.
pub fn credit_line_column(row: &Row, column: &str) -> rusqlite::Result<String> {
    let json: String = row.get(column)?;
    credits_from_json(&json)
        .map(|credits| crate::credits::library_line(&credits))
        .map_err(|error| column_conversion_error(row, column, error.to_string()))
}

pub fn find_release_by_id_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DbRelease>, DbError> {
    sql.query_row(&release_rows_sql("r.id = ?"), params![release_id], row_to_release)
        .optional()
        .map_err(DbError::from)
}

pub fn get_releases_for_album_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Vec<DbRelease>, DbError> {
    sql.query(
        &release_rows_sql("r.album_id = ? ORDER BY r.album_order, r.id"),
        params![album_id],
        row_to_release,
    )
    .map_err(DbError::from)
}

/// A release's files in natural order of their names, case aside, the way
/// the import folder lists them, id breaking a tie: the order every list of
/// a release's files shows (detail, gallery, storage, export), read from the
/// index of each release's files in that order.
pub(super) const RELEASE_FILES_IN_ORDER: &str = concat!(
    "SELECT ",
    release_file_columns!(),
    " FROM release_files AS f WHERE f.release_id = ?1 \
     ORDER BY f.original_filename COLLATE natural_order, f.id"
);

pub fn get_files_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbFile>, DbError> {
    let files = sql.query(RELEASE_FILES_IN_ORDER, params![release_id], row_to_file)?;
    Ok(files)
}

/// Every catalog's description of a release, in the order surfaces list
/// catalogs (`bae_catalog_rank`), read from the index of each release's
/// records in that order.
pub(super) const RELEASE_RECORDS_IN_ORDER: &str =
    "SELECT rr.catalog, rr.kind, rr.key, rr.album_key \
     FROM release_records rr \
     WHERE rr.release_id = ?1 \
     ORDER BY bae_catalog_rank(rr.catalog)";

/// Every catalog's description of a release, in the order surfaces list
/// catalogs.
pub fn get_release_records_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, DbError> {
    Ok(sql.query(RELEASE_RECORDS_IN_ORDER, params![release_id], |row| {
        let identity = bae_catalogs::records::MetadataRef::new(
            parsed_column(row, "catalog")?,
            row.get::<_, String>("key")?,
        );
        match row.get::<_, String>("kind")?.as_str() {
            "pressing" => {
                Ok(bae_catalogs::records::ReleaseRecord::new(&identity, row.get("album_key")?))
            }
            "album" => Ok(bae_catalogs::records::ReleaseRecord::album(&identity)),
            kind => {
                Err(column_conversion_error(row, "kind", format!("unknown record kind: {kind}")))
            }
        }
    })?)
}

/// The error of reading column `column` (its name or index) of `row`, whose
/// stored value the mapper could not turn into its typed value, `cause`
/// saying why: it names the column's index and the type its value is stored
/// as, so a corrupt column surfaces like any other bad read instead of
/// panicking or silently mis-defaulting.
pub fn column_conversion_error<I: rusqlite::RowIndex>(
    row: &Row,
    column: I,
    cause: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> rusqlite::Error {
    // The column was just read, so its index resolves; if it somehow doesn't,
    // that lookup error is itself a faithful failure to return.
    let index = match column.idx(row.as_ref()) {
        Ok(index) => index,
        Err(error) => return error,
    };
    match row.get_ref(index) {
        Ok(value) => {
            rusqlite::Error::FromSqlConversionFailure(index, value.data_type(), cause.into())
        }
        Err(error) => error,
    }
}

/// Read column `column` (its name or index) of `row` as the JSON text of a
/// `T`, surfacing text that isn't one as a column-conversion error.
pub fn json_column<T, I>(row: &Row, column: I) -> rusqlite::Result<T>
where
    T: serde::de::DeserializeOwned,
    I: rusqlite::RowIndex + Copy,
{
    let text: String = row.get(column)?;
    serde_json::from_str(&text)
        .map_err(|error| column_conversion_error(row, column, format!("{text:?}: {error}")))
}

/// Read a named rfc3339 timestamp column, surfacing a malformed value as a
/// column-conversion error rather than panicking on the parse.
pub fn rfc3339_column(row: &Row, column: &str) -> rusqlite::Result<DateTime<Utc>> {
    let raw: String = row.get(column)?;
    DateTime::parse_from_rfc3339(&raw).map(|dt| dt.with_timezone(&Utc)).map_err(|e| {
        column_conversion_error(
            row,
            column,
            format!("{column} {raw:?} is not a valid rfc3339 timestamp: {e}"),
        )
    })
}

/// Read a named integer column that holds a count, a size or a position,
/// which the schema keeps non-negative, as the unsigned type `T` it is
/// counted in, surfacing a negative one, or one past `T`, as a
/// column-conversion error.
pub(super) fn count_column<T>(row: &Row, column: &str) -> rusqlite::Result<T>
where
    T: TryFrom<i64, Error = std::num::TryFromIntError>,
{
    let raw: i64 = row.get(column)?;
    T::try_from(raw).map_err(|error| {
        column_conversion_error(row, column, format!("{column} {raw} is not a count: {error}"))
    })
}

/// [`count_column`] for a nullable column.
pub(super) fn optional_count_column<T>(row: &Row, column: &str) -> rusqlite::Result<Option<T>>
where
    T: TryFrom<i64, Error = std::num::TryFromIntError>,
{
    match row.get::<_, Option<i64>>(column)? {
        None => Ok(None),
        Some(_) => count_column(row, column).map(Some),
    }
}

/// Read a named text column back into the type whose `FromStr` it was written
/// from, surfacing a word no variant answers to as a column-conversion error.
pub fn parsed_column<T>(row: &Row, column: &str) -> rusqlite::Result<T>
where
    T: std::str::FromStr<Err = String>,
{
    let raw: String = row.get(column)?;
    raw.parse::<T>().map_err(|e| column_conversion_error(row, column, e))
}

// ─── Entity row readers ──────────────────────────────────────────────────────
//
// A query that joins several entity tables aliases each table's columns behind
// a per-entity prefix (`artist_name`, `album_title`); a query that selects one
// table leaves them bare. Same read either way, so each entity has one reader
// taking that prefix, plus a bare-column wrapper — which is also the `fn(&Row)`
// value the single-table queries hand to `query` / `query_row`.

pub fn row_to_artist_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbArtist> {
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

pub fn row_to_album_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbAlbum> {
    let column = |name: &str| format!("{prefix}{name}");
    Ok(DbAlbum {
        id: row.get(column("id").as_str())?,
        title: row.get(column("title").as_str())?,
        year: row.get(column("year").as_str())?,
        is_compilation: row.get(column("is_compilation").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

pub fn row_to_track_with_prefix(row: &Row, prefix: &str) -> rusqlite::Result<DbTrack> {
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
            bae_types::pressing::Medium::from_key(&key).ok_or_else(|| {
                column_conversion_error(row, "medium", format!("unknown release medium {key:?}"))
            })
        })
        .transpose()?;
    Ok(DbReleaseMedium {
        id: row.get("id")?,
        release_id: row.get("release_id")?,
        position: row.get("position")?,
        medium: bae_types::mediums::ReleaseMedium { carrier, sides: row.get("side_count")? },
        created_at: rfc3339_column(row, "created_at")?,
    })
}

/// A release's mediums, in order.
pub fn release_mediums_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<DbReleaseMediums, DbError> {
    Ok(DbReleaseMediums(sql.query(
        "SELECT * FROM release_mediums WHERE release_id = ? ORDER BY position",
        params![release_id],
        row_to_release_medium,
    )?))
}

/// A `SELECT` of the `releases` rows `condition` names (and orders), each with
/// its labels, its barcodes and its media as one `labels`, one `barcodes` and
/// one `media` column, as [`row_to_release`] reads them.
pub(super) fn release_rows_sql(condition: &str) -> String {
    format!(
        "SELECT r.*, {labels} AS labels, {barcodes} AS barcodes, {media} AS media \
         FROM releases r WHERE {condition}",
        labels = super::pressing_columns::release_labels_sql("r.id"),
        barcodes = super::pressing_columns::release_barcodes_sql("r.id"),
        media = super::pressing_columns::release_media_sql("r.id"),
    )
}

pub(super) fn row_to_release(row: &Row) -> rusqlite::Result<DbRelease> {
    Ok(DbRelease {
        id: row.get("id")?,
        album_id: row.get("album_id")?,
        release_name: row.get("release_name")?,
        pressing: bae_types::pressing::Pressing {
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

/// A stored `source_audio_layout` value, read from `row`'s column `column`.
pub(super) fn source_audio_layout_value<I: rusqlite::RowIndex>(
    row: &Row,
    column: I,
    layout: &str,
) -> rusqlite::Result<crate::source_audio::SourceAudioLayout> {
    match layout {
        "file" => Ok(crate::source_audio::SourceAudioLayout::File),
        "cue" => Ok(crate::source_audio::SourceAudioLayout::Cue),
        other => Err(column_conversion_error(
            row,
            column,
            format!("invalid source_audio_layout {other:?}"),
        )),
    }
}

pub fn row_to_file(row: &Row) -> rusqlite::Result<DbFile> {
    let layout = row
        .get::<_, Option<String>>("source_audio_layout")?
        .map(|layout| source_audio_layout_value(row, "source_audio_layout", &layout))
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
        ) => Some(crate::source_audio::SourceAudioFile {
            layout,
            content_type: ContentType::from_mime(&content_type),
            duration_ms,
            format: bae_types::audio_format::AudioFormat {
                codec: ContentType::from_mime(&content_type).display_name().to_string(),
                sample_rate_hz,
                bits_per_sample,
                bitrate_kbps,
                channels,
            },
        }),
        columns => {
            return Err(column_conversion_error(
                row,
                "source_audio_content_type",
                format!("inconsistent release source-audio facts: {columns:?}"),
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
        file: row.get("file")?,
        stored: row.get("stored")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

pub fn row_to_artist(row: &Row) -> rusqlite::Result<DbArtist> {
    row_to_artist_with_prefix(row, "")
}

pub fn row_to_album(row: &Row) -> rusqlite::Result<DbAlbum> {
    row_to_album_with_prefix(row, "")
}

pub fn row_to_track(row: &Row) -> rusqlite::Result<DbTrack> {
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
    u32::try_from(raw).ok().and_then(std::num::NonZeroU32::new).ok_or_else(|| {
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
        column_conversion_error(row, "role", format!("unknown audio segment role: {role_text}"))
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

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;

use super::*;

pub fn find_album_by_id_on(
    sql: &impl coven::SqlRead,
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
    sql: &impl coven::SqlRead,
    album_id: &str,
) -> Result<Vec<LibraryCredit>, DbError> {
    // Each credit in list order, showing the artist it resolves to after
    // merges; a list naming one artist twice gives it twice.
    sql.query(
        &format!(
            r#"
            SELECT {columns} FROM album_artists credit
            JOIN artists artist ON artist.id = {shown}
            {credit_name}
            WHERE credit.album_id = ?1
            ORDER BY credit.position
            "#,
            columns = LIBRARY_CREDIT_COLUMNS,
            shown = shown_artist_id("credit.artist_id"),
            credit_name = credit_name_join("credit", "credit_name"),
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
        credit: credit_name_column(row, "credit_key", "credit_name")?,
        join: row.get("credit_join")?,
    })
}

/// The join of a credit row `credit` (an `album_artists` or `track_artists`
/// row) to the other printed name it names, if any, as `name`.
pub fn credit_name_join(credit: &str, name: &str) -> String {
    format!(
        "LEFT JOIN artist_credits {name} \
             ON {name}.artist_id = {credit}.artist_id AND {name}.name_key = {credit}.credit_key"
    )
}

/// The other printed name a [`credit_name_join`] found, from its key and
/// its name: both, or neither when the credit prints its artist's own name.
/// One without the other is a join no query makes, and `Err` says which.
fn joined_credit_name(
    key: Option<String>,
    name: Option<String>,
) -> Result<Option<crate::credits::CreditName>, String> {
    match (key, name) {
        (Some(key), Some(name)) => Ok(Some(crate::credits::CreditName { key, name })),
        (None, None) => Ok(None),
        (Some(key), None) => Err(format!("credit name {key:?} has no name")),
        (None, Some(name)) => Err(format!("credit name {name:?} has no key")),
    }
}

/// The `joined_credit_name` of a row's `key` and `name` columns, read in a row
/// closure.
pub fn credit_name_column(
    row: &Row,
    key: &str,
    name: &str,
) -> rusqlite::Result<Option<crate::credits::CreditName>> {
    joined_credit_name(row.get(key)?, row.get(name)?)
        .map_err(|error| column_conversion_error(row, key, error))
}

/// One credit as [`super::query::album_credits_json_sql`] spells it.
#[derive(serde::Deserialize)]
pub(super) struct CreditJson {
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
        .map_err(|error| DbError::unreadable_json("credit list", json, error))?;
    credits_of(credits)
}

/// The credits of a parsed credit list, in list order.
pub(super) fn credits_of(credits: Vec<CreditJson>) -> Result<Vec<LibraryCredit>, DbError> {
    credits
        .into_iter()
        .map(|credit| {
            let created_at = DateTime::parse_from_rfc3339(&credit.created_at)
                .map_err(|_unparsed| {
                    DbError::unreadable(
                        format!("artist {} created_at", credit.id),
                        &credit.created_at,
                    )
                })?
                .with_timezone(&Utc);
            let credit_name =
                joined_credit_name(credit.credit_key, credit.credit_name).map_err(|error| {
                    DbError::unreadable(format!("artist {} credit name", credit.id), error)
                })?;
            Ok(LibraryCredit {
                artist: DbArtist {
                    id: credit.id,
                    name: credit.name,
                    sort_name: credit.sort_name,
                    discogs_artist_id: credit.discogs_artist_id,
                    musicbrainz_artist_id: credit.musicbrainz_artist_id,
                    created_at,
                },
                credit: credit_name,
                join: credit.join,
            })
        })
        .collect()
}

/// The line a credit-list JSON column prints, read in a row closure.
pub fn credit_line_column(
    row: &Row,
    column: &str,
) -> rusqlite::Result<bae_types::credits::CreditLine> {
    let json: String = row.get(column)?;
    credits_from_json(&json)
        .map(|credits| crate::credits::library_line(&credits))
        .map_err(|error| column_conversion_error(row, column, error.to_string()))
}

/// The line a nullable credit-list JSON column prints, read in a row closure;
/// `None` for NULL.
pub fn optional_credit_line_column(
    row: &Row,
    column: &str,
) -> rusqlite::Result<Option<bae_types::credits::CreditLine>> {
    let Some(json) = row.get::<_, Option<String>>(column)? else {
        return Ok(None);
    };
    credits_from_json(&json)
        .map(|credits| Some(crate::credits::library_line(&credits)))
        .map_err(|error| column_conversion_error(row, column, error.to_string()))
}

pub fn find_release_by_id_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Option<DbRelease>, DbError> {
    sql.query_row(&release_rows_sql("r.id = ?"), params![release_id], row_to_release)
        .optional()
        .map_err(DbError::from)
}

pub fn get_releases_for_album_on(
    sql: &impl coven::SqlRead,
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
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Vec<DbFile>, DbError> {
    let files = sql.query(RELEASE_FILES_IN_ORDER, params![release_id], row_to_file)?;
    Ok(files)
}

/// Every catalog's description of the release `release` (an SQL expression
/// naming its id), in the order surfaces list catalogs, as a JSON array for
/// [`records_column`] that is empty for a release no catalog describes.
pub(super) fn release_records_json_sql(release: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
                 'catalog', ordered.catalog, 'key', ordered.key, 'kind', ordered.kind, \
                 'album_key', ordered.album_key)) \
          FROM (SELECT rr.catalog, rr.key, rr.kind, rr.album_key FROM release_records rr \
                WHERE rr.release_id = {release} ORDER BY bae_catalog_rank(rr.catalog)) \
              AS ordered)"
    )
}

/// The release's records as [`release_records_json_sql`] read them into
/// `column`.
pub(super) fn records_column(
    row: &Row,
    column: &str,
) -> rusqlite::Result<Vec<bae_catalogs::records::ReleaseRecord>> {
    let text: String = row.get(column)?;
    let records: Vec<serde_json::Value> =
        serde_json::from_str(&text).map_err(|error| column_conversion_error(row, column, error))?;
    records.iter().map(|record| record_of(row, column, record)).collect()
}

/// Each of the releases `?1` names, a JSON array of release ids, as one row
/// in the array's order (`json_each` walks it so) holding its records, as
/// [`records_column`] reads them.
pub fn releases_records_in_order_sql() -> String {
    format!("SELECT {} AS records FROM json_each(?1) r", release_records_json_sql("r.value"))
}

/// Every catalog's description of a release, in the order surfaces list
/// catalogs.
pub fn get_release_records_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, DbError> {
    Ok(sql.query_row(
        &format!("SELECT {} AS records", release_records_json_sql("?1")),
        params![release_id],
        |row| records_column(row, "records"),
    )?)
}

/// Every catalog's description of each of `release_ids`, in one statement:
/// one list for each id, in the ids' order, each in the order surfaces list
/// catalogs; empty for a release no catalog describes.
pub fn get_releases_records_on(
    sql: &impl coven::SqlRead,
    release_ids: &[String],
) -> Result<Vec<Vec<bae_catalogs::records::ReleaseRecord>>, DbError> {
    Ok(sql.query(
        &releases_records_in_order_sql(),
        params![serde_json::json!(release_ids).to_string()],
        |row| records_column(row, "records"),
    )?)
}

/// One record of a `records` column's JSON array.
fn record_of(
    row: &Row,
    column: &str,
    record: &serde_json::Value,
) -> rusqlite::Result<bae_catalogs::records::ReleaseRecord> {
    let text = |field: &str| record[field].as_str().map(str::to_string);
    let unreadable = |reason: String| column_conversion_error(row, column, reason);
    let catalog = text("catalog")
        .ok_or_else(|| unreadable("a record has no catalog".to_string()))?
        .parse()
        .map_err(unreadable)?;
    let key = text("key").ok_or_else(|| unreadable("a record has no key".to_string()))?;
    let identity = bae_catalogs::records::MetadataRef::new(catalog, key);
    match record["kind"].as_str() {
        Some("pressing") => {
            Ok(bae_catalogs::records::ReleaseRecord::new(&identity, text("album_key")))
        }
        Some("album") => Ok(bae_catalogs::records::ReleaseRecord::album(&identity)),
        kind => Err(unreadable(format!("unknown record kind: {kind:?}"))),
    }
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
    sql: &impl coven::SqlRead,
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
#[cfg(desktop)]
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

/// The columns of a release file, by the names the `release_files` row, its
/// size and whether it is stored read under (`release_file_columns!()`),
/// whether a row reads them or a JSON object spells them.
#[derive(serde::Deserialize)]
pub(super) struct FileColumns {
    id: String,
    release_id: String,
    original_filename: String,
    content_type: String,
    created_at: String,
    file: coven::FileId,
    role: String,
    document_kind: Option<String>,
    source_audio_layout: Option<String>,
    source_audio_content_type: Option<String>,
    source_audio_container: Option<String>,
    source_audio_duration_ms: Option<u64>,
    source_audio_sample_rate_hz: Option<i64>,
    source_audio_bits_per_sample: Option<i64>,
    source_audio_bitrate_kbps: Option<i64>,
    source_audio_channels: Option<i64>,
    file_size: u64,
    stored: i64,
}

/// The names of [`FileColumns`] that come from `release_files` itself.
const FILE_ROW_COLUMNS: [&str; 16] = [
    "id",
    "release_id",
    "original_filename",
    "content_type",
    "created_at",
    "file",
    "role",
    "document_kind",
    "source_audio_layout",
    "source_audio_content_type",
    "source_audio_container",
    "source_audio_duration_ms",
    "source_audio_sample_rate_hz",
    "source_audio_bits_per_sample",
    "source_audio_bitrate_kbps",
    "source_audio_channels",
];

/// A JSON object of one release file for [`file_of`]: `f`, a row of
/// `release_files`, with the `size` coven records for it and whether it is
/// `stored` (SQL expressions).
pub(super) fn file_json_sql(size: &str, stored: &str) -> String {
    let columns: Vec<String> =
        FILE_ROW_COLUMNS.iter().map(|name| format!("'{name}', f.{name}")).collect();
    format!("json_object({}, 'file_size', {size}, 'stored', {stored})", columns.join(", "))
}

impl FileColumns {
    fn of_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            release_id: row.get("release_id")?,
            original_filename: row.get("original_filename")?,
            content_type: row.get("content_type")?,
            created_at: row.get("created_at")?,
            file: row.get("file")?,
            role: row.get("role")?,
            document_kind: row.get("document_kind")?,
            source_audio_layout: row.get("source_audio_layout")?,
            source_audio_content_type: row.get("source_audio_content_type")?,
            source_audio_container: row.get("source_audio_container")?,
            source_audio_duration_ms: row.get("source_audio_duration_ms")?,
            source_audio_sample_rate_hz: row.get("source_audio_sample_rate_hz")?,
            source_audio_bits_per_sample: row.get("source_audio_bits_per_sample")?,
            source_audio_bitrate_kbps: row.get("source_audio_bitrate_kbps")?,
            source_audio_channels: row.get("source_audio_channels")?,
            file_size: bae_store::read::count_column(row, "file_size")?,
            stored: row.get("stored")?,
        })
    }
}

/// The release file `columns` describe; a value that names no file fact is
/// the column it sits in and why.
pub(super) fn file_of(columns: FileColumns) -> Result<DbFile, (&'static str, String)> {
    let layout = columns
        .source_audio_layout
        .as_deref()
        .map(|layout| match layout {
            "file" => Ok(crate::source_audio::SourceAudioLayout::File),
            "cue" => Ok(crate::source_audio::SourceAudioLayout::Cue),
            other => Err(("source_audio_layout", format!("invalid source_audio_layout {other:?}"))),
        })
        .transpose()?;
    let source_audio = match (
        columns.source_audio_content_type,
        columns.source_audio_container,
        columns.source_audio_duration_ms,
        columns.source_audio_sample_rate_hz,
        columns.source_audio_bits_per_sample,
        columns.source_audio_bitrate_kbps,
        columns.source_audio_channels,
    ) {
        (None, None, None, None, None, None, None) if layout.is_none() => None,
        (
            Some(content_type),
            Some(container),
            Some(duration_ms),
            Some(sample_rate_hz),
            bits_per_sample,
            bitrate_kbps,
            Some(channels),
        ) => Some(crate::source_audio::SourceAudioFile {
            layout,
            content_type: ContentType::from_mime(&content_type),
            container: bae_util::audio_container::AudioContainer::from_name(&container),
            duration_ms,
            format: bae_types::audio_format::AudioFormat {
                codec: ContentType::from_mime(&content_type).display_name().to_string(),
                sample_rate_hz,
                bits_per_sample,
                bitrate_kbps,
                channels,
            },
        }),
        facts => {
            return Err((
                "source_audio_content_type",
                format!("inconsistent release source-audio facts: {facts:?}"),
            ))
        }
    };
    let role =
        crate::ReleaseFileRole::from_db_values(&columns.role, columns.document_kind.as_deref())
            .ok_or_else(|| {
                (
                    "role",
                    format!(
                        "invalid release file role {:?} with document kind {:?}",
                        columns.role, columns.document_kind
                    ),
                )
            })?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&columns.created_at)
        .map(|created_at| created_at.with_timezone(&chrono::Utc))
        .map_err(|error| {
            (
                "created_at",
                format!(
                    "created_at {:?} is not a valid rfc3339 timestamp: {error}",
                    columns.created_at
                ),
            )
        })?;
    Ok(DbFile {
        id: columns.id,
        release_id: columns.release_id,
        original_filename: columns.original_filename,
        file_size: columns.file_size,
        content_type: ContentType::from_mime(&columns.content_type),
        role,
        source_audio,
        file: columns.file,
        stored: columns.stored != 0,
        created_at,
    })
}

pub fn row_to_file(row: &Row) -> rusqlite::Result<DbFile> {
    file_of(FileColumns::of_row(row)?)
        .map_err(|(column, reason)| column_conversion_error(row, column, reason))
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

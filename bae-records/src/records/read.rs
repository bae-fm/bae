use super::*;

pub fn find_album_by_id_on(
    sql: &impl bae_store::WriterRead,
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
    sql: &impl bae_store::WriterRead,
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
pub(super) fn joined_credit_name(
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

/// An artist as a JSON object of its row's columns spells it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub(crate) struct ArtistJson {
    id: String,
    name: String,
    sort_name: Option<String>,
    discogs_artist_id: Option<String>,
    musicbrainz_artist_id: Option<String>,
    created_at: String,
}

impl ArtistJson {
    pub(super) fn into_artist(self) -> Result<DbArtist, DbError> {
        let created_at =
            bae_util::timestamp::parse_rfc3339(&self.created_at).map_err(|_unparsed| {
                DbError::unreadable(format!("artist {} created_at", self.id), &self.created_at)
            })?;
        Ok(DbArtist {
            id: self.id,
            name: self.name,
            sort_name: self.sort_name,
            discogs_artist_id: self.discogs_artist_id,
            musicbrainz_artist_id: self.musicbrainz_artist_id,
            created_at,
        })
    }
}

/// One credit as [`super::query::album_credits_json_sql`] spells it.
#[derive(serde::Deserialize)]
pub(super) struct CreditJson {
    #[serde(flatten)]
    artist: ArtistJson,
    credit_key: Option<String>,
    credit_name: Option<String>,
    join: Option<String>,
}

/// Parses the credits in a JSON column of
/// [`super::query::album_credits_json_sql`] and returns the line they print.
pub fn credit_line_of_json(json: &str) -> Result<bae_types::credits::CreditLine, DbError> {
    Ok(crate::credits::library_line(&credits_from_json(json)?))
}

/// The line a credit list prints, as a field of a read's JSON document that
/// holds the list as [`super::credit_fragments::TrackCredits`] writes it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(try_from = "Vec<CreditJson>")]
pub struct CreditListLine(bae_types::credits::CreditLine);

impl CreditListLine {
    /// The line the list prints.
    pub fn into_line(self) -> bae_types::credits::CreditLine {
        self.0
    }
}

impl TryFrom<Vec<CreditJson>> for CreditListLine {
    type Error = DbError;

    fn try_from(credits: Vec<CreditJson>) -> Result<Self, DbError> {
        Ok(Self(crate::credits::library_line(&credits_of(credits)?)))
    }
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
            let artist = credit.artist.into_artist()?;
            let credit_name =
                joined_credit_name(credit.credit_key, credit.credit_name).map_err(|error| {
                    DbError::unreadable(format!("artist {} credit name", artist.id), error)
                })?;
            Ok(LibraryCredit { artist, credit: credit_name, join: credit.join })
        })
        .collect()
}

/// An album's year as its column holds it, `None` for an album that states
/// none.
pub(super) fn album_year(year: Option<i64>) -> Result<Option<i32>, DbError> {
    year.map(|year| {
        i32::try_from(year).map_err(|_error| DbError::unreadable("an album's year", year))
    })
    .transpose()
}

/// The line a credit-list JSON column prints, read in a row closure.
pub fn credit_line_column(
    row: &Row,
    column: &str,
) -> rusqlite::Result<bae_types::credits::CreditLine> {
    credit_line_of(row, column, &row.get::<_, String>(column)?)
}

/// The line the credit list `json`, read from `column` of `row`, prints.
fn credit_line_of(
    row: &Row,
    column: &str,
    json: &str,
) -> rusqlite::Result<bae_types::credits::CreditLine> {
    credits_from_json(json)
        .map(|credits| crate::credits::library_line(&credits))
        .map_err(|error| column_conversion_error(row, column, error.to_string()))
}

pub fn find_release_by_id_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Option<DbRelease>, DbError> {
    sql.query_row(&release_rows_sql("r.id = ?"), params![release_id], row_to_release)
        .optional()
        .map_err(DbError::from)
}

pub fn get_releases_for_album_on(
    sql: &impl bae_store::WriterRead,
    album_id: &str,
) -> Result<Vec<DbRelease>, DbError> {
    sql.query(
        &release_rows_sql("r.album_id = ? ORDER BY r.album_order, r.id"),
        params![album_id],
        row_to_release,
    )
    .map_err(DbError::from)
}

// One release's catalog records.
sqlive_compose::read!(
    writer OneReleaseRecords(release: String),
    r#"
    SELECT listed.*
    FROM @crate::records::fragments::ReleaseRecords AS listed
    WHERE listed.records_release_id = :release
"#
);

/// The rows the JSON array `json`, which holds `what`, has.
pub(super) fn rows_of<T: serde::de::DeserializeOwned>(
    json: &str,
    what: &str,
) -> Result<Vec<T>, DbError> {
    serde_json::from_str(json).map_err(|error| DbError::unreadable_json(what, json, error))
}

/// The release's records as the document `json` of
/// [`crate::records::fragments::ReleaseRecords`] holds them.
pub fn records_of_json(json: &str) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, DbError> {
    let records: Vec<StoredRecord> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a release's records", json, error))?;
    records
        .into_iter()
        .map(|record| {
            record_of(record).map_err(|reason| DbError::unreadable("a release's records", reason))
        })
        .collect()
}

/// Every catalog's description of a release, in the order surfaces list
/// catalogs: none for a release the library does not hold.
pub fn get_release_records_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, DbError> {
    let statement = OneReleaseRecordsParams { release: release_id.to_owned() };
    match bae_store::statement_optional(sql, &statement)? {
        Some(row) => records_of_json(row.listed.records_json()),
        None => Ok(Vec::new()),
    }
}

/// One record as [`ReleaseRecords`](super::fragments::ReleaseRecords) writes
/// it into its array.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum StoredRecord {
    Pressing { catalog: String, key: String, album_key: Option<String> },
    Album { catalog: String, key: String },
}

/// The record `record` stores; a catalog name bae doesn't know is refused.
fn record_of(
    record: StoredRecord,
) -> Result<
    bae_catalogs::records::ReleaseRecord,
    <bae_catalogs::catalog::Catalog as std::str::FromStr>::Err,
> {
    let identity = |catalog: String,
                    key: String|
     -> Result<_, <bae_catalogs::catalog::Catalog as std::str::FromStr>::Err> {
        let catalog: bae_catalogs::catalog::Catalog = catalog.parse()?;
        Ok(bae_catalogs::records::MetadataRef::new(catalog, key))
    };
    Ok(match record {
        StoredRecord::Pressing { catalog, key, album_key } => {
            bae_catalogs::records::ReleaseRecord::new(&identity(catalog, key)?, album_key)
        }
        StoredRecord::Album { catalog, key } => {
            bae_catalogs::records::ReleaseRecord::album(&identity(catalog, key)?)
        }
    })
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
        stated_length_ms: row.get(column("stated_length_ms").as_str())?,
        created_at: rfc3339_column(row, &column("created_at"))?,
    })
}

#[cfg(any(test, feature = "test-utils"))]
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
#[cfg(any(test, feature = "test-utils"))]
pub fn release_mediums_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Vec<DbReleaseMedium>, DbError> {
    Ok(sql.query(
        "SELECT * FROM release_mediums WHERE release_id = ? ORDER BY position",
        params![release_id],
        row_to_release_medium,
    )?)
}

/// A `SELECT` of the `releases` rows `condition` names (and orders), each with
/// its labels, its barcodes, its media and its Discogs details as one
/// `labels`, one `barcodes`, one `media` and one `discogs_details` column, as
/// [`row_to_release`] reads them.
pub(super) fn release_rows_sql(condition: &str) -> String {
    format!(
        "SELECT r.*, {labels} AS labels, {barcodes} AS barcodes, {media} AS media, \
                {discogs_details} AS discogs_details \
         FROM releases r WHERE {condition}",
        labels = super::pressing_columns::release_labels_sql("r.id"),
        barcodes = super::pressing_columns::release_barcodes_sql("r.id"),
        media = super::pressing_columns::release_media_sql("r.id"),
        discogs_details = super::pressing_columns::release_discogs_details_sql("r.id"),
    )
}

pub(super) fn row_to_release(row: &Row) -> rusqlite::Result<DbRelease> {
    Ok(DbRelease {
        id: row.get("id")?,
        album_id: row.get("album_id")?,
        release_name: row.get("release_name")?,
        pressing: bae_types::pressing::Pressing {
            typed: bae_types::pressing::TypedPressing {
                year: row.get("year")?,
                labels: super::pressing_columns::read_labels(row, "labels")?,
                barcodes: super::pressing_columns::read_barcodes(row, "barcodes")?,
            },
            facts: super::pressing_columns::read_facts(row, "")?,
        },
        source_folder_name: row.get("source_folder_name")?,
        album_loudness_lufs: row.get("album_loudness_lufs")?,
        album_peak_linear: row.get("album_peak_linear")?,
        created_at: rfc3339_column(row, "created_at")?,
    })
}

/// A stored `source_audio_layout` value, or why it is none.
pub(super) fn source_audio_layout_of(
    layout: &str,
) -> Result<crate::source_audio::SourceAudioLayout, String> {
    match layout {
        "file" => Ok(crate::source_audio::SourceAudioLayout::File),
        "cue" => Ok(crate::source_audio::SourceAudioLayout::Cue),
        other => Err(format!("invalid source_audio_layout {other:?}")),
    }
}

/// The columns of one release file, under the names
/// [`ReleaseFileRecord`](super::file_fragments::ReleaseFileRecord) gives them,
/// as that row or a JSON object holds them.
#[derive(serde::Deserialize)]
pub(super) struct FileColumns {
    pub(super) id: String,
    pub(super) release_id: String,
    pub(super) original_filename: String,
    pub(super) content_type: String,
    pub(super) created_at: String,
    pub(super) file: coven::FileId,
    pub(super) role: String,
    pub(super) document_kind: Option<String>,
    pub(super) source_audio_layout: Option<String>,
    pub(super) source_audio_content_type: Option<String>,
    pub(super) source_audio_container: Option<String>,
    pub(super) source_audio_duration_ms: Option<u64>,
    pub(super) source_audio_sample_rate_hz: Option<i64>,
    pub(super) source_audio_bits_per_sample: Option<i64>,
    pub(super) source_audio_bitrate_kbps: Option<i64>,
    pub(super) source_audio_channels: Option<i64>,
    pub(super) file_size: u64,
    pub(super) stored: i64,
}

/// Build the release file that `columns` describe. When a column holds a
/// value that file_of cannot read, file_of returns that column's name and the
/// reason.
pub(super) fn file_of(columns: FileColumns) -> Result<DbFile, (&'static str, String)> {
    let layout = columns
        .source_audio_layout
        .as_deref()
        .map(source_audio_layout_of)
        .transpose()
        .map_err(|reason| ("source_audio_layout", reason))?;
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
    let created_at = bae_util::timestamp::parse_rfc3339(&columns.created_at).map_err(|error| {
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
pub(super) fn track_audio_count(
    raw: i64,
    column: &str,
    track_id: &str,
) -> Result<std::num::NonZeroU32, String> {
    u32::try_from(raw)
        .ok()
        .and_then(std::num::NonZeroU32::new)
        .ok_or_else(|| format!("track {track_id} has {column} {raw}, not a count above zero"))
}

/// A track's stored bit depth: absent for lossy audio, else 1 to 32.
pub(super) fn track_bit_depth(
    raw: Option<i64>,
    track_id: &str,
) -> Result<Option<std::num::NonZeroU32>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    u32::try_from(raw)
        .ok()
        .filter(|bits| (1..=32).contains(bits))
        .and_then(std::num::NonZeroU32::new)
        .map(Some)
        .ok_or_else(|| {
            format!("track {track_id} has bits_per_sample {raw}, not a depth of 1 to 32")
        })
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;

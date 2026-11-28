use super::*;

// The album `album`; no row when the library holds no such album.
coven::read!(
    writer OneAlbum(album: String),
    r#"
    SELECT album.*
    FROM @crate::records::browse_fragments::AlbumRecord AS album
    WHERE album.album_id = :album
"#
);

pub fn find_album_by_id_on(
    sql: &impl bae_store::WriterRead,
    album_id: &str,
) -> Result<Option<DbAlbum>, DbError> {
    let statement = OneAlbumParams { album: album_id.to_owned() };
    bae_store::statement_optional(sql, &statement)?.map(|row| row.album.into_album()).transpose()
}

// The album `album`'s credits in list order
// ([`AlbumCredits`](super::fragments::AlbumCredits)); no row when the library
// holds no such album.
coven::read!(
    writer CreditsOfAlbum(album: String),
    r#"
    SELECT @crate::records::fragments::AlbumCredits(parent: album) AS listed
    FROM albums AS album
    WHERE album.id = :album
"#
);

/// The album's credits in list order, each the artist it shows after merges;
/// a list naming one artist twice gives it twice. None for an album the
/// library does not hold.
pub fn get_artists_for_album_on(
    sql: &impl bae_store::WriterRead,
    album_id: &str,
) -> Result<Vec<LibraryCredit>, DbError> {
    let statement = CreditsOfAlbumParams { album: album_id.to_owned() };
    match bae_store::statement_optional(sql, &statement)? {
        Some(row) => {
            let listed =
                row.listed.ok_or_else(|| DbError::unreadable("an album's credits", album_id))?;
            credits_from_json(&listed.credits)
        }
        None => Ok(Vec::new()),
    }
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

// A release's own columns, with its labels, barcodes, media and Discogs
// details.
coven::fragment!(
    ReleaseColumns { year: Option<i32> },
    r#"
    SELECT release.album_id, release.release_name, release.year,
           release.country, release.region, release.status, release.packaging,
           release.source_folder_name, release.album_loudness_lufs, release.album_peak_linear,
           release.created_at AS release_created_at,
           @crate::records::pressing_columns::ReleaseLabels(parent: release) AS labels,
           @crate::records::pressing_columns::ReleaseBarcodes(parent: release) AS barcodes,
           @crate::records::pressing_columns::ReleaseMedia(parent: release) AS media,
           @crate::records::pressing_columns::ReleaseDiscogsDetails(parent: release)
               AS discogs_details,
           release.id AS columns_release_id
    FROM releases AS release
"#
);

impl ReleaseColumns {
    /// The release these columns hold.
    pub(super) fn release(&self) -> Result<DbRelease, DbError> {
        let facts = super::pressing_columns::facts_of_columns(
            self.country.clone(),
            self.region.clone(),
            self.status.as_deref(),
            self.packaging.as_deref(),
            &self.discogs_details.discogs_details,
            &self.media.media,
        )?;
        Ok(DbRelease {
            id: self.columns_release_id.clone(),
            album_id: self.album_id.clone(),
            release_name: self.release_name.clone(),
            pressing: bae_types::pressing::Pressing {
                typed: bae_types::pressing::TypedPressing {
                    year: self.year,
                    labels: super::pressing_columns::labels_of_json(&self.labels.labels)?,
                    barcodes: super::pressing_columns::barcodes_of_json(&self.barcodes.barcodes)?,
                },
                facts,
            },
            source_folder_name: self.source_folder_name.clone(),
            album_loudness_lufs: self.album_loudness_lufs,
            album_peak_linear: self.album_peak_linear,
            created_at: bae_util::timestamp::parse_rfc3339(&self.release_created_at).map_err(
                |_unparsed| DbError::unreadable("a release's created_at", &self.release_created_at),
            )?,
        })
    }
}

// The release `release`; no row when the library holds no such release.
coven::read!(
    writer OneRelease(release: String),
    r#"
    SELECT held.*
    FROM @crate::records::read::ReleaseColumns AS held
    WHERE held.columns_release_id = :release
"#
);

pub fn find_release_by_id_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Option<DbRelease>, DbError> {
    let statement = OneReleaseParams { release: release_id.to_owned() };
    bae_store::statement_optional(sql, &statement)?.map(|row| row.held.release()).transpose()
}

// The releases of the album `album`, in album order.
coven::read!(
    writer ReleasesOfAlbum(album: String),
    r#"
    SELECT held.*
    FROM @crate::records::read::ReleaseColumns AS held
    JOIN releases AS ordered ON ordered.id = held.columns_release_id
    WHERE ordered.album_id = :album
    ORDER BY ordered.album_order, ordered.id
"#
);

pub fn get_releases_for_album_on(
    sql: &impl bae_store::WriterRead,
    album_id: &str,
) -> Result<Vec<DbRelease>, DbError> {
    let statement = ReleasesOfAlbumParams { album: album_id.to_owned() };
    bae_store::statement_rows(sql, &statement)?.iter().map(|row| row.held.release()).collect()
}

// One release's catalog records.
coven::read!(
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

// The release `release`'s mediums, in order.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer MediumsOfRelease(release: String) {
        position: std::num::NonZeroU16,
        side_count: std::num::NonZeroU8,
    },
    r#"
    SELECT medium.id, medium.release_id, medium.position, medium.medium, medium.side_count,
           medium.created_at
    FROM release_mediums AS medium
    WHERE medium.release_id = :release
    ORDER BY medium.position
"#
);

/// A release's mediums, in order.
#[cfg(any(test, feature = "test-utils"))]
pub fn release_mediums_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
) -> Result<Vec<DbReleaseMedium>, DbError> {
    let statement = MediumsOfReleaseParams { release: release_id.to_owned() };
    bae_store::statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| {
            let carrier = row
                .medium
                .map(|key| {
                    bae_types::pressing::Medium::from_key(&key)
                        .ok_or_else(|| DbError::unreadable("a release medium", key))
                })
                .transpose()?;
            let created_at =
                bae_util::timestamp::parse_rfc3339(&row.created_at).map_err(|_unparsed| {
                    DbError::unreadable("a release medium's created_at", &row.created_at)
                })?;
            Ok(DbReleaseMedium {
                id: row.id,
                release_id: row.release_id,
                position: row.position,
                medium: bae_types::mediums::ReleaseMedium { carrier, sides: row.side_count },
                created_at,
            })
        })
        .collect()
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

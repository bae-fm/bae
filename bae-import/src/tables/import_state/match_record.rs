//! A stored match as one JSON object: the SQL that returns a match joined as
//! `found` with the rows that name it, and the reader that turns it back into
//! the match. A card stores its releases as these, and every read of a match
//! (a verdict's, a run's, a search's) selects one.

use super::match_rows::{
    catalog_of, match_of, source_of, standing_of, MatchColumns, MatchEntries, MatchRow,
    StoredMatches, StoredMedium,
};
use super::*;
use crate::tables::album_link_rows::AlbumLinkRow;
use crate::tables::cover_labels;
use crate::tables::stored_credits;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::remote_image::{DownscaledCopy, RemoteCover, RemoteImageSet};
use bae_catalogs::search::{MetadataResult, SourceTracks};
use bae_types::pressing::StatedMedia;

sqlive_compose::fragment!(
    MatchRecord on import_candidate_match {
        record: String
    },
    r#"
    SELECT (
            json_object('position', @parent.position, 'pressing', @parent.pressing, 'listing',
                @parent.listing, 'media_kind', @parent.media_kind, 'album_links',
                @parent.album_links, 'source', @parent.source, 'release_id', @parent.release_id,
                'title', @parent.title, 'credits', @parent.credits, 'year', @parent.year,
                'country', @parent.country, 'region', @parent.region, 'status', @parent.status,
                'packaging', @parent.packaging, 'source_group_id', @parent.source_group_id,
                'named_note', @parent.named_note, 'discogs_details',
                json(@parent.discogs_details), 'cover', CASE WHEN @parent.cover_url IS NOT NULL
                THEN json_object('url', @parent.cover_url, 'label', @parent.cover_label,
                'source', @parent.cover_source, 'standing', @parent.cover_standing) END,
                'source_tracks', CASE WHEN @parent.source_tracks_kind = 'listed' THEN
                json_object('kind', 'listed', 'count', @parent.source_tracks_count) WHEN
                @parent.source_tracks_kind IS NOT NULL THEN json_object('kind',
                @parent.source_tracks_kind) END, 'document_failure', CASE WHEN
                @parent.document_failure IS NOT NULL THEN json_object('kind',
                @parent.document_failure, 'status', @parent.document_failure_status) END,
                'album_first_year',(
            SELECT @crate::tables::source_releases::row_fragments::MatchAlbumFirstYear(parent: own)
            FROM import_candidate_match AS own
            WHERE own.match_id = @parent.match_id
            ), 'labels', json((
            SELECT json_group_array(json_object('name', entry.name, 'catalog_number',
                entry.catalog_number, 'musicbrainz_label_id', entry.musicbrainz_label_id,
                'discogs_label_id', entry.discogs_label_id) ORDER BY entry.ordinal)
            FROM import_candidate_match_label AS entry
            WHERE entry.match_id = @parent.match_id
            )), 'barcodes', json((
            SELECT json_group_array(entry.barcode ORDER BY entry.ordinal)
            FROM import_candidate_match_barcode AS entry
            WHERE entry.match_id = @parent.match_id
            )), 'cover_copies', json((
            SELECT json_group_array(json_object('url', copy.url, 'max_edge', copy.max_edge)
                ORDER BY copy.max_edge)
            FROM import_candidate_match_cover_copy AS copy
            WHERE copy.match_id = @parent.match_id
            )), 'media', json((
            SELECT json_group_array(json_object('kind', entry.media_kind, 'medium',
                entry.medium, 'quantity', entry.quantity) ORDER BY entry.ordinal)
            FROM import_candidate_match_medium AS entry
            WHERE entry.match_id = @parent.match_id
            )), 'links', json((
            SELECT json_group_array(json_array(entry.catalog, entry.key) ORDER BY entry.ordinal)
            FROM import_candidate_match_link AS entry
            WHERE entry.match_id = @parent.match_id
            )), 'album_link_rows', json((
            SELECT json_group_array(json_object('catalog', entry.catalog, 'key', entry.key)
                ORDER BY entry.ordinal)
            FROM import_candidate_match_album_link AS entry
            WHERE entry.match_id = @parent.match_id
        )))
    ) AS record
"#);

/// One medium of a match as [`match_record_sql`] holds it.
#[derive(serde::Deserialize)]
struct StoredMatchMedium {
    kind: String,
    medium: Option<String>,
    quantity: u32,
}

/// A match's cover as [`match_record_sql`] holds it: the four columns the
/// table sets and clears together. Its copies are rows of their own, which
/// [`match_of`] adds.
#[derive(serde::Deserialize)]
struct StoredCover {
    url: String,
    label: String,
    source: String,
    standing: String,
}

/// What a match's catalog said of its tracklist, as [`match_record_sql`]
/// holds it.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredSourceTracks {
    Listed { count: u32 },
    Nothing,
    Unreadable,
}

/// Why a match's document lookup failed, as [`match_record_sql`] holds it.
#[derive(serde::Deserialize)]
struct StoredFailure {
    kind: String,
    status: Option<i64>,
}

/// One match as [`match_record_sql`] answers it.
#[derive(serde::Deserialize)]
pub(super) struct StoredRecord {
    position: i64,
    pressing: Option<i64>,
    listing: String,
    media_kind: String,
    album_links: String,
    source: String,
    pub(super) release_id: String,
    title: String,
    credits: Option<String>,
    year: Option<i32>,
    country: Option<String>,
    region: Option<String>,
    status: Option<String>,
    packaging: Option<String>,
    discogs_details: Vec<bae_types::pressing::DiscogsDetail>,
    cover: Option<StoredCover>,
    source_group_id: Option<String>,
    source_tracks: Option<StoredSourceTracks>,
    document_failure: Option<StoredFailure>,
    album_first_year: Option<i32>,
    named_note: Option<String>,
    labels: Vec<bae_types::pressing::ReleaseLabel>,
    barcodes: Vec<String>,
    cover_copies: Vec<DownscaledCopy>,
    media: Vec<StoredMatchMedium>,
    links: Vec<(String, String)>,
    album_link_rows: Vec<AlbumLinkRow>,
}

impl StoredRecord {
    /// The match `self` holds, with `content_hash` naming whose it is when it
    /// is unreadable.
    pub(super) fn match_row(self, content_hash: &str) -> Result<MatchRow, DbError> {
        let Self {
            position,
            pressing,
            listing,
            media_kind,
            album_links,
            source,
            release_id,
            title,
            credits,
            year,
            country,
            region,
            status,
            packaging,
            discogs_details,
            cover,
            source_group_id,
            source_tracks,
            document_failure,
            album_first_year,
            named_note,
            labels,
            barcodes,
            cover_copies,
            media,
            links,
            album_link_rows,
        } = self;
        let cover_art = cover
            .map(|cover| {
                Ok::<_, DbError>(RemoteCover {
                    image: RemoteImageSet::original(cover.url),
                    label: cover_labels::label_of(&cover.label)?,
                    source: catalog_of(&cover.source)?,
                    standing: standing_of(&cover.standing)?,
                })
            })
            .transpose()?;
        let source_tracks = source_tracks.map(|tracks| match tracks {
            StoredSourceTracks::Listed { count } => SourceTracks::Listed { count },
            StoredSourceTracks::Nothing => SourceTracks::Nothing,
            StoredSourceTracks::Unreadable => SourceTracks::Unreadable,
        });
        let facts = bae_records::records::pressing_columns::facts_without_media_of(
            country,
            region,
            status.as_deref(),
            packaging.as_deref(),
            discogs_details,
        )
        .map_err(|detail| DbError::unreadable("a stored match's pressing facts", detail))?;
        let columns = MatchColumns {
            content_hash: content_hash.to_string(),
            position,
            pressing,
            listing,
            media_kind,
            album_links,
            result: MetadataResult {
                source: source_of(&source)?,
                release_id,
                title,
                artist: stored_credits::line_of(credits.as_deref()).map_err(|error| {
                    DbError::unreadable_json(
                        "a stored match's credits",
                        credits.as_deref().unwrap_or_default(),
                        error,
                    )
                })?,
                year,
                labels: Vec::new(),
                area: facts.area,
                status: facts.status,
                packaging: facts.packaging,
                discogs_details: facts.discogs_details,
                barcodes: Vec::new(),
                media: StatedMedia::Undescribed,
                links: Vec::new(),
                cover_art,
                source_group_id,
                album_links: AlbumLinks::NotAsked,
                source_tracks,
                document_failure: document_failure
                    .map(|failure| {
                        super::lookup_failure_columns::failure_named(&failure.kind, failure.status)
                    })
                    .transpose()?,
                // The SQL reads the year from the stored catalog release, so
                // a later fetch of that release changes it.
                album_first_year,
                // Only a run's ranking reads these, so a verdict keeps neither.
                track_titles: Vec::new(),
                notes: Vec::new(),
            },
            named_note,
        };
        let entries = MatchEntries {
            labels,
            barcodes,
            cover_copies,
            media: media
                .into_iter()
                .map(|stored| {
                    Ok(StoredMedium {
                        kind: stored.kind,
                        medium: crate::tables::medium_of(
                            "import_candidate_match_medium.medium",
                            stored.medium,
                        )?,
                        quantity: stored.quantity,
                    })
                })
                .collect::<Result<_, DbError>>()?,
            links,
            album_links: album_link_rows,
        };
        match_of(columns, entries)
    }
}

/// The match a [`match_record_sql`] column holds.
pub(super) fn match_row_of(content_hash: &str, json: &str) -> Result<MatchRow, DbError> {
    serde_json::from_str::<StoredRecord>(json)
        .map_err(|error| DbError::unreadable_json("a stored match", json, error))?
        .match_row(content_hash)
}

/// The matches a JSON array of [`match_record_sql`] objects holds, in its
/// order.
pub(super) fn match_rows_of(content_hash: &str, json: &str) -> Result<StoredMatches, DbError> {
    let records = serde_json::from_str::<Vec<StoredRecord>>(json)
        .map_err(|error| DbError::unreadable_json("a candidate's stored matches", json, error))?;
    Ok(StoredMatches {
        rows: records
            .into_iter()
            .map(|record| record.match_row(content_hash))
            .collect::<Result<_, _>>()?,
    })
}

/// The JSON of the match a read of [`MatchRecord`] returned for a row of
/// `owner`, which every match row has.
fn record_json(record: Option<MatchRecord>, owner: &str) -> Result<String, DbError> {
    record
        .map(|record| record.record)
        .ok_or_else(|| DbError::unreadable(format!("a stored match of {owner}"), None::<String>))
}

sqlive_compose::read!(
    SearchRunMatches(run: i64),
    r#"
    SELECT part.position, found.position AS found_position,
        @crate::tables::import_state::match_record::MatchRecord(parent: found) AS record
    FROM import_candidate_match AS found
    JOIN import_candidate_search_source AS part
        ON part.search_run = found.search_run AND part.catalog = found.source
    WHERE found.search_run = :run
    ORDER BY part.position, found.position
"#
);

/// The releases of the search `run`, under its catalog parts in their order,
/// each part's in the order it returned them.
pub(super) fn load_search_run_matches_on(
    sql: &impl coven::SqlRead,
    run: i64,
) -> Result<Vec<MatchRow>, DbError> {
    let name = format!("search {run}");
    let request = SearchRunMatchesParams { run };
    statement_rows(sql, &request)?
        .into_iter()
        .map(|row| match_row_of(&name, &record_json(row.record, &name)?))
        .collect()
}

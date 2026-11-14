//! A stored match as one JSON object: the SQL that answers a match joined as
//! `found` with what hangs off it, and the reader that turns it back into the
//! match. A card carries its releases as these, and every read of a match
//! (a verdict's, a run's, a search's) selects one.

use super::match_rows::{
    catalog_of, match_of, source_of, standing_of, MatchColumns, MatchEntries, MatchRow,
    StoredMatches, StoredMedium,
};
use super::verdict_rows::Owner;
use super::*;
use crate::tables::album_link_rows::AlbumLinkRow;
use crate::tables::cover_labels;
use crate::tables::stored_credits;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::remote_image::{DownscaledCopy, RemoteCover, RemoteImageSet};
use bae_catalogs::search::{MetadataResult, SourceTracks};
use bae_types::pressing::{Medium, StatedMedia};

/// A match joined as `found` as one JSON object: its own columns, its album's
/// first year from the stored catalog release it names, and what hangs off it,
/// each list in its stored order: its labels, barcodes, cover copies, media,
/// links and album links. [`match_row_of`] reads it back.
pub(super) fn match_record_sql(found: &str) -> String {
    let child = |table: &str, value: &str, order: &str| {
        format!(
            "json((SELECT json_group_array({value} ORDER BY {order}) FROM {table} AS entry \
                   WHERE entry.content_hash IS {found}.content_hash \
                     AND entry.identify_run IS {found}.identify_run \
                     AND entry.search_run IS {found}.search_run \
                     AND entry.position = {found}.position))"
        )
    };
    let columns = [
        "position",
        "pressing",
        "listing",
        "media_kind",
        "album_links",
        "source",
        "release_id",
        "title",
        "credits",
        "year",
        "country",
        "region",
        "status",
        "packaging",
        "cover_url",
        "cover_label",
        "cover_source",
        "cover_standing",
        "source_group_id",
        "source_tracks_kind",
        "source_tracks_count",
        "document_failure",
        "document_failure_status",
        "named_note",
    ]
    .map(|column| format!("'{column}', {found}.{column}"))
    .join(", ");
    let copies = format!(
        "json({})",
        crate::tables::downscaled_copies::copies_json_sql(
            "import_candidate_match_cover_copy",
            &format!(
                "copy.content_hash IS {found}.content_hash \
                 AND copy.identify_run IS {found}.identify_run \
                 AND copy.search_run IS {found}.search_run \
                 AND copy.position = {found}.position"
            ),
        )
    );
    format!(
        "json_object({columns}, \
             'discogs_details', json({found}.discogs_details), \
             'album_first_year', (SELECT fetched.album_first_year FROM source_release AS fetched \
                                  WHERE fetched.catalog = {found}.source \
                                    AND fetched.release_id = {found}.release_id), \
             'labels', {labels}, 'barcodes', {barcodes}, 'cover_copies', {copies}, \
             'media', {media}, 'links', {links}, 'album_link_rows', {album_links})",
        labels = child(
            "import_candidate_match_label",
            "json_object('name', entry.name, 'catalog_number', entry.catalog_number, \
                 'musicbrainz_label_id', entry.musicbrainz_label_id, \
                 'discogs_label_id', entry.discogs_label_id)",
            "entry.ordinal"
        ),
        barcodes = child("import_candidate_match_barcode", "entry.barcode", "entry.ordinal"),
        copies = copies,
        media = child(
            "import_candidate_match_medium",
            "json_object('kind', entry.media_kind, 'medium', entry.medium, \
                         'quantity', entry.quantity)",
            "entry.ordinal",
        ),
        links = child(
            "import_candidate_match_link",
            "json_array(entry.catalog, entry.key)",
            "entry.ordinal",
        ),
        album_links = child(
            "import_candidate_match_album_link",
            "json_object('catalog', entry.catalog, 'key', entry.key)",
            "entry.ordinal",
        ),
    )
}

/// One medium of a match as [`match_record_sql`] holds it.
#[derive(serde::Deserialize)]
struct StoredMatchMedium {
    kind: String,
    medium: Option<String>,
    quantity: u32,
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
    cover_url: Option<String>,
    cover_label: Option<String>,
    cover_source: Option<String>,
    cover_standing: Option<String>,
    source_group_id: Option<String>,
    source_tracks_kind: Option<String>,
    source_tracks_count: Option<i64>,
    document_failure: Option<String>,
    document_failure_status: Option<i64>,
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
            cover_url,
            cover_label,
            cover_source,
            cover_standing,
            source_group_id,
            source_tracks_kind,
            source_tracks_count,
            document_failure,
            document_failure_status,
            album_first_year,
            named_note,
            labels,
            barcodes,
            cover_copies,
            media,
            links,
            album_link_rows,
        } = self;
        // The table sets and clears the four cover columns together; the
        // copies are rows of their own, which [`match_of`] adds.
        let cover_art = match (cover_url, cover_source, cover_label, cover_standing) {
            (Some(url), Some(source), Some(label), Some(standing)) => Some(RemoteCover {
                image: RemoteImageSet::original(url),
                label: cover_labels::label_of(&label)?,
                source: catalog_of(&source)?,
                standing: standing_of(&standing)?,
            }),
            _ => None,
        };
        let source_tracks = match source_tracks_kind.as_deref() {
            None => None,
            Some("nothing") => Some(SourceTracks::Nothing),
            Some("unreadable") => Some(SourceTracks::Unreadable),
            Some("listed") => {
                let count = source_tracks_count.ok_or_else(|| {
                    DbError::unreadable(
                        "import_candidate_match.source_tracks_count of a listed tracklist",
                        None::<i64>,
                    )
                })?;
                Some(SourceTracks::Listed {
                    count: to_u32(count, "import_candidate_match.source_tracks_count")?,
                })
            }
            Some(other) => {
                return Err(DbError::unreadable("import_candidate_match.source_tracks_kind", other))
            }
        };
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
                document_failure: super::lookup_failure_columns::failure_of(
                    document_failure,
                    document_failure_status,
                )?,
                // Read from the stored catalog release the match names, so a
                // later fetch of it shows.
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
                        medium: stored
                            .medium
                            .map(|key| {
                                Medium::from_key(&key).ok_or_else(|| {
                                    DbError::unreadable(
                                        "import_candidate_match_medium.medium",
                                        &key,
                                    )
                                })
                            })
                            .transpose()?,
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

/// The releases the verdict-shaped `owner` holds, in position order.
pub(super) fn load_owner_matches_on(
    sql: &impl coven::SqlRead,
    owner: Owner<'_>,
) -> Result<StoredMatches, DbError> {
    let column = owner.column();
    let rows = sql.query(
        &format!(
            "SELECT COALESCE(found.content_hash, 'run ' || found.identify_run) AS content_hash, \
                    {record} AS record \
             FROM import_candidate_match AS found \
             WHERE found.{column} = ? \
             ORDER BY found.position",
            record = match_record_sql("found"),
        ),
        params![owner],
        |row| {
            Ok(match_row_of(
                &row.get::<_, String>("content_hash")?,
                &row.get::<_, String>("record")?,
            ))
        },
    )?;
    Ok(StoredMatches { rows: rows.into_iter().collect::<Result<_, _>>()? })
}

/// The releases of the search `run`, under its catalog parts in their order,
/// each part's in the order it returned them.
pub(super) fn load_search_run_matches_on(
    sql: &impl coven::SqlRead,
    run: i64,
) -> Result<Vec<MatchRow>, DbError> {
    sql.query(
        &format!(
            "SELECT 'search ' || found.search_run AS content_hash, {record} AS record \
             FROM import_candidate_match AS found \
             JOIN import_candidate_search_source AS part \
                 ON part.search_run = found.search_run AND part.catalog = found.source \
             WHERE found.search_run = ? \
             ORDER BY part.position, found.position",
            record = match_record_sql("found"),
        ),
        [run],
        |row| {
            Ok(match_row_of(
                &row.get::<_, String>("content_hash")?,
                &row.get::<_, String>("record")?,
            ))
        },
    )?
    .into_iter()
    .collect()
}

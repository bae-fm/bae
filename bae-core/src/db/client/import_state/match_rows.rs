//! A verdict's release-match rows: one per release it found, where the run
//! lists it, what the record said and the rows hanging off it, written and
//! read back.

use super::super::album_link_rows::{AlbumLinkRow, StatementColumns};
use super::verdict_rows::unreadable;
use super::*;
use crate::identify::LookupProvenance;
use crate::import::album_links::AlbumLinks;
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::MetadataRef;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::remote_image::{CoverStanding, DownscaledCopy, RemoteCover, RemoteImageSet};
use bae_types::pressing::{Medium, StatedFormat, StatedMedia};
use std::str::FromStr;

fn source_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored).map_err(DbError::Message)
}

fn standing_of(stored: &str) -> Result<CoverStanding, DbError> {
    CoverStanding::from_column(stored).ok_or_else(|| unreadable("cover_standing", stored))
}

/// Where a release a verdict stores is listed, with what an offered or
/// narrowed-out one carries: which lookups returned it and its pressing row,
/// and for an offered one the note the folder's text names its row by.
#[derive(Clone, Copy)]
pub(super) enum Written<'a> {
    Offered(&'a LookupProvenance, u32, Option<&'a str>),
    NarrowedOut(&'a LookupProvenance, u32),
    Unlisted,
}

/// One match at `position`, in place of the one there, with the rows under
/// it. The rows under the match that stood there are already gone.
pub(super) fn store_match(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    position: usize,
    result: &MetadataResult,
    listing: Written<'_>,
) -> Result<(), DbError> {
    let position = i64::try_from(position).map_err(|error| {
        DbError::Message(format!(
            "a match list is longer than SQLite counts: {error}"
        ))
    })?;
    let (listing, provenance, pressing, named_note) = match listing {
        Written::Offered(provenance, pressing, named_note) => (
            LISTING_OFFERED,
            Some(provenance),
            Some(pressing),
            named_note,
        ),
        Written::NarrowedOut(provenance, pressing) => {
            (LISTING_NARROWED_OUT, Some(provenance), Some(pressing), None)
        }
        Written::Unlisted => (LISTING_UNLISTED, None, None, None),
    };
    let cover = result.cover_art.as_ref();
    let (tracks_kind, tracks_count) = match &result.source_tracks {
        None => (None, None),
        Some(SourceTracks::Nothing) => (Some("nothing"), None),
        Some(SourceTracks::Unreadable) => (Some("unreadable"), None),
        Some(SourceTracks::Listed { count }) => (Some("listed"), Some(i64::from(*count))),
    };
    let album_links_kind = match &result.album_links {
        AlbumLinks::NotAsked => ALBUM_LINKS_NOT_ASKED,
        AlbumLinks::Read(_) => ALBUM_LINKS_READ,
        AlbumLinks::Unread => ALBUM_LINKS_UNREAD,
    };
    let (media_kind, media_entries): (&str, Vec<(Option<Medium>, u32)>) = match &result.media {
        StatedMedia::Undescribed => (MEDIA_UNDESCRIBED, Vec::new()),
        StatedMedia::PerMedium(entries) => (
            MEDIA_PER_MEDIUM,
            entries.iter().map(|medium| (*medium, 1)).collect(),
        ),
        StatedMedia::Formats(formats) => (
            MEDIA_FORMATS,
            formats
                .iter()
                .map(|format| (format.medium, format.quantity))
                .collect(),
        ),
    };
    let facts = super::super::pressing_columns::FactColumns::of(&result.facts());
    let document_failure =
        super::lookup_failure_columns::failure_columns(result.document_failure.as_ref());
    sql.execute(
        "INSERT INTO import_candidate_match \
             (content_hash, position, pressing, source, release_id, title, artist, year, \
              labels, country, region, status, packaging, discogs_details, \
              media_kind, cover_url, cover_label, cover_source, \
              cover_standing, source_group_id, album_links, source_tracks_kind, \
              source_tracks_count, by_disc_id, by_barcode, by_catalog, by_isrc, by_search, \
              by_pressing, listing, document_failure, \
              document_failure_status, album_first_year, track_titles, notes, named_note) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (content_hash, position) DO UPDATE SET \
             pressing = excluded.pressing, source = excluded.source, \
             release_id = excluded.release_id, title = excluded.title, \
             artist = excluded.artist, year = excluded.year, labels = excluded.labels, \
             country = excluded.country, region = excluded.region, status = excluded.status, \
             packaging = excluded.packaging, discogs_details = excluded.discogs_details, \
             media_kind = excluded.media_kind, cover_url = excluded.cover_url, \
             cover_label = excluded.cover_label, cover_source = excluded.cover_source, \
             cover_standing = excluded.cover_standing, \
             source_group_id = excluded.source_group_id, album_links = excluded.album_links, \
             source_tracks_kind = excluded.source_tracks_kind, \
             source_tracks_count = excluded.source_tracks_count, \
             by_disc_id = excluded.by_disc_id, by_barcode = excluded.by_barcode, \
             by_catalog = excluded.by_catalog, by_isrc = excluded.by_isrc, \
             by_search = excluded.by_search, by_pressing = excluded.by_pressing, \
             listing = excluded.listing, \
             document_failure = excluded.document_failure, \
             document_failure_status = excluded.document_failure_status, \
             album_first_year = excluded.album_first_year, \
             track_titles = excluded.track_titles, notes = excluded.notes, \
             named_note = excluded.named_note",
        params![
            content_hash,
            position,
            pressing,
            result.source.as_str(),
            result.release_id,
            result.title,
            result.artist,
            result.year,
            super::super::pressing_columns::labels_column(&result.labels),
            facts.country,
            facts.region,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            media_kind,
            cover.map(|cover| cover.image.url.as_str()),
            cover.map(|cover| cover.label.as_str()),
            cover.map(|cover| cover.source.as_str()),
            cover.map(|cover| cover.standing.as_str()),
            result.source_group_id,
            album_links_kind,
            tracks_kind,
            tracks_count,
            provenance.map(|provenance| provenance.by_disc_id),
            provenance.map(|provenance| provenance.by_barcode),
            provenance.map(|provenance| provenance.by_catalog),
            provenance.map(|provenance| provenance.by_isrc),
            provenance.map(|provenance| provenance.by_search),
            provenance.map(|provenance| provenance.by_pressing),
            listing,
            document_failure.kind,
            document_failure.status,
            result.album_first_year,
            serde_json::to_string(&result.track_titles).expect("titles serialize"),
            serde_json::to_string(&result.notes).expect("notes serialize"),
            named_note,
        ],
    )?;
    if let Some(cover) = cover {
        for copy in &cover.image.downscaled {
            sql.execute(
                "INSERT INTO import_candidate_match_cover_copy \
                     (content_hash, position, cover_url, max_edge, url) \
                 VALUES (?, ?, ?, ?, ?)",
                params![
                    content_hash,
                    position,
                    cover.image.url,
                    copy.max_edge,
                    copy.url
                ],
            )?;
        }
    }
    for (ordinal, barcode) in result.barcodes.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_barcode (content_hash, position, ordinal, barcode) \
             VALUES (?, ?, ?, ?)",
            params![content_hash, position, ordinal_column(ordinal)?, barcode],
        )?;
    }
    for (ordinal, (medium, quantity)) in media_entries.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_medium \
                 (content_hash, position, media_kind, ordinal, medium, quantity) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position,
                media_kind,
                ordinal_column(ordinal)?,
                medium.map(Medium::key),
                quantity,
            ],
        )?;
    }
    for (ordinal, link) in result.links.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_link (content_hash, position, ordinal, catalog, key) \
             VALUES (?, ?, ?, ?, ?)",
            params![
                content_hash,
                position,
                ordinal_column(ordinal)?,
                link.catalog.as_str(),
                link.key
            ],
        )?;
    }
    for (ordinal, link) in result.album_links.read().iter().enumerate() {
        let StatementColumns {
            stated,
            wikidata_item,
            musicbrainz_release,
            release,
        } = StatementColumns::of(&link.stated);
        sql.execute(
            "INSERT INTO import_candidate_match_album_link \
                 (content_hash, position, ordinal, catalog, key, stated, wikidata_item, \
                  musicbrainz_release, release_catalog, release_key) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                position,
                ordinal_column(ordinal)?,
                link.album.catalog.as_str(),
                link.album.key,
                stated,
                wikidata_item,
                musicbrainz_release,
                release.map(|release| release.catalog.as_str()),
                release.map(|release| release.key.as_str()),
            ],
        )?;
    }
    Ok(())
}

pub(super) fn ordinal_column(ordinal: usize) -> Result<i64, DbError> {
    i64::try_from(ordinal).map_err(|error| {
        DbError::Message(format!(
            "a match's list is longer than SQLite counts: {error}"
        ))
    })
}

/// The stored `listing` values.
const LISTING_OFFERED: &str = "offered";
const LISTING_NARROWED_OUT: &str = "narrowed_out";
const LISTING_UNLISTED: &str = "unlisted";

/// The stored `album_links` values, one per [`AlbumLinks`] shape.
const ALBUM_LINKS_NOT_ASKED: &str = "not_asked";
const ALBUM_LINKS_READ: &str = "read";
const ALBUM_LINKS_UNREAD: &str = "unread";

/// The stored `media_kind` values, one per [`StatedMedia`] shape.
pub(super) const MEDIA_UNDESCRIBED: &str = "undescribed";
pub(super) const MEDIA_PER_MEDIUM: &str = "per_medium";
pub(super) const MEDIA_FORMATS: &str = "formats";

/// One medium row of a match: the media kind it was written for, the carrier
/// it names and how many of it.
pub(super) struct StoredMedium {
    pub(super) kind: String,
    pub(super) medium: Option<Medium>,
    pub(super) quantity: u32,
}

/// The ordinal-ordered rows the child tables hold for one match, read on its
/// row and handed to [`match_of`].
pub(super) struct MatchEntries {
    pub(super) barcodes: Vec<String>,
    /// The downscaled copies of the match's cover.
    pub(super) cover_copies: Vec<DownscaledCopy>,
    /// The kind rides on every medium row, so a row whose kind disagrees with
    /// its match's is unreadable rather than silently reinterpreted.
    pub(super) media: Vec<StoredMedium>,
    pub(super) links: Vec<(String, String)>,
    /// The album link rows, for a match whose album links were read.
    pub(super) album_links: Vec<AlbumLinkRow>,
}

/// Where a stored release is listed, with what an offered or narrowed-out
/// one carries.
pub(super) enum Listing {
    Offered(ListedMatch),
    NarrowedOut(ListedMatch),
    /// Found by a cell of the ledger and listed in neither: only the cell
    /// shows it.
    Unlisted,
}

/// What a release a verdict lists carries beside the release: which lookups
/// named it, and which pressing row of its list the run put it in.
pub(super) struct ListedMatch {
    pub(super) provenance: LookupProvenance,
    /// The row's number within this list, as the run numbered it. Rows are
    /// read, never re-formed: a list of a run's answers does not hold what
    /// the run decided its rows against.
    pub(super) pressing: u32,
    /// The note the folder's text names its row by — see
    /// [`crate::identify::Findings::named_notes`].
    pub(super) named_note: Option<String>,
}

/// One stored release of a verdict, at its position, and where it is listed.
pub(super) struct MatchRow {
    pub(super) position: i64,
    pub(super) result: MetadataResult,
    pub(super) listing: Listing,
}

/// One candidate's stored releases, in position order: the verdict's
/// matches, the lead first, the releases its signals' agreement narrowed
/// out, and the ones only its ledger's cells list.
#[derive(Default)]
pub(crate) struct StoredMatches {
    pub(super) rows: Vec<MatchRow>,
}

/// One match row's own columns, before the child tables' rows are joined
/// to it.
pub(super) struct MatchColumns {
    pub(super) content_hash: String,
    pub(super) position: i64,
    pressing: Option<i64>,
    listing: String,
    media_kind: String,
    /// The stored `album_links` value; [`match_of`] reads it with the album
    /// link rows.
    album_links: String,
    /// The result with its list fields still empty; [`match_of`] fills them.
    result: MetadataResult,
    provenance: Option<LookupProvenance>,
    named_note: Option<String>,
}

/// The match its columns and its child rows describe. The media kind and the
/// medium rows have to agree: a kind with no entries is `undescribed`, a
/// `per_medium` row counts one medium, and a row of another kind is a
/// different match's.
pub(super) fn match_of(columns: MatchColumns, entries: MatchEntries) -> Result<MatchRow, DbError> {
    let MatchColumns {
        content_hash,
        position,
        pressing,
        listing,
        media_kind,
        album_links: album_links_kind,
        mut result,
        provenance,
        named_note,
    } = columns;
    let MatchEntries {
        barcodes,
        cover_copies,
        media,
        links,
        album_links,
    } = entries;
    let mismatch = |what: &str| {
        DbError::Message(format!(
            "match {position} of {content_hash} holds {media_kind} media but {what}"
        ))
    };
    for stored in &media {
        if stored.kind != media_kind {
            return Err(mismatch(&format!("a {} medium row", stored.kind)));
        }
    }
    result.media = match media_kind.as_str() {
        MEDIA_UNDESCRIBED => {
            if !media.is_empty() {
                return Err(mismatch("lists medium rows"));
            }
            StatedMedia::Undescribed
        }
        MEDIA_PER_MEDIUM => StatedMedia::PerMedium(
            media
                .into_iter()
                .map(|stored| {
                    (stored.quantity == 1)
                        .then_some(stored.medium)
                        .ok_or_else(|| mismatch("a medium row counts more than one"))
                })
                .collect::<Result<_, _>>()?,
        ),
        MEDIA_FORMATS => StatedMedia::Formats(
            media
                .into_iter()
                .map(|stored| StatedFormat {
                    medium: stored.medium,
                    quantity: stored.quantity,
                })
                .collect(),
        ),
        other => return Err(unreadable("media_kind", other)),
    };
    result.barcodes = barcodes;
    // Copy rows reference their match's cover, so a match without one has none.
    if let Some(cover) = &mut result.cover_art {
        cover.image = RemoteImageSet::with_copies(cover.image.url.clone(), cover_copies);
    }
    result.links = links
        .into_iter()
        .map(|(catalog, key)| Ok(MetadataRef::new(source_of(&catalog)?, key)))
        .collect::<Result<_, DbError>>()?;
    result.album_links = match album_links_kind.as_str() {
        ALBUM_LINKS_READ => AlbumLinks::Read(
            album_links
                .into_iter()
                .map(AlbumLinkRow::link)
                .collect::<Result<_, DbError>>()?,
        ),
        ALBUM_LINKS_NOT_ASKED | ALBUM_LINKS_UNREAD if !album_links.is_empty() => {
            return Err(DbError::Message(format!(
                "match {position} of {content_hash} holds album link rows but its album links \
                 are {album_links_kind}"
            )))
        }
        ALBUM_LINKS_NOT_ASKED => AlbumLinks::NotAsked,
        ALBUM_LINKS_UNREAD => AlbumLinks::Unread,
        other => return Err(unreadable("album_links", other)),
    };
    let listed = |pressing: Option<i64>, provenance: Option<LookupProvenance>| {
        let (Some(pressing), Some(provenance)) = (pressing, provenance) else {
            return Err(DbError::Message(format!(
                "match {position} of {content_hash} is {listing} but names no pressing row or \
                 no lookups"
            )));
        };
        let pressing = u32::try_from(pressing).map_err(|error| {
            DbError::Message(format!(
                "match {position} of {content_hash} names pressing row {pressing}: {error}"
            ))
        })?;
        Ok(ListedMatch {
            provenance,
            pressing,
            named_note: None,
        })
    };
    let listing = match listing.as_str() {
        LISTING_OFFERED => Listing::Offered(ListedMatch {
            named_note,
            ..listed(pressing, provenance)?
        }),
        LISTING_NARROWED_OUT if named_note.is_none() => {
            Listing::NarrowedOut(listed(pressing, provenance)?)
        }
        LISTING_UNLISTED if pressing.is_none() && provenance.is_none() && named_note.is_none() => {
            Listing::Unlisted
        }
        LISTING_NARROWED_OUT | LISTING_UNLISTED => {
            return Err(DbError::Message(format!(
                "match {position} of {content_hash} is {listing} but holds what only a listed \
                 or offered release holds"
            )))
        }
        other => return Err(unreadable("listing", other)),
    };
    Ok(MatchRow {
        position,
        result,
        listing,
    })
}

pub(super) fn read_match_row(row: &Row<'_>) -> Result<MatchColumns, DbError> {
    let cover_url: Option<String> = row.get("cover_url")?;
    let cover_source: Option<String> = row.get("cover_source")?;
    let cover_art = match (cover_url, cover_source) {
        // The copies are child rows; [`match_of`] adds them.
        (Some(url), Some(source)) => Some(RemoteCover {
            image: RemoteImageSet::original(url),
            label: row.get("cover_label")?,
            source: source_of(&source)?,
            standing: standing_of(&row.get::<_, String>("cover_standing")?)?,
        }),
        // The table sets and clears the four cover columns together.
        _ => None,
    };
    let tracks_kind: Option<String> = row.get("source_tracks_kind")?;
    let source_tracks = match tracks_kind.as_deref() {
        None => None,
        Some("nothing") => Some(SourceTracks::Nothing),
        Some("unreadable") => Some(SourceTracks::Unreadable),
        Some("listed") => {
            let count: i64 = row
                .get::<_, Option<i64>>("source_tracks_count")?
                .ok_or_else(|| {
                    DbError::Message("a listed source tracklist states no count".to_string())
                })?;
            Some(SourceTracks::Listed {
                count: u32::try_from(count).map_err(|error| {
                    DbError::Message(format!(
                        "a source tracklist's count is out of range: {error}"
                    ))
                })?,
            })
        }
        Some(other) => return Err(unreadable("source_tracks_kind", other)),
    };
    let source: String = row.get("source")?;
    let facts = super::super::pressing_columns::read_facts_without_media(row, "")?;
    Ok(MatchColumns {
        content_hash: row.get("content_hash")?,
        position: row.get("position")?,
        pressing: row.get("pressing")?,
        listing: row.get("listing")?,
        media_kind: row.get("media_kind")?,
        album_links: row.get("album_links")?,
        result: MetadataResult {
            source: source_of(&source)?,
            release_id: row.get("release_id")?,
            title: row.get("title")?,
            artist: row.get("artist")?,
            year: row.get("year")?,
            labels: super::super::pressing_columns::read_labels(row, "labels")?,
            area: facts.area,
            status: facts.status,
            packaging: facts.packaging,
            discogs_details: facts.discogs_details,
            barcodes: Vec::new(),
            media: StatedMedia::Undescribed,
            links: Vec::new(),
            cover_art,
            source_group_id: row.get("source_group_id")?,
            album_links: AlbumLinks::NotAsked,
            source_tracks,
            document_failure: super::lookup_failure_columns::failure_of(
                row.get("document_failure")?,
                row.get("document_failure_status")?,
            )?,
            album_first_year: row.get("album_first_year")?,
            track_titles: read_strings(row, "track_titles")?,
            notes: read_strings(row, "notes")?,
        },
        provenance: read_provenance(row)?,
        named_note: row.get("named_note")?,
    })
}

/// Which lookups returned a match, from its six columns: all set for a
/// listed release, all NULL for an unlisted one.
fn read_provenance(row: &Row<'_>) -> Result<Option<LookupProvenance>, DbError> {
    let columns: [Option<bool>; 6] = [
        row.get("by_disc_id")?,
        row.get("by_barcode")?,
        row.get("by_catalog")?,
        row.get("by_isrc")?,
        row.get("by_search")?,
        row.get("by_pressing")?,
    ];
    match columns {
        [None, None, None, None, None, None] => Ok(None),
        [Some(by_disc_id), Some(by_barcode), Some(by_catalog), Some(by_isrc), Some(by_search), Some(by_pressing)] => {
            Ok(Some(LookupProvenance {
                by_disc_id,
                by_barcode,
                by_catalog,
                by_isrc,
                by_search,
                by_pressing,
            }))
        }
        _ => Err(DbError::Message(
            "a match states some of the lookups that returned it and not others".to_string(),
        )),
    }
}

/// Read a match's column that holds a JSON array of strings.
fn read_strings(row: &Row<'_>, column: &str) -> Result<Vec<String>, DbError> {
    let stored: String = row.get(column)?;
    serde_json::from_str(&stored)
        .map_err(|error| DbError::Message(format!("{column} {stored:?}: {error}")))
}

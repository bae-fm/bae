//! The release-match rows: one per release a verdict or a run in flight
//! found, or a typed search's catalog returned, where it is listed, what the
//! record said and the rows hanging off it, written and read back.
//! Its tables are `match_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("match_rows.sql");

use super::verdict_rows::Owner;
use super::*;
use crate::tables::album_link_rows::AlbumLinkRow;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::records::MetadataRef;
use bae_catalogs::remote_image::{CoverStanding, DownscaledCopy, RemoteCover, RemoteImageSet};
use bae_catalogs::search::{MetadataResult, SourceTracks};
use bae_types::pressing::{Medium, StatedFormat, StatedMedia};
use std::str::FromStr;

fn source_of(stored: &str) -> Result<LookupCatalog, DbError> {
    LookupCatalog::from_str(stored)
        .map_err(|_unknown| DbError::unreadable("a match's stored lookup catalog", stored))
}

fn catalog_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored)
        .map_err(|_unknown| DbError::unreadable("a match's stored catalog", stored))
}

fn standing_of(stored: &str) -> Result<CoverStanding, DbError> {
    CoverStanding::from_column(stored)
        .ok_or_else(|| DbError::unreadable("import_candidate_match.cover_standing", stored))
}

/// Whose match rows these are: a verdict-shaped owner's — a candidate's
/// verdict or a run in flight — or a typed search's, whose rows hang off the
/// catalog part that returned them.
#[derive(Clone, Copy, Debug)]
pub(super) enum MatchOwner<'a> {
    Verdict(Owner<'a>),
    Search(i64),
}

impl MatchOwner<'_> {
    /// The column that names this owner on every row it owns.
    pub(super) fn column(self) -> &'static str {
        match self {
            Self::Verdict(owner) => owner.column(),
            Self::Search(_) => "search_run",
        }
    }

    fn content_hash(self) -> Option<String> {
        match self {
            Self::Verdict(owner) => owner.content_hash(),
            Self::Search(_) => None,
        }
    }

    fn run(self) -> Option<i64> {
        match self {
            Self::Verdict(owner) => owner.run(),
            Self::Search(_) => None,
        }
    }

    fn search(self) -> Option<i64> {
        match self {
            Self::Verdict(_) => None,
            Self::Search(search) => Some(search),
        }
    }
}

/// Where a stored release is listed, with what an offered or narrowed-out
/// one carries: its pressing row, and for an offered one the note the
/// folder's text names its row by. A search's release is returned, and no
/// pick has read it when it lands.
#[derive(Clone, Copy)]
pub(super) enum Written<'a> {
    Offered(u32, Option<&'a str>),
    NarrowedOut(u32),
    Unlisted,
    Returned,
}

/// One match at `position`, in place of the one there, with the rows under
/// it. The rows under the match that stood there are already gone. Only a
/// search writes a returned release, and only a verdict-shaped owner a listed
/// or unlisted one.
/// Delete the rows below every match the verdict `owner` holds, which name
/// each match by its owner and position, so the matches a write stores next
/// carry only their own.
pub(super) fn delete_verdict_match_entries(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
) -> Result<(), DbError> {
    let column = owner.column();
    for table in [
        "import_candidate_match_cover_copy",
        "import_candidate_match_label",
        "import_candidate_match_barcode",
        "import_candidate_match_medium",
        "import_candidate_match_link",
        "import_candidate_match_album_link",
    ] {
        sql.execute(&format!("DELETE FROM {table} WHERE {column} = ?"), params![owner])?;
    }
    Ok(())
}

/// Delete the verdict `owner`'s matches from `position` on: those an earlier
/// verdict held past the ones a write just stored.
pub(super) fn delete_verdict_matches_from(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
    position: usize,
) -> Result<(), DbError> {
    let column = owner.column();
    sql.execute(
        &format!("DELETE FROM import_candidate_match WHERE {column} = ? AND position >= ?"),
        params![owner, ordinal_column(position)?],
    )?;
    Ok(())
}

/// Delete every release a search part asking `catalog` returned.
pub(super) fn delete_search_matches_of(
    sql: &CovenWrite<'_>,
    catalog: LookupCatalog,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_match WHERE search_run IS NOT NULL AND source = ?",
        [catalog.as_str()],
    )?;
    Ok(())
}

pub(super) fn store_match(
    sql: &CovenWrite<'_>,
    owner: MatchOwner<'_>,
    position: usize,
    result: &MetadataResult,
    listing: Written<'_>,
) -> Result<(), DbError> {
    let position = ordinal_column(position)?;
    let (listing, pressing, named_note) = match (owner, listing) {
        (MatchOwner::Verdict(_), Written::Offered(pressing, named_note)) => {
            (LISTING_OFFERED, Some(pressing), named_note)
        }
        (MatchOwner::Verdict(_), Written::NarrowedOut(pressing)) => {
            (LISTING_NARROWED_OUT, Some(pressing), None)
        }
        (MatchOwner::Verdict(_), Written::Unlisted) => (LISTING_UNLISTED, None, None),
        (MatchOwner::Search(_), Written::Returned) => (LISTING_RETURNED, None, None),
        (MatchOwner::Verdict(_), Written::Returned)
        | (
            MatchOwner::Search(_),
            Written::Offered(..) | Written::NarrowedOut(_) | Written::Unlisted,
        ) => panic!("{owner:?} stores a release listed as no owner of its kind lists one"),
    };
    // A search's release hangs off the part that answered with it, which
    // holds releases only while it stands answered.
    let (search_state, picked) = match owner {
        MatchOwner::Search(_) => (Some(SEARCH_STATE_DONE), Some(false)),
        MatchOwner::Verdict(_) => (None, None),
    };
    let cover = result.cover_art.as_ref();
    let (tracks_kind, tracks_count) = match &result.source_tracks {
        None => (None, None),
        Some(SourceTracks::Nothing) => (Some("nothing"), None),
        Some(SourceTracks::Unreadable) => (Some("unreadable"), None),
        Some(SourceTracks::Listed { count }) => (Some("listed"), Some(i64::from(*count))),
    };
    let album_links_kind = album_links_column(&result.album_links);
    let (media_kind, media_entries): (&str, Vec<(Option<Medium>, u32)>) = match &result.media {
        StatedMedia::Undescribed => (MEDIA_UNDESCRIBED, Vec::new()),
        StatedMedia::PerMedium(entries) => {
            (MEDIA_PER_MEDIUM, entries.iter().map(|medium| (*medium, 1)).collect())
        }
        StatedMedia::Formats(formats) => {
            (MEDIA_FORMATS, formats.iter().map(|format| (format.medium, format.quantity)).collect())
        }
    };
    let facts = bae_records::records::pressing_columns::FactColumns::of(&result.facts());
    let document_failure =
        super::lookup_failure_columns::failure_columns(result.document_failure.as_ref());
    let (content_hash, run, search, column) =
        (owner.content_hash(), owner.run(), owner.search(), owner.column());
    sql.execute(
        &format!(
            "INSERT INTO import_candidate_match \
             (content_hash, identify_run, search_run, search_state, picked, position, pressing, \
              source, release_id, title, artist, year, \
              country, region, status, packaging, discogs_details, \
              media_kind, cover_url, cover_label, cover_source, \
              cover_standing, source_group_id, album_links, source_tracks_kind, \
              source_tracks_count, listing, document_failure, document_failure_status, \
              named_note) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
                 ?, ?, ?, ?) \
         ON CONFLICT ({column}, position) DO UPDATE SET \
             pressing = excluded.pressing, source = excluded.source, \
             release_id = excluded.release_id, title = excluded.title, \
             artist = excluded.artist, year = excluded.year, \
             country = excluded.country, region = excluded.region, status = excluded.status, \
             packaging = excluded.packaging, discogs_details = excluded.discogs_details, \
             media_kind = excluded.media_kind, cover_url = excluded.cover_url, \
             cover_label = excluded.cover_label, cover_source = excluded.cover_source, \
             cover_standing = excluded.cover_standing, \
             source_group_id = excluded.source_group_id, album_links = excluded.album_links, \
             source_tracks_kind = excluded.source_tracks_kind, \
             source_tracks_count = excluded.source_tracks_count, \
             listing = excluded.listing, \
             document_failure = excluded.document_failure, \
             document_failure_status = excluded.document_failure_status, \
             named_note = excluded.named_note"
        ),
        params![
            content_hash,
            run,
            search,
            search_state,
            picked,
            position,
            pressing,
            result.source.as_str(),
            result.release_id,
            result.title,
            result.artist,
            result.year,
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
            listing,
            document_failure.kind,
            document_failure.status,
            named_note,
        ],
    )?;
    if let Some(cover) = cover {
        for copy in &cover.image.downscaled {
            sql.execute(
                "INSERT INTO import_candidate_match_cover_copy \
                     (content_hash, identify_run, search_run, position, cover_url, max_edge, \
                      url) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    content_hash,
                    run,
                    search,
                    position,
                    cover.image.url,
                    copy.max_edge,
                    copy.url
                ],
            )?;
        }
    }
    for (ordinal, label) in result.labels.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_label \
                 (content_hash, identify_run, search_run, position, ordinal, name, \
                  catalog_number, musicbrainz_label_id, discogs_label_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                run,
                search,
                position,
                ordinal_column(ordinal)?,
                label.name(),
                label.catalog_number(),
                label.musicbrainz_label_id(),
                label.discogs_label_id()
            ],
        )?;
    }
    for (ordinal, barcode) in result.barcodes.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_barcode \
                 (content_hash, identify_run, search_run, position, ordinal, barcode) \
             VALUES (?, ?, ?, ?, ?, ?)",
            params![content_hash, run, search, position, ordinal_column(ordinal)?, barcode],
        )?;
    }
    for (ordinal, (medium, quantity)) in media_entries.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_medium \
                 (content_hash, identify_run, search_run, position, media_kind, ordinal, medium, \
                  quantity) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                run,
                search,
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
            "INSERT INTO import_candidate_match_link \
                 (content_hash, identify_run, search_run, position, ordinal, catalog, key) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                run,
                search,
                position,
                ordinal_column(ordinal)?,
                link.catalog.as_str(),
                link.key
            ],
        )?;
    }
    store_album_link_rows(sql, owner, position, &result.album_links)
}

/// The album link rows of the match at `position`, which holds none yet.
fn store_album_link_rows(
    sql: &CovenWrite<'_>,
    owner: MatchOwner<'_>,
    position: i64,
    links: &AlbumLinks,
) -> Result<(), DbError> {
    for (ordinal, album) in links.read().iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_match_album_link \
                 (content_hash, identify_run, search_run, position, ordinal, catalog, key) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                owner.content_hash(),
                owner.run(),
                owner.search(),
                position,
                ordinal_column(ordinal)?,
                album.catalog.as_str(),
                album.key,
            ],
        )?;
    }
    Ok(())
}

/// Mark every row of `release` the search `search_run` holds picked, each
/// stating `links` as its album on the other catalog, in place of what it
/// stated.
pub(super) fn store_picked(
    sql: &CovenWrite<'_>,
    search_run: i64,
    release: &bae_catalogs::records::LookupRef,
    links: &AlbumLinks,
) -> Result<(), DbError> {
    let positions = sql.query(
        "SELECT position FROM import_candidate_match \
         WHERE search_run = ? AND source = ? AND release_id = ?",
        params![search_run, release.catalog.as_str(), release.key],
        |row| row.get::<_, i64>(0),
    )?;
    for position in positions {
        sql.execute(
            "DELETE FROM import_candidate_match_album_link WHERE search_run = ? AND position = ?",
            params![search_run, position],
        )?;
        sql.execute(
            "UPDATE import_candidate_match SET picked = 1, album_links = ? \
             WHERE search_run = ? AND position = ?",
            params![album_links_column(links), search_run, position],
        )?;
        store_album_link_rows(sql, MatchOwner::Search(search_run), position, links)?;
    }
    Ok(())
}

/// The stored `album_links` value of `links`.
fn album_links_column(links: &AlbumLinks) -> &'static str {
    match links {
        AlbumLinks::NotAsked => ALBUM_LINKS_NOT_ASKED,
        AlbumLinks::Read(_) => ALBUM_LINKS_READ,
        AlbumLinks::Unread => ALBUM_LINKS_UNREAD,
    }
}

pub(super) fn ordinal_column(ordinal: usize) -> Result<i64, DbError> {
    to_i64(ordinal, "a match's place in its list")
}

/// The stored `listing` values.
const LISTING_OFFERED: &str = "offered";
const LISTING_NARROWED_OUT: &str = "narrowed_out";
const LISTING_UNLISTED: &str = "unlisted";
const LISTING_RETURNED: &str = "returned";

/// The `search_state` a search's release names its answered part by.
const SEARCH_STATE_DONE: &str = "done";

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
    pub(super) labels: Vec<bae_types::pressing::ReleaseLabel>,
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
#[derive(Debug)]
pub(super) enum Listing {
    Offered(ListedMatch),
    NarrowedOut(ListedMatch),
    /// Found by a cell of the ledger and listed in neither: only the cell
    /// shows it.
    Unlisted,
    /// Returned by a typed search's catalog, and whether a pick has read its
    /// documents.
    Returned {
        picked: bool,
    },
}

/// What a release a verdict lists carries beside the release: which pressing
/// row of its list the run put it in.
#[derive(Debug)]
pub(super) struct ListedMatch {
    /// The row's number within this list, as the run numbered it. Rows are
    /// read, never re-formed: a list of a run's answers does not hold what
    /// the run decided its rows against.
    pub(super) pressing: u32,
    /// The note the folder's text names its row by — see
    /// [`bae_identify::Findings::named_notes`].
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
    picked: Option<bool>,
    media_kind: String,
    /// The stored `album_links` value; [`match_of`] reads it with the album
    /// link rows.
    album_links: String,
    /// The result with its list fields still empty; [`match_of`] fills them.
    result: MetadataResult,
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
        picked,
        media_kind,
        album_links: album_links_kind,
        mut result,
        named_note,
    } = columns;
    let MatchEntries { labels, barcodes, cover_copies, media, links, album_links } = entries;
    let mismatch = |what: &str, stored: &dyn std::fmt::Debug| {
        DbError::unreadable(
            format!("the {what} of match {position} of {content_hash}, of {media_kind} media"),
            stored,
        )
    };
    for stored in &media {
        if stored.kind != media_kind {
            return Err(mismatch("kind of a medium row", &stored.kind));
        }
    }
    result.media = match media_kind.as_str() {
        MEDIA_UNDESCRIBED => {
            if !media.is_empty() {
                return Err(mismatch("medium rows", &media.len()));
            }
            StatedMedia::Undescribed
        }
        MEDIA_PER_MEDIUM => StatedMedia::PerMedium(
            media
                .into_iter()
                .map(|stored| {
                    (stored.quantity == 1)
                        .then_some(stored.medium)
                        .ok_or_else(|| mismatch("quantity of a medium row", &stored.quantity))
                })
                .collect::<Result<_, _>>()?,
        ),
        MEDIA_FORMATS => StatedMedia::Formats(
            media
                .into_iter()
                .map(|stored| StatedFormat { medium: stored.medium, quantity: stored.quantity })
                .collect(),
        ),
        other => return Err(DbError::unreadable("import_candidate_match.media_kind", other)),
    };
    result.labels = labels;
    result.barcodes = barcodes;
    // Copy rows reference their match's cover, so a match without one has none.
    if let Some(cover) = &mut result.cover_art {
        cover.image = RemoteImageSet::with_copies(cover.image.url.clone(), cover_copies);
    }
    result.links = links
        .into_iter()
        .map(|(catalog, key)| Ok(MetadataRef::new(catalog_of(&catalog)?, key)))
        .collect::<Result<_, DbError>>()?;
    result.album_links = match album_links_kind.as_str() {
        ALBUM_LINKS_READ => AlbumLinks::Read(
            album_links.into_iter().map(AlbumLinkRow::album).collect::<Result<_, DbError>>()?,
        ),
        ALBUM_LINKS_NOT_ASKED | ALBUM_LINKS_UNREAD if !album_links.is_empty() => {
            return Err(DbError::unreadable(
                format!(
                    "the album link rows of match {position} of {content_hash}, whose album \
                     links are {album_links_kind}"
                ),
                album_links.len(),
            ))
        }
        ALBUM_LINKS_NOT_ASKED => AlbumLinks::NotAsked,
        ALBUM_LINKS_UNREAD => AlbumLinks::Unread,
        other => return Err(DbError::unreadable("import_candidate_match.album_links", other)),
    };
    let listed = |pressing: Option<i64>| {
        let Some(pressing) = pressing else {
            return Err(DbError::unreadable(
                format!("the pressing row of match {position} of {content_hash}, {listing}"),
                None::<i64>,
            ));
        };
        let pressing = to_u32(
            pressing,
            &format!("the pressing row match {position} of {content_hash} names"),
        )?;
        Ok(ListedMatch { pressing, named_note: None })
    };
    let listing = match listing.as_str() {
        LISTING_OFFERED => Listing::Offered(ListedMatch { named_note, ..listed(pressing)? }),
        LISTING_NARROWED_OUT if named_note.is_none() => Listing::NarrowedOut(listed(pressing)?),
        LISTING_UNLISTED if pressing.is_none() && named_note.is_none() => Listing::Unlisted,
        LISTING_RETURNED if pressing.is_none() && named_note.is_none() => Listing::Returned {
            picked: picked.ok_or_else(|| {
                DbError::unreadable(
                    format!("whether returned match {position} of {content_hash} was picked"),
                    None::<bool>,
                )
            })?,
        },
        LISTING_NARROWED_OUT | LISTING_UNLISTED | LISTING_RETURNED => {
            return Err(DbError::unreadable(
                format!(
                    "the pressing row and named note of match {position} of {content_hash}, \
                     {listing}"
                ),
                (pressing, &named_note),
            ))
        }
        other => return Err(DbError::unreadable("import_candidate_match.listing", other)),
    };
    Ok(MatchRow { position, result, listing })
}

pub(super) fn read_match_row(row: &Row<'_>) -> Result<MatchColumns, DbError> {
    let cover_url: Option<String> = row.get("cover_url")?;
    let cover_source: Option<String> = row.get("cover_source")?;
    let cover_art = match (cover_url, cover_source) {
        // The copies are child rows; [`match_of`] adds them.
        (Some(url), Some(source)) => Some(RemoteCover {
            image: RemoteImageSet::original(url),
            label: row.get("cover_label")?,
            source: catalog_of(&source)?,
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
            let count: i64 =
                row.get::<_, Option<i64>>("source_tracks_count")?.ok_or_else(|| {
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
    let source: String = row.get("source")?;
    let facts = bae_records::records::pressing_columns::read_facts_without_media(row, "")?;
    Ok(MatchColumns {
        content_hash: row.get("content_hash")?,
        position: row.get("position")?,
        pressing: row.get("pressing")?,
        listing: row.get("listing")?,
        picked: row.get("picked")?,
        media_kind: row.get("media_kind")?,
        album_links: row.get("album_links")?,
        result: MetadataResult {
            source: source_of(&source)?,
            release_id: row.get("release_id")?,
            title: row.get("title")?,
            artist: row.get("artist")?,
            year: row.get("year")?,
            labels: Vec::new(),
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
            // Read from the stored catalog release the match names, so a
            // later fetch of it shows.
            album_first_year: row.get("album_first_year")?,
            // Only a run's ranking reads these, so a verdict keeps neither.
            track_titles: Vec::new(),
            notes: Vec::new(),
        },
        named_note: row.get("named_note")?,
    })
}

/// A match's own columns joined as `found`, and its album's first year from
/// the stored catalog release it names, joined as `fetched`.
pub(super) const MATCH_COLUMNS: &str =
    "found.position, found.pressing, found.source, found.release_id, \
     found.title, found.artist, found.year, found.country, found.region, \
     found.status, found.packaging, found.discogs_details, found.media_kind, found.cover_url, \
     found.cover_label, found.cover_source, found.cover_standing, found.source_group_id, \
     found.album_links, found.source_tracks_kind, found.source_tracks_count, found.listing, \
     found.picked, found.document_failure, found.document_failure_status, \
     fetched.album_first_year, found.named_note";

/// What hangs off a match joined as `found`, each list a JSON array in its
/// stored order: its labels, its barcodes, its cover's copies, its media, its
/// links and its album links.
pub(super) fn match_entries_columns() -> String {
    let child = |table: &str, value: &str, order: &str| {
        format!(
            "(SELECT json_group_array({value} ORDER BY {order}) FROM {table} AS entry \
              WHERE entry.content_hash IS found.content_hash \
                AND entry.identify_run IS found.identify_run \
                AND entry.search_run IS found.search_run \
                AND entry.position = found.position)"
        )
    };
    format!(
        "{labels} AS match_labels, {barcodes} AS match_barcodes, \
         {copies} AS match_cover_copies, \
         {media} AS match_media, {links} AS match_links, {album_links} AS match_album_links",
        labels = child(
            "import_candidate_match_label",
            "json_object('name', entry.name, 'catalog_number', entry.catalog_number, \
                 'musicbrainz_label_id', entry.musicbrainz_label_id, \
                 'discogs_label_id', entry.discogs_label_id)",
            "entry.ordinal"
        ),
        barcodes = child("import_candidate_match_barcode", "entry.barcode", "entry.ordinal"),
        copies = crate::tables::downscaled_copies::copies_json_sql(
            "import_candidate_match_cover_copy",
            "copy.content_hash IS found.content_hash AND copy.identify_run IS found.identify_run \
             AND copy.search_run IS found.search_run AND copy.position = found.position",
        ),
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

/// One match's child lists as SQLite hands them over, still JSON.
pub(super) struct StoredMatchEntries {
    labels: String,
    barcodes: String,
    cover_copies: String,
    media: String,
    links: String,
    album_links: String,
}

pub(super) fn read_match_entries(row: &Row<'_>) -> rusqlite::Result<StoredMatchEntries> {
    Ok(StoredMatchEntries {
        labels: row.get("match_labels")?,
        barcodes: row.get("match_barcodes")?,
        cover_copies: row.get("match_cover_copies")?,
        media: row.get("match_media")?,
        links: row.get("match_links")?,
        album_links: row.get("match_album_links")?,
    })
}

/// One medium of a match as [`match_entries_columns`] holds it.
#[derive(serde::Deserialize)]
struct StoredMatchMedium {
    kind: String,
    medium: Option<String>,
    quantity: u32,
}

impl StoredMatchEntries {
    pub(super) fn entries(self) -> Result<MatchEntries, DbError> {
        fn list<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> Result<Vec<T>, DbError> {
            serde_json::from_str(json).map_err(|error| {
                DbError::unreadable_json(format!("a match's stored {what}"), json, error)
            })
        }
        Ok(MatchEntries {
            labels: list(&self.labels, "labels")?,
            barcodes: list(&self.barcodes, "barcodes")?,
            cover_copies: crate::tables::downscaled_copies::copies_of(&self.cover_copies)?,
            media: list::<StoredMatchMedium>(&self.media, "media")?
                .into_iter()
                .map(|stored| {
                    Ok(StoredMedium {
                        kind: stored.kind,
                        medium: stored
                            .medium
                            .map(|key| {
                                bae_types::pressing::Medium::from_key(&key).ok_or_else(|| {
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
            links: list(&self.links, "links")?,
            album_links: list(&self.album_links, "album links")?,
        })
    }
}

/// The releases of every run in flight, or of `key`'s runs alone, by run,
/// each run's in position order.
pub(super) fn load_run_matches_on(
    sql: &impl coven::SqlRead,
    key: Option<&crate::CandidateKey>,
) -> Result<HashMap<i64, StoredMatches>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT 'run ' || found.identify_run AS content_hash, \
                    found.identify_run AS match_run, {MATCH_COLUMNS}, {entries} \
             FROM import_candidate_match AS found \
             JOIN import_identify_run AS run ON run.identify_run = found.identify_run \
             LEFT JOIN source_release AS fetched \
                 ON fetched.catalog = found.source AND fetched.release_id = found.release_id \
             WHERE ?1 IS NULL OR run.candidate_key = ?1 \
             ORDER BY found.identify_run, found.position",
            entries = match_entries_columns(),
        ),
        params![key],
        |row| {
            Ok((
                row.get::<_, i64>("match_run")?,
                read_match_row(row).and_then(|columns| {
                    Ok((columns, read_match_entries(row).map_err(DbError::from)?))
                }),
            ))
        },
    )?;
    let mut matches: HashMap<i64, StoredMatches> = HashMap::new();
    for (run, found) in rows {
        let (columns, entries) = found?;
        matches.entry(run).or_default().rows.push(match_of(columns, entries.entries()?)?);
    }
    Ok(matches)
}

/// The releases every typed search holds, or `key`'s search alone, by search,
/// each search's under its catalog parts in their order, each part's in the
/// order it returned them.
pub(super) fn load_search_matches_on(
    sql: &impl coven::SqlRead,
    key: Option<&crate::CandidateKey>,
) -> Result<HashMap<i64, Vec<MatchRow>>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT 'search ' || found.search_run AS content_hash, \
                    found.search_run AS match_search, {MATCH_COLUMNS}, {entries} \
             FROM import_candidate_match AS found \
             JOIN import_candidate_search AS search ON search.search_run = found.search_run \
             JOIN import_candidate_search_source AS part \
                 ON part.search_run = found.search_run AND part.catalog = found.source \
             LEFT JOIN source_release AS fetched \
                 ON fetched.catalog = found.source AND fetched.release_id = found.release_id \
             WHERE ?1 IS NULL OR search.candidate_key = ?1 \
             ORDER BY found.search_run, part.position, found.position",
            entries = match_entries_columns(),
        ),
        params![key],
        |row| {
            Ok((
                row.get::<_, i64>("match_search")?,
                read_match_row(row).and_then(|columns| {
                    Ok((columns, read_match_entries(row).map_err(DbError::from)?))
                }),
            ))
        },
    )?;
    let mut matches: HashMap<i64, Vec<MatchRow>> = HashMap::new();
    for (search, found) in rows {
        let (columns, entries) = found?;
        matches.entry(search).or_default().push(match_of(columns, entries.entries()?)?);
    }
    Ok(matches)
}

#[cfg(test)]
#[path = "match_rows_tests.rs"]
mod tests;

//! The release-match rows: one per release a verdict or a run in flight
//! found, or a typed search's catalog returned, where it is listed, what the
//! record said and the rows hanging off it, written and read back.
//! Its tables are `match_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("match_rows.sql");

use super::verdict_rows::Owner;
use super::*;
use crate::tables::album_link_rows::{album_links_column, album_links_of, AlbumLinkRow};
use crate::tables::cover_labels;
use crate::tables::stored_credits;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::records::MetadataRef;
use bae_catalogs::remote_image::{CoverStanding, DownscaledCopy, RemoteImageSet};
use bae_catalogs::search::{MetadataResult, SourceTracks};
use bae_types::pressing::{Medium, StatedFormat, StatedMedia};
use std::str::FromStr;

pub(super) fn source_of(stored: &str) -> Result<LookupCatalog, DbError> {
    LookupCatalog::from_str(stored)
        .map_err(|_unknown| DbError::unreadable("a match's stored lookup catalog", stored))
}

pub(super) fn catalog_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored)
        .map_err(|_unknown| DbError::unreadable("a match's stored catalog", stored))
}

pub(super) fn standing_of(stored: &str) -> Result<CoverStanding, DbError> {
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

    pub(super) fn content_hash(self) -> Option<String> {
        match self {
            Self::Verdict(owner) => owner.content_hash(),
            Self::Search(_) => None,
        }
    }

    pub(super) fn run(self) -> Option<i64> {
        match self {
            Self::Verdict(owner) => owner.run(),
            Self::Search(_) => None,
        }
    }

    pub(super) fn search(self) -> Option<i64> {
        match self {
            Self::Verdict(_) => None,
            Self::Search(search) => Some(search),
        }
    }
}

/// Where a release of a verdict-shaped owner stands in its list as the
/// rules laid the list out, and what the folder's text agrees with about it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Placed {
    pub(super) place: super::layout::Place,
    pub(super) agreements: bae_identify::Agreements,
}

/// Where a stored release is listed, with what an offered or narrowed-out
/// one carries: where the list's layout puts it, its pressing row included,
/// and for an offered one the note the folder's text names its row by. A search's
/// release is returned, and no pick has read it when it lands.
#[derive(Clone, Copy)]
pub(super) enum Written<'a> {
    Offered(Placed, Option<&'a str>),
    NarrowedOut(Placed),
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
    // `store_match` writes a search's release into its own row and card, and
    // the same write then groups the whole list again before it ends.
    let (listing, named_note, placed) = match (owner, listing) {
        (MatchOwner::Verdict(_), Written::Offered(placed, named_note)) => {
            (LISTING_OFFERED, named_note, Some(placed))
        }
        (MatchOwner::Verdict(_), Written::NarrowedOut(placed)) => {
            (LISTING_NARROWED_OUT, None, Some(placed))
        }
        (MatchOwner::Verdict(_), Written::Unlisted) => (LISTING_UNLISTED, None, None),
        (MatchOwner::Search(_), Written::Returned) => (
            LISTING_RETURNED,
            None,
            Some(Placed {
                place: super::layout::Place {
                    pressing: to_u32(position, "a search release's row in its list")?,
                    card: to_u32(position, "a search release's place in its list")?,
                    record_order: 0,
                    section: 0,
                    row_order: 0,
                },
                agreements: bae_identify::Agreements::NONE,
            }),
        ),
        (MatchOwner::Verdict(_), Written::Returned)
        | (
            MatchOwner::Search(_),
            Written::Offered(..) | Written::NarrowedOut(..) | Written::Unlisted,
        ) => panic!("{owner:?} stores a release listed as no owner of its kind lists one"),
    };
    let pressing = placed.map(|placed| i64::from(placed.place.pressing));
    let own_card = placed.map(|placed| i64::from(placed.place.card));
    let record_order = placed.map(|placed| i64::from(placed.place.record_order));
    let section = placed.map(|placed| i64::from(placed.place.section));
    let row_order = placed.map(|placed| i64::from(placed.place.row_order));
    let agrees = placed.map(|placed| placed.agreements).unwrap_or(bae_identify::Agreements::NONE);
    // A search's release hangs off the part that answered with it, which
    // holds releases only while it stands answered.
    let search_state = match owner {
        MatchOwner::Search(_) => Some(SEARCH_STATE_DONE),
        MatchOwner::Verdict(_) => None,
    };
    let cover = result.cover_art.as_ref();
    let cover_label = cover.map(|cover| cover_labels::label_json(&cover.label)).transpose()?;
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
             (content_hash, identify_run, search_run, search_state, position, pressing, card, \
              record_order, section, row_order, agrees_disc_id, agrees_barcode, agrees_catalog, agrees_label, \
              agrees_year, agrees_country, agrees_title, agrees_artist, \
              source, release_id, title, credits, year, \
              country, region, status, packaging, discogs_details, \
              media_kind, cover_url, cover_label, cover_source, \
              cover_standing, source_group_id, album_links, source_tracks_kind, \
              source_tracks_count, listing, document_failure, document_failure_status, \
              named_note) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT ({column}, position) DO UPDATE SET \
             pressing = excluded.pressing, card = excluded.card, \
             record_order = excluded.record_order, section = excluded.section, \
             row_order = excluded.row_order, \
             agrees_disc_id = excluded.agrees_disc_id, agrees_barcode = excluded.agrees_barcode, \
             agrees_catalog = excluded.agrees_catalog, agrees_label = excluded.agrees_label, \
             agrees_year = excluded.agrees_year, agrees_country = excluded.agrees_country, \
             agrees_title = excluded.agrees_title, agrees_artist = excluded.agrees_artist, \
             source = excluded.source, \
             release_id = excluded.release_id, title = excluded.title, \
             credits = excluded.credits, year = excluded.year, \
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
            position,
            pressing,
            own_card,
            record_order,
            section,
            row_order,
            agrees.disc_id,
            agrees.barcode,
            agrees.catalog,
            agrees.label,
            agrees.year,
            agrees.country,
            agrees.title,
            agrees.artist,
            result.source.as_str(),
            result.release_id,
            result.title,
            result.artist.as_ref().map(stored_credits::line_json).transpose()?,
            result.year,
            facts.country,
            facts.region,
            facts.status,
            facts.packaging,
            facts.discogs_details,
            media_kind,
            cover.map(|cover| cover.image.url.as_str()),
            cover_label,
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
pub(super) fn store_album_link_rows(
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
    /// Returned by a typed search's catalog.
    Returned,
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
    pub(super) pressing: Option<i64>,
    pub(super) listing: String,
    pub(super) media_kind: String,
    /// The stored `album_links` value; [`match_of`] reads it with the album
    /// link rows.
    pub(super) album_links: String,
    /// The result with its list fields still empty; [`match_of`] fills them.
    pub(super) result: MetadataResult,
    pub(super) named_note: Option<String>,
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
        cover.image = RemoteImageSet::stored(cover.image.url.clone(), cover_copies);
    }
    result.links = links
        .into_iter()
        .map(|(catalog, key)| Ok(MetadataRef::new(catalog_of(&catalog)?, key)))
        .collect::<Result<_, DbError>>()?;
    result.album_links = album_links_of(
        &format!("match {position} of {content_hash}"),
        &album_links_kind,
        album_links,
    )?;
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
        LISTING_RETURNED if pressing.is_some() && named_note.is_none() => Listing::Returned,
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

#[cfg(test)]
#[path = "match_rows_tests.rs"]
mod tests;

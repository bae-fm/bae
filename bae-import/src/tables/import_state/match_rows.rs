//! The release-match rows: one per release a verdict or a run in flight
//! found, or a typed search's catalog returned, where it is listed, what the
//! record said and the rows hanging off it, written and read back.
//! Its tables are `match_rows.sql`, which every device's library holds.

use super::verdict_rows::VerdictId;
use super::*;
use crate::tables::album_link_rows::{album_links_column, album_links_of, AlbumLinkRow};
use crate::tables::cover_labels;
use crate::tables::stored_credits;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::records::MetadataRef;
use bae_catalogs::remote_image::{CoverStanding, DownscaledCopy, RemoteImageSet};
use bae_catalogs::search::{MetadataResult, SourceTracks};
use bae_store::WriteStatements as _;
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

/// The id of an `import_candidate_match` row, which every row under the
/// match names it by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MatchId(pub(super) i64);

impl rusqlite::types::FromSql for MatchId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        i64::column_result(value).map(Self)
    }
}

impl rusqlite::ToSql for MatchId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

/// Whose match rows these are: a verdict's, whether a candidate's or a run
/// in flight's, or a typed search's, whose rows hang off the catalog part that
/// returned them.
#[derive(Clone, Copy, Debug)]
pub(super) enum MatchOwner {
    Verdict(VerdictId),
    Search(i64),
}

impl MatchOwner {
    pub(super) fn verdict(self) -> Option<VerdictId> {
        match self {
            Self::Verdict(verdict) => Some(verdict),
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

sqlive_compose::write!(
    DeleteVerdictCoverCopies(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_match_cover_copy
    WHERE match_id IN (SELECT match_id FROM import_candidate_match WHERE verdict_id = :verdict)
"#
);

sqlive_compose::write!(
    DeleteVerdictMatchLabels(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_match_label
    WHERE match_id IN (SELECT match_id FROM import_candidate_match WHERE verdict_id = :verdict)
"#
);

sqlive_compose::write!(
    DeleteVerdictMatchBarcodes(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_match_barcode
    WHERE match_id IN (SELECT match_id FROM import_candidate_match WHERE verdict_id = :verdict)
"#
);

sqlive_compose::write!(
    DeleteVerdictMatchMediums(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_match_medium
    WHERE match_id IN (SELECT match_id FROM import_candidate_match WHERE verdict_id = :verdict)
"#
);

sqlive_compose::write!(
    DeleteVerdictMatchLinks(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_match_link
    WHERE match_id IN (SELECT match_id FROM import_candidate_match WHERE verdict_id = :verdict)
"#
);

sqlive_compose::write!(
    DeleteVerdictAlbumLinks(verdict: super::verdict_rows::VerdictId),
    r#"
    DELETE FROM import_candidate_match_album_link
    WHERE match_id IN (SELECT match_id FROM import_candidate_match WHERE verdict_id = :verdict)
"#
);

/// Delete the rows below every match the verdict `verdict` holds, so the
/// matches a write stores next carry only their own.
pub(super) fn delete_verdict_match_entries(
    sql: &CovenWrite<'_>,
    verdict: VerdictId,
) -> Result<(), DbError> {
    sql.run(&DeleteVerdictCoverCopiesParams { verdict })?;
    sql.run(&DeleteVerdictMatchLabelsParams { verdict })?;
    sql.run(&DeleteVerdictMatchBarcodesParams { verdict })?;
    sql.run(&DeleteVerdictMatchMediumsParams { verdict })?;
    sql.run(&DeleteVerdictMatchLinksParams { verdict })?;
    sql.run(&DeleteVerdictAlbumLinksParams { verdict })?;
    Ok(())
}

sqlive_compose::write!(
    DeleteVerdictMatchesFrom(verdict: super::verdict_rows::VerdictId, position: i64),
    r#"
    DELETE FROM import_candidate_match WHERE verdict_id = :verdict AND position >= :position
"#
);

/// Delete `verdict`'s matches from `position` on: those an earlier verdict
/// held past the ones a write just stored.
pub(super) fn delete_verdict_matches_from(
    sql: &CovenWrite<'_>,
    verdict: VerdictId,
    position: usize,
) -> Result<(), DbError> {
    sql.run(&DeleteVerdictMatchesFromParams { verdict, position: ordinal_column(position)? })?;
    Ok(())
}

sqlive_compose::write!(
    DeleteSearchMatches(source: &'p str),
    r#"
    DELETE FROM import_candidate_match WHERE search_run IS NOT NULL AND source = :source
"#
);

/// Delete every release a search part asking `catalog` returned.
pub(super) fn delete_search_matches_of(
    sql: &CovenWrite<'_>,
    catalog: LookupCatalog,
) -> Result<(), DbError> {
    sql.run(&DeleteSearchMatchesParams { source: catalog.as_str() })?;
    Ok(())
}

// A match, written over the one its owner holds at the position. The only
// rows an insert here can conflict with are that one: the owner's, by
// (verdict_id, position) or by (search_run, position).
sqlive_compose::write!(
    UpsertMatch(
        verdict_id: Option<super::verdict_rows::VerdictId>,
        search_run: Option<i64>,
        search_state: Option<&'static str>,
        position: i64,
        pressing: Option<i64>,
        card: Option<i64>,
        record_order: Option<i64>,
        section: Option<i64>,
        row_order: Option<i64>,
        agrees_disc_id: bool,
        agrees_barcode: bool,
        agrees_catalog: bool,
        agrees_label: bool,
        agrees_year: bool,
        agrees_country: bool,
        agrees_title: bool,
        agrees_artist: bool,
        source: &'p str,
        release_id: &'p str,
        title: &'p str,
        credits: Option<String>,
        year: Option<i32>,
        country: Option<&'static str>,
        region: Option<&'static str>,
        status: Option<&'static str>,
        packaging: Option<&'static str>,
        discogs_details: &'p str,
        media_kind: &'p str,
        cover_url: Option<&'p str>,
        cover_label: Option<String>,
        cover_source: Option<&'p str>,
        cover_standing: Option<&'p str>,
        source_group_id: Option<&'p str>,
        album_links: &'static str,
        source_tracks_kind: Option<&'static str>,
        source_tracks_count: Option<i64>,
        listing: &'static str,
        document_failure: Option<&'static str>,
        document_failure_status: Option<i64>,
        named_note: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_match
        (verdict_id, search_run, search_state, position, pressing, card, record_order, section,
        row_order, agrees_disc_id, agrees_barcode, agrees_catalog, agrees_label, agrees_year,
        agrees_country, agrees_title, agrees_artist, source, release_id, title, credits, year,
        country, region, status, packaging, discogs_details, media_kind, cover_url, cover_label,
        cover_source, cover_standing, source_group_id, album_links, source_tracks_kind,
        source_tracks_count, listing, document_failure, document_failure_status, named_note)
    VALUES (:verdict_id, :search_run, :search_state, :position, :pressing, :card, :record_order,
        :section, :row_order, :agrees_disc_id, :agrees_barcode, :agrees_catalog, :agrees_label,
        :agrees_year, :agrees_country, :agrees_title, :agrees_artist, :source, :release_id,
        :title, :credits, :year, :country, :region, :status, :packaging, :discogs_details,
        :media_kind, :cover_url, :cover_label, :cover_source, :cover_standing, :source_group_id,
        :album_links, :source_tracks_kind, :source_tracks_count, :listing, :document_failure,
        :document_failure_status, :named_note)
    ON CONFLICT DO UPDATE SET
        pressing = excluded.pressing, card = excluded.card, record_order =
        excluded.record_order, section = excluded.section, row_order = excluded.row_order,
        agrees_disc_id = excluded.agrees_disc_id, agrees_barcode = excluded.agrees_barcode,
        agrees_catalog = excluded.agrees_catalog, agrees_label = excluded.agrees_label,
        agrees_year = excluded.agrees_year, agrees_country = excluded.agrees_country,
        agrees_title = excluded.agrees_title, agrees_artist = excluded.agrees_artist, source =
        excluded.source, release_id = excluded.release_id, title = excluded.title, credits =
        excluded.credits, year = excluded.year, country = excluded.country, region =
        excluded.region, status = excluded.status, packaging = excluded.packaging,
        discogs_details = excluded.discogs_details, media_kind = excluded.media_kind, cover_url
        = excluded.cover_url, cover_label = excluded.cover_label, cover_source =
        excluded.cover_source, cover_standing = excluded.cover_standing, source_group_id =
        excluded.source_group_id, album_links = excluded.album_links, source_tracks_kind =
        excluded.source_tracks_kind, source_tracks_count = excluded.source_tracks_count, listing
        = excluded.listing, document_failure = excluded.document_failure,
        document_failure_status = excluded.document_failure_status, named_note =
        excluded.named_note
    RETURNING match_id
"#
);

sqlive_compose::write!(
    InsertMatchCoverCopy(
        match_id: super::match_rows::MatchId,
        cover_url: &'p str,
        max_edge: u32,
        url: &'p str,
    ),
    r#"
    INSERT INTO import_candidate_match_cover_copy
        (match_id, cover_url, max_edge, url)
    VALUES (:match_id, :cover_url, :max_edge, :url)
"#
);

sqlive_compose::write!(
    InsertMatchLabel(
        match_id: super::match_rows::MatchId,
        ordinal: i64,
        name: Option<&'p str>,
        catalog_number: Option<&'p str>,
        musicbrainz_label_id: Option<&'p str>,
        discogs_label_id: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_match_label
        (match_id, ordinal, name, catalog_number, musicbrainz_label_id, discogs_label_id)
    VALUES (:match_id, :ordinal, :name, :catalog_number, :musicbrainz_label_id,
        :discogs_label_id)
"#
);

sqlive_compose::write!(
    InsertMatchBarcode(match_id: super::match_rows::MatchId, ordinal: i64, barcode: &'p str),
    r#"
    INSERT INTO import_candidate_match_barcode
        (match_id, ordinal, barcode)
    VALUES (:match_id, :ordinal, :barcode)
"#
);

sqlive_compose::write!(
    InsertMatchMedium(
        match_id: super::match_rows::MatchId,
        media_kind: &'p str,
        ordinal: i64,
        medium: Option<&'static str>,
        quantity: u32,
    ),
    r#"
    INSERT INTO import_candidate_match_medium
        (match_id, media_kind, ordinal, medium, quantity)
    VALUES (:match_id, :media_kind, :ordinal, :medium, :quantity)
"#
);

sqlive_compose::write!(
    InsertMatchLink(
        match_id: super::match_rows::MatchId,
        ordinal: i64,
        catalog: &'p str,
        key: &'p str,
    ),
    r#"
    INSERT INTO import_candidate_match_link
        (match_id, ordinal, catalog, key)
    VALUES (:match_id, :ordinal, :catalog, :key)
"#
);

/// One match at `position`, in place of the one there, with the rows under
/// it. The rows under the match that stood there are already gone. Only a
/// search writes a returned release, and only a verdict-shaped owner a listed
/// or unlisted one.
pub(super) fn store_match(
    sql: &CovenWrite<'_>,
    owner: MatchOwner,
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
    let (verdict, search) = (owner.verdict(), owner.search());
    let match_id = MatchId(
        sql.returning_one(&UpsertMatchParams {
            verdict_id: verdict,
            search_run: search,
            search_state,
            position,
            pressing,
            card: own_card,
            record_order,
            section,
            row_order,
            agrees_disc_id: agrees.disc_id,
            agrees_barcode: agrees.barcode,
            agrees_catalog: agrees.catalog,
            agrees_label: agrees.label,
            agrees_year: agrees.year,
            agrees_country: agrees.country,
            agrees_title: agrees.title,
            agrees_artist: agrees.artist,
            source: result.source.as_str(),
            release_id: &result.release_id,
            title: &result.title,
            credits: result.artist.as_ref().map(stored_credits::line_json).transpose()?,
            year: result.year,
            country: facts.country,
            region: facts.region,
            status: facts.status,
            packaging: facts.packaging,
            discogs_details: &facts.discogs_details,
            media_kind,
            cover_url: cover.map(|cover| cover.image.url.as_str()),
            cover_label,
            cover_source: cover.map(|cover| cover.source.as_str()),
            cover_standing: cover.map(|cover| cover.standing.as_str()),
            source_group_id: result.source_group_id.as_deref(),
            album_links: album_links_kind,
            source_tracks_kind: tracks_kind,
            source_tracks_count: tracks_count,
            listing,
            document_failure: document_failure.kind,
            document_failure_status: document_failure.status,
            named_note,
        })?
        .match_id,
    );
    if let Some(cover) = cover {
        for copy in &cover.image.downscaled {
            sql.run(&InsertMatchCoverCopyParams {
                match_id,
                cover_url: &cover.image.url,
                max_edge: copy.max_edge,
                url: &copy.url,
            })?;
        }
    }
    for (ordinal, label) in result.labels.iter().enumerate() {
        sql.run(&InsertMatchLabelParams {
            match_id,
            ordinal: ordinal_column(ordinal)?,
            name: label.name(),
            catalog_number: label.catalog_number(),
            musicbrainz_label_id: label.musicbrainz_label_id(),
            discogs_label_id: label.discogs_label_id(),
        })?;
    }
    for (ordinal, barcode) in result.barcodes.iter().enumerate() {
        sql.run(&InsertMatchBarcodeParams {
            match_id,
            ordinal: ordinal_column(ordinal)?,
            barcode,
        })?;
    }
    for (ordinal, (medium, quantity)) in media_entries.iter().enumerate() {
        sql.run(&InsertMatchMediumParams {
            match_id,
            media_kind,
            ordinal: ordinal_column(ordinal)?,
            medium: medium.map(Medium::key),
            quantity: *quantity,
        })?;
    }
    for (ordinal, link) in result.links.iter().enumerate() {
        sql.run(&InsertMatchLinkParams {
            match_id,
            ordinal: ordinal_column(ordinal)?,
            catalog: link.catalog.as_str(),
            key: &link.key,
        })?;
    }
    store_album_link_rows(sql, match_id, &result.album_links)?;
    Ok(())
}

sqlive_compose::write!(
    InsertMatchAlbumLink(
        match_id: super::match_rows::MatchId,
        ordinal: i64,
        catalog: &'p str,
        key: &'p str,
    ),
    r#"
    INSERT INTO import_candidate_match_album_link
        (match_id, ordinal, catalog, key)
    VALUES (:match_id, :ordinal, :catalog, :key)
"#
);

/// The album link rows of the match `match_id`, which holds none yet.
pub(super) fn store_album_link_rows(
    sql: &CovenWrite<'_>,
    match_id: MatchId,
    links: &AlbumLinks,
) -> Result<(), DbError> {
    for (ordinal, album) in links.read().iter().enumerate() {
        sql.run(&InsertMatchAlbumLinkParams {
            match_id,
            ordinal: ordinal_column(ordinal)?,
            catalog: album.catalog.as_str(),
            key: &album.key,
        })?;
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

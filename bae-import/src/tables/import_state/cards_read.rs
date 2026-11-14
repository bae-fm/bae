//! The album cards of a list of releases (a typed search's, a ledger cell's),
//! read in one statement from what the write that stored the list laid out:
//! each release's row, card and place among its row's records, the text of
//! each card, and what each row lists.
//!
//! The statement orders and groups what is stored the way the cards are
//! drawn: a card's catalog buckets, its sections when it holds two albums of
//! one catalog, its rows by year, its cover and the catalogs it names. It
//! answers the cards as nested JSON, each record the release itself as
//! [`match_record_sql`] holds it, and [`groups_of`] reads that into the cards.

use super::match_record::{match_record_sql, StoredRecord};
use super::*;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::records::{LookupRef, MetadataRef};
use bae_catalogs::search::MetadataResult;
use bae_identify::release_group::{
    AlbumHeading, Pressing, PressingSection, ReleaseGroup, ReleaseGroupSource, StatedLabelLine,
    Statement, Statements,
};
use bae_identify::{Agreements, AlbumLink, RowAgreements, SharedAlbum, VerdictCards};
use bae_types::pressing::{MediaCount, ReleaseArea};
use std::str::FromStr;

/// The SQL of a scalar subquery reading the cards of one list.
///
/// `members` selects the list's releases as `ord` (the place in the list, 0
/// first, in the order the list's loader returns them), `position` (the
/// release's match row), `card`, `pressing`, `record_order`, `kind` (the
/// listing the row is of), `source`, `release_id`, `title`, `year`,
/// `source_group_id`, `album_links`, `cover_url`, `cover_standing`, `links` (a
/// JSON list of the albums its statements name), `record` (the release as
/// [`match_record_sql`] holds it, which a card carries whole), `named_note`, and the eight
/// flags `a_disc_id`, `a_barcode`, `a_catalog`, `a_label`, `a_year`,
/// `a_country`, `a_title` and `a_artist` the folder's text agrees with it by
/// (all 0 for a list nothing judges). `text_owner` is
/// a condition over `t` selecting the list's rows of the card text and row
/// statement tables.
pub(super) fn cards_sql(members: &str, text_owner: &str) -> String {
    format!(
        "(WITH \
          m AS ({members}), \
          bk AS (SELECT m.*, CASE WHEN m.source_group_id IS NULL THEN '#' || m.ord \
                                  ELSE m.source || ':' || m.source_group_id END AS bucket \
                 FROM m), \
          buckets AS ( \
              SELECT card, bucket, source, source_group_id AS group_id, MIN(ord) AS first_ord, \
                     MAX(album_links = 'unread') AS unread, \
                     ROW_NUMBER() OVER (PARTITION BY card \
                                        ORDER BY bae_catalog_rank(source), MIN(ord)) AS bidx \
              FROM bk GROUP BY card, bucket), \
          recs AS (SELECT bk.*, buckets.bidx FROM bk JOIN buckets USING (card, bucket)), \
          splits AS (SELECT card, COUNT(*) > COUNT(DISTINCT source) AS splits \
                     FROM buckets GROUP BY card), \
          pressings AS ( \
              SELECT card, pressing, kind, MIN(bidx) AS album, \
                     MIN(bidx * 4294967296 + ord) AS place, \
                     MAX(year) FILTER (WHERE record_order = 0) AS lead_year, \
                     json_group_array(json(record) ORDER BY record_order) AS records, \
                     MAX(json_object('catalog', source, 'key', release_id)) \
                         FILTER (WHERE record_order = 0) AS lead, \
                     MAX(a_disc_id) + MAX(a_barcode) + MAX(a_catalog) + MAX(a_label) \
                         + MAX(a_year) + MAX(a_country) AS agreements, \
                     json_object('disc_id', MAX(a_disc_id), 'barcode', MAX(a_barcode), \
                                 'catalog', MAX(a_catalog), 'label', MAX(a_label), \
                                 'year', MAX(a_year), 'country', MAX(a_country), \
                                 'title', MAX(a_title), 'artist', MAX(a_artist)) AS agreed \
              FROM recs GROUP BY card, pressing, kind), \
          rowj AS ( \
              SELECT p.*, s.splits, \
                     json_object( \
                         'records', json(p.records), \
                         'agreed', json(p.agreed), \
                         'notes', json((SELECT json_group_array(json_object( \
                                 'note', n.note, 'catalogs', json(n.catalogs)) ORDER BY n.first) \
                             FROM (SELECT r.named_note AS note, MIN(r.record_order) AS first, \
                                          CASE WHEN (SELECT COUNT(DISTINCT x.named_note) \
                                                     FROM recs AS x \
                                                     WHERE x.card = p.card AND x.pressing = p.pressing \
                                                       AND x.kind = p.kind) > 1 \
                                               THEN (SELECT json_group_array(c.source \
                                                         ORDER BY bae_catalog_rank(c.source)) \
                                                     FROM (SELECT DISTINCT y.source AS source \
                                                           FROM recs AS y \
                                                           WHERE y.card = p.card \
                                                             AND y.pressing = p.pressing \
                                                             AND y.kind = p.kind \
                                                             AND y.named_note = r.named_note) AS c) \
                                               ELSE '[]' END AS catalogs \
                                   FROM recs AS r \
                                   WHERE r.card = p.card AND r.pressing = p.pressing \
                                     AND r.kind = p.kind AND r.named_note IS NOT NULL \
                                   GROUP BY r.named_note) AS n)), \
                         'statements', json((SELECT json_group_array(json_object( \
                                 'fact', t.fact, 'line', t.line, 'ordinal', t.ordinal, \
                                 'value', t.value, 'catalogs', json(t.catalogs)) \
                                 ORDER BY t.line, t.ordinal) \
                             FROM import_candidate_row_statement AS t \
                             WHERE {text_owner} AND t.listing = p.kind \
                               AND t.pressing = p.pressing))) AS row \
              FROM pressings AS p JOIN splits AS s USING (card)), \
          secj AS ( \
              SELECT r.card, CASE WHEN r.splits THEN r.album ELSE 0 END AS section_no, \
                     json_object( \
                         'album', CASE WHEN r.splits THEN json((SELECT json_object( \
                                 'title', (SELECT x.title FROM bk AS x WHERE x.ord = b.first_ord), \
                                 'catalog', b.source, \
                                 'group_url', bae_catalog_group_url(b.source, b.group_id), \
                                 'unread', b.unread) \
                             FROM buckets AS b WHERE b.card = r.card AND b.bidx = r.album)) END, \
                         'pressings', json((json_group_array(json(r.row) \
                             ORDER BY r.agreements DESC, r.lead_year IS NULL, r.lead_year, r.place) \
                             FILTER (WHERE r.kind <> 'narrowed_out'))), \
                         'narrowed_out', json((json_group_array(json(r.row) \
                             ORDER BY r.agreements DESC, r.lead_year IS NULL, r.lead_year, r.place) \
                             FILTER (WHERE r.kind = 'narrowed_out')))) AS section \
              FROM rowj AS r GROUP BY r.card, CASE WHEN r.splits THEN r.album ELSE 0 END), \
          extras AS ( \
              SELECT recs.card, json_extract(link.value, '$.catalog') AS catalog, \
                     json_extract(link.value, '$.key') AS key, \
                     ROW_NUMBER() OVER (PARTITION BY recs.card, json_extract(link.value, '$.catalog') \
                                        ORDER BY recs.bidx, recs.ord, link.key) AS nth, \
                     recs.bidx * 1000000000000 + recs.ord * 1000 + link.key AS seen \
              FROM recs, json_each(recs.links) AS link), \
          ranked AS ( \
              SELECT card, MAX(kind <> 'narrowed_out') AS offers, \
                     COALESCE(MAX(agreements) FILTER (WHERE kind <> 'narrowed_out'), \
                              MAX(agreements)) AS best \
              FROM pressings GROUP BY card), \
          cards AS (SELECT DISTINCT card FROM bk) \
          SELECT json_group_array(json_object( \
              'id', COALESCE((SELECT b.group_id FROM buckets AS b \
                              WHERE b.card = c.card AND b.group_id IS NOT NULL \
                              ORDER BY b.bidx LIMIT 1), \
                             (SELECT x.release_id FROM bk AS x \
                              WHERE x.ord = (SELECT b.first_ord FROM buckets AS b \
                                             WHERE b.card = c.card AND b.bidx = 1))), \
              'titles', json((SELECT json_group_array(t.text ORDER BY t.ordinal) \
                              FROM import_candidate_card_text AS t \
                              WHERE {text_owner} AND t.card = c.card AND t.kind = 'title')), \
              'artists', json((SELECT json_group_array(t.text ORDER BY t.ordinal) \
                               FROM import_candidate_card_text AS t \
                               WHERE {text_owner} AND t.card = c.card AND t.kind = 'artist')), \
              'labels', json((SELECT json_group_array(t.text ORDER BY t.ordinal) \
                              FROM import_candidate_card_text AS t \
                              WHERE {text_owner} AND t.card = c.card AND t.kind = 'label')), \
              'cover', json((SELECT json(r.record) FROM recs AS r \
                             WHERE r.card = c.card AND r.cover_url IS NOT NULL \
                             ORDER BY r.cover_standing = 'stated' DESC, r.bidx, r.ord LIMIT 1)), \
              'leads', json((SELECT json_group_array(json(r.lead) \
                                 ORDER BY CASE WHEN r.splits THEN r.album ELSE 0 END, \
                                          r.agreements DESC, r.lead_year IS NULL, r.lead_year, \
                                          r.place) \
                             FROM rowj AS r \
                             WHERE r.card = c.card AND r.kind <> 'narrowed_out')), \
              'sources', json((SELECT json_group_array(json_object( \
                                  'catalog', s.catalog, 'group_url', s.group_url, \
                                  'unread', s.unread) \
                                  ORDER BY bae_catalog_rank(s.catalog), s.seen) \
                  FROM (SELECT b.source AS catalog, \
                               bae_catalog_group_url(b.source, b.group_id) AS group_url, \
                               b.unread AS unread, b.bidx AS seen \
                        FROM buckets AS b WHERE b.card = c.card \
                        UNION ALL \
                        SELECT e.catalog, bae_catalog_group_url(e.catalog, e.key), 0, e.seen \
                        FROM extras AS e \
                        WHERE e.card = c.card AND e.nth = 1 \
                          AND e.catalog NOT IN (SELECT b.source FROM buckets AS b \
                                                WHERE b.card = c.card)) AS s)), \
              'year_min', (SELECT MIN(year) FROM m WHERE m.card = c.card), \
              'year_max', (SELECT MAX(year) FROM m WHERE m.card = c.card), \
              'sections', json((SELECT json_group_array(json(secj.section) ORDER BY secj.section_no) \
                                FROM secj WHERE secj.card = c.card))) \
              ORDER BY k.offers DESC, k.best DESC, c.card) \
          FROM cards AS c JOIN ranked AS k ON k.card = c.card)"
    )
}

/// The SQL of a scalar subquery reading the cards of the typed search whose
/// number `search_run` names, the SQL of a column of the search's own row.
pub(super) fn search_cards_sql(search_run: &str) -> String {
    cards_sql(
        &format!(
            "SELECT ROW_NUMBER() OVER (ORDER BY part.position, found.position) - 1 AS ord, \
                    found.position, found.card, found.pressing, found.record_order, \
                    'returned' AS kind, found.source, found.release_id, found.title, \
                    found.year, found.source_group_id, found.album_links, found.cover_url, \
                    found.cover_standing, {record} AS record, 0 AS a_disc_id, 0 AS a_barcode, 0 AS a_catalog, 0 AS a_label, 0 AS a_year, \
                    0 AS a_country, 0 AS a_title, 0 AS a_artist, NULL AS named_note, \
                    (SELECT json_group_array(json_object('catalog', link.catalog, 'key', link.key) \
                                             ORDER BY link.ordinal) \
                     FROM import_candidate_match_album_link AS link \
                     WHERE link.search_run = found.search_run \
                       AND link.position = found.position) AS links \
             FROM import_candidate_match AS found \
             JOIN import_candidate_search_source AS part \
               ON part.search_run = found.search_run AND part.catalog = found.source \
             WHERE found.search_run = {search_run}",
            record = match_record_sql("found"),
        ),
        &format!("t.search_run = {search_run} AND t.step = ''"),
    )
}

/// The SQL of a scalar subquery reading the cards of the ledger cell the
/// lookup row `lookup` names, a column of a read over the ledger's lookups.
pub(super) fn cell_cards_sql(lookup: &str) -> String {
    let owner = format!(
        "{{table}}.content_hash IS {lookup}.content_hash \
         AND {{table}}.identify_run IS {lookup}.identify_run \
         AND {{table}}.step = {lookup}.step AND {{table}}.position = {lookup}.position \
         AND {{table}}.cell = {lookup}.cell"
    );
    cards_sql(
        &format!(
            "SELECT found.ordinal AS ord, found.match_position AS position, found.card, \
                    found.pressing, found.record_order, 'answered' AS kind, matched.source, \
                    matched.release_id, matched.title, matched.year, matched.source_group_id, \
                    matched.album_links, matched.cover_url, matched.cover_standing, \
                    {record} AS record, 0 AS a_disc_id, 0 AS a_barcode, 0 AS a_catalog, 0 AS a_label, 0 AS a_year, \
                    0 AS a_country, 0 AS a_title, 0 AS a_artist, NULL AS named_note, \
                    (SELECT json_group_array(json_object('catalog', link.catalog, \
                                                         'key', link.key) \
                                             ORDER BY link.ordinal) \
                     FROM import_candidate_match_album_link AS link \
                     WHERE link.content_hash IS matched.content_hash \
                       AND link.identify_run IS matched.identify_run \
                       AND link.position = matched.position) AS links \
             FROM import_candidate_lookup_release AS found \
             JOIN import_candidate_match AS matched \
               ON matched.content_hash IS found.content_hash \
              AND matched.identify_run IS found.identify_run \
              AND matched.position = found.match_position \
             WHERE {found}",
            found = owner.replace("{table}", "found"),
            record = match_record_sql("matched"),
        ),
        &owner.replace("{table}", "t"),
    )
}

/// The SQL of a scalar subquery reading the cards of the verdict-shaped row
/// joined as `verdict` (a candidate's verdict, or a run's): the cards of its
/// offered and set-aside releases with what the folder's text agrees with
/// about each row, how many rows were set aside, whether the offered rows
/// are several pressings of one album, and the check against the folder and
/// the release the verdict decides on (`import_candidate_verdict_decision`).
/// A JSON object read by [`verdict_cards_of`].
pub(super) fn verdict_cards_sql(verdict: &str) -> String {
    let owner = format!(
        "{{table}}.content_hash IS {verdict}.content_hash \
         AND {{table}}.identify_run IS {verdict}.identify_run"
    );
    let found = owner.replace("{table}", "found");
    let listed = format!("{found} AND found.listing IN ('offered', 'narrowed_out')");
    let cards = cards_sql(
        &format!(
            "SELECT found.position AS ord, found.position, found.card, found.pressing, \
                    found.record_order, found.listing AS kind, found.source, found.release_id, \
                    found.title, found.year, found.source_group_id, found.album_links, \
                    found.cover_url, found.cover_standing, {record} AS record, \
                    found.agrees_disc_id AS a_disc_id, found.agrees_barcode AS a_barcode, \
                    found.agrees_catalog AS a_catalog, found.agrees_label AS a_label, \
                    found.agrees_year AS a_year, found.agrees_country AS a_country, \
                    found.agrees_title AS a_title, found.agrees_artist AS a_artist, \
                    found.named_note, \
                    (SELECT json_group_array(json_object('catalog', link.catalog, 'key', link.key) \
                                             ORDER BY link.ordinal) \
                     FROM import_candidate_match_album_link AS link \
                     WHERE link.content_hash IS found.content_hash \
                       AND link.identify_run IS found.identify_run \
                       AND link.position = found.position) AS links \
             FROM import_candidate_match AS found WHERE {listed}",
            record = match_record_sql("found"),
        ),
        &format!("{} AND t.step = ''", owner.replace("{table}", "t")),
    );
    format!(
        "(WITH listed_cards AS MATERIALIZED (SELECT {cards} AS json), \
              offered AS (SELECT found.card, found.pressing, found.source, found.source_group_id \
                          FROM import_candidate_match AS found \
                          WHERE {found} AND found.listing = 'offered') \
          SELECT json_object('cards', json(listed_cards.json), \
            'folder_check', (SELECT decision.folder_check \
                             FROM import_candidate_verdict_decision AS decision \
                             WHERE decision.content_hash IS {verdict}.content_hash \
                               AND decision.identify_run IS {verdict}.identify_run), \
            'unattended_pick', (SELECT decision.unattended_pick \
                                FROM import_candidate_verdict_decision AS decision \
                                WHERE decision.content_hash IS {verdict}.content_hash \
                                  AND decision.identify_run IS {verdict}.identify_run), \
            'narrowed_out_count', (SELECT COUNT(DISTINCT found.pressing) \
                                   FROM import_candidate_match AS found \
                                   WHERE {found} AND found.listing = 'narrowed_out'), \
            'shared_album', \
                CASE WHEN (SELECT COUNT(DISTINCT card) FROM offered) = 1 \
                      AND (SELECT COUNT(DISTINCT pressing) FROM offered) >= 2 \
                      AND NOT EXISTS (SELECT 1 FROM offered WHERE source_group_id IS NOT NULL \
                                      GROUP BY source HAVING COUNT(DISTINCT source_group_id) > 1) \
                      AND EXISTS (SELECT 1 FROM offered WHERE source_group_id IS NOT NULL) \
                THEN json_object( \
                    'leads', json((SELECT json_group_array(json(lead.value)) \
                                   FROM json_each(listed_cards.json) AS card, \
                                        json_each(card.value, '$.leads') AS lead)), \
                    'albums', json((SELECT json_group_array(json_object( \
                                        'catalog', album.source, 'key', album.group_id) \
                                        ORDER BY bae_catalog_rank(album.source)) \
                                    FROM (SELECT source, MIN(source_group_id) AS group_id \
                                          FROM offered WHERE source_group_id IS NOT NULL \
                                          GROUP BY source) AS album))) END) \
           FROM listed_cards)"
    )
}

/// The cards of the verdict stored under `content_hash`, which holds
/// `verdict`: what its rows stored, selected in one statement.
pub(crate) fn load_verdict_cards_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
    verdict: &bae_identify::TerminalVerdict,
) -> Result<VerdictCards, DbError> {
    if verdict.findings().is_none() {
        return Ok(VerdictCards::default());
    }
    let json: String = sql.query_row(
        &format!(
            "SELECT {cards} FROM import_candidate_verdict AS verdict \
             WHERE verdict.content_hash = ?",
            cards = verdict_cards_sql("verdict")
        ),
        [content_hash],
        |row| row.get(0),
    )?;
    verdict_cards_of(&json)
}

/// What [`verdict_cards_sql`] answers.
#[derive(serde::Deserialize)]
pub(super) struct StoredVerdictCards {
    cards: Vec<StoredCard>,
    narrowed_out_count: u32,
    /// The offered rows when they are several pressings of one album.
    shared_album: Option<StoredSharedAlbum>,
    /// 'cd_rip', 'not_cd_audio' or 'source_tracks_unknown'.
    folder_check: Option<String>,
    unattended_pick: Option<String>,
}

/// The offered rows of one album: each row's lead release in the cards' order,
/// and the album each catalog files them under.
#[derive(serde::Deserialize)]
struct StoredSharedAlbum {
    leads: Vec<StoredRef>,
    albums: Vec<StoredRef>,
}

/// A release or an album a catalog names.
#[derive(serde::Deserialize)]
struct StoredRef {
    catalog: String,
    key: String,
}

impl StoredSharedAlbum {
    fn shared_album(self) -> Result<SharedAlbum, DbError> {
        let leads = self
            .leads
            .into_iter()
            .map(|lead| {
                LookupCatalog::from_str(&lead.catalog)
                    .map(|catalog| LookupRef::new(catalog, lead.key))
                    .map_err(|_unknown| DbError::unreadable("a shared album's lead", &lead.catalog))
            })
            .collect::<Result<Vec<_>, DbError>>()?;
        let albums = self
            .albums
            .into_iter()
            .map(|album| Ok(MetadataRef::new(catalog_of(&album.catalog)?, album.key)))
            .collect::<Result<Vec<_>, DbError>>()?;
        let link = AlbumLink::new(albums).ok_or_else(|| {
            DbError::unreadable("a shared album's albums", "none, or two of one catalog")
        })?;
        Ok(SharedAlbum { leads, link })
    }
}

/// The cards of a verdict that [`verdict_cards_sql`] answered, with each
/// row's agreements and notes.
pub(super) fn verdict_cards_of(json: &str) -> Result<VerdictCards, DbError> {
    let StoredVerdictCards {
        cards,
        narrowed_out_count,
        shared_album,
        folder_check,
        unattended_pick,
    } = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a verdict's stored cards", json, error))?;
    let mut agreements: Vec<(String, RowAgreements)> = Vec::new();
    for card in &cards {
        let rows = card
            .sections
            .iter()
            .flat_map(|section| &section.pressings)
            .chain(card.sections.iter().flat_map(|section| &section.narrowed_out));
        for row in rows {
            let stated = RowAgreements {
                fields: row.agreed.agreements(),
                notes: row
                    .notes
                    .iter()
                    .map(|note| {
                        Ok(Statement {
                            value: note.note.clone(),
                            catalogs: note
                                .catalogs
                                .iter()
                                .map(|catalog| {
                                    LookupCatalog::from_str(catalog).map_err(|_unknown| {
                                        DbError::unreadable("a note's catalog", catalog)
                                    })
                                })
                                .collect::<Result<_, DbError>>()?,
                        })
                    })
                    .collect::<Result<_, DbError>>()?,
            };
            for record in &row.records {
                agreements.push((record.release_id.clone(), stated.clone()));
            }
        }
    }
    Ok(VerdictCards {
        groups: groups_from(cards)?,
        agreements,
        narrowed_out_count,
        shared_album: shared_album.map(StoredSharedAlbum::shared_album).transpose()?,
        folder_check: folder_check.as_deref().map(folder_check_of).transpose()?,
        unattended_pick,
    })
}

/// The check against the folder a verdict row's decision names.
fn folder_check_of(stored: &str) -> Result<bae_identify::FolderCheck, DbError> {
    use bae_identify::{FolderCheck, MediumConflict};
    match stored {
        "source_tracks_unknown" => Ok(FolderCheck::SourceTracksUnknown),
        "cd_rip" => Ok(FolderCheck::MediumDisagrees { folder: MediumConflict::CdRip }),
        "not_cd_audio" => Ok(FolderCheck::MediumDisagrees { folder: MediumConflict::NotCdAudio }),
        other => Err(DbError::unreadable("a verdict's folder check", other)),
    }
}

/// One card as [`cards_sql`] answers it.
#[derive(serde::Deserialize)]
pub(super) struct StoredCard {
    id: String,
    titles: Vec<String>,
    artists: Vec<String>,
    labels: Vec<String>,
    cover: Option<StoredRecord>,
    sources: Vec<StoredSource>,
    year_min: Option<i32>,
    year_max: Option<i32>,
    sections: Vec<StoredSection>,
}

#[derive(serde::Deserialize)]
struct StoredSource {
    catalog: String,
    group_url: Option<String>,
    unread: i64,
}

#[derive(serde::Deserialize)]
struct StoredHeading {
    title: String,
    catalog: String,
    group_url: Option<String>,
    unread: i64,
}

#[derive(serde::Deserialize)]
struct StoredSection {
    album: Option<StoredHeading>,
    pressings: Vec<StoredRow>,
    narrowed_out: Vec<StoredRow>,
}

#[derive(serde::Deserialize)]
struct StoredRow {
    records: Vec<StoredRecord>,
    statements: Vec<StoredStatement>,
    agreed: StoredAgreed,
    notes: Vec<StoredNote>,
}

/// Which of its fields the folder's text agrees with about a row.
#[derive(serde::Deserialize)]
struct StoredAgreed {
    disc_id: i64,
    barcode: i64,
    catalog: i64,
    label: i64,
    year: i64,
    country: i64,
    title: i64,
    artist: i64,
}

impl StoredAgreed {
    fn agreements(&self) -> Agreements {
        Agreements {
            disc_id: self.disc_id != 0,
            barcode: self.barcode != 0,
            catalog: self.catalog != 0,
            label: self.label != 0,
            year: self.year != 0,
            country: self.country != 0,
            title: self.title != 0,
            artist: self.artist != 0,
        }
    }
}

#[derive(serde::Deserialize)]
struct StoredNote {
    note: String,
    catalogs: Vec<String>,
}

#[derive(serde::Deserialize)]
struct StoredStatement {
    fact: String,
    line: usize,
    value: String,
    catalogs: Vec<String>,
}

fn catalog_of(stored: &str) -> Result<Catalog, DbError> {
    Catalog::from_str(stored).map_err(|_unknown| DbError::unreadable("a card's catalog", stored))
}

fn source_of(
    catalog: &str,
    group_url: Option<String>,
    unread: i64,
) -> Result<ReleaseGroupSource, DbError> {
    Ok(ReleaseGroupSource {
        source: catalog_of(catalog)?,
        group_url,
        album_links_unread: unread != 0,
    })
}

/// A row's statements as the facts of its stored rows, each in its order.
fn statements_of(stored: Vec<StoredStatement>) -> Result<Statements, DbError> {
    let mut statements = Statements {
        years: Vec::new(),
        labels: Vec::new(),
        barcodes: Vec::new(),
        areas: Vec::new(),
        media: Vec::new(),
    };
    fn stated<T>(value: T, catalogs: &[String]) -> Result<Statement<T>, DbError> {
        Ok(Statement {
            value,
            catalogs: catalogs
                .iter()
                .map(|catalog| {
                    LookupCatalog::from_str(catalog)
                        .map_err(|_unknown| DbError::unreadable("a statement's catalog", catalog))
                })
                .collect::<Result<_, _>>()?,
        })
    }
    fn json<T: serde::de::DeserializeOwned>(what: &str, value: &str) -> Result<T, DbError> {
        serde_json::from_str(value).map_err(|error| DbError::unreadable_json(what, value, error))
    }
    for StoredStatement { fact, line, value, catalogs } in stored {
        match fact.as_str() {
            "year" => statements.years.push(stated(
                value
                    .parse::<i32>()
                    .map_err(|_bad| DbError::unreadable("a stated year", &value))?,
                &catalogs,
            )?),
            "label_name" | "catalog_number" => {
                // A row's label lines are numbered from zero, in order.
                while statements.labels.len() <= line {
                    statements
                        .labels
                        .push(StatedLabelLine { names: Vec::new(), catalog_numbers: Vec::new() });
                }
                if fact == "label_name" {
                    statements.labels[line].names.push(value);
                } else {
                    statements.labels[line].catalog_numbers.push(stated(value, &catalogs)?);
                }
            }
            "barcode" => statements.barcodes.push(stated(value, &catalogs)?),
            "area" => statements
                .areas
                .push(stated(json::<ReleaseArea>("a stated area", &value)?, &catalogs)?),
            "media" => statements
                .media
                .push(stated(json::<Vec<MediaCount>>("stated media", &value)?, &catalogs)?),
            other => return Err(DbError::unreadable("import_candidate_row_statement.fact", other)),
        }
    }
    Ok(statements)
}

/// The cards of a list that [`cards_sql`] answered.
pub(super) fn groups_of(json: &str) -> Result<Vec<ReleaseGroup>, DbError> {
    let cards: Vec<StoredCard> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a list's stored cards", json, error))?;
    groups_from(cards)
}

/// The cards of a list that [`cards_sql`] answered, already read from their
/// JSON.
pub(super) fn groups_from(cards: Vec<StoredCard>) -> Result<Vec<ReleaseGroup>, DbError> {
    let release = |record: StoredRecord| -> Result<MetadataResult, DbError> {
        Ok(record.match_row("a card's release")?.result)
    };
    let row = |stored: StoredRow| -> Result<Pressing, DbError> {
        let records = stored.records.into_iter().map(release).collect::<Result<Vec<_>, _>>()?;
        Ok(Pressing::stored(records, statements_of(stored.statements)?))
    };
    cards
        .into_iter()
        .map(|card| {
            Ok(ReleaseGroup {
                id: card.id,
                titles: card.titles,
                artists: card
                    .artists
                    .iter()
                    .map(|artist| {
                        serde_json::from_str::<Vec<bae_types::credits::StoredCredit>>(artist)
                            .map(|credits| {
                                bae_types::credits::CreditLine::from_stored_credits(&credits)
                            })
                            .map_err(|error| {
                                DbError::unreadable_json("a card's stored artist", artist, error)
                            })
                    })
                    .collect::<Result<_, _>>()?,
                labels: card.labels,
                cover_art: card
                    .cover
                    .map(release)
                    .transpose()?
                    .and_then(|release| release.cover_art),
                sources: card
                    .sources
                    .into_iter()
                    .map(|source| source_of(&source.catalog, source.group_url, source.unread))
                    .collect::<Result<_, _>>()?,
                year_min: card.year_min,
                year_max: card.year_max,
                sections: card
                    .sections
                    .into_iter()
                    .map(|section| {
                        Ok(PressingSection {
                            album: section
                                .album
                                .map(|heading| {
                                    Ok::<_, DbError>(AlbumHeading {
                                        title: heading.title,
                                        source: source_of(
                                            &heading.catalog,
                                            heading.group_url,
                                            heading.unread,
                                        )?,
                                    })
                                })
                                .transpose()?,
                            pressings: section
                                .pressings
                                .into_iter()
                                .map(row)
                                .collect::<Result<_, _>>()?,
                            narrowed_out: section
                                .narrowed_out
                                .into_iter()
                                .map(row)
                                .collect::<Result<_, _>>()?,
                        })
                    })
                    .collect::<Result<_, DbError>>()?,
            })
        })
        .collect()
}

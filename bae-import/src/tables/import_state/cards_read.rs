//! The album cards of a list of releases (a typed search's, a ledger cell's),
//! which one statement reads from the layout that the list's write stored:
//! each release's row, card and place among its row's records, the text of
//! each card, and what each row lists.
//!
//! The statement orders and groups the stored rows the way the pane draws the
//! cards: a card's catalog buckets, its sections when it holds two albums of
//! one catalog, its rows by year, its cover and the catalogs it names. The
//! statement returns the cards as nested JSON, each record the release itself
//! as [`match_record_sql`] holds it, and [`groups_of`] reads that JSON into
//! the cards.

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
/// JSON list of the albums its statements name), `named_note`, and the eight
/// flags `a_disc_id`, `a_barcode`, `a_catalog`, `a_label`, `a_year`,
/// `a_country`, `a_title` and `a_artist` the folder's text agrees with it by
/// (all 0 for a list nothing judges). `records` selects the same releases'
/// `ord` and `record` (the release as [`match_record_sql`] holds it, which a
/// card carries whole). `text_owner` is a condition over `t` selecting the
/// list's rows of the card text and row statement tables.
///
/// The statement names each of its WITH names as few times as it can, because
/// SQLite copies a WITH name's whole definition at every reference, and a
/// statement that holds many copies stops with "VIEWs and/or subqueries
/// nested too deep" once its cursors pass `SQLITE_LIMIT_EXPR_DEPTH` (1000).
/// So `recs` holds every per-release fact the later names read (bucket,
/// album index, whether the card splits), and the releases' JSON comes in at
/// the two places that carry it, through `records`, never through `recs`.
pub(super) fn cards_sql(members: &str, records: &str, text_owner: &str) -> String {
    format!(
        "(WITH \
          recs AS ( \
              SELECT w.*, MAX(w.bucket_nth) OVER (PARTITION BY w.card) > 1 AS splits \
              FROM (SELECT v.*, \
                           DENSE_RANK() OVER (PARTITION BY v.card \
                                              ORDER BY bae_catalog_rank(v.source), \
                                                       v.first_ord) AS bidx, \
                           DENSE_RANK() OVER (PARTITION BY v.card, v.source \
                                              ORDER BY v.first_ord) AS bucket_nth \
                    FROM (SELECT b.*, \
                                 MIN(b.ord) OVER (PARTITION BY b.card, b.bucket) AS first_ord, \
                                 MAX(b.album_links = 'unread') \
                                     OVER (PARTITION BY b.card, b.bucket) AS unread, \
                                 FIRST_VALUE(b.title) \
                                     OVER (PARTITION BY b.card, b.bucket ORDER BY b.ord) \
                                     AS bucket_title \
                          FROM (SELECT m.*, \
                                       CASE WHEN m.source_group_id IS NULL THEN '#' || m.ord \
                                            ELSE m.source || ':' || m.source_group_id END \
                                       AS bucket \
                                FROM ({members}) AS m) AS b) AS v) AS w), \
          pressings AS ( \
              SELECT r.card, r.pressing, r.kind, MIN(r.bidx) AS album, MAX(r.splits) AS splits, \
                     MIN(r.bidx * 4294967296 + r.ord) AS place, \
                     MAX(r.year) FILTER (WHERE r.record_order = 0) AS lead_year, \
                     json_group_array(json(rc.record) ORDER BY r.record_order) AS records, \
                     MAX(json_object('catalog', r.source, 'key', r.release_id)) \
                         FILTER (WHERE r.record_order = 0) AS lead, \
                     MAX(r.a_disc_id) + MAX(r.a_barcode) + MAX(r.a_catalog) + MAX(r.a_label) \
                         + MAX(r.a_year) + MAX(r.a_country) AS agreements, \
                     json_object('disc_id', MAX(r.a_disc_id), 'barcode', MAX(r.a_barcode), \
                                 'catalog', MAX(r.a_catalog), 'label', MAX(r.a_label), \
                                 'year', MAX(r.a_year), 'country', MAX(r.a_country), \
                                 'title', MAX(r.a_title), 'artist', MAX(r.a_artist)) AS agreed, \
                     (json_group_array(json_object( \
                          'title', r.bucket_title, 'catalog', r.source, \
                          'group_url', bae_catalog_group_url(r.source, r.source_group_id), \
                          'unread', r.unread) ORDER BY r.bidx, r.ord)) ->> 0 AS album_json \
              FROM recs AS r JOIN ({records}) AS rc ON rc.ord = r.ord \
              GROUP BY r.card, r.pressing, r.kind), \
          notes AS ( \
              SELECT g.card, g.pressing, g.kind, \
                     json_group_array(json_object( \
                         'note', g.note, \
                         'catalogs', json(CASE WHEN g.noted > 1 THEN g.catalogs ELSE '[]' END)) \
                         ORDER BY g.first) AS notes \
              FROM (SELECT h.*, COUNT(*) OVER (PARTITION BY h.card, h.pressing, h.kind) AS noted \
                    FROM (SELECT d.card, d.pressing, d.kind, d.named_note AS note, \
                                 MIN(d.first) AS first, \
                                 json_group_array(d.source ORDER BY bae_catalog_rank(d.source)) \
                                     AS catalogs \
                          FROM (SELECT DISTINCT r.card, r.pressing, r.kind, r.named_note, \
                                       r.source, \
                                       MIN(r.record_order) OVER (PARTITION BY r.card, r.pressing, \
                                                                 r.kind, r.named_note) AS first \
                                FROM recs AS r WHERE r.named_note IS NOT NULL) AS d \
                          GROUP BY d.card, d.pressing, d.kind, d.named_note) AS h) AS g \
              GROUP BY g.card, g.pressing, g.kind), \
          rowj AS ( \
              SELECT p.*, \
                     json_object( \
                         'records', json(p.records), \
                         'agreed', json(p.agreed), \
                         'notes', json(COALESCE(n.notes, '[]')), \
                         'statements', json((SELECT json_group_array(json_object( \
                                 'fact', t.fact, 'line', t.line, 'ordinal', t.ordinal, \
                                 'value', t.value, 'catalogs', json(t.catalogs)) \
                                 ORDER BY t.line, t.ordinal) \
                             FROM import_candidate_row_statement AS t \
                             WHERE {text_owner} AND t.listing = p.kind \
                               AND t.pressing = p.pressing))) AS row \
              FROM pressings AS p \
              LEFT JOIN notes AS n \
                  ON n.card = p.card AND n.pressing = p.pressing AND n.kind = p.kind), \
          secj AS ( \
              SELECT r.card, CASE WHEN r.splits THEN r.album ELSE 0 END AS section_no, \
                     json_object( \
                         'album', CASE WHEN r.splits THEN json(r.album_json) END, \
                         'pressings', json((json_group_array(json(r.row) \
                             ORDER BY r.agreements DESC, r.lead_year IS NULL, r.lead_year, r.place) \
                             FILTER (WHERE r.kind <> 'narrowed_out'))), \
                         'narrowed_out', json((json_group_array(json(r.row) \
                             ORDER BY r.agreements DESC, r.lead_year IS NULL, r.lead_year, r.place) \
                             FILTER (WHERE r.kind = 'narrowed_out')))) AS section, \
                     json_group_array(json(r.lead) \
                         ORDER BY r.agreements DESC, r.lead_year IS NULL, r.lead_year, r.place) \
                         FILTER (WHERE r.kind <> 'narrowed_out') AS leads, \
                     MAX(r.kind <> 'narrowed_out') AS offers, \
                     MAX(r.agreements) FILTER (WHERE r.kind <> 'narrowed_out') AS best_offered, \
                     MAX(r.agreements) AS best_any \
              FROM rowj AS r GROUP BY r.card, CASE WHEN r.splits THEN r.album ELSE 0 END), \
          card_secs AS ( \
              SELECT s.card, json_group_array(json(s.section) ORDER BY s.section_no) AS sections, \
                     json_group_array(json(s.leads) ORDER BY s.section_no) AS leads, \
                     MAX(s.offers) AS offers, \
                     COALESCE(MAX(s.best_offered), MAX(s.best_any)) AS best \
              FROM secj AS s GROUP BY s.card), \
          card_rows AS ( \
              SELECT r.card, MIN(r.year) AS year_min, MAX(r.year) AS year_max, \
                     COALESCE( \
                         (json_group_array(r.source_group_id ORDER BY r.bidx, r.ord) \
                              FILTER (WHERE r.source_group_id IS NOT NULL)) ->> 0, \
                         (json_group_array(r.release_id ORDER BY r.bidx, r.ord)) ->> 0) AS id, \
                     (json_group_array(r.ord ORDER BY r.cover_standing = 'stated' DESC, \
                                       r.bidx, r.ord) \
                          FILTER (WHERE r.cover_url IS NOT NULL)) ->> 0 AS cover_ord \
              FROM recs AS r GROUP BY r.card), \
          card_sources AS ( \
              SELECT y.card, \
                     json_group_array(json_object('catalog', y.catalog, \
                                                  'group_url', y.group_url, \
                                                  'unread', y.unread) \
                                      ORDER BY bae_catalog_rank(y.catalog), y.seen) AS sources \
              FROM (SELECT x.*, \
                           MIN(x.extra) OVER (PARTITION BY x.card, x.catalog) AS lowest, \
                           ROW_NUMBER() OVER (PARTITION BY x.card, x.catalog, x.extra \
                                              ORDER BY x.seen) AS nth \
                    FROM (SELECT r.card, r.source AS catalog, \
                                 bae_catalog_group_url(r.source, r.source_group_id) AS group_url, \
                                 r.unread AS unread, r.bidx AS seen, 0 AS extra \
                          FROM recs AS r WHERE r.ord = r.first_ord \
                          UNION ALL \
                          SELECT r.card, json_extract(l.value, '$.catalog'), \
                                 bae_catalog_group_url(json_extract(l.value, '$.catalog'), \
                                                       json_extract(l.value, '$.key')), \
                                 0, r.bidx * 1000000000000 + r.ord * 1000 + l.key, 1 \
                          FROM recs AS r, json_each(r.links) AS l) AS x) AS y \
              WHERE y.extra = 0 OR (y.lowest = 1 AND y.nth = 1) \
              GROUP BY y.card) \
          SELECT json_group_array(json_object( \
              'id', c.id, \
              'titles', json((SELECT json_group_array(t.text ORDER BY t.ordinal) \
                              FROM import_candidate_card_text AS t \
                              WHERE {text_owner} AND t.card = c.card AND t.kind = 'title')), \
              'artists', json((SELECT json_group_array(t.text ORDER BY t.ordinal) \
                               FROM import_candidate_card_text AS t \
                               WHERE {text_owner} AND t.card = c.card AND t.kind = 'artist')), \
              'labels', json((SELECT json_group_array(t.text ORDER BY t.ordinal) \
                              FROM import_candidate_card_text AS t \
                              WHERE {text_owner} AND t.card = c.card AND t.kind = 'label')), \
              'cover', json((SELECT json(rc.record) FROM ({records}) AS rc \
                             WHERE rc.ord = c.cover_ord)), \
              'leads', json((SELECT json_group_array(json(lead.value)) \
                             FROM json_each(cs.leads) AS sec, json_each(sec.value) AS lead)), \
              'sources', json(COALESCE(srcs.sources, '[]')), \
              'year_min', c.year_min, 'year_max', c.year_max, \
              'sections', json(cs.sections)) \
              ORDER BY cs.offers DESC, cs.best DESC, c.card) \
          FROM card_rows AS c JOIN card_secs AS cs ON cs.card = c.card \
          LEFT JOIN card_sources AS srcs ON srcs.card = c.card)"
    )
}

/// The SQL of a scalar subquery reading the cards of the typed search whose
/// number `search_run` names, the SQL of a column of the search's own row.
pub(super) fn search_cards_sql(search_run: &str) -> String {
    let from = format!(
        "FROM import_candidate_match AS found \
         JOIN import_candidate_search_source AS part \
           ON part.search_run = found.search_run AND part.catalog = found.source \
         WHERE found.search_run = {search_run}"
    );
    let ord = "ROW_NUMBER() OVER (ORDER BY part.position, found.position) - 1";
    cards_sql(
        &format!(
            "SELECT {ord} AS ord, \
                    found.position, found.card, found.pressing, found.record_order, \
                    'returned' AS kind, found.source, found.release_id, found.title, \
                    found.year, found.source_group_id, found.album_links, found.cover_url, \
                    found.cover_standing, 0 AS a_disc_id, 0 AS a_barcode, 0 AS a_catalog, \
                    0 AS a_label, 0 AS a_year, 0 AS a_country, 0 AS a_title, 0 AS a_artist, \
                    NULL AS named_note, \
                    (SELECT json_group_array(json_object('catalog', link.catalog, 'key', link.key) \
                                             ORDER BY link.ordinal) \
                     FROM import_candidate_match_album_link AS link \
                     WHERE link.search_run = found.search_run \
                       AND link.position = found.position) AS links \
             {from}"
        ),
        &format!(
            "SELECT {ord} AS ord, {record} AS record {from}",
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
    let from = format!(
        "FROM import_candidate_lookup_release AS found \
         JOIN import_candidate_match AS matched \
           ON matched.content_hash IS found.content_hash \
          AND matched.identify_run IS found.identify_run \
          AND matched.position = found.match_position \
         WHERE {found}",
        found = owner.replace("{table}", "found"),
    );
    cards_sql(
        &format!(
            "SELECT found.ordinal AS ord, found.match_position AS position, found.card, \
                    found.pressing, found.record_order, 'answered' AS kind, matched.source, \
                    matched.release_id, matched.title, matched.year, matched.source_group_id, \
                    matched.album_links, matched.cover_url, matched.cover_standing, \
                    0 AS a_disc_id, 0 AS a_barcode, 0 AS a_catalog, 0 AS a_label, 0 AS a_year, \
                    0 AS a_country, 0 AS a_title, 0 AS a_artist, NULL AS named_note, \
                    (SELECT json_group_array(json_object('catalog', link.catalog, \
                                                         'key', link.key) \
                                             ORDER BY link.ordinal) \
                     FROM import_candidate_match_album_link AS link \
                     WHERE link.content_hash IS matched.content_hash \
                       AND link.identify_run IS matched.identify_run \
                       AND link.position = matched.position) AS links \
             {from}"
        ),
        &format!(
            "SELECT found.ordinal AS ord, {record} AS record {from}",
            record = match_record_sql("matched"),
        ),
        &owner.replace("{table}", "t"),
    )
}

/// The SQL of a scalar subquery reading the cards of the verdict-shaped row
/// joined as `verdict` (a candidate's verdict, or a run's): the cards of its
/// offered and set-aside releases with what the folder's text agrees with
/// about each row, how many rows the agreement check set aside, whether the
/// offered rows are several pressings of one album, and the check against the
/// folder and the release the verdict decides on
/// (`import_candidate_verdict_decision`). [`verdict_cards_of`] reads the JSON
/// object the subquery returns.
pub(super) fn verdict_cards_sql(verdict: &str) -> String {
    let owner = format!(
        "{{table}}.content_hash IS {verdict}.content_hash \
         AND {{table}}.identify_run IS {verdict}.identify_run"
    );
    let found = owner.replace("{table}", "found");
    let listed = format!("{found} AND found.listing IN ('offered', 'narrowed_out')");
    let from = format!("FROM import_candidate_match AS found WHERE {listed}");
    let cards = cards_sql(
        &format!(
            "SELECT found.position AS ord, found.position, found.card, found.pressing, \
                    found.record_order, found.listing AS kind, found.source, found.release_id, \
                    found.title, found.year, found.source_group_id, found.album_links, \
                    found.cover_url, found.cover_standing, \
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
             {from}"
        ),
        &format!(
            "SELECT found.position AS ord, {record} AS record {from}",
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
                // The layout write numbers a row's label lines from zero, in
                // order.
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

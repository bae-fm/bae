//! The album cards of a list of releases (a typed search's, a ledger cell's),
//! which one statement reads from the layout that the list's write stored:
//! each release's row, card and place among its row's records, the text of
//! each card, and what each row lists.
//!
//! The statement orders and groups the stored rows the way the pane draws the
//! cards: a card's catalog buckets, its sections when it holds two albums of
//! one catalog, its rows by year, its cover and the catalogs it names. The
//! statement also reads, for each row, whether the library holds the row's
//! lead release as a pressing. The
//! statement returns the cards as nested JSON, each record the release itself
//! as [`match_record_sql`] holds it, and [`groups_of`] reads that JSON into
//! the cards.

use super::match_record::{match_record_sql, StoredRecord};
use super::*;
use crate::tables::album_link_rows::AlbumLinkRow;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::records::LookupRef;
use bae_catalogs::search::MetadataResult;
use bae_identify::release_group::{
    AlbumHeading, Pressing, PressingSection, ReleaseGroup, ReleaseGroupSource, RowChoice,
    StatedLabelLine, Statement, Statements,
};
use bae_identify::{Agreements, AlbumLink, RowAgreements, SharedAlbum, VerdictCards};
use bae_records::records::release_records::pressing_held_sql;
use bae_types::pressing::{MediaCount, ReleaseArea};
use std::str::FromStr;

/// The SQL of a scalar subquery reading the cards of one list, as the write
/// that laid the list out stored them: each card in its place among the
/// list's cards, its sections in order, and each section's rows in their
/// place.
///
/// `members` selects the list's releases as `ord` (the place in the list, 0
/// first, in the order the list's loader returns them), `card`, `pressing`,
/// `record_order`, `section`, `row_order`, `kind` (the listing the row is
/// of), `source`, `release_id`, `named_note`, and the eight flags
/// `a_disc_id`, `a_barcode`, `a_catalog`, `a_label`, `a_year`, `a_country`,
/// `a_title` and `a_artist` the folder's text agrees with it by (all 0 for a
/// list nothing judges), and the two flags `chosen` and `pick_being_read` of
/// what the list's candidate picked ([`choice_columns_sql`]). `records`
/// selects the same releases' `ord` and `record` (the release as
/// [`match_record_sql`] holds it, which a card carries whole). `text_owner`
/// is a condition over `t` selecting the list's rows of the card, card text
/// and row statement tables.
///
/// The statement names each of its WITH names as few times as it can, because
/// SQLite copies a WITH name's whole definition at every reference, and a
/// statement that holds many copies stops with "VIEWs and/or subqueries
/// nested too deep" once its cursors pass `SQLITE_LIMIT_EXPR_DEPTH` (1000).
/// So the releases' JSON comes in at the two places that carry it, through
/// `records`, never through `recs`.
pub(super) fn cards_sql(members: &str, records: &str, text_owner: &str) -> String {
    format!(
        "(WITH \
          recs AS (SELECT m.* FROM ({members}) AS m), \
          pressings AS ( \
              SELECT r.card, r.pressing, r.kind, MIN(r.section) AS section, \
                     MIN(r.row_order) AS row_order, \
                     MAX(r.chosen) AS chosen, MAX(r.pick_being_read) AS pick_being_read, \
                     json_group_array(json(rc.record) ORDER BY r.record_order) AS records, \
                     MAX(json_object('catalog', r.source, 'key', r.release_id)) \
                         FILTER (WHERE r.record_order = 0) AS lead, \
                     MAX({lead_held}) FILTER (WHERE r.record_order = 0) AS in_library, \
                     json_object('disc_id', MAX(r.a_disc_id), 'barcode', MAX(r.a_barcode), \
                                 'catalog', MAX(r.a_catalog), 'label', MAX(r.a_label), \
                                 'year', MAX(r.a_year), 'country', MAX(r.a_country), \
                                 'title', MAX(r.a_title), 'artist', MAX(r.a_artist)) AS agreed \
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
                         'in_library', p.in_library, \
                         'agreed', json(p.agreed), \
                         'chosen', p.chosen, 'pick_being_read', p.pick_being_read, \
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
              SELECT r.card, r.section, \
                     json_object( \
                         'pressings', json((json_group_array(json(r.row) ORDER BY r.row_order) \
                             FILTER (WHERE r.kind <> 'narrowed_out'))), \
                         'narrowed_out', json((json_group_array(json(r.row) ORDER BY r.row_order) \
                             FILTER (WHERE r.kind = 'narrowed_out')))) AS section_json, \
                     json_group_array(json(r.lead) ORDER BY r.row_order) \
                         FILTER (WHERE r.kind <> 'narrowed_out') AS leads \
              FROM rowj AS r GROUP BY r.card, r.section), \
          card_secs AS ( \
              SELECT s.card, \
                     json_group_array(json(s.section_json) ORDER BY s.section) AS sections, \
                     json_group_array(json(s.leads) ORDER BY s.section) AS leads \
              FROM secj AS s GROUP BY s.card) \
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
              'sources', json(c.sources), \
              'headings', json(c.headings), \
              'year_min', c.year_min, 'year_max', c.year_max, \
              'sections', json(cs.sections)) \
              ORDER BY c.card_order) \
          FROM (SELECT t.* FROM import_candidate_card AS t WHERE {text_owner}) AS c \
          JOIN card_secs AS cs ON cs.card = c.card)",
        lead_held = pressing_held_sql("r.source", "r.release_id"),
    )
}

/// The SQL of the member columns `chosen` and `pick_being_read` of the match
/// row `found`, for a list whose candidate's files `files` names (NULL for a
/// library release's list, whose rows hold neither). `pick` is the SQL of the
/// release the list's verdict picks on its own; a list that is no verdict's
/// has none.
///
/// `chosen` marks a release when bae linked the candidate to it. Before bae
/// links the candidate to a pressing, it also marks the release the verdict
/// picked by itself while bae reads that release in full.
fn choice_columns_sql(found: &str, files: &str, pick: Option<&str>) -> String {
    let linked = format!("release_link.content_hash = {files} AND release_link.kind = 'pressing'");
    // SQLite checks the standing first, so it skips the pick's subquery
    // whenever bae is not reading a pick.
    let being_read = match pick {
        Some(pick) => format!(
            "(EXISTS (SELECT 1 FROM import_candidate_standing AS reading \
                      WHERE reading.content_hash = {files} \
                        AND reading.identification = 'reading_pick') \
              AND {found}.release_id IS {pick})"
        ),
        None => "0".to_string(),
    };
    format!(
        "(EXISTS (SELECT 1 FROM import_candidate_release_link AS release_link \
                  WHERE {linked} AND release_link.source = {found}.source \
                    AND release_link.release_id = {found}.release_id) \
          OR ({being_read} \
              AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link AS release_link \
                              WHERE {linked}))) AS chosen, \
         {being_read} AS pick_being_read"
    )
}

/// The SQL of the content hash of the folder candidate that the match row
/// `found` belongs to. A stored verdict's row carries the hash itself. A row
/// of a run that is still going gets it from the standing that names the run.
/// The SQL is NULL for a library release's run.
fn run_files_sql(found: &str) -> String {
    format!(
        "COALESCE({found}.content_hash, \
                  (SELECT run_standing.content_hash \
                   FROM import_candidate_standing AS run_standing \
                   WHERE run_standing.identify_run = {found}.identify_run))"
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
                    found.card, found.pressing, found.record_order, found.section, \
                    found.row_order, 'returned' AS kind, found.source, found.release_id, \
                    0 AS a_disc_id, 0 AS a_barcode, 0 AS a_catalog, \
                    0 AS a_label, 0 AS a_year, 0 AS a_country, 0 AS a_title, 0 AS a_artist, \
                    NULL AS named_note, {choice} \
             {from}",
            choice = choice_columns_sql(
                "found",
                "(SELECT asked.content_hash FROM import_candidate_search AS asked \
                  WHERE asked.search_run = found.search_run)",
                None,
            ),
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
            "SELECT found.ordinal AS ord, found.card, found.pressing, found.record_order, \
                    found.section, found.row_order, 'answered' AS kind, matched.source, \
                    matched.release_id, \
                    0 AS a_disc_id, 0 AS a_barcode, 0 AS a_catalog, 0 AS a_label, 0 AS a_year, \
                    0 AS a_country, 0 AS a_title, 0 AS a_artist, NULL AS named_note, {choice} \
             {from}",
            choice = choice_columns_sql("matched", &run_files_sql("matched"), None),
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
/// offered rows are several pressings of one album, what a found verdict's
/// write stored as its decision (the release it picks, or why it picks
/// none), and the failures no cell of its ledger shows. [`verdict_cards_of`]
/// reads the JSON object the subquery returns.
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
            "SELECT found.position AS ord, found.card, found.pressing, found.record_order, \
                    found.section, found.row_order, found.listing AS kind, found.source, \
                    found.release_id, found.agrees_disc_id AS a_disc_id, \
                    found.agrees_barcode AS a_barcode, \
                    found.agrees_catalog AS a_catalog, found.agrees_label AS a_label, \
                    found.agrees_year AS a_year, found.agrees_country AS a_country, \
                    found.agrees_title AS a_title, found.agrees_artist AS a_artist, \
                    found.named_note, {choice} \
             {from}",
            choice = choice_columns_sql(
                "found",
                &run_files_sql("found"),
                Some(&format!("{verdict}.unattended_pick")),
            ),
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
            'decision', json(CASE WHEN {verdict}.kind = 'found' \
                                  THEN json_object('unattended_pick', {verdict}.unattended_pick, \
                                                   'declined', {verdict}.declined, \
                                                   'medium_conflict', {verdict}.medium_conflict) \
                                  END), \
            'failure_lines', {failure_lines}, \
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
           FROM listed_cards)",
        failure_lines = super::verdict_failure_rows::failure_lines_sql(verdict),
    )
}

/// Returns the cards of the verdict stored under `content_hash`, which its
/// rows stored, in one statement. Returns no cards unless the verdict holds
/// findings.
pub(crate) fn load_verdict_cards_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
) -> Result<VerdictCards, DbError> {
    let json: Option<String> = sql
        .query_row(
            &format!(
                "SELECT {cards} FROM import_candidate_verdict AS verdict \
                 WHERE verdict.content_hash = ? AND {holds_findings}",
                cards = verdict_cards_sql("verdict"),
                holds_findings = super::verdict_rows::holds_findings_sql("verdict"),
            ),
            [content_hash],
            |row| row.get(0),
        )
        .optional()?;
    json.map_or_else(|| Ok(VerdictCards::default()), |json| verdict_cards_of(&json))
}

/// Returns the cards of the verdict of the current state of the release the
/// scan stores at `key`, in one statement that binds only the key. Returns no
/// cards unless that verdict holds findings.
pub(crate) fn load_candidate_verdict_cards_on(
    sql: &impl coven::SqlRead,
    key: &crate::CandidateKey,
) -> Result<VerdictCards, DbError> {
    let json: Option<String> = sql
        .query_row(
            &format!(
                "SELECT {cards} FROM import_candidate_state AS state \
                 JOIN import_candidate_verdict AS verdict \
                     ON verdict.content_hash = state.content_hash \
                 WHERE {current} AND {holds_findings}",
                cards = verdict_cards_sql("verdict"),
                current = super::current_at_scan_sql("state"),
                holds_findings = super::verdict_rows::holds_findings_sql("verdict"),
            ),
            named_params! { ":key": key },
            |row| row.get(0),
        )
        .optional()?;
    json.map_or_else(|| Ok(VerdictCards::default()), |json| verdict_cards_of(&json))
}

/// What [`verdict_cards_sql`] answers.
#[derive(serde::Deserialize)]
pub(super) struct StoredVerdictCards {
    cards: Vec<StoredCard>,
    narrowed_out_count: u32,
    /// The offered rows when they are several pressings of one album.
    shared_album: Option<StoredSharedAlbum>,
    /// What a found verdict decides. The statement returns NULL for every
    /// other kind of verdict, whose decision row declines nothing and picks
    /// nothing.
    decision: Option<StoredDecision>,
    /// The failures no cell of the ledger shows, as
    /// [`super::verdict_failure_rows::failure_lines_sql`] answers them.
    failure_lines: Option<super::verdict_failure_rows::StoredFailureLines>,
}

/// What a found verdict's write stored as its decision: the release it picks,
/// or why it picks none, with the medium conflict the reason may name.
#[derive(serde::Deserialize)]
struct StoredDecision {
    unattended_pick: Option<String>,
    declined: Option<String>,
    medium_conflict: Option<String>,
}

impl StoredDecision {
    fn decision(self) -> Result<bae_identify::FoundDecision, DbError> {
        match (self.unattended_pick, self.declined) {
            (Some(lead), None) => Ok(bae_identify::FoundDecision::Picks(lead)),
            (None, Some(declined)) => {
                let medium_conflict =
                    super::verdict_rows::medium_conflict_of(self.medium_conflict)?;
                let declined = super::verdict_rows::declined_of(&declined, medium_conflict)?;
                Ok(bae_identify::FoundDecision::Declined(declined.folder_check()))
            }
            (lead, declined) => Err(DbError::unreadable("a verdict's decision", (lead, declined))),
        }
    }
}

/// The offered rows of one album: each row's lead release in the cards' order,
/// and the album each catalog files them under.
#[derive(serde::Deserialize)]
struct StoredSharedAlbum {
    leads: Vec<StoredLead>,
    albums: Vec<AlbumLinkRow>,
}

/// A row's lead release: the catalog that names it and its key there.
#[derive(serde::Deserialize)]
struct StoredLead {
    catalog: String,
    key: String,
}

impl StoredSharedAlbum {
    fn shared_album(self) -> Result<SharedAlbum, DbError> {
        let leads = self
            .leads
            .into_iter()
            .map(|lead| Ok(LookupRef::new(super::match_rows::source_of(&lead.catalog)?, lead.key)))
            .collect::<Result<Vec<_>, DbError>>()?;
        let albums = self
            .albums
            .into_iter()
            .map(AlbumLinkRow::album)
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
    serde_json::from_str::<StoredVerdictCards>(json)
        .map_err(|error| DbError::unreadable_json("a verdict's stored cards", json, error))?
        .cards()
}

impl StoredVerdictCards {
    /// The cards these stored cards hold, with each row's agreements and
    /// notes.
    pub(super) fn cards(self) -> Result<VerdictCards, DbError> {
        let StoredVerdictCards { cards, narrowed_out_count, shared_album, decision, failure_lines } =
            self;
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
            decision: decision.map(StoredDecision::decision).transpose()?,
            failure_lines: failure_lines
                .map(super::verdict_failure_rows::failure_lines_of)
                .transpose()?,
        })
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
    /// Each section's album, for a card that splits its rows by album.
    headings: Option<Vec<StoredHeading>>,
    sections: Vec<StoredSection>,
}

#[derive(serde::Deserialize)]
struct StoredSource {
    catalog: String,
    group_url: Option<String>,
    unread: bool,
}

#[derive(serde::Deserialize)]
struct StoredHeading {
    title: String,
    catalog: String,
    group_url: Option<String>,
    unread: bool,
}

#[derive(serde::Deserialize)]
struct StoredSection {
    pressings: Vec<StoredRow>,
    narrowed_out: Vec<StoredRow>,
}

#[derive(serde::Deserialize)]
struct StoredRow {
    records: Vec<StoredRecord>,
    /// 1 when the library holds the lead record's release as a pressing.
    in_library: i64,
    statements: Vec<StoredStatement>,
    agreed: StoredAgreed,
    notes: Vec<StoredNote>,
    chosen: i64,
    pick_being_read: i64,
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
    unread: bool,
) -> Result<ReleaseGroupSource, DbError> {
    Ok(ReleaseGroupSource { source: catalog_of(catalog)?, group_url, album_links_unread: unread })
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
        let choice =
            RowChoice { chosen: stored.chosen != 0, pick_being_read: stored.pick_being_read != 0 };
        Ok(Pressing::stored(
            records,
            statements_of(stored.statements)?,
            choice,
            stored.in_library != 0,
        ))
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
                sections: {
                    let mut headings = card.headings.map(Vec::into_iter);
                    card.sections
                        .into_iter()
                        .map(|section| {
                            Ok(PressingSection {
                                album: headings
                                    .as_mut()
                                    .map(|headings| {
                                        headings.next().ok_or_else(|| {
                                            DbError::unreadable(
                                                "a card's album headings",
                                                "fewer than its sections",
                                            )
                                        })
                                    })
                                    .transpose()?
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
                        .collect::<Result<_, DbError>>()?
                },
            })
        })
        .collect()
}

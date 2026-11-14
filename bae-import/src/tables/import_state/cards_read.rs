//! The album cards of a list of releases (a typed search's, a ledger cell's),
//! read in one statement from what the write that stored the list laid out:
//! each release's row, card and place among its row's records, the text of
//! each card, and what each row lists.
//!
//! The statement orders and groups what is stored the way the cards are
//! drawn: a card's catalog buckets, its sections when it holds two albums of
//! one catalog, its rows by year, its cover and the catalogs it names. It
//! answers the cards as nested JSON, each record the place of its release in
//! the list, and [`groups_of`] reads that into the cards.

use super::*;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::search::MetadataResult;
use bae_identify::release_group::{
    AlbumHeading, Pressing, PressingSection, ReleaseGroup, ReleaseGroupSource, StatedLabelLine,
    Statement, Statements,
};
use bae_types::pressing::{MediaCount, ReleaseArea};
use std::str::FromStr;

/// The SQL of a scalar subquery reading the cards of one list.
///
/// `members` selects the list's releases as `ord` (the place in the list, 0
/// first, in the order the list's loader returns them), `position` (the
/// release's match row), `card`, `pressing`, `record_order`, `kind` (the
/// listing the row is of), `source`, `release_id`, `title`, `year`,
/// `source_group_id`, `album_links`, `cover_url`, `cover_standing` and
/// `links` (a JSON list of the albums its statements name). `text_owner` is
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
                     json_group_array(ord ORDER BY record_order) AS records \
              FROM recs GROUP BY card, pressing, kind), \
          rowj AS ( \
              SELECT p.*, s.splits, \
                     json_object( \
                         'records', json(p.records), \
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
                             ORDER BY r.lead_year IS NULL, r.lead_year, r.place) \
                             FILTER (WHERE r.kind <> 'narrowed_out'))), \
                         'narrowed_out', json((json_group_array(json(r.row) \
                             ORDER BY r.lead_year IS NULL, r.lead_year, r.place) \
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
                     0 AS best \
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
              'cover', (SELECT r.ord FROM recs AS r \
                        WHERE r.card = c.card AND r.cover_url IS NOT NULL \
                        ORDER BY r.cover_standing = 'stated' DESC, r.bidx, r.ord LIMIT 1), \
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
                    found.cover_standing, \
                    (SELECT json_group_array(json_object('catalog', link.catalog, 'key', link.key) \
                                             ORDER BY link.ordinal) \
                     FROM import_candidate_match_album_link AS link \
                     WHERE link.search_run = found.search_run \
                       AND link.position = found.position) AS links \
             FROM import_candidate_match AS found \
             JOIN import_candidate_search_source AS part \
               ON part.search_run = found.search_run AND part.catalog = found.source \
             WHERE found.search_run = {search_run}"
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
            found = owner.replace("{table}", "found")
        ),
        &owner.replace("{table}", "t"),
    )
}

/// One card as [`cards_sql`] answers it.
#[derive(serde::Deserialize)]
pub(super) struct StoredCard {
    id: String,
    titles: Vec<String>,
    artists: Vec<String>,
    labels: Vec<String>,
    cover: Option<usize>,
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
    records: Vec<usize>,
    statements: Vec<StoredStatement>,
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

/// The cards of a list that [`cards_sql`] answered, over the list's own
/// releases in the order the statement numbers them.
pub(super) fn groups_of(
    json: &str,
    list: &[&MetadataResult],
) -> Result<Vec<ReleaseGroup>, DbError> {
    let cards: Vec<StoredCard> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json("a list's stored cards", json, error))?;
    groups_from(cards, list)
}

/// The cards of a list that [`cards_sql`] answered, already read from their
/// JSON, over the list's own releases in the order the statement numbers
/// them.
pub(super) fn groups_from(
    cards: Vec<StoredCard>,
    list: &[&MetadataResult],
) -> Result<Vec<ReleaseGroup>, DbError> {
    let release = |place: usize| -> Result<&MetadataResult, DbError> {
        list.get(place).copied().ok_or_else(|| {
            DbError::unreadable("a card's release, which the list does not hold", place)
        })
    };
    let row = |stored: StoredRow| -> Result<Pressing, DbError> {
        let records = stored
            .records
            .into_iter()
            .map(|place| release(place).cloned())
            .collect::<Result<Vec<_>, _>>()?;
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
                    .map(|place| release(place).map(|release| release.cover_art.clone()))
                    .transpose()?
                    .flatten(),
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

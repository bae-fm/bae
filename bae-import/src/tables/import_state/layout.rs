//! How one list of releases lays out into pressing rows and album cards, as
//! the tables keep it. A typed search's list and each ledger cell's list are
//! laid out by the write that changes what they hold, so a read selects the
//! rows, the cards and the text the cards name; nothing groups a list again
//! as it is read.
//!
//! The rules are the identify domain's (`bae_identify::release_group`); this
//! module takes what they decided for a list, in list order, and writes each
//! release's place and the text of each card and row beside it.

use super::match_rows::{
    album_links_column, load_search_run_matches_on, ordinal_column, store_album_link_rows,
    MatchOwner,
};
use super::search_rows::load_picks_on;
use super::*;
use bae_catalogs::album_links::{self, AlbumLinks, GroupLinks};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::MetadataResult;
use bae_identify::release_group::{group_results, unranked, ReleaseGroup, Statement, Statements};
use std::collections::HashMap;

/// Where one release of a list stands: its row, its card and its place among
/// the records of its row. Rows and cards are numbered by where each first
/// appears in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Place {
    pub(super) pressing: u32,
    pub(super) card: u32,
    pub(super) record_order: u32,
}

/// The titles, artists and labels a card names, in the order its records give
/// them.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CardText {
    pub(super) card: u32,
    pub(super) titles: Vec<String>,
    pub(super) artists: Vec<bae_types::credits::CreditLine>,
    pub(super) labels: Vec<String>,
}

/// What a row lists where its records state a fact.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RowStatements {
    pub(super) pressing: u32,
    pub(super) statements: Statements,
}

/// One list as the identify domain's rules laid it out.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ListLayout {
    /// Each release's place, aligned with the list.
    pub(super) places: Vec<Place>,
    pub(super) cards: Vec<CardText>,
    pub(super) rows: Vec<RowStatements>,
}

impl ListLayout {
    /// The layout of `list` that `groups`, the cards the rules made of it,
    /// hold.
    pub(super) fn of(list: &[MetadataResult], groups: &[ReleaseGroup]) -> Self {
        let at: HashMap<(LookupCatalog, &str), usize> = list
            .iter()
            .enumerate()
            .map(|(at, result)| ((result.source, result.release_id.as_str()), at))
            .collect();
        let index_of = |release: &MetadataResult| -> usize {
            *at.get(&(release.source, release.release_id.as_str()))
                .expect("a card holds only releases of the list it was made from")
        };
        // Every row of every card, with the list index of each of its records.
        struct Found<'a> {
            card: usize,
            records: Vec<usize>,
            pressing: &'a bae_identify::release_group::Pressing,
        }
        let mut found: Vec<Found<'_>> = Vec::new();
        for (card, group) in groups.iter().enumerate() {
            for pressing in group.pressings().chain(group.narrowed_out()) {
                found.push(Found {
                    card,
                    records: pressing.releases.iter().map(index_of).collect(),
                    pressing,
                });
            }
        }
        let first_of = |records: &[usize]| *records.iter().min().expect("a row holds a record");
        let mut by_first: Vec<usize> = (0..found.len()).collect();
        by_first.sort_by_key(|&row| first_of(&found[row].records));
        let mut pressing_of = vec![0u32; found.len()];
        for (number, row) in by_first.into_iter().enumerate() {
            pressing_of[row] = number as u32;
        }
        let mut card_first = vec![usize::MAX; groups.len()];
        for row in &found {
            card_first[row.card] = card_first[row.card].min(first_of(&row.records));
        }
        let mut places: Vec<Option<Place>> = vec![None; list.len()];
        for (row, found) in found.iter().enumerate() {
            for (record_order, &release) in found.records.iter().enumerate() {
                let place = Place {
                    pressing: pressing_of[row],
                    card: card_first[found.card] as u32,
                    record_order: record_order as u32,
                };
                assert!(
                    places[release].replace(place).is_none(),
                    "a release is a record of one row"
                );
            }
        }
        let cards = groups
            .iter()
            .enumerate()
            .map(|(card, group)| CardText {
                card: card_first[card] as u32,
                titles: group.titles.clone(),
                artists: group.artists.clone(),
                labels: group.labels.clone(),
            })
            .collect();
        let rows = found
            .iter()
            .enumerate()
            .map(|(row, found)| RowStatements {
                pressing: pressing_of[row],
                statements: found.pressing.statements().clone(),
            })
            .collect();
        Self {
            places: places
                .into_iter()
                .map(|place| place.expect("every release of the list is on a card"))
                .collect(),
            cards,
            rows,
        }
    }
}

/// Whose rows a list's text belongs to, and which list of theirs it is.
#[derive(Clone, Copy)]
pub(super) struct ListOwner<'a> {
    pub(super) content_hash: Option<&'a str>,
    pub(super) identify_run: Option<i64>,
    pub(super) search_run: Option<i64>,
    /// The cell's step, empty for the owner's own list.
    pub(super) step: &'a str,
    pub(super) position: i64,
    pub(super) cell: i64,
}

impl<'a> ListOwner<'a> {
    /// A search's own list.
    pub(super) fn search(run: i64) -> Self {
        Self {
            content_hash: None,
            identify_run: None,
            search_run: Some(run),
            step: "",
            position: 0,
            cell: 0,
        }
    }
}

/// Write `layout`'s card text and row statements as `owner`'s, in place of
/// what its list held. `listing` is what the rows of the list are listed as.
pub(super) fn store_list_text(
    sql: &CovenWrite<'_>,
    owner: ListOwner<'_>,
    listing: &str,
    layout: &ListLayout,
) -> Result<(), DbError> {
    let ListOwner { content_hash, identify_run, search_run, step, position, cell } = owner;
    for table in ["import_candidate_card_text", "import_candidate_row_statement"] {
        sql.execute(
            &format!(
                "DELETE FROM {table} WHERE content_hash IS ?1 AND identify_run IS ?2 \
                 AND search_run IS ?3 AND step = ?4 AND position = ?5 AND cell = ?6"
            ),
            params![content_hash, identify_run, search_run, step, position, cell],
        )?;
    }
    let card_text = |card: u32, kind: &str, ordinal: usize, text: &str| {
        sql.execute(
            "INSERT INTO import_candidate_card_text \
                 (content_hash, identify_run, search_run, step, position, cell, card, kind, \
                  ordinal, text) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                identify_run,
                search_run,
                step,
                position,
                cell,
                card,
                kind,
                ordinal_column(ordinal)?,
                text
            ],
        )?;
        Ok::<_, DbError>(())
    };
    for card in &layout.cards {
        for (ordinal, title) in card.titles.iter().enumerate() {
            card_text(card.card, "title", ordinal, title)?;
        }
        for (ordinal, artist) in card.artists.iter().enumerate() {
            card_text(card.card, "artist", ordinal, &stored_credits::line_json(artist)?)?;
        }
        for (ordinal, label) in card.labels.iter().enumerate() {
            card_text(card.card, "label", ordinal, label)?;
        }
    }
    for row in &layout.rows {
        let statement =
            |fact: &str, line: usize, ordinal: usize, value: &str, catalogs: &[LookupCatalog]| {
                let catalogs: Vec<&str> = catalogs.iter().map(|catalog| catalog.as_str()).collect();
                sql.execute(
                    "INSERT INTO import_candidate_row_statement \
                     (content_hash, identify_run, search_run, step, position, cell, listing, \
                      pressing, fact, line, ordinal, value, catalogs) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        content_hash,
                        identify_run,
                        search_run,
                        step,
                        position,
                        cell,
                        listing,
                        row.pressing,
                        fact,
                        ordinal_column(line)?,
                        ordinal_column(ordinal)?,
                        value,
                        serde_json::to_string(&catalogs)
                            .map_err(|error| DbError::encode("a statement's catalogs", error))?,
                    ],
                )?;
                Ok::<_, DbError>(())
            };
        let Statements { years, labels, barcodes, areas, media } = &row.statements;
        let states = |fact: &str, stated: Vec<(String, &[LookupCatalog])>| {
            for (ordinal, (value, catalogs)) in stated.iter().enumerate() {
                statement(fact, 0, ordinal, value, catalogs)?;
            }
            Ok::<_, DbError>(())
        };
        states("year", worded(years, |year| Ok(year.to_string()))?)?;
        for (line, label) in labels.iter().enumerate() {
            for (ordinal, name) in label.names.iter().enumerate() {
                statement("label_name", line, ordinal, name, &[])?;
            }
            for (ordinal, number) in label.catalog_numbers.iter().enumerate() {
                statement("catalog_number", line, ordinal, &number.value, &number.catalogs)?;
            }
        }
        states("barcode", worded(barcodes, |barcode| Ok(barcode.clone()))?)?;
        states("area", worded(areas, |area| json_of("a row's area", area))?)?;
        states("media", worded(media, |counts| json_of("a row's media", counts))?)?;
    }
    Ok(())
}

/// Lay out the typed search `run`'s whole list again, inside the write that
/// changed it, and store what the rules decided: each release's row, card
/// and place among its row's records, what each MusicBrainz group was read to
/// be from the picks and the list, and the text of each card and row. Answers
/// what each group was read to be.
pub(super) fn lay_out_search_on(
    sql: &CovenWrite<'_>,
    run: i64,
) -> Result<Vec<GroupLinks>, DbError> {
    let rows = load_search_run_matches_on(sql, run)?;
    let picks: HashMap<String, AlbumLinks> =
        load_picks_on(sql, run)?.into_iter().map(|pick| (pick.release_id, pick.links)).collect();
    let mut results: Vec<MetadataResult> = rows
        .iter()
        .map(|row| MetadataResult { album_links: AlbumLinks::NotAsked, ..row.result.clone() })
        .collect();
    let read = album_links::read_groups(&results.iter().collect::<Vec<_>>(), |release| {
        picks.get(release).cloned()
    });
    for result in &mut results {
        album_links::apply(result, &read);
    }
    // Typed search: nothing was judged against the candidate's own text, so
    // the rows keep the pressing-year order alone.
    let groups = group_results(unranked(results.clone()), None);
    let layout = ListLayout::of(&results, &groups);
    for ((row, result), place) in rows.iter().zip(&results).zip(&layout.places) {
        sql.execute(
            "UPDATE import_candidate_match \
             SET pressing = ?, card = ?, record_order = ?, album_links = ? \
             WHERE search_run = ? AND position = ?",
            params![
                place.pressing,
                place.card,
                place.record_order,
                album_links_column(&result.album_links),
                run,
                row.position
            ],
        )?;
        sql.execute(
            "DELETE FROM import_candidate_match_album_link WHERE search_run = ? AND position = ?",
            params![run, row.position],
        )?;
        store_album_link_rows(sql, MatchOwner::Search(run), row.position, &result.album_links)?;
    }
    store_list_text(sql, ListOwner::search(run), "returned", &layout)?;
    Ok(read)
}

/// Each statement's value as the row stores it, with the catalogs stating it.
fn worded<T>(
    statements: &[Statement<T>],
    word: impl Fn(&T) -> Result<String, DbError>,
) -> Result<Vec<(String, &[LookupCatalog])>, DbError> {
    statements
        .iter()
        .map(|statement| Ok((word(&statement.value)?, statement.catalogs.as_slice())))
        .collect()
}

fn json_of(what: &str, value: &impl serde::Serialize) -> Result<String, DbError> {
    serde_json::to_string(value).map_err(|error| DbError::encode(what, error))
}

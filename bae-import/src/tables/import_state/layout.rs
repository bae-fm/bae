//! How bae places one list of releases into pressing rows and album cards, as
//! the tables keep it. The write that changes a typed search's list or a
//! ledger cell's list also groups it into rows and cards and stores where
//! each card, section and row shows and the text the cards name, so a read
//! only selects them and never groups or orders a list again.
//!
//! The rules are the identify domain's (`bae_identify::release_group`); this
//! module takes what they decided for a list, in list order, and writes each
//! release's place, each card's place, heading, cover and catalogs, and the
//! text of each card and row beside it.

use super::match_record::load_search_run_matches_on;
use super::match_rows::{album_links_column, ordinal_column, store_album_link_rows, MatchOwner};
use super::search_rows::load_picks_on;
use super::*;
use bae_catalogs::album_links::{self, AlbumLinks, GroupLinks};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::MetadataResult;
use bae_identify::release_group::{
    group_results, unranked, AlbumHeading, ReleaseGroup, ReleaseGroupSource, Statement, Statements,
};
use std::collections::HashMap;

/// Where one release of a list stands: its row, its card, its place among
/// the records of its row, the section of the card its row shows in and the
/// row's place among that section's rows of its listing. A card's number is
/// the list place of its first record. `ListLayout::of` numbers rows in the
/// order each first appears in the list, and `ListLayout::of_verdict` keeps
/// the numbers the run gave them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Place {
    pub(super) pressing: u32,
    pub(super) card: u32,
    pub(super) record_order: u32,
    pub(super) section: u32,
    pub(super) row_order: u32,
}

/// One card as the grouping rules made it: its number, its place among the
/// list's cards, what names and heads it, the list place of the release whose
/// cover it shows, and the titles, artists and labels it names, in the order
/// its records give them.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CardLayout {
    pub(super) card: u32,
    pub(super) card_order: u32,
    pub(super) id: String,
    pub(super) cover_ord: Option<u32>,
    pub(super) year_min: Option<i32>,
    pub(super) year_max: Option<i32>,
    pub(super) sources: Vec<ReleaseGroupSource>,
    /// Each section's album heading, for a card that splits its rows by
    /// album; `None` for a card of one section.
    pub(super) headings: Option<Vec<AlbumHeading>>,
    pub(super) titles: Vec<String>,
    pub(super) artists: Vec<bae_types::credits::CreditLine>,
    pub(super) labels: Vec<String>,
}

/// What a row lists where its records state a fact.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RowStatements {
    /// What the row is listed as: the list's own listing, or for a verdict's
    /// list whether the row is offered or set aside.
    pub(super) listing: &'static str,
    pub(super) pressing: u32,
    pub(super) statements: Statements,
}

/// One list as the identify domain's rules laid it out.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ListLayout {
    /// Each release's place, aligned with the list.
    pub(super) places: Vec<Place>,
    pub(super) cards: Vec<CardLayout>,
    pub(super) rows: Vec<RowStatements>,
}

impl ListLayout {
    /// Where each release of `list` sits in `groups`, the cards the grouping
    /// rules made from it. The layout lists every row as `listing`. The layout
    /// numbers the rows in the order their first records appear in the list.
    pub(super) fn of(
        list: &[MetadataResult],
        groups: &[ReleaseGroup],
        listing: &'static str,
    ) -> Self {
        Self::laid_out(list, groups, |_first, rank| rank, |_| listing)
    }

    /// Where each release of a verdict's `list` sits in `groups`. The list
    /// holds its `offered` releases first and the ones the agreement check
    /// set aside after. The rows keep the numbers the run gave them: `rows`
    /// holds each release's row number, in list order. The offered rows and
    /// the set-aside rows each count from their own start.
    pub(super) fn of_verdict(
        list: &[MetadataResult],
        groups: &[ReleaseGroup],
        rows: &[u32],
        offered: usize,
    ) -> Self {
        assert_eq!(list.len(), rows.len(), "each release of a verdict's list names its row");
        Self::laid_out(
            list,
            groups,
            |first, _| rows[first],
            |first| if first < offered { "offered" } else { "narrowed_out" },
        )
    }

    /// `number` names a row from the list place of its first record and its
    /// rank among the rows by that place; `listing` says what a row is listed
    /// as from the same place.
    fn laid_out(
        list: &[MetadataResult],
        groups: &[ReleaseGroup],
        number: impl Fn(usize, u32) -> u32,
        listing: impl Fn(usize) -> &'static str,
    ) -> Self {
        let at: HashMap<(LookupCatalog, &str), usize> = list
            .iter()
            .enumerate()
            .map(|(at, result)| ((result.source, result.release_id.as_str()), at))
            .collect();
        let index_of = |release: &MetadataResult| -> usize {
            *at.get(&(release.source, release.release_id.as_str()))
                .expect("a card holds only releases of the list it was made from")
        };
        // Every row of every card, with the list index of each of its records,
        // the section it shows in and its place among that section's rows of
        // its listing.
        struct Found<'a> {
            card: usize,
            records: Vec<usize>,
            pressing: &'a bae_identify::release_group::Pressing,
            section: usize,
            row_order: usize,
        }
        let mut found: Vec<Found<'_>> = Vec::new();
        for (card, group) in groups.iter().enumerate() {
            for (section, rows) in group.sections.iter().enumerate() {
                for listed in [&rows.pressings, &rows.narrowed_out] {
                    for (row_order, pressing) in listed.iter().enumerate() {
                        found.push(Found {
                            card,
                            records: pressing.releases.iter().map(index_of).collect(),
                            pressing,
                            section,
                            row_order,
                        });
                    }
                }
            }
        }
        let first_of = |records: &[usize]| *records.iter().min().expect("a row holds a record");
        let mut by_first: Vec<usize> = (0..found.len()).collect();
        by_first.sort_by_key(|&row| first_of(&found[row].records));
        let mut pressing_of = vec![0u32; found.len()];
        for (rank, row) in by_first.into_iter().enumerate() {
            pressing_of[row] = number(first_of(&found[row].records), rank as u32);
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
                    section: found.section as u32,
                    row_order: found.row_order as u32,
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
            .map(|(card, group)| {
                // The card shows the cover the rules chose from one of its
                // releases: the first of its releases in the list that carries
                // that cover.
                let cover_ord = group.cover_art.as_ref().map(|cover| {
                    found
                        .iter()
                        .filter(|row| row.card == card)
                        .flat_map(|row| row.records.iter().copied())
                        .filter(|&at| list[at].cover_art.as_ref() == Some(cover))
                        .min()
                        .expect("the rules take a card's cover from one of its releases")
                        as u32
                });
                let splits = group.sections.iter().any(|section| section.album.is_some());
                CardLayout {
                    card: card_first[card] as u32,
                    card_order: card as u32,
                    id: group.id.clone(),
                    cover_ord,
                    year_min: group.year_min,
                    year_max: group.year_max,
                    sources: group.sources.clone(),
                    headings: splits.then(|| {
                        group.sections.iter().filter_map(|section| section.album.clone()).collect()
                    }),
                    titles: group.titles.clone(),
                    artists: group.artists.clone(),
                    labels: group.labels.clone(),
                }
            })
            .collect();
        let rows = found
            .iter()
            .enumerate()
            .map(|(row, found)| RowStatements {
                listing: listing(first_of(&found.records)),
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
    /// A verdict's or a run's own list.
    pub(super) fn verdict(owner: super::verdict_rows::Owner<'a>) -> Self {
        Self {
            content_hash: match owner {
                super::verdict_rows::Owner::Candidate(content_hash) => Some(content_hash),
                super::verdict_rows::Owner::Run(_) => None,
            },
            identify_run: owner.run(),
            search_run: None,
            step: "",
            position: 0,
            cell: 0,
        }
    }

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
/// what its list held.
pub(super) fn store_list_text(
    sql: &CovenWrite<'_>,
    owner: ListOwner<'_>,
    layout: &ListLayout,
) -> Result<(), DbError> {
    let ListOwner { content_hash, identify_run, search_run, step, position, cell } = owner;
    for table in
        ["import_candidate_card", "import_candidate_card_text", "import_candidate_row_statement"]
    {
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
    let source_json = |source: &ReleaseGroupSource| {
        serde_json::json!({
            "catalog": source.source.as_str(),
            "group_url": source.group_url,
            "unread": source.album_links_unread,
        })
    };
    for card in &layout.cards {
        let sources = serde_json::Value::Array(card.sources.iter().map(source_json).collect());
        let headings = card.headings.as_ref().map(|headings| {
            serde_json::Value::Array(
                headings
                    .iter()
                    .map(|heading| {
                        let mut json = source_json(&heading.source);
                        json["title"] = serde_json::Value::String(heading.title.clone());
                        json
                    })
                    .collect(),
            )
            .to_string()
        });
        sql.execute(
            "INSERT INTO import_candidate_card \
                 (content_hash, identify_run, search_run, step, position, cell, card, \
                  card_order, id, cover_ord, year_min, year_max, sources, headings) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                content_hash,
                identify_run,
                search_run,
                step,
                position,
                cell,
                card.card,
                card.card_order,
                card.id,
                card.cover_ord,
                card.year_min,
                card.year_max,
                sources.to_string(),
                headings,
            ],
        )?;
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
                        row.listing,
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
/// and place among its row's records, the album links that `read_groups`
/// works out for each MusicBrainz group from the picks and the list, and the
/// text of each card and row. Returns those links.
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
    // A typed search does not compare its results with the candidate's own
    // text, so the rows keep the pressing-year order alone.
    let groups = group_results(unranked(results.clone()), None);
    let layout = ListLayout::of(&results, &groups, "returned");
    for ((row, result), place) in rows.iter().zip(&results).zip(&layout.places) {
        sql.execute(
            "UPDATE import_candidate_match \
             SET pressing = ?, card = ?, record_order = ?, section = ?, row_order = ?, \
                 album_links = ? \
             WHERE search_run = ? AND position = ?",
            params![
                place.pressing,
                place.card,
                place.record_order,
                place.section,
                place.row_order,
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
    store_list_text(sql, ListOwner::search(run), &layout)?;
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

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;

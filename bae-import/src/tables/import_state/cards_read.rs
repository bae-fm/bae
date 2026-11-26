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

use super::match_record::StoredRecord;
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
use bae_types::pressing::{MediaCount, ReleaseArea};
use std::str::FromStr;

sqlive_compose::read!(
    CardsOfCandidate(candidate: String),
    r#"
    SELECT cards.*
    FROM @crate::tables::import_state::card_fragments::VerdictCards(
        owner: (SELECT :candidate AS content_hash, NULL AS identify_run)) AS cards
"#
);

/// Returns the cards of the verdict stored under `content_hash`, which its
/// rows stored, in one statement. Returns no cards unless the verdict holds
/// findings.
pub(crate) fn load_verdict_cards_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
) -> Result<VerdictCards, DbError> {
    let request = CardsOfCandidateParams { candidate: content_hash.to_string() };
    let row = sql
        .query(CardsOfCandidate::SQL, request.params().as_slice(), CardsOfCandidateRow::read)?
        .into_iter()
        .next();
    verdict_cards_of_column(row.and_then(|row| row.cards.verdict_cards).as_deref())
}

/// The cards the JSON document `json` holds, which a pane's read selects;
/// none for a candidate whose verdict holds no findings.
pub(crate) fn verdict_cards_of_column(json: Option<&str>) -> Result<VerdictCards, DbError> {
    json.map_or_else(|| Ok(VerdictCards::default()), verdict_cards_of)
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
                // order, and each line states a name or a catalog number, so
                // the statements, read in line order, reach each line after
                // the one before it.
                let lines = &mut statements.labels;
                if line == lines.len() {
                    lines.push(StatedLabelLine { names: Vec::new(), catalog_numbers: Vec::new() });
                }
                let label = lines
                    .get_mut(line)
                    .ok_or_else(|| DbError::unreadable("a row's label line", line))?;
                if fact == "label_name" {
                    label.names.push(value);
                } else {
                    label.catalog_numbers.push(stated(value, &catalogs)?);
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

#[cfg(test)]
#[path = "cards_read_tests.rs"]
mod tests;

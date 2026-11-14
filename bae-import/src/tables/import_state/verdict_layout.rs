//! How a verdict's or a run's releases lie out as rows and cards, judged
//! against the folder's own text. The write that changes the releases, the
//! text or the numbers struck out works the layout out and stores it with the
//! rows: each release's card and place among its row's records, which of its
//! fields the text agrees with, and the text of each card and row. A read
//! selects what was stored and never judges or groups the rows again.

use super::layout::{store_list_text, ListLayout, ListOwner, Place};
use super::match_rows::{load_owner_matches_on, ordinal_column, Placed, StoredMatches};
use super::verdict_rows::{outcome_of, read_verdict_row, verdict_columns, Outcome, Owner};
use super::*;
use bae_catalogs::search::MetadataResult;
use bae_identify::signals::{SourcedValue, TextLine};
use bae_identify::{
    Agreements, CandidateText, CatalogStepView, Findings, IdentifyRunView, JudgedFindings,
};

/// A verdict's releases as the rules laid them out, aligned with
/// [`Findings::releases`]: the offered releases, then the ones set aside.
pub(super) struct VerdictLayout {
    places: Vec<Place>,
    agreements: Vec<Agreements>,
    text: ListLayout,
}

impl VerdictLayout {
    /// What a verdict that found nothing lays out.
    pub(super) fn empty() -> Self {
        Self {
            places: Vec::new(),
            agreements: Vec::new(),
            text: ListLayout { places: Vec::new(), cards: Vec::new(), rows: Vec::new() },
        }
    }

    /// The layout of `findings` judged against `text`; `ledger` is the run's,
    /// whose disc-ID lookups say which releases agree on a disc ID.
    pub(super) fn of(
        findings: &Findings,
        ledger: Option<&IdentifyRunView>,
        text: &CandidateText,
        track_count: u32,
    ) -> Self {
        let judged = JudgedFindings::of(findings, ledger, text, track_count);
        let list: Vec<MetadataResult> = findings.releases().cloned().collect();
        let rows: Vec<u32> =
            findings.pressings.iter().chain(&findings.narrowed_out.pressings).copied().collect();
        // The two lists number their rows apart, as the run numbered them.
        let layout = ListLayout::of_verdict(&list, &judged.groups, &rows, findings.matches.len());
        Self { places: layout.places.clone(), agreements: judged.agreements, text: layout }
    }

    /// Where the release at `index` of [`Findings::releases`] stands, and
    /// what the text agrees with about it.
    pub(super) fn placed(&self, index: usize) -> Placed {
        let place = self.places[index];
        Placed {
            card: place.card,
            record_order: place.record_order,
            agreements: self.agreements[index],
        }
    }

    /// Store the text of each card and row as `owner`'s own list.
    pub(super) fn store_text(&self, sql: &CovenWrite<'_>, owner: Owner<'_>) -> Result<(), DbError> {
        store_list_text(sql, ListOwner::verdict(owner), &self.text)
    }
}

/// The text `owner`'s rows are judged and ordered against: its stored
/// signals' lines and barcodes, with the numbers struck out of them. A
/// candidate's are its lookup choices'; a run's are the rows its `ledger`
/// leaves out.
pub(super) fn verdict_text_on(
    sql: &CovenWrite<'_>,
    owner: Owner<'_>,
    ledger: Option<&IdentifyRunView>,
) -> Result<CandidateText, DbError> {
    let column = owner.column();
    let pool: Vec<TextLine> = sql
        .query(
            &format!(
                "SELECT text, origin FROM import_candidate_text_line \
                 WHERE {column} = ? ORDER BY position"
            ),
            params![owner],
            |row| Ok((row.get::<_, String>("text")?, row.get::<_, String>("origin")?)),
        )?
        .into_iter()
        .map(|(text, origin)| {
            Ok(TextLine {
                origin: origin.parse().map_err(|_unknown| {
                    DbError::unreadable("a stored text line's origin", &origin)
                })?,
                text,
            })
        })
        .collect::<Result<_, DbError>>()?;
    let barcodes: Vec<SourcedValue> = sql.query(
        &format!(
            "SELECT value, origin_path FROM import_candidate_signal_value \
             WHERE {column} = ? AND list = 'barcode' ORDER BY position"
        ),
        params![owner],
        |row| Ok(SourcedValue { value: row.get("value")?, origin_path: row.get("origin_path")? }),
    )?;
    let struck_out: Vec<String> = match owner {
        Owner::Candidate(content_hash) => sql.query(
            "SELECT value FROM import_candidate_discounted_catalog WHERE content_hash = ?",
            [content_hash],
            |row| row.get(0),
        )?,
        Owner::Run(_) => match ledger.map(|ledger| &ledger.catalog) {
            Some(CatalogStepView::Numbers { rows, .. }) => {
                rows.iter().filter(|row| row.excluded).map(|row| row.value.clone()).collect()
            }
            Some(CatalogStepView::NoneFound) | None => Vec::new(),
        },
    };
    Ok(CandidateText::of(&pool, &struck_out, &barcodes))
}

/// Lay out `owner`'s stored releases again, as the write that changed what
/// they are judged against ends: its signals' text or the numbers struck out.
/// An owner with no verdict, or one that found nothing, holds no layout.
pub(super) fn lay_out_matches_on(sql: &CovenWrite<'_>, owner: Owner<'_>) -> Result<(), DbError> {
    let column = owner.column();
    let row = sql
        .query(
            &format!(
                "SELECT {verdict} FROM import_candidate_verdict AS verdict \
                 WHERE verdict.{column} = ?",
                verdict = verdict_columns(),
            ),
            params![owner],
            |row| Ok(read_verdict_row(row)),
        )?
        .into_iter()
        .next();
    let Some(row) = row else {
        return Ok(());
    };
    let Some(row) = row? else {
        return Ok(());
    };
    let found: StoredMatches = load_owner_matches_on(sql, owner)?;
    let (outcome, _) = outcome_of(row, found)?;
    let outcome = Outcome::of(&outcome);
    let (Some(findings), Some(track_count)) = (outcome.findings(), outcome.track_count()) else {
        return Ok(());
    };
    let text = verdict_text_on(sql, owner, outcome.ledger())?;
    let layout = VerdictLayout::of(findings, outcome.ledger(), &text, track_count);
    for index in 0..findings.releases().count() {
        let placed = layout.placed(index);
        let agrees = placed.agreements;
        sql.execute(
            &format!(
                "UPDATE import_candidate_match \
                 SET card = ?, record_order = ?, agrees_disc_id = ?, agrees_barcode = ?, \
                     agrees_catalog = ?, agrees_label = ?, agrees_year = ?, agrees_country = ?, \
                     agrees_title = ?, agrees_artist = ? \
                 WHERE {column} = ? AND position = ?"
            ),
            params![
                placed.card,
                placed.record_order,
                agrees.disc_id,
                agrees.barcode,
                agrees.catalog,
                agrees.label,
                agrees.year,
                agrees.country,
                agrees.title,
                agrees.artist,
                owner,
                ordinal_column(index)?,
            ],
        )?;
    }
    layout.store_text(sql, owner)
}

#[cfg(test)]
#[path = "verdict_layout_tests.rs"]
mod tests;

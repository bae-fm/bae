//! The identify state as every surface renders it: [`IdentifyState`]'s
//! [`Findings`] folded into ranked album cards with their badges and library
//! statuses, and the run laid out as a ledger, [`IdentifyRunView`], one row
//! per value and one cell per provider. The bridge and automation copy it
//! field by field.
//!
//! The ledger is recorded once as a run ends and stored with its verdict.

use super::agreements::{judged_results, Agreements, CandidateText};
use super::combine::LookupAnswers;
use super::combine::{combine_results, Findings, LibraryStatuses};
use super::row_facts::FolderFacts;
use super::state::{
    BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState, IsrcProgress, LookupResults,
    LookupState, SearchProgress, SignalsContext, ValueLookup,
};
use super::NotAskedReason;
use crate::db::LibraryStatus;
use crate::import::release_group::{
    group_formed_rows, group_results, Judgements, Pressing, ReleaseGroup,
};
use crate::import::shared_album::SharedAlbum;
use crate::import::Catalog;
use crate::signals::{ArtworkScan, DiscIdSignal, LookupFailure};

/// One provider's lookup of one value: a cell of the ledger. The ledger's
/// types are serializable because [`super::TerminalVerdict`] stores it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LookupView {
    /// Not asked yet: the codes are still being read off the artwork.
    Queued,
    NotAsked {
        reason: NotAskedReason,
    },
    LookingUp,
    /// How many pressings the lookup named, and the album cards they fold
    /// into.
    Found {
        count: u32,
        groups: Vec<ReleaseGroup>,
    },
    NoMatch,
    Failed {
        failure: LookupFailure,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderCell {
    pub source: Catalog,
    pub lookup: LookupView,
}

/// One value extraction found, as a row of the ledger.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SignalValueRow {
    pub value: String,
    /// Whether the person left the value out: a barcode they unchecked, a
    /// catalog number they struck out.
    pub excluded: bool,
    /// One per provider in the run, in the run's provider order.
    pub cells: Vec<ProviderCell>,
}

/// The disc ID, read off a LOG or CUE and looked up on
/// [`Catalog::DISC_ID_CATALOG`] alone, so it has one lookup and no cells.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiscIdStepView {
    /// Extraction has not reported yet.
    Reading,
    /// No LOG or CUE to read one off.
    Absent,
    /// The CUE lays out audio at a sample rate no CD has, so it was not read.
    NotCdAudio,
    Read {
        disc_id: String,
        lookup: LookupView,
    },
}

/// The barcodes, read off the artwork and the CUE sheets, each asked of every
/// provider.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BarcodeStepView {
    /// No barcode source at all.
    Absent,
    /// The run does not read cover art, and no CUE sheet states a code.
    CoverArtOff,
    /// There was a source and it held no code.
    NoCodes,
    /// One row per code, in the order first seen. While `scanning`, more rows
    /// may come and every cell is queued.
    Rows {
        scanning: bool,
        rows: Vec<SignalValueRow>,
    },
}

/// A catalog number the folder's text offers that is not in effect: neither
/// picked, nor carried by a release the run found, nor struck out. A surface
/// folds these behind their count, for the person to pick.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CatalogCandidateView {
    pub value: String,
}

/// The catalog numbers: the ones in effect, each searched on every catalog,
/// and the rest offered.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CatalogStepView {
    /// Extraction found no catalog number to offer, none is in effect, and
    /// extraction is not still looking.
    NoneFound,
    /// No catalog number found, and the run does not read cover art.
    CoverArtOff,
    Numbers {
        /// Whether the artwork is still being read, so more numbers may come.
        scanning: bool,
        /// The numbers in effect — the picked ones in the order picked, then
        /// the ones the text prints that a found release carries, in the
        /// order found — each with its search; then the numbers the person
        /// struck out, left out and asked of nobody.
        rows: Vec<SignalValueRow>,
        /// The numbers the text offers that are none of those.
        candidates: Vec<CatalogCandidateView>,
    },
}

/// The ISRCs the audio's tags carry, looked up together on
/// [`Catalog::ISRC_CATALOG`] alone, so the step has one lookup and no cells.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IsrcStepView {
    /// Extraction has not reported yet.
    Reading,
    /// No audio file's tags carry one.
    Absent,
    /// Every code, each once, in the files' order.
    Read {
        isrcs: Vec<String>,
        lookup: LookupView,
    },
}

/// The title search, asked of every provider when the identifiers name
/// nothing.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SearchStepView {
    NotAsked {
        reason: NotAskedReason,
    },
    /// The identifiers answered; no search was needed.
    NotNeeded,
    /// Nothing to search by: the draft has no title.
    NoTitle,
    /// The identifiers are still being looked up.
    Waiting {
        album: String,
        artist: String,
    },
    Searched {
        album: String,
        /// Blank where the draft names no album artist.
        artist: String,
        cells: Vec<ProviderCell>,
    },
}

/// The run as a ledger: the three identifiers, the ISRCs and the title
/// search.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IdentifyRunView {
    /// The providers the run asks, in cell order.
    pub providers: Vec<Catalog>,
    pub disc_id: DiscIdStepView,
    pub barcode: BarcodeStepView,
    pub catalog: CatalogStepView,
    pub isrc: IsrcStepView,
    pub search: SearchStepView,
}

/// One candidate's identify state as a surface renders it. A settled state
/// carries the ledger its run recorded, or none when there was nothing to lay
/// out.
///
/// Its `groups` are every card the answers make, the rows agreement set aside
/// on their album's card beside the offered ones: the cards that offer a row
/// first, then the cards whose rows were all set aside. `narrowed_out_count`
/// is how many rows were set aside across every card — what a surface's
/// "more" disclosure counts.
#[derive(Debug, Clone, PartialEq)]
pub enum IdentifyStateView {
    Idle,

    /// Lookups in flight, with what the answers so far combine to.
    Triangulating {
        run: IdentifyRunView,
        groups: Vec<ReleaseGroup>,
        library_statuses: Vec<LibraryStatus>,
        agreements: Vec<(String, RowAgreements)>,
        narrowed_out_count: u32,
    },

    /// The matches, one card per release group.
    Found {
        run: Option<IdentifyRunView>,
        groups: Vec<ReleaseGroup>,
        /// One per release, offered or set aside.
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// Each row's badges, keyed by release id.
        agreements: Vec<(String, RowAgreements)>,
        narrowed_out_count: u32,
        /// The check against the folder the found release failed: why the
        /// verdict picks none of its releases.
        folder_check: Option<super::FolderCheck>,
        /// Whether the verdict picks its one release unattended.
        picks_unattended: bool,
        /// Whether the offered rows are several pressings of one album,
        /// which the folder can be linked to with its pressing unknown.
        offers_shared_album: bool,
    },

    NotFoundAnywhere {
        run: Option<IdentifyRunView>,
    },

    /// Nothing was looked up, so a surface offers manual search.
    ManualOnly {
        track_count: u32,
        run: Option<IdentifyRunView>,
    },

    /// bae broke on its own side and the run ended there, with why.
    Error {
        failure: crate::signals::InternalFailure,
    },

    /// A lookup failed, with whatever the lookups that answered combine to.
    Failed {
        run: Option<IdentifyRunView>,
        failures: Vec<super::IdentifyFailure>,
        groups: Vec<ReleaseGroup>,
        library_statuses: Vec<LibraryStatus>,
        agreements: Vec<(String, RowAgreements)>,
        narrowed_out_count: u32,
        /// Whether the offered rows are several pressings of one album, as
        /// for `Found`.
        offers_shared_album: bool,
    },
}

impl From<IdentifyState> for IdentifyStateView {
    fn from(state: IdentifyState) -> Self {
        match state {
            IdentifyState::Idle => IdentifyStateView::Idle,

            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc,
                search,
                context,
            } => {
                let (findings, library_statuses) =
                    live_findings(&discid, &barcode, &catalog, &isrc, &search, &context);
                let folded = fold(
                    findings,
                    library_statuses,
                    &context.text,
                    context.audio.track_count,
                );
                IdentifyStateView::Triangulating {
                    run: run_view(&discid, &barcode, &catalog, &isrc, &search, &context),
                    groups: folded.groups,
                    library_statuses: folded.library_statuses,
                    agreements: folded.agreements,
                    narrowed_out_count: folded.narrowed_out_count,
                }
            }

            IdentifyState::Found {
                findings,
                library_statuses,
                track_count,
                ledger,
                context,
            } => {
                let summary = super::VerdictSummary::of_found(&findings, track_count);
                let folded = fold(findings, library_statuses, &context.text, track_count);
                IdentifyStateView::Found {
                    run: ledger,
                    groups: folded.groups,
                    library_statuses: folded.library_statuses,
                    track_count,
                    agreements: folded.agreements,
                    narrowed_out_count: folded.narrowed_out_count,
                    folder_check: summary.folder_check(),
                    picks_unattended: summary.picks_unattended(),
                    offers_shared_album: folded.offers_shared_album,
                }
            }

            IdentifyState::NotFoundAnywhere { ledger, context: _ } => {
                IdentifyStateView::NotFoundAnywhere { run: ledger }
            }

            IdentifyState::ManualOnly {
                track_count,
                ledger,
                context: _,
            } => IdentifyStateView::ManualOnly {
                track_count,
                run: ledger,
            },

            IdentifyState::Error {
                failure,
                context: _,
            } => IdentifyStateView::Error { failure },

            IdentifyState::Failed {
                failures,
                findings,
                library_statuses,
                track_count,
                ledger,
                context,
            } => {
                let folded = fold(findings, library_statuses, &context.text, track_count);
                IdentifyStateView::Failed {
                    run: ledger,
                    failures,
                    groups: folded.groups,
                    library_statuses: folded.library_statuses,
                    agreements: folded.agreements,
                    narrowed_out_count: folded.narrowed_out_count,
                    offers_shared_album: folded.offers_shared_album,
                }
            }
        }
    }
}

/// What the answered lookups combine to so far, the same way the settle
/// combines them.
fn live_findings(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
    isrc: &IsrcProgress,
    search: &SearchProgress,
    context: &SignalsContext,
) -> (Findings, LibraryStatuses) {
    combine_results(
        LookupAnswers {
            disc_id: discid.results(),
            barcode: barcode.results(),
            catalog: catalog.results(),
            isrc: isrc.results(),
            search: search.results(),
            pressing: context
                .pressings
                .iter()
                .flat_map(super::state::PressingLookup::results)
                .collect(),
        },
        &context.text,
        context.folder_audio(),
    )
}

/// What the folder's text agrees with about one row: its badges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowAgreements {
    /// The fields of its pressing the text states.
    pub fields: Agreements,
    /// The note of its records the ranking's notes point went to it for —
    /// see [`Findings::named_notes`]. Never stated of a row set aside.
    pub notes: Option<String>,
}

/// A state's answers as cards, with every row's library status and badges.
struct Folded {
    groups: Vec<ReleaseGroup>,
    library_statuses: Vec<LibraryStatus>,
    agreements: Vec<(String, RowAgreements)>,
    narrowed_out_count: u32,
    /// Whether the offered rows are several pressings of one album.
    offers_shared_album: bool,
}

/// A state's answers as cards, and what the cards' badges are read from.
struct Cards {
    groups: Vec<ReleaseGroup>,
    judgements: Judgements,
    named_notes: Vec<super::combine::NamedNote>,
}

/// Judge the offered and set-aside releases against the candidate's text and
/// fold both into album cards as one grouping, so an album is one card either
/// way.
fn cards(findings: Findings, text: &CandidateText, track_count: u32) -> Cards {
    let facts = FolderFacts::of(text, findings.releases());
    // The medium conflict is stated by the folder check, not here.
    let Findings {
        matches,
        provenance,
        pressings,
        narrowed_out,
        medium_conflict: _,
        named_notes,
    } = findings;
    let offered = judged_results(matches, &provenance, text, &facts);
    let set_aside = judged_results(narrowed_out.matches, &narrowed_out.provenance, text, &facts);
    let judgements = Judgements::of(
        &offered
            .iter()
            .chain(&set_aside)
            .cloned()
            .collect::<Vec<_>>(),
        Some(track_count),
    );
    let groups = group_formed_rows(
        offered,
        &pressings,
        set_aside,
        &narrowed_out.pressings,
        Some(track_count),
    );
    Cards {
        groups,
        judgements,
        named_notes,
    }
}

/// The album a stored verdict's offered rows are several pressings of, as
/// the candidate's text ranks and cards them: what the pane offers to link
/// the folder to, its pressing unknown. `None` for a verdict that offers no
/// rows.
pub(crate) fn shared_album_of(
    verdict: super::TerminalVerdict,
    text: &CandidateText,
) -> Option<SharedAlbum> {
    match verdict {
        super::TerminalVerdict::Found {
            findings,
            track_count,
            ..
        }
        | super::TerminalVerdict::Failed {
            findings,
            track_count,
            ..
        } => SharedAlbum::of(&cards(findings, text, track_count).groups),
        super::TerminalVerdict::NotFoundAnywhere { .. }
        | super::TerminalVerdict::ManualOnly { .. }
        | super::TerminalVerdict::Error { .. } => None,
    }
}

/// A state's answers as cards, with every row's library status and badges.
/// Badges belong to a row, so every release id in a row answers with the
/// row's badges.
fn fold(
    findings: Findings,
    library_statuses: LibraryStatuses,
    text: &CandidateText,
    track_count: u32,
) -> Folded {
    let Cards {
        groups: cards,
        judgements,
        named_notes,
    } = cards(findings, text, track_count);
    let named_note = |pressing: &Pressing| {
        pressing.releases.iter().find_map(|release| {
            named_notes
                .iter()
                .find(|named| {
                    named.release.catalog == release.source
                        && named.release.key == release.release_id
                })
                .map(|named| named.note.clone())
        })
    };
    let agreements = cards
        .iter()
        .flat_map(|group| group.pressings().chain(group.narrowed_out()))
        .flat_map(|pressing| {
            let agreements = RowAgreements {
                fields: pressing.agreements(&judgements),
                notes: named_note(pressing),
            };
            pressing
                .releases
                .iter()
                .map(move |release| (release.release_id.clone(), agreements.clone()))
        })
        .collect();
    let narrowed_out_count = cards
        .iter()
        .map(|group| group.narrowed_out().count() as u32)
        .sum();
    Folded {
        offers_shared_album: SharedAlbum::of(&cards).is_some(),
        groups: cards,
        library_statuses: library_statuses
            .matches
            .into_iter()
            .chain(library_statuses.narrowed_out)
            .collect(),
        agreements,
        narrowed_out_count,
    }
}

mod ledger;
use ledger::{
    barcode_step, catalog_step, disc_id_step, identifiers_found_something, isrc_step, search_step,
};

/// The run as it stands, laid out as its ledger.
pub(super) fn run_view(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
    isrc: &IsrcProgress,
    search: &SearchProgress,
    context: &SignalsContext,
) -> IdentifyRunView {
    let scanning = matches!(context.artwork, ArtworkScan::Reading { .. });
    IdentifyRunView {
        providers: context.providers.clone(),
        disc_id: disc_id_step(discid, context),
        barcode: barcode_step(barcode, context, scanning),
        catalog: catalog_step(catalog, context, scanning),
        isrc: isrc_step(isrc, context),
        search: search_step(
            search,
            identifiers_found_something(discid, barcode, catalog),
            context,
        ),
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "view/catalog_row_tests.rs"]
mod catalog_row_tests;

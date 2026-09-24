//! The identify state, shaped for the surfaces that render it.
//!
//! [`IdentifyState`] is the reducer's working shape. It carries the whole
//! [`SignalsContext`] through every state so each landing answer re-combines
//! without re-fetching, and it keeps
//! `matches`, `library_statuses` and `provenance` as three index-aligned
//! vectors because that is what `combine` hands it.
//!
//! No surface wants that shape, and every surface wants the same *other*
//! shape: the matches folded into their release-group cards, ranked and
//! badged by how much of the candidate's own text agrees with them, the
//! catalog numbers that ranking read out of the text offered back as chips,
//! each result paired with its library status, the agreements keyed by
//! release id,
//! the run laid out as a ledger — one row per value extraction found, with
//! where it was found beside it and one cell per provider asked about it —
//! and the context's raw inputs left behind. Those are domain decisions, so they are made here, once, and a field
//! that must not cross is simply absent from the type.
//!
//! The ledger, [`IdentifyRunView`], is also what a run stores: the reducer
//! records it once as the run ends, and a settled state — live or stood back
//! up from its stored verdict — carries that recording rather than a rebuild.
//!
//! The transports (`bae-bridge`'s uniffi records, `bae-automation`'s JSON) mirror
//! this view into their own wire types field by field and decide nothing.

use super::agreements::{judged_results, Agreements, CandidateText};
use super::combine::{combine_results, CombineOutcome, LookupProvenance, NarrowedOut};
use super::state::{
    BarcodeLookupState, BarcodeProgress, CatalogLookup, CatalogProgress, DiscidProgress,
    IdentifyState, LookupResults, LookupState, SearchProgress, SignalsContext,
};
use crate::db::LibraryStatus;
use crate::import::release_group::{group_formed_rows, group_results, Judgements, ReleaseGroup};
use crate::import::search::MetadataResult;
use crate::import::Catalog;
use crate::signals::{ArtworkScan, DiscIdSignal, ImageRegion, LookupFailure, SignalOrigin};
use crate::util::text::squash;
use std::collections::HashSet;

/// How one provider's lookup of one value is going — one cell of the ledger.
///
/// `Serialize`/`Deserialize`, here and on everything else the ledger is made
/// of: a run records its ledger when it ends, and
/// [`super::TerminalVerdict`] persists it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LookupView {
    /// Not asked yet: the provider's walk through the codes has not reached
    /// this one.
    Queued,
    /// Never asked: the provider's walk ended at an earlier code, matched or
    /// failed, so this one was not needed.
    NotAsked,
    LookingUp,
    /// The lookup named releases: how many pressings, and the album cards
    /// they fold into, so a surface can show what the count stands for.
    Found {
        count: u32,
        groups: Vec<ReleaseGroup>,
    },
    NoMatch,
    Failed {
        failure: LookupFailure,
    },
}

/// One place a value was read: the origin, the file where the origin is one,
/// and where on that image where the detector said.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ValueSource {
    pub origin: SignalOrigin,
    /// The candidate-relative path of the file, where the origin is a file.
    pub file: Option<String>,
    pub region: Option<ImageRegion>,
}

/// One provider's cell of a value's row.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderCell {
    pub source: Catalog,
    pub lookup: LookupView,
}

/// One value extraction found, as a row of the ledger: where it was found,
/// and every provider's lookup of it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SignalValueRow {
    pub value: String,
    /// Every place the value was read, in the order it was read there.
    pub sources: Vec<ValueSource>,
    /// Whether the person left this value out of the run, so no provider was
    /// asked about it and every cell says as much. Always false for a catalog
    /// number: a row exists only for a number the run looks up.
    pub excluded: bool,
    /// One per provider in the run, in the run's provider order.
    pub cells: Vec<ProviderCell>,
}

/// Which kind of artifact a disc ID was read off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiscIdFileKind {
    Log,
    Cue,
}

/// The file a disc ID was read off.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiscIdFile {
    pub kind: DiscIdFileKind,
    /// The candidate-relative path.
    pub file: String,
}

/// The disc ID: read off a LOG or CUE, then looked up on
/// [`Catalog::DISC_ID_CATALOG`] — the one provider with a disc-ID
/// endpoint, so this step has one lookup and no cells. That one provider is
/// also why the step, alone among them, has to say when it was not asked: the
/// other steps say it by drawing no column for the source.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiscIdStepView {
    /// Extraction has not reported yet.
    Reading,
    /// No LOG or CUE to read one off.
    Absent,
    /// A LOG or CUE was there and no disc ID could be derived from it.
    ReadFailed { failure: LookupFailure },
    Read {
        disc_id: String,
        /// The file it came from. `None` for a release re-identified from its
        /// stored tracks.
        source: Option<DiscIdFile>,
        lookup: LookupView,
    },
    /// A disc ID was read and the source that answers disc IDs was not among
    /// the run's providers, so nothing looked it up. The value still stands —
    /// it is the folder's, not the run's — with nothing to say about what it
    /// matched, and there is nothing for a person to switch: the source to ask
    /// is switched on in Settings.
    ReadNotAsked {
        disc_id: String,
        source: Option<DiscIdFile>,
    },
    /// A disc ID was read and the person left it out of the run. The value
    /// stands with nothing looked up against it, and asking about it again is
    /// theirs to do.
    LeftOut {
        disc_id: String,
        source: Option<DiscIdFile>,
    },
}

/// The barcode: read off the artwork and the CUE sheets, then every provider
/// tries the codes in order on its own.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BarcodeStepView {
    /// No barcode source at all.
    Absent,
    /// There was a source and it held no code.
    NoCodes,
    /// Reading the candidate's barcodes failed, so no provider was asked.
    ScanFailed { failure: LookupFailure },
    /// One row per code the candidate carries, in the order they were first
    /// seen, each with every provider's lookup of it — a row the person left
    /// out says so and its cells were never asked. While the artwork is still
    /// being read, `scanning` says more rows may come and every cell is queued:
    /// the walks start once the codes have settled.
    Rows {
        scanning: bool,
        rows: Vec<SignalValueRow>,
    },
}

/// One catalog number extraction found and the run is not looking up: a
/// tile, offered for the person to activate.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CatalogCandidateView {
    pub value: String,
    pub sources: Vec<ValueSource>,
}

/// One catalog number the candidate's text states about a release it is
/// offering — a chip in the Catalog # row, and a control over what the text
/// is taken to state.
///
/// Not part of the run's ledger: which numbers these are follows from the
/// releases the run brought back and the text they are read against, so they
/// are derived on every read rather than recorded once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogAgreementView {
    pub value: String,
    /// Whether the person struck it out, so the releases carrying it earn no
    /// catalog agreement from the text. The chip stands either way: struck
    /// out is a state to come back from.
    pub discounted: bool,
}

/// The catalog number: the run looks up only the numbers the person picks
/// out of the ones extraction turned up, each on its own.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CatalogStepView {
    /// Extraction found no catalog number to offer, and is not still looking.
    NoneFound,
    Numbers {
        /// Whether the artwork is still being read, so more numbers may come.
        scanning: bool,
        /// The chosen numbers, in the order they were chosen, each with every
        /// provider's lookup of it.
        rows: Vec<SignalValueRow>,
        /// The numbers not chosen, in the order they were first seen.
        candidates: Vec<CatalogCandidateView>,
    },
}

/// The title search: the run's last step, asked of every provider at once
/// when the three identifiers named nothing between them.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SearchStepView {
    /// The identifiers answered; no search was needed.
    NotNeeded,
    /// Nothing to search by: the draft has no title.
    NoTitle,
    /// The identifiers are still being looked up, so whether these words are
    /// searched is not decided yet.
    Waiting { album: String, artist: String },
    /// The words that were searched, and every provider's lookup of them.
    Searched {
        album: String,
        /// Blank where the draft names no album artist; the title alone was
        /// searched.
        artist: String,
        /// One per provider in the run, in the run's provider order.
        cells: Vec<ProviderCell>,
    },
}

/// The run as a ledger: the three identifiers and the title search behind
/// them, each carrying what extraction produced for it and every provider's
/// lookup of it, so a surface lists the run row by row and each cell settles
/// on its own.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IdentifyRunView {
    /// The providers the run asks, in the order their cells are listed. Named
    /// up front so a surface can draw the columns before any row exists.
    pub providers: Vec<Catalog>,
    pub disc_id: DiscIdStepView,
    pub barcode: BarcodeStepView,
    pub catalog: CatalogStepView,
    pub search: SearchStepView,
}

/// The rows agreement left out, as a surface offers them behind its "more"
/// disclosure. The matches and these are grouped as one list, so an album is
/// one card whichever side of the disclosure its rows are on: a card the
/// matches are on carries its own rows set aside as `narrowed_out`, and only
/// an album none of whose rows is offered is a card here. Their library
/// statuses and badges are the state's own, beside the matches'. Empty when
/// nothing was narrowed — one signal answering alone, signals that shared
/// nothing, and a candidate whose text stands behind none of the answers,
/// which is offered whole rather than emptied.
#[derive(Debug, Clone, Default)]
pub struct NarrowedOutView {
    /// The cards none of whose rows is offered.
    pub groups: Vec<ReleaseGroup>,
    /// How many rows agreement set aside, on every card: the matches' and
    /// these.
    pub count: u32,
}

impl NarrowedOutView {
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// One candidate's identify state as a surface renders it.
///
/// A settled state carries the ledger its run recorded when it ended, so what
/// the pane shows afterwards is the last frame the run showed. It carries none
/// when extraction handed the run nothing to lay out — a folder with no disc
/// ID, no barcode source and no catalog number — and for a verdict whose
/// stored row records none.
#[derive(Debug, Clone)]
pub enum IdentifyStateView {
    Idle,

    /// Lookups in flight, laid out as the ledger, with the matches every
    /// answered lookup has combined to so far — the same combine the settle
    /// runs, so what a person sees mid-run is what the verdict lands on, and a
    /// row that has landed does not jump at settle.
    Triangulating {
        run: IdentifyRunView,
        groups: Vec<ReleaseGroup>,
        library_statuses: Vec<LibraryStatus>,
        agreements: Vec<(String, Agreements)>,
        /// What the lookups answered so far that the agreement so far leaves
        /// out — the same list the settled state lands on, as it stands.
        narrowed_out: NarrowedOutView,
    },

    /// The matches, bucketed into their release groups — one card per group,
    /// with its pressings beneath. Usually one card; signals that named
    /// different releases give several, which is the same list of things to
    /// pick from either way.
    Found {
        run: Option<IdentifyRunView>,
        /// The match list, folded into group cards in match order — each card
        /// with its own rows agreement set aside beside the offered ones.
        groups: Vec<ReleaseGroup>,
        /// One per pressing, offered or set aside; each carries its own
        /// `release_id`.
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// What the candidate's own text agrees with about each pressing,
        /// offered or set aside, keyed by release id — the row's badges, and
        /// what ordered the rows.
        /// It is derived per result, and the results are now inside the group
        /// cards, so the alignment is re-expressed as a key here rather than
        /// left for a surface to reconstruct.
        agreements: Vec<(String, Agreements)>,
        /// The releases agreement left out, for the surface to offer behind a
        /// disclosure.
        narrowed_out: NarrowedOutView,
        /// The catalog numbers the folder states about the offered releases,
        /// as the Catalog # row's chips.
        catalog_agreements: Vec<CatalogAgreementView>,
    },

    NotFoundAnywhere {
        run: Option<IdentifyRunView>,
    },

    /// No disc-ID artifact and no barcode source: nothing ran, so a surface
    /// offers manual search rather than claiming it looked and found nothing.
    /// The run is there when extraction found catalog numbers the person can
    /// still activate.
    ManualOnly {
        track_count: u32,
        run: Option<IdentifyRunView>,
    },

    /// A lookup failed, with whatever the surviving evidence still combined
    /// to. `groups` is folded exactly as `Found`'s is, so a surface renders one
    /// result area either way and names the failures beside it. It is empty
    /// when nothing answered, and for a failure resumed from its stored
    /// verdict.
    Failed {
        run: Option<IdentifyRunView>,
        failures: Vec<super::IdentifyFailure>,
        groups: Vec<ReleaseGroup>,
        library_statuses: Vec<LibraryStatus>,
        agreements: Vec<(String, Agreements)>,
        narrowed_out: NarrowedOutView,
        /// As `Found`'s: the numbers the folder states about whatever the
        /// surviving evidence still offers.
        catalog_agreements: Vec<CatalogAgreementView>,
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
                search,
                context,
            } => {
                let (matches, library_statuses, provenance, pressings, narrowed_out) =
                    live_matches(&discid, &barcode, &catalog, &search, &context);
                let folded = fold(
                    matches,
                    library_statuses,
                    provenance,
                    &pressings,
                    narrowed_out,
                    &context.text,
                );
                IdentifyStateView::Triangulating {
                    run: run_view(&discid, &barcode, &catalog, &search, &context),
                    groups: folded.groups,
                    library_statuses: folded.library_statuses,
                    agreements: folded.agreements,
                    narrowed_out: folded.narrowed_out,
                }
            }

            IdentifyState::Found {
                matches,
                library_statuses,
                track_count,
                provenance,
                pressings,
                narrowed_out,
                ledger,
                context,
            } => {
                let catalog_agreements = catalog_agreements(&matches, &provenance, &context.text);
                let folded = fold(
                    matches,
                    library_statuses,
                    provenance,
                    &pressings,
                    narrowed_out,
                    &context.text,
                );
                IdentifyStateView::Found {
                    run: ledger.map(|run| without_chip_tiles(run, &catalog_agreements)),
                    groups: folded.groups,
                    library_statuses: folded.library_statuses,
                    track_count,
                    agreements: folded.agreements,
                    narrowed_out: folded.narrowed_out,
                    catalog_agreements,
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

            IdentifyState::Failed {
                failures,
                matches,
                library_statuses,
                provenance,
                pressings,
                narrowed_out,
                track_count: _,
                ledger,
                context,
            } => {
                let catalog_agreements = catalog_agreements(&matches, &provenance, &context.text);
                let folded = fold(
                    matches,
                    library_statuses,
                    provenance,
                    &pressings,
                    narrowed_out,
                    &context.text,
                );
                IdentifyStateView::Failed {
                    run: ledger.map(|run| without_chip_tiles(run, &catalog_agreements)),
                    failures,
                    groups: folded.groups,
                    library_statuses: folded.library_statuses,
                    agreements: folded.agreements,
                    narrowed_out: folded.narrowed_out,
                    catalog_agreements,
                }
            }
        }
    }
}

/// What the answered lookups combine to so far. Each signal contributes what
/// its providers have returned — a provider still looking adds nothing yet —
/// and a disc ID the run was told to leave out adds nothing at all, exactly as
/// the settle treats it. A lookup that has not answered leaves its signal
/// empty, which combine reads as taking no part, so the first answer shows on
/// its own and later ones narrow or widen it the way the verdict will.
fn live_matches(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
    search: &SearchProgress,
    context: &SignalsContext,
) -> (
    Vec<MetadataResult>,
    Vec<LibraryStatus>,
    Vec<LookupProvenance>,
    Vec<u32>,
    NarrowedOut,
) {
    let outcome = combine_results(
        discid.results(),
        barcode.results(),
        catalog.results(),
        search.results(),
        &context.text,
    );
    match outcome {
        CombineOutcome::Found {
            matches,
            library_statuses,
            provenance,
            pressings,
            narrowed_out,
        } => (
            matches,
            library_statuses,
            provenance,
            pressings,
            narrowed_out,
        ),
        CombineOutcome::NotFoundAnywhere => (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            NarrowedOut::default(),
        ),
    }
}

/// A state's answers as a surface lists them: its cards, and the library
/// status and badges of every row on them.
struct Folded {
    groups: Vec<ReleaseGroup>,
    library_statuses: Vec<LibraryStatus>,
    agreements: Vec<(String, Agreements)>,
    narrowed_out: NarrowedOutView,
}

/// Judge the matches and the releases agreement set aside against the
/// candidate's own text, fold both lists into album cards as one — which is
/// also what orders the rows — and key the agreements by release id.
///
/// One grouping, so an album is one card whichever list its rows are on: a
/// card the matches are on carries its rows set aside beside the offered
/// ones, and a card none of whose rows is offered goes behind the disclosure.
///
/// The badges are the row's, not the release's: a row is one physical object
/// picked whole, so what the two sources' records of it agree with is one set
/// of badges, and both release ids answer with it. Judging is per release
/// because the fields being looked for are, and once the releases are inside
/// the cards that alignment is no longer expressible.
///
/// This is the one place a stored verdict's rows are judged. The rows are the
/// ones the run built — `pressings` says which row each release is in — and a
/// run's own rows were judged by `combine` against this same text, so a row
/// does not change what it says, or which records it holds, between the run
/// and the read.
fn fold(
    matches: Vec<MetadataResult>,
    library_statuses: Vec<LibraryStatus>,
    provenance: Vec<LookupProvenance>,
    pressings: &[u32],
    narrowed_out: NarrowedOut,
    text: &CandidateText,
) -> Folded {
    let offered = judged_results(matches, &provenance, text);
    let set_aside = judged_results(narrowed_out.matches, &narrowed_out.provenance, text);
    let judgements = Judgements::of(
        &offered
            .iter()
            .chain(&set_aside)
            .cloned()
            .collect::<Vec<_>>(),
    );
    let cards = group_formed_rows(offered, pressings, set_aside, &narrowed_out.pressings);
    let agreements = cards
        .iter()
        .flat_map(|group| group.pressings().chain(group.narrowed_out()))
        .flat_map(|pressing| {
            let agreements = pressing.agreements(&judgements);
            pressing
                .releases
                .iter()
                .map(move |release| (release.release_id.clone(), agreements))
        })
        .collect();
    let count = cards
        .iter()
        .map(|group| group.narrowed_out().count() as u32)
        .sum();
    let (groups, set_aside_cards): (Vec<ReleaseGroup>, Vec<ReleaseGroup>) = cards
        .into_iter()
        .partition(|group| group.pressings().next().is_some());
    Folded {
        groups,
        library_statuses: library_statuses
            .into_iter()
            .chain(narrowed_out.library_statuses)
            .collect(),
        agreements,
        narrowed_out: NarrowedOutView {
            groups: set_aside_cards,
            count,
        },
    }
}

/// The catalog numbers the candidate's text states about the releases it is
/// offering — the chips in the Catalog # row.
///
/// A number is one of these when an offered release carries it and the text
/// prints it: those are exactly the numbers whose striking out changes what
/// the list says. A release the catalog lookup itself brought back states its
/// own number, so it contributes none: striking that number out would leave
/// its agreement standing, and the chip would say it had done something it
/// had not. Neither does a release the folder said nothing about and the run
/// set aside — what is behind the disclosure ranks nothing.
///
/// A struck-out number is still one of these. It is what the person comes
/// back to, checked off, when they want it counted again.
fn catalog_agreements(
    matches: &[MetadataResult],
    provenance: &[LookupProvenance],
    text: &CandidateText,
) -> Vec<CatalogAgreementView> {
    let mut seen: HashSet<String> = HashSet::new();
    matches
        .iter()
        .zip(provenance)
        .filter(|(_, lookup)| !lookup.by_catalog)
        .filter_map(|(result, _)| result.catalog_number.as_deref())
        .filter(|value| text.states(value) && seen.insert(squash(value)))
        .map(|value| CatalogAgreementView {
            discounted: text.is_struck_out(value),
            value: value.to_string(),
        })
        .collect()
}

/// The recorded ledger as a settled pane draws it. A number one of the
/// offered releases carries is a chip that ranks them, so it is not also a
/// tile that would look it up: the tiles left under the table are the numbers
/// nothing came back carrying.
fn without_chip_tiles(mut run: IdentifyRunView, chips: &[CatalogAgreementView]) -> IdentifyRunView {
    if let CatalogStepView::Numbers { candidates, .. } = &mut run.catalog {
        let chipped: HashSet<String> = chips.iter().map(|chip| squash(&chip.value)).collect();
        candidates.retain(|tile| !chipped.contains(&squash(&tile.value)));
    }
    run
}

mod ledger;
use ledger::{barcode_step, catalog_step, disc_id_step, identifiers_found_something, search_step};

/// The run as it stands: the three pipes laid out against the inputs and the
/// providers the run asks. The reducer records this when the run ends, and
/// what it recorded is what every later reader shows.
pub(super) fn run_view(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
    search: &SearchProgress,
    context: &SignalsContext,
) -> IdentifyRunView {
    let scanning = matches!(context.artwork, ArtworkScan::Reading { .. });
    IdentifyRunView {
        providers: context.providers.clone(),
        disc_id: disc_id_step(discid, context),
        barcode: barcode_step(barcode, context, scanning),
        catalog: catalog_step(catalog, context, scanning),
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
#[path = "view/catalog_chip_tests.rs"]
mod catalog_chip_tests;

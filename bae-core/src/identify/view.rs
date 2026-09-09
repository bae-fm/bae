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

use super::agreements::{judged_results, squash, Agreements, CandidateText};
use super::combine::{combine_results, CombineOutcome, LookupProvenance, NarrowedOut};
use super::state::{
    BarcodeLookupState, BarcodeProgress, CatalogLookup, CatalogProgress, DiscidProgress,
    IdentifyState, LookupResults, LookupState, SignalsContext,
};
use crate::db::LibraryStatus;
use crate::import::release_group::{group_results, Judgements, ReleaseGroup};
use crate::import::search::MetadataResult;
use crate::import::MetadataSource;
use crate::signals::{ArtworkScan, DiscIdSignal, ImageRegion, LookupFailure, SignalOrigin};
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
    pub source: MetadataSource,
    pub lookup: LookupView,
}

/// One value extraction found, as a row of the ledger: where it was found,
/// and every provider's lookup of it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SignalValueRow {
    pub value: String,
    /// Every place the value was read, in the order it was read there.
    pub sources: Vec<ValueSource>,
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
/// [`MetadataSource::DISC_ID_SOURCE`] — the one provider with a disc-ID
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
    /// A disc ID was read and the source that answers disc IDs was not asked,
    /// so nothing looked it up. The value still stands — it is the folder's,
    /// not the run's — with nothing to say about what it matched.
    ReadNotAsked {
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
    /// One row per code, each with every provider's lookup of it. While the
    /// artwork is still being read, `scanning` says more rows may come and
    /// every cell is queued: the walks start once the codes have settled.
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

/// The run as a ledger: the three signals, each carrying what extraction
/// produced for it and every provider's lookup of it, so a surface lists the
/// run row by row and each cell settles on its own.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IdentifyRunView {
    /// The providers the run asks, in the order their cells are listed. Named
    /// up front so a surface can draw the columns before any row exists.
    pub providers: Vec<MetadataSource>,
    pub disc_id: DiscIdStepView,
    pub barcode: BarcodeStepView,
    pub catalog: CatalogStepView,
}

/// The releases agreement left out, as a surface lists them: folded into album
/// cards like the matches, with the same per-pressing library statuses and
/// badges. Empty when nothing was narrowed — one signal answering alone,
/// signals that shared nothing, and a candidate whose text stands behind none
/// of the answers, which is offered whole rather than emptied.
#[derive(Debug, Clone, Default)]
pub struct NarrowedOutView {
    pub groups: Vec<ReleaseGroup>,
    /// One per pressing; each carries its own `release_id`.
    pub library_statuses: Vec<LibraryStatus>,
    /// Per-pressing agreements, keyed by release id, as `Found`'s are.
    pub agreements: Vec<(String, Agreements)>,
}

impl NarrowedOutView {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
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
        /// The match list, folded into group cards in match order.
        groups: Vec<ReleaseGroup>,
        /// One per pressing; each carries its own `release_id`.
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// What the candidate's own text agrees with about each pressing,
        /// keyed by release id — the row's badges, and what ordered the rows.
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
                context,
            } => {
                let (matches, library_statuses, provenance, narrowed_out) =
                    live_matches(&discid, &barcode, &catalog, &context);
                let (groups, agreements) = fold_matches(matches, provenance, &context.text);
                IdentifyStateView::Triangulating {
                    run: run_view(&discid, &barcode, &catalog, &context),
                    groups,
                    library_statuses,
                    agreements,
                    narrowed_out: fold_narrowed_out(narrowed_out, &context.text),
                }
            }

            IdentifyState::Found {
                matches,
                library_statuses,
                track_count,
                provenance,
                narrowed_out,
                ledger,
                context,
            } => {
                let catalog_agreements = catalog_agreements(&matches, &provenance, &context.text);
                let (groups, agreements) = fold_matches(matches, provenance, &context.text);
                IdentifyStateView::Found {
                    run: ledger.map(|run| without_chip_tiles(run, &catalog_agreements)),
                    groups,
                    library_statuses,
                    track_count,
                    agreements,
                    narrowed_out: fold_narrowed_out(narrowed_out, &context.text),
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
                narrowed_out,
                track_count: _,
                ledger,
                context,
            } => {
                let catalog_agreements = catalog_agreements(&matches, &provenance, &context.text);
                let (groups, agreements) = fold_matches(matches, provenance, &context.text);
                IdentifyStateView::Failed {
                    run: ledger.map(|run| without_chip_tiles(run, &catalog_agreements)),
                    failures,
                    groups,
                    library_statuses,
                    agreements,
                    narrowed_out: fold_narrowed_out(narrowed_out, &context.text),
                    catalog_agreements,
                }
            }
        }
    }
}

/// What the answered lookups combine to so far. Each signal contributes what
/// its providers have returned — a provider still looking adds nothing yet —
/// and a signal the user unchecked adds nothing at all, exactly as the settle
/// treats it. A lookup that has not answered leaves its signal empty, which
/// combine reads as taking no part, so the first answer shows on its own and
/// later ones narrow or widen it the way the verdict will.
fn live_matches(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
    context: &SignalsContext,
) -> (
    Vec<MetadataResult>,
    Vec<LibraryStatus>,
    Vec<LookupProvenance>,
    NarrowedOut,
) {
    let outcome = combine_results(
        context.disc.active(discid.results()),
        context.barcode.active(barcode.results()),
        context.catalog.active(catalog.results()),
        &context.text,
    );
    match outcome {
        CombineOutcome::Found {
            matches,
            library_statuses,
            provenance,
            narrowed_out,
        } => (matches, library_statuses, provenance, narrowed_out),
        CombineOutcome::NotFoundAnywhere => {
            (Vec::new(), Vec::new(), Vec::new(), NarrowedOut::default())
        }
    }
}

/// Judge each match against the candidate's own text, fold the list into its
/// group cards — which is also what orders the rows — and key the agreements
/// by release id.
///
/// The badges are the row's, not the release's: a row is one physical object
/// picked whole, so what the two sources' records of it agree with is one set
/// of badges, and both release ids answer with it. Judging is per release
/// because the fields being looked for are, and once the releases are inside
/// the cards that alignment is no longer expressible.
///
/// This is the one place a stored verdict's rows are judged. A run's own rows
/// were judged by `combine`, against this same text, so a row does not change
/// what it says between the run and the read.
fn fold_matches(
    matches: Vec<MetadataResult>,
    provenance: Vec<LookupProvenance>,
    text: &CandidateText,
) -> (Vec<ReleaseGroup>, Vec<(String, Agreements)>) {
    let judged = judged_results(matches, &provenance, text);
    let judgements = Judgements::of(&judged);
    let groups = group_results(judged);
    let keyed = groups
        .iter()
        .flat_map(|group| &group.pressings)
        .flat_map(|pressing| {
            let agreements = pressing.agreements(&judgements);
            pressing
                .releases
                .iter()
                .map(move |release| (release.release_id.clone(), agreements))
        })
        .collect();
    (groups, keyed)
}

/// The narrowed-out releases, folded into their album cards the way the
/// matches are, so a surface lists both the same way.
fn fold_narrowed_out(narrowed_out: NarrowedOut, text: &CandidateText) -> NarrowedOutView {
    let NarrowedOut {
        matches,
        library_statuses,
        provenance,
    } = narrowed_out;
    let (groups, agreements) = fold_matches(matches, provenance, text);
    NarrowedOutView {
        groups,
        library_statuses,
        agreements,
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

/// The run as it stands: the three pipes laid out against the inputs and the
/// providers the run asks. The reducer records this when the run ends, and
/// what it recorded is what every later reader shows.
pub(super) fn run_view(
    discid: &DiscidProgress,
    barcode: &BarcodeProgress,
    catalog: &CatalogProgress,
    context: &SignalsContext,
) -> IdentifyRunView {
    let scanning = matches!(context.artwork, ArtworkScan::Reading { .. });
    IdentifyRunView {
        providers: context.providers.clone(),
        disc_id: disc_id_step(discid, context),
        barcode: barcode_step(barcode, context, scanning),
        catalog: catalog_step(catalog, context, scanning),
    }
}

/// The disc-ID step: what extraction read, from the context, and how far
/// MusicBrainz's lookup of it has got, from the pipe.
fn disc_id_step(progress: &DiscidProgress, context: &SignalsContext) -> DiscIdStepView {
    let (disc_id, source_file) = match &context.disc.signal {
        DiscIdSignal::Computed {
            disc_id,
            source_file,
            ..
        } => (disc_id.clone(), source_file.clone()),
        DiscIdSignal::Absent { .. } => {
            return match progress {
                DiscidProgress::Computing => DiscIdStepView::Reading,
                _ => DiscIdStepView::Absent,
            }
        }
        DiscIdSignal::Failed { failure, .. } => {
            return DiscIdStepView::ReadFailed {
                failure: failure.clone(),
            }
        }
    };
    if let DiscidProgress::NotAsked { .. } = progress {
        return DiscIdStepView::ReadNotAsked {
            disc_id,
            source: source_file.map(disc_id_file),
        };
    }
    let lookup = match progress {
        // The track count is a settled-state concern — it reaches a surface
        // through the terminal state, not through progress.
        DiscidProgress::Computing | DiscidProgress::LookingUp => LookupView::LookingUp,
        DiscidProgress::Done { results, .. } => found_or_no_match(results),
        DiscidProgress::Skipped { .. } | DiscidProgress::NotAsked { .. } => {
            unreachable!("a computed disc ID is skipped only by the early return above")
        }
        DiscidProgress::Failed { failure, .. } => LookupView::Failed {
            failure: failure.clone(),
        },
    };
    DiscIdStepView::Read {
        disc_id,
        source: source_file.map(disc_id_file),
        lookup,
    }
}

/// The file a disc ID was read off, by the kind of artifact it is. A disc ID
/// is derived from a rip log or a cue sheet and nothing else, so a file that
/// is not a log is a sheet.
fn disc_id_file(file: String) -> DiscIdFile {
    let is_log = std::path::Path::new(&file)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("log"));
    DiscIdFile {
        kind: if is_log {
            DiscIdFileKind::Log
        } else {
            DiscIdFileKind::Cue
        },
        file,
    }
}

fn barcode_step(
    progress: &BarcodeProgress,
    context: &SignalsContext,
    scanning: bool,
) -> BarcodeStepView {
    match progress {
        // The walks start once the codes settle: every code read so far is a
        // row whose cells wait, and more rows may still come.
        BarcodeProgress::Scanning => BarcodeStepView::Rows {
            scanning: true,
            rows: context
                .barcode
                .code_values()
                .into_iter()
                .map(|code| SignalValueRow {
                    sources: sources_of(&context.barcode.codes, &code),
                    cells: context
                        .providers
                        .iter()
                        .map(|&source| ProviderCell {
                            source,
                            lookup: LookupView::Queued,
                        })
                        .collect(),
                    value: code,
                })
                .collect(),
        },
        BarcodeProgress::NoCodes => BarcodeStepView::NoCodes,
        BarcodeProgress::ScanFailed { failure } => BarcodeStepView::ScanFailed {
            failure: failure.clone(),
        },
        BarcodeProgress::Skipped => BarcodeStepView::Absent,
        // The codes are the run's and nobody was asked about them: every row
        // stands with its cells saying so, rather than reading as a lookup
        // that found nothing.
        BarcodeProgress::NotAsked { codes } => BarcodeStepView::Rows {
            scanning: false,
            rows: codes
                .iter()
                .map(|code| SignalValueRow {
                    sources: sources_of(&context.barcode.codes, code),
                    cells: context
                        .providers
                        .iter()
                        .map(|&source| ProviderCell {
                            source,
                            lookup: LookupView::NotAsked,
                        })
                        .collect(),
                    value: code.clone(),
                })
                .collect(),
        },
        BarcodeProgress::Lookups { codes, providers } => BarcodeStepView::Rows {
            scanning,
            rows: codes
                .iter()
                .enumerate()
                .map(|(index, code)| SignalValueRow {
                    value: code.clone(),
                    sources: sources_of(&context.barcode.codes, code),
                    cells: providers
                        .iter()
                        .map(|provider| ProviderCell {
                            source: provider.source,
                            lookup: barcode_cell(&provider.state, index, codes),
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}

/// One provider's cell for the code at `index`, from where its walk is. A
/// walk asks the codes in order and stops at the first match or failure, so
/// where it is says what it did with every code: the ones before it missed,
/// the one it is on it is asking about, and the ones after wait — or, once it
/// has stopped, were never needed.
fn barcode_cell(walk: &BarcodeLookupState, index: usize, codes: &[String]) -> LookupView {
    match walk {
        BarcodeLookupState::Trying { index: at } => match index.cmp(at) {
            std::cmp::Ordering::Less => LookupView::NoMatch,
            std::cmp::Ordering::Equal => LookupView::LookingUp,
            std::cmp::Ordering::Greater => LookupView::Queued,
        },
        BarcodeLookupState::Matched { code, results } => {
            let at = codes
                .iter()
                .position(|c| c == code)
                .expect("a walk matches one of the codes it asks");
            match index.cmp(&at) {
                std::cmp::Ordering::Less => LookupView::NoMatch,
                std::cmp::Ordering::Equal => found_or_no_match(results),
                std::cmp::Ordering::Greater => LookupView::NotAsked,
            }
        }
        BarcodeLookupState::Exhausted => LookupView::NoMatch,
        BarcodeLookupState::Failed { failure, index: at } => match index.cmp(at) {
            std::cmp::Ordering::Less => LookupView::NoMatch,
            std::cmp::Ordering::Equal => LookupView::Failed {
                failure: failure.clone(),
            },
            std::cmp::Ordering::Greater => LookupView::NotAsked,
        },
    }
}

fn catalog_step(
    progress: &CatalogProgress,
    context: &SignalsContext,
    scanning: bool,
) -> CatalogStepView {
    let numbers = context.catalog.number_values();
    if numbers.is_empty() && !scanning {
        return CatalogStepView::NoneFound;
    }
    let rows = progress
        .lookups()
        .iter()
        .map(|lookup| catalog_row(lookup, context))
        .collect();
    let candidates = numbers
        .into_iter()
        .filter(|value| !context.catalog.is_chosen(value))
        .map(|value| CatalogCandidateView {
            sources: sources_of(&context.catalog.numbers, &value),
            value,
        })
        .collect();
    CatalogStepView::Numbers {
        scanning,
        rows,
        candidates,
    }
}

fn catalog_row(lookup: &CatalogLookup, context: &SignalsContext) -> SignalValueRow {
    SignalValueRow {
        value: lookup.value.clone(),
        sources: sources_of(&context.catalog.numbers, &lookup.value),
        cells: lookup
            .providers
            .iter()
            .map(|provider| ProviderCell {
                source: provider.source,
                lookup: match &provider.state {
                    LookupState::LookingUp => LookupView::LookingUp,
                    LookupState::Done { results } => found_or_no_match(results),
                    LookupState::Failed { failure } => LookupView::Failed {
                        failure: failure.clone(),
                    },
                },
            })
            .collect(),
    }
}

/// Every place `value` was read, in the order it was read there.
fn sources_of(sightings: &[crate::signals::SourcedValue], value: &str) -> Vec<ValueSource> {
    sightings
        .iter()
        .filter(|sighting| sighting.value == value)
        .map(|sighting| ValueSource {
            origin: sighting.origin,
            file: sighting.origin_path.clone(),
            region: sighting.region,
        })
        .collect()
}

/// What a settled lookup turned up: its releases folded into album cards, or
/// nothing. One cell of the ledger — what this lookup alone saw, before
/// anything else narrowed it — so the rows are the lookup's own order.
fn found_or_no_match(results: &LookupResults) -> LookupView {
    if results.is_empty() {
        return LookupView::NoMatch;
    }
    let groups = group_results(crate::import::release_group::unranked(
        results.iter().map(|(result, _)| result.clone()).collect(),
    ));
    LookupView::Found {
        count: groups
            .iter()
            .map(|group| group.pressings.len() as u32)
            .sum(),
        groups,
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;

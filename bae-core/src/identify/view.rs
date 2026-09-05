//! The identify state, shaped for the surfaces that render it.
//!
//! [`IdentifyState`] is the reducer's working shape. It carries the whole
//! [`SignalsContext`](super::state::SignalsContext) through every state so a
//! toggle or a re-run can re-combine without re-fetching, and it keeps
//! `matches`, `library_statuses` and `provenance` as three index-aligned
//! vectors because that is what `combine` hands it.
//!
//! No surface wants that shape, and every surface wants the same *other* shape:
//! the matches folded into their release-group cards, each result paired with its
//! library status, provenance keyed by release id, a run in flight laid out as
//! the steps it is taking with each provider's part of each, and the context's
//! raw inputs — the user's exclusions — left behind. Those are domain
//! decisions, so they are made here, once, and a field that must not cross is
//! simply absent from the type.
//!
//! The transports (`bae-bridge`'s uniffi records, `bae-automation`'s JSON) mirror
//! this view into their own wire types field by field and decide nothing.

use super::combine::ResultProvenance;
use super::state::{
    BarcodeLookupState, BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState,
    LookupState, SignalsContext,
};
use crate::db::LibraryStatus;
use crate::import::release_group::ReleaseGroup;
use crate::import::MetadataSource;
use crate::signals::{DiscIdSignal, LookupFailure};

/// How one provider's lookup of one value is going. The results themselves are
/// not here: mid-flight, a surface shows only how many came back — the set
/// itself surfaces from the terminal state.
#[derive(Debug, Clone, PartialEq)]
pub enum LookupView {
    LookingUp,
    Found { count: u32 },
    NoMatch,
    Failed { failure: LookupFailure },
}

/// The disc ID: read off a LOG or CUE, then looked up on MusicBrainz — the one
/// provider with a disc-ID endpoint, so this step has one lookup and no list.
#[derive(Debug, Clone, PartialEq)]
pub enum DiscIdStepView {
    /// Extraction has not reported yet.
    Reading,
    /// No LOG or CUE to read one off.
    Absent,
    /// A LOG or CUE was there and no disc ID could be derived from it.
    ReadFailed { failure: LookupFailure },
    Read {
        disc_id: String,
        /// The candidate-relative path of the file it came from. `None` for a
        /// release re-identified from its stored tracks.
        source_file: Option<String>,
        lookup: LookupView,
    },
}

/// One provider's walk through the candidate's barcodes.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderBarcodeLookupView {
    pub source: MetadataSource,
    pub state: BarcodeLookupView,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BarcodeLookupView {
    /// Asking about `barcode`, the `position`th of `total`.
    Trying {
        barcode: String,
        position: u32,
        total: u32,
    },
    /// A code matched. `None` when the provider's answer was stood back up
    /// from a settled run, which keeps what it found but not which code
    /// found it.
    Matched {
        barcode: Option<String>,
        count: u32,
    },
    /// Every code tried, none matched.
    Exhausted,
    Failed {
        failure: LookupFailure,
    },
}

/// The barcode: read off the artwork (or a CUE `CATALOG` field), then every
/// provider tries the codes in order on its own.
#[derive(Debug, Clone, PartialEq)]
pub enum BarcodeStepView {
    /// The artwork is still being read; the lookups start once it has been.
    AwaitingArtwork,
    /// No barcode source at all.
    Absent,
    /// There was a source and it held no code.
    NoCodes,
    /// Reading the candidate's barcodes failed, so no provider was asked.
    ScanFailed { failure: LookupFailure },
    Lookups {
        codes: Vec<String>,
        providers: Vec<ProviderBarcodeLookupView>,
    },
}

/// One provider's part of the chosen catalog number's lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderLookupView {
    pub source: MetadataSource,
    pub state: LookupView,
}

/// The catalog number: the run looks one up only once the user picks it out
/// of the numbers extraction turned up.
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogStepView {
    /// Extraction found no catalog number to offer.
    NoneFound,
    /// Numbers were found and none is chosen yet: the step waits on a pick.
    Unchosen { available: u32 },
    Chosen {
        value: String,
        lookups: Vec<ProviderLookupView>,
    },
}

/// A run in flight, as the steps it is taking. One entry per signal, each
/// carrying what extraction produced for it and every provider's lookup of it,
/// so a surface lists the run row by row and each row settles on its own.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentifyRunView {
    pub disc_id: DiscIdStepView,
    pub barcode: BarcodeStepView,
    pub catalog: CatalogStepView,
}

/// One candidate's identify state as a surface renders it.
#[derive(Debug, Clone)]
pub enum IdentifyStateView {
    Idle,

    /// Lookups in flight, laid out as the steps the run is taking.
    Triangulating {
        run: IdentifyRunView,
    },

    /// The matches, bucketed into their release groups — one card per group,
    /// with its pressings beneath. Usually one card; signals that named
    /// different releases give several, which is the same list of things to
    /// pick from either way.
    Found {
        /// The match list, folded into group cards in match order.
        groups: Vec<ReleaseGroup>,
        /// One per pressing; each carries its own `release_id`.
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// Per-pressing provenance, keyed by release id. `combine` produces it
        /// index-aligned with the matches, and the matches are now inside the
        /// group cards, so the alignment is re-expressed as a key here rather
        /// than left for a surface to reconstruct.
        provenance: Vec<(String, ResultProvenance)>,
    },

    NotFoundAnywhere,

    /// No disc-ID artifact and no barcode source: nothing ran, so a surface
    /// offers manual search rather than claiming it looked and found nothing.
    ManualOnly {
        track_count: u32,
    },

    /// A lookup failed, with whatever the surviving evidence still combined
    /// to. `groups` is folded exactly as `Found`'s is, so a surface renders one
    /// result area either way and names the failures beside it. It is empty
    /// when nothing answered, and for a failure resumed from its stored
    /// verdict.
    Failed {
        failures: Vec<super::IdentifyFailure>,
        groups: Vec<ReleaseGroup>,
        library_statuses: Vec<LibraryStatus>,
        provenance: Vec<(String, ResultProvenance)>,
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
            } => IdentifyStateView::Triangulating {
                run: IdentifyRunView {
                    disc_id: disc_id_step(discid, &context),
                    barcode: barcode_step(barcode),
                    catalog: catalog_step(catalog, &context),
                },
            },

            IdentifyState::Found {
                matches,
                library_statuses,
                track_count,
                provenance,
                context: _,
            } => {
                let (groups, provenance) = fold_matches(matches, provenance);
                IdentifyStateView::Found {
                    groups,
                    library_statuses,
                    track_count,
                    provenance,
                }
            }

            IdentifyState::NotFoundAnywhere { context: _ } => IdentifyStateView::NotFoundAnywhere,

            IdentifyState::ManualOnly {
                track_count,
                context: _,
            } => IdentifyStateView::ManualOnly { track_count },

            IdentifyState::Failed {
                failures,
                matches,
                library_statuses,
                provenance,
                track_count: _,
                context: _,
            } => {
                let (groups, provenance) = fold_matches(matches, provenance);
                IdentifyStateView::Failed {
                    failures,
                    groups,
                    library_statuses,
                    provenance,
                }
            }
        }
    }
}

/// Fold a match list into its group cards, keying the provenance by release id
/// first: `combine` produces it index-aligned with the matches, and once the
/// matches are inside the cards that alignment is no longer expressible.
fn fold_matches(
    matches: Vec<crate::import::search::MetadataResult>,
    provenance: Vec<ResultProvenance>,
) -> (Vec<ReleaseGroup>, Vec<(String, ResultProvenance)>) {
    let keyed = matches
        .iter()
        .map(|result| result.release_id.clone())
        .zip(provenance)
        .collect();
    (crate::import::release_group::group_results(matches), keyed)
}

/// The disc-ID step: what extraction read, from the context, and how far
/// MusicBrainz's lookup of it has got, from the pipe.
fn disc_id_step(progress: DiscidProgress, context: &SignalsContext) -> DiscIdStepView {
    let (disc_id, source_file) = match &context.disc_id {
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
    let lookup = match progress {
        // The track count is a settled-state concern — it reaches a surface
        // through the terminal state, not through progress.
        DiscidProgress::Computing | DiscidProgress::LookingUp => LookupView::LookingUp,
        DiscidProgress::Done { results, .. } => found_or_no_match(results.len()),
        DiscidProgress::Skipped { .. } => {
            unreachable!("a computed disc ID is never skipped")
        }
        DiscidProgress::Failed { failure, .. } => LookupView::Failed { failure },
    };
    DiscIdStepView::Read {
        disc_id,
        source_file,
        lookup,
    }
}

fn barcode_step(progress: BarcodeProgress) -> BarcodeStepView {
    match progress {
        BarcodeProgress::Scanning => BarcodeStepView::AwaitingArtwork,
        BarcodeProgress::NoCodes => BarcodeStepView::NoCodes,
        BarcodeProgress::ScanFailed { failure } => BarcodeStepView::ScanFailed { failure },
        BarcodeProgress::Skipped => BarcodeStepView::Absent,
        BarcodeProgress::Lookups { codes, providers } => {
            let total = codes.len() as u32;
            let providers = providers
                .into_iter()
                .map(|provider| ProviderBarcodeLookupView {
                    source: provider.source,
                    state: match provider.state {
                        BarcodeLookupState::Trying { index } => BarcodeLookupView::Trying {
                            barcode: codes[index].clone(),
                            position: index as u32 + 1,
                            total,
                        },
                        BarcodeLookupState::Matched { code, results } => {
                            BarcodeLookupView::Matched {
                                barcode: code,
                                count: results.len() as u32,
                            }
                        }
                        BarcodeLookupState::Exhausted => BarcodeLookupView::Exhausted,
                        BarcodeLookupState::Failed { failure } => {
                            BarcodeLookupView::Failed { failure }
                        }
                    },
                })
                .collect();
            BarcodeStepView::Lookups { codes, providers }
        }
    }
}

fn catalog_step(progress: CatalogProgress, context: &SignalsContext) -> CatalogStepView {
    let Some(value) = &context.chosen_catalog else {
        return if context.catalogs.is_empty() {
            CatalogStepView::NoneFound
        } else {
            CatalogStepView::Unchosen {
                available: context.catalogs.len() as u32,
            }
        };
    };
    let lookups = match progress {
        CatalogProgress::Skipped => Vec::new(),
        CatalogProgress::Lookups { lookups } => lookups
            .into_iter()
            .map(|lookup| ProviderLookupView {
                source: lookup.source,
                state: match lookup.state {
                    LookupState::LookingUp => LookupView::LookingUp,
                    LookupState::Done { results } => found_or_no_match(results.len()),
                    LookupState::Failed { failure } => LookupView::Failed { failure },
                },
            })
            .collect(),
    };
    CatalogStepView::Chosen {
        value: value.clone(),
        lookups,
    }
}

fn found_or_no_match(count: usize) -> LookupView {
    if count == 0 {
        LookupView::NoMatch
    } else {
        LookupView::Found {
            count: count as u32,
        }
    }
}

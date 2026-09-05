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

use super::combine::{combine_results, CombineOutcome, ResultProvenance};
use super::state::{
    BarcodeLookupState, BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState,
    LookupState, SignalsContext,
};
use crate::db::LibraryStatus;
use crate::import::release_group::ReleaseGroup;
use crate::import::MetadataSource;
use crate::signals::{ArtworkScan, DiscIdSignal, LookupFailure, SignalOrigin};

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

/// The artwork: read one image at a time for barcodes and text. A source,
/// not a signal, so it has no lookups of its own — what it turns up feeds the
/// barcode and catalog steps.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtworkStepView {
    /// Nothing to read: no images, or no analyzer on this platform.
    Absent,
    /// Reading `current`, the `position`th of `total`, with what the images
    /// read so far have turned up.
    Reading {
        /// The image's candidate-relative path; `None` for a library
        /// release's stored cover.
        current: Option<String>,
        position: u32,
        total: u32,
        barcodes: u32,
        catalogs: u32,
    },
    /// Every image read.
    Read {
        images: u32,
        barcodes: u32,
        catalogs: u32,
    },
    /// Reading stopped at a failure, `read` images in.
    Failed {
        failure: LookupFailure,
        read: u32,
        total: u32,
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
    /// The providers the run asks, in the order their rows are listed.
    /// Named up front so a surface can show a step's provider rows before
    /// that step's lookups have started.
    pub providers: Vec<MetadataSource>,
    pub disc_id: DiscIdStepView,
    pub artwork: ArtworkStepView,
    pub barcode: BarcodeStepView,
    pub catalog: CatalogStepView,
}

/// One candidate's identify state as a surface renders it.
#[derive(Debug, Clone)]
pub enum IdentifyStateView {
    Idle,

    /// Lookups in flight, laid out as the steps the run is taking, with the
    /// matches every answered lookup has combined to so far — the same
    /// combine the settle runs, so what a person sees mid-run is what the
    /// verdict lands on, and a row that has landed does not jump at settle.
    Triangulating {
        run: IdentifyRunView,
        groups: Vec<ReleaseGroup>,
        library_statuses: Vec<LibraryStatus>,
        provenance: Vec<(String, ResultProvenance)>,
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
            } => {
                let (matches, library_statuses, provenance) =
                    live_matches(&discid, &barcode, &catalog, &context);
                let (groups, provenance) = fold_matches(matches, provenance);
                IdentifyStateView::Triangulating {
                    run: IdentifyRunView {
                        providers: context.providers.clone(),
                        disc_id: disc_id_step(discid, &context),
                        artwork: artwork_step(&context),
                        barcode: barcode_step(barcode),
                        catalog: catalog_step(catalog, &context),
                    },
                    groups,
                    library_statuses,
                    provenance,
                }
            }

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
    Vec<crate::import::search::MetadataResult>,
    Vec<LibraryStatus>,
    Vec<ResultProvenance>,
) {
    let unless = |excluded: bool, results: Vec<_>| if excluded { Vec::new() } else { results };
    let outcome = combine_results(
        unless(context.disc_excluded, discid.results()),
        unless(context.barcode_excluded, barcode.results()),
        unless(context.chosen_catalog.is_none(), catalog.results()),
    );
    match outcome {
        CombineOutcome::Found {
            matches,
            library_statuses,
            provenance,
        } => (matches, library_statuses, provenance),
        CombineOutcome::NotFoundAnywhere => (Vec::new(), Vec::new(), Vec::new()),
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

/// The artwork step: where the pass is, from the latest snapshot, and what
/// the images read so far turned up — only what came off the artwork, since
/// a CUE sheet's barcode or a folder name's catalog number is not its doing.
fn artwork_step(context: &SignalsContext) -> ArtworkStepView {
    let from_artwork = |values: &[crate::signals::SourcedValue]| {
        values
            .iter()
            .filter(|v| v.origin == SignalOrigin::Artwork)
            .count() as u32
    };
    let barcodes = from_artwork(&context.barcode_codes);
    let catalogs = from_artwork(&context.catalogs);
    match &context.artwork {
        ArtworkScan::Absent => ArtworkStepView::Absent,
        ArtworkScan::Reading {
            current,
            position,
            total,
        } => ArtworkStepView::Reading {
            current: current.clone(),
            position: *position,
            total: *total,
            barcodes,
            catalogs,
        },
        ArtworkScan::Done { total } => ArtworkStepView::Read {
            images: *total,
            barcodes,
            catalogs,
        },
        ArtworkScan::Failed {
            failure,
            read,
            total,
        } => ArtworkStepView::Failed {
            failure: failure.clone(),
            read: *read,
            total: *total,
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::LibraryStatus;
    use crate::identify::state::{BarcodeLookupState, ProviderBarcodeLookup};
    use crate::import::search::MetadataResult;
    use crate::import::MetadataSource;
    use crate::signals::{SignalOrigin, SourcedValue};

    fn result(source: MetadataSource, release_id: &str) -> (MetadataResult, LibraryStatus) {
        (
            MetadataResult {
                source,
                release_id: release_id.to_string(),
                title: "Album".to_string(),
                artist: None,
                year: None,
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
                cover_art: None,
                source_group_id: Some("g".to_string()),
                source_tracks: None,
            },
            LibraryStatus {
                release_id: release_id.to_string(),
                release_in_library: false,
                album_in_library: false,
                album_title: None,
                album_id: None,
            },
        )
    }

    fn context() -> SignalsContext {
        SignalsContext {
            providers: vec![MetadataSource::MusicBrainz, MetadataSource::Discogs],
            disc_id: DiscIdSignal::Absent { track_count: 9 },
            artwork: crate::signals::ArtworkScan::Absent,
            barcode_codes: vec![SourcedValue::new("A".to_string(), SignalOrigin::Artwork)],
            had_barcode_source: true,
            catalogs: Vec::new(),
            chosen_catalog: None,
            disc_excluded: false,
            barcode_excluded: false,
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            catalog_results: Vec::new(),
            discid_failure: None,
            barcode_failures: Vec::new(),
            barcode_scan_failure: None,
            catalog_failures: Vec::new(),
            matched_barcode: None,
            track_count: 9,
        }
    }

    fn in_flight(context: SignalsContext) -> IdentifyState {
        IdentifyState::Triangulating {
            discid: DiscidProgress::Skipped { track_count: 9 },
            barcode: BarcodeProgress::Lookups {
                codes: vec!["A".to_string()],
                providers: vec![
                    ProviderBarcodeLookup {
                        source: MetadataSource::MusicBrainz,
                        state: BarcodeLookupState::Trying { index: 0 },
                    },
                    ProviderBarcodeLookup {
                        source: MetadataSource::Discogs,
                        state: BarcodeLookupState::Matched {
                            code: Some("A".to_string()),
                            results: vec![result(MetadataSource::Discogs, "dg-1")],
                        },
                    },
                ],
            },
            catalog: CatalogProgress::Skipped,
            context,
        }
    }

    /// What one provider found shows while the other is still looking, with
    /// the provenance the settled verdict will give it.
    #[test]
    fn a_landed_provider_s_matches_show_before_the_other_answers() {
        let IdentifyStateView::Triangulating {
            groups, provenance, ..
        } = IdentifyStateView::from(in_flight(context()))
        else {
            panic!("a run in flight");
        };
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].pressings[0].releases[0].release_id, "dg-1");
        assert_eq!(provenance.len(), 1);
        assert_eq!(provenance[0].0, "dg-1");
        assert!(provenance[0].1.by_barcode);
    }

    /// The artwork step counts only what came off the artwork: a barcode from
    /// a CUE sheet or a catalog number from the folder name is not its doing.
    #[test]
    fn the_artwork_step_counts_only_what_the_artwork_turned_up() {
        let mut context = context();
        context.artwork = ArtworkScan::Reading {
            current: Some("Back.jpg".to_string()),
            position: 2,
            total: 3,
        };
        context.barcode_codes = vec![
            SourcedValue::new("A".to_string(), SignalOrigin::Artwork),
            SourcedValue::new("B".to_string(), SignalOrigin::CueSheet),
        ];
        context.catalogs = vec![
            SourcedValue::new("LBL-1".to_string(), SignalOrigin::FolderName),
            SourcedValue::new("LBL-2".to_string(), SignalOrigin::Artwork),
            SourcedValue::new("LBL-3".to_string(), SignalOrigin::Artwork),
        ];
        let IdentifyStateView::Triangulating { run, .. } =
            IdentifyStateView::from(in_flight(context))
        else {
            panic!("a run in flight");
        };
        assert_eq!(
            run.artwork,
            ArtworkStepView::Reading {
                current: Some("Back.jpg".to_string()),
                position: 2,
                total: 3,
                barcodes: 1,
                catalogs: 2,
            }
        );
    }

    /// A signal the user unchecked contributes nothing mid-run, as it will
    /// contribute nothing at settle.
    #[test]
    fn an_excluded_signal_s_matches_do_not_show() {
        let mut context = context();
        context.barcode_excluded = true;
        let IdentifyStateView::Triangulating { groups, .. } =
            IdentifyStateView::from(in_flight(context))
        else {
            panic!("a run in flight");
        };
        assert!(groups.is_empty());
    }
}

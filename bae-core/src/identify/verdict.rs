//! The identify pipeline's terminal outcome — what
//! [`crate::db::DbImportCandidateState`] persists, as one
//! `import_candidate_verdict` row and the `import_candidate_match` rows that
//! hang off it.
//!
//! [`IdentifyState`] is the reducer's own working shape: it carries a full
//! [`SignalsContext`] (raw signal inputs, the user's exclusions) through every
//! state so a toggle or a re-run can re-combine without re-fetching, and it has
//! `Idle` and `Triangulating` variants that are mid-flight, not a verdict at
//! all. None of that belongs on disk. [`TerminalVerdict`] is the shape that
//! does: only the four states identification can actually end on, holding only
//! what the candidate's next launch needs back.
//!
//! `LibraryStatus` is deliberately absent from every variant. It is mutable
//! local state — another import landing can flip it — so storing it would
//! freeze a snapshot nothing invalidates. A reader re-checks it live, against
//! the release ids named here, rather than trusting a stored copy.
//!
//! The reducer makes partial evidence unrepresentable as a successful terminal
//! state: any active lookup failure produces `IdentifyState::Failed`. Only
//! `Idle` and `Triangulating` have no terminal verdict.

use super::combine::ResultProvenance;
use super::state::{
    BarcodeEvidence, CatalogEvidence, DiscIdEvidence, IdentifyState, RecordedWalk, SignalsContext,
    WalkEnd,
};
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::MetadataSource;
use crate::signals::{ArtworkScan, BarcodeSignal, DiscIdSignal, LookupFailure, Signals};

/// Which lookup failed, and — where several providers answer it — which
/// provider. The disc-ID endpoint is MusicBrainz's alone, and release details
/// are fetched from the source that named the release, so those two name no
/// provider; the barcode and catalog lookups ask every configured provider
/// independently, so one failing is a fact about that provider.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IdentifyFailure {
    DiscId(LookupFailure),
    /// Reading the candidate's barcodes failed, so no provider was asked. Not
    /// a provider's failure, which is why it names none.
    BarcodeScan(LookupFailure),
    Barcode(SourceFailure),
    Catalog(SourceFailure),
    ReleaseDetails(LookupFailure),
}

impl IdentifyFailure {
    /// The provider that could not answer, where the step asks several. The
    /// disc-ID endpoint is MusicBrainz's alone, reading the candidate's
    /// barcodes asks nobody, and release details come from the source that
    /// named the release — so those three name no provider.
    fn source(&self) -> Option<MetadataSource> {
        match self {
            Self::Barcode(failure) | Self::Catalog(failure) => Some(failure.source),
            Self::DiscId(_) | Self::BarcodeScan(_) | Self::ReleaseDetails(_) => None,
        }
    }
}

/// The identify pipeline's outcome once it can no longer change without new
/// input from the user or a re-run. Built from [`IdentifyState`]'s four
/// terminal variants (`Found`, `NotFoundAnywhere`, `ManualOnly`, `Failed`);
/// `Idle` and `Triangulating` have no terminal verdict, hence the fallible
/// conversion below.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TerminalVerdict {
    /// One or more results — what `combine` produced, and what the sidebar and
    /// the Ready rule both work from directly.
    Found {
        matches: Vec<MetadataResult>,
        track_count: u32,
        /// Index-aligned with `matches`: which signal(s) produced or confirmed
        /// each one, for the sidebar's "matched on disc ID / barcode / text"
        /// evidence line.
        provenance: Vec<ResultProvenance>,
        /// Which of the candidate's barcodes the lookup that produced the
        /// barcode matches ran against. `None` when no barcode matched.
        ///
        /// The one lookup input the verdict keeps. The rest are recomputable
        /// facts a re-run re-extracts, but this one names *which* of several
        /// stored barcodes is the one that found the release — and the stored
        /// barcode rows carry the file each was read off, so this pointer is
        /// what puts a chip on that image and no other.
        matched_barcode: Option<String>,
    },
    /// Both signals ran and settled on zero results. Distinct from a transport
    /// failure — nothing about this candidate's row goes unwritten; "we looked
    /// everywhere and there is nothing" is itself the answer.
    NotFoundAnywhere,
    /// Nothing to look up at all: no disc-ID artifact and no barcode source.
    /// Distinct from `NotFoundAnywhere` — there, signals ran and matched
    /// nothing; here, none ran, so a reader offers manual search rather than
    /// claiming a lookup that never happened.
    ManualOnly { track_count: u32 },
    /// At least one provider step failed, so partial evidence must not be
    /// classified as a complete answer.
    Failed {
        failures: Vec<IdentifyFailure>,
        track_count: u32,
    },
}

impl TryFrom<IdentifyState> for TerminalVerdict {
    /// The state handed back unchanged when it isn't terminal yet (`Idle` or
    /// `Triangulating`).
    type Error = IdentifyState;

    fn try_from(state: IdentifyState) -> Result<Self, Self::Error> {
        match state {
            IdentifyState::Found {
                matches,
                track_count,
                provenance,
                // A live per-release check at read time, not a stored copy —
                // see the module doc.
                library_statuses: _,
                context,
            } => Ok(Self::Found {
                matches,
                track_count,
                provenance,
                matched_barcode: context.barcode.matched,
            }),

            IdentifyState::NotFoundAnywhere { context: _ } => Ok(Self::NotFoundAnywhere),

            IdentifyState::ManualOnly {
                track_count,
                context: _,
            } => Ok(Self::ManualOnly { track_count }),

            // The partial matches a failed state carries are live evidence of
            // what the other source found, not a stored answer: the failure is
            // what the next launch has to know, and re-running is what turns
            // partial evidence into a verdict.
            IdentifyState::Failed {
                failures,
                track_count,
                matches: _,
                library_statuses: _,
                provenance: _,
                context: _,
            } => Ok(Self::Failed {
                failures,
                track_count,
            }),

            other @ (IdentifyState::Idle | IdentifyState::Triangulating { .. }) => Err(other),
        }
    }
}

impl TerminalVerdict {
    /// Every release this verdict names — what a resumer checks live library
    /// status for before standing the state back up.
    pub fn named_releases(&self) -> Vec<&MetadataResult> {
        match self {
            Self::Found { matches, .. } => matches.iter().collect(),
            Self::NotFoundAnywhere | Self::ManualOnly { .. } | Self::Failed { .. } => Vec::new(),
        }
    }

    /// The identify state this stored verdict stands back up as — what opening
    /// an answered candidate shows without running anything.
    ///
    /// A settled run is two halves, stored side by side in one write:
    /// `signals` is what extraction read off the folder — the disc ID and the
    /// file it came from, every barcode sighting, every catalog number — and
    /// the verdict is what the providers answered about them. Put back
    /// together they are the run as it settled, so a resumed candidate lays
    /// the same ledger out as the live run did, and a cell that failed offers
    /// its retry.
    ///
    /// Two things no write keeps, so no resume has them: the catalog numbers
    /// the person had chosen (they come back as tiles to choose again), and a
    /// failed run's partial matches — the failure is what stores, and
    /// re-running is what turns partial evidence into an answer.
    ///
    /// `signals` is `None` for a candidate whose signals are not stored: then
    /// there are no inputs, so there is no ledger — the matches, the barcode
    /// that found them, and nothing else.
    ///
    /// `status_of` is the live library check for a release id the verdict
    /// names, never a stored copy (see the module doc).
    pub fn resume_state(
        self,
        signals: Option<&Signals>,
        status_of: &impl Fn(&MetadataResult) -> LibraryStatus,
    ) -> IdentifyState {
        let providers = self.resumed_providers();
        match self {
            Self::Found {
                matches,
                track_count,
                provenance,
                matched_barcode,
            } => {
                let library_statuses: Vec<LibraryStatus> = matches.iter().map(status_of).collect();
                let context = match signals {
                    Some(signals) => found_run(
                        stored_inputs(signals, providers),
                        &matches,
                        &library_statuses,
                        &provenance,
                        matched_barcode,
                    ),
                    None => SignalsContext {
                        barcode: BarcodeEvidence {
                            matched: matched_barcode,
                            ..Default::default()
                        },
                        ..no_inputs(track_count)
                    },
                };
                IdentifyState::Found {
                    matches,
                    library_statuses,
                    track_count,
                    provenance,
                    context,
                }
            }
            Self::NotFoundAnywhere => IdentifyState::NotFoundAnywhere {
                context: match signals {
                    Some(signals) => exhausted_run(stored_inputs(signals, providers)),
                    None => no_inputs(0),
                },
            },
            Self::ManualOnly { track_count } => IdentifyState::ManualOnly {
                track_count,
                // Nothing ran, so there is nothing to record onto the inputs:
                // they are the whole run. Catalog numbers among them come back
                // as tiles to choose.
                context: match signals {
                    Some(signals) => stored_inputs(signals, providers),
                    None => no_inputs(track_count),
                },
            },
            // A stored failure resumes with no matches: what one source found
            // before the other failed was never stored, so a resumed failure
            // offers the re-run rather than a partial list.
            Self::Failed {
                failures,
                track_count,
            } => IdentifyState::Failed {
                context: match signals {
                    Some(signals) => failed_run(stored_inputs(signals, providers), &failures),
                    None => no_inputs(track_count),
                },
                failures,
                track_count,
                matches: Vec::new(),
                library_statuses: Vec::new(),
                provenance: Vec::new(),
            },
        }
    }

    /// The providers a resumed run lays its columns out for. The run's own
    /// list is not stored and the verdict is what is left of it: MusicBrainz
    /// answers every run, and Discogs was in this one when the verdict names
    /// it — as the source of a match, or of a failure. A run that asked
    /// Discogs and heard neither from it resumes as a MusicBrainz-only ledger,
    /// which is as much as the stored answer says.
    fn resumed_providers(&self) -> Vec<MetadataSource> {
        let asked_discogs = match self {
            Self::Found { matches, .. } => matches
                .iter()
                .any(|result| result.source == MetadataSource::Discogs),
            Self::Failed { failures, .. } => failures
                .iter()
                .any(|failure| failure.source() == Some(MetadataSource::Discogs)),
            Self::NotFoundAnywhere | Self::ManualOnly { .. } => false,
        };
        if asked_discogs {
            vec![MetadataSource::MusicBrainz, MetadataSource::Discogs]
        } else {
            vec![MetadataSource::MusicBrainz]
        }
    }
}

/// What extraction read, as the run had it. Nothing has been asked yet — the
/// verdict's own answers are recorded onto this below.
fn stored_inputs(signals: &Signals, providers: Vec<MetadataSource>) -> SignalsContext {
    SignalsContext {
        providers,
        // No artwork pass runs behind a stored verdict, so no row waits on one.
        artwork: ArtworkScan::Absent,
        disc: DiscIdEvidence {
            signal: signals.disc_id.clone(),
            ..Default::default()
        },
        barcode: BarcodeEvidence {
            codes: signals.barcode.codes().to_vec(),
            had_source: !matches!(signals.barcode, BarcodeSignal::Absent),
            ..Default::default()
        },
        catalog: CatalogEvidence {
            numbers: signals.text.catalogs().to_vec(),
            chosen: Vec::new(),
        },
        track_count: signals.disc_id.track_count(),
    }
}

/// The context a verdict resumes with when its candidate's signals are not
/// stored. `disc_id: Absent` here means "not retained", not "the folder had no
/// disc artifact" — the distinction never leaves core: with no inputs at all
/// there is no ledger to lay out and no toolbar to draw.
fn no_inputs(track_count: u32) -> SignalsContext {
    SignalsContext {
        providers: Vec::new(),
        artwork: ArtworkScan::Absent,
        disc: DiscIdEvidence {
            signal: DiscIdSignal::Absent { track_count },
            ..Default::default()
        },
        barcode: BarcodeEvidence::default(),
        catalog: CatalogEvidence::default(),
        track_count,
    }
}

/// What a `Found` verdict's providers answered: each match lands on the signal
/// its provenance names, and every provider's walk through the codes ended
/// where its match says — on the code the verdict points at, or having tried
/// them all.
fn found_run(
    mut context: SignalsContext,
    matches: &[MetadataResult],
    library_statuses: &[LibraryStatus],
    provenance: &[ResultProvenance],
    matched_barcode: Option<String>,
) -> SignalsContext {
    let found_by = |names: fn(&ResultProvenance) -> bool| {
        matches
            .iter()
            .zip(provenance)
            .zip(library_statuses)
            .filter(|((_, provenance), _)| names(provenance))
            .map(|((result, _), status)| (result.clone(), status.clone()))
            .collect::<Vec<_>>()
    };
    context.disc.results = found_by(|provenance| provenance.by_disc_id);
    context.barcode.results = found_by(|provenance| provenance.by_barcode);

    // A walk that matched names one of the codes it asked about, and the codes
    // and the verdict are written together, so the code the verdict points at
    // is one of them.
    let codes = context.barcode.code_values();
    let matched = matched_barcode.filter(|code| codes.contains(code));
    let answered = |source| {
        context
            .barcode
            .results
            .iter()
            .any(|(result, _)| result.source == source)
    };
    let walks = context
        .providers
        .iter()
        .map(|&source| RecordedWalk {
            source,
            end: match &matched {
                Some(code) if answered(source) => WalkEnd::Matched { code: code.clone() },
                _ => WalkEnd::Exhausted,
            },
        })
        .collect();
    context.barcode.walks = walks;
    context.barcode.matched = matched;
    context
}

/// A run that asked everything it had and matched nothing: every provider's
/// walk ran out of codes.
fn exhausted_run(mut context: SignalsContext) -> SignalsContext {
    let walks = context
        .providers
        .iter()
        .map(|&source| RecordedWalk {
            source,
            end: WalkEnd::Exhausted,
        })
        .collect();
    context.barcode.walks = walks;
    context
}

/// What a `Failed` verdict says of each step: the disc-ID lookup's failure,
/// the failure that stopped the barcodes being read at all, and every provider
/// that could not answer about a barcode. Which code a failed walk stopped on
/// is not stored, and every walk starts at the first code, so that is where
/// the ledger puts the warning. The providers that did answer show no matches:
/// a failed verdict stores none.
fn failed_run(mut context: SignalsContext, failures: &[IdentifyFailure]) -> SignalsContext {
    context.disc.failure = failures.iter().find_map(|failure| match failure {
        IdentifyFailure::DiscId(failure) => Some(failure.clone()),
        _ => None,
    });
    context.barcode.scan_failure = failures.iter().find_map(|failure| match failure {
        IdentifyFailure::BarcodeScan(failure) => Some(failure.clone()),
        _ => None,
    });
    context.barcode.failures = failures
        .iter()
        .filter_map(|failure| match failure {
            IdentifyFailure::Barcode(failure) => Some(failure.clone()),
            _ => None,
        })
        .collect();
    let first_code = context.barcode.code_values().into_iter().next();
    let walks = match &first_code {
        Some(first) => context
            .providers
            .iter()
            .map(|&source| RecordedWalk {
                source,
                end: if context
                    .barcode
                    .failures
                    .iter()
                    .any(|failure| failure.source == source)
                {
                    WalkEnd::Failed {
                        code: first.clone(),
                    }
                } else {
                    WalkEnd::Exhausted
                },
            })
            .collect(),
        // No code to walk, so no walk ran: the barcode pipe stands back up
        // from the codes alone.
        None => Vec::new(),
    };
    context.barcode.walks = walks;
    context
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::LibraryStatus;
    use crate::identify::state::{BarcodeProgress, DiscidProgress};
    use crate::import::MetadataSource;

    fn mk_result(release_id: &str) -> MetadataResult {
        MetadataResult::for_test(MetadataSource::MusicBrainz, release_id, Some("group-1"))
    }

    /// A bare context, standing in for whatever the reducer would have
    /// accumulated by this point — its contents don't matter to these tests,
    /// only that `Idle`/`Triangulating` carry one and still aren't terminal.
    fn mk_context(track_count: u32) -> SignalsContext {
        SignalsContext {
            providers: Vec::new(),
            artwork: crate::signals::ArtworkScan::Absent,
            disc: DiscIdEvidence {
                signal: crate::signals::DiscIdSignal::Absent { track_count },
                ..Default::default()
            },
            barcode: BarcodeEvidence::default(),
            catalog: Default::default(),
            track_count,
        }
    }

    /// `Idle` and `Triangulating` are not verdicts — the conversion must reject
    /// them (not silently invent an empty verdict), and hand the state back.
    #[test]
    fn in_flight_states_have_no_terminal_verdict() {
        assert!(TerminalVerdict::try_from(IdentifyState::Idle).is_err());
        assert!(TerminalVerdict::try_from(IdentifyState::Triangulating {
            discid: DiscidProgress::Computing,
            barcode: BarcodeProgress::Scanning,
            catalog: crate::identify::CatalogProgress::Skipped,
            context: mk_context(0),
        })
        .is_err());
    }

    fn found_state() -> IdentifyState {
        IdentifyState::Found {
            matches: vec![mk_result("rel-1")],
            library_statuses: vec![LibraryStatus::absent("rel-1")],
            track_count: 11,
            provenance: vec![ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
            }],
            context: mk_context(11),
        }
    }

    /// `Found` keeps its matches and provenance, and drops
    /// `library_statuses` — that's re-checked live, not stored. `mk_context`
    /// carries no recorded failure, so this also stands as the positive case:
    /// a `Found` reached with both lookups completing converts normally.
    #[test]
    fn found_drops_library_status_and_keeps_the_rest() {
        let verdict = TerminalVerdict::try_from(found_state()).unwrap();
        assert_eq!(
            verdict,
            TerminalVerdict::Found {
                matches: vec![mk_result("rel-1")],
                track_count: 11,
                provenance: vec![ResultProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    by_catalog: false,
                }],
                matched_barcode: None,
            }
        );
    }

    /// A candidate whose signals were never stored stands its verdict back up
    /// with its matches, and with the barcode that found them: the barcode rows
    /// say which image each was read off, so without this pointer a resumed
    /// candidate could not tell which of several images the release was
    /// identified from.
    #[test]
    fn a_resumed_found_keeps_the_barcode_that_matched() {
        let verdict = TerminalVerdict::Found {
            matches: vec![mk_result("rel-1")],
            track_count: 11,
            provenance: vec![ResultProvenance {
                by_disc_id: false,
                by_barcode: true,
                by_catalog: false,
            }],
            matched_barcode: Some("5099969394522".to_string()),
        };
        let IdentifyState::Found { context, .. } =
            verdict.resume_state(None, &|result| LibraryStatus::absent(&result.release_id))
        else {
            panic!("a found verdict resumes as Found");
        };
        assert_eq!(context.barcode.matched.as_deref(), Some("5099969394522"));
        // With no stored signals there are no inputs to stand the run back up
        // from, so the resumed state has no ledger and no signals toolbar.
        assert!(context.barcode.codes.is_empty());
        assert!(!context.has_inputs());
    }

    /// The signals stored beside a verdict: a disc ID read off a rip log, one
    /// barcode read off the back cover, one catalog number off the sheet.
    fn stored_signals() -> Signals {
        Signals {
            disc_id: DiscIdSignal::Computed {
                disc_id: "disc-1".to_string(),
                track_count: 11,
                source_file: Some("rip/Album.LOG".to_string()),
            },
            barcode: BarcodeSignal::Settled {
                codes: vec![crate::signals::SourcedValue::in_file(
                    "5099969394522".to_string(),
                    crate::signals::SignalOrigin::Artwork,
                    "back.jpg".to_string(),
                )],
            },
            text: crate::signals::TextSignal::Settled {
                catalogs: vec![crate::signals::SourcedValue::new(
                    "LBL-1".to_string(),
                    crate::signals::SignalOrigin::CueSheet,
                )],
                free_text: Vec::new(),
            },
            durations: Default::default(),
        }
    }

    /// A candidate's stored signals are the run's inputs, so a verdict resumed
    /// with them carries what extraction read — every sighting with where it
    /// was read — and the run stands back up from it. The numbers the person
    /// had chosen are not stored, so they come back as numbers to choose.
    #[test]
    fn a_resumed_verdict_carries_the_stored_signal_inputs() {
        let verdict = TerminalVerdict::Failed {
            failures: vec![IdentifyFailure::Barcode(SourceFailure {
                source: MetadataSource::Discogs,
                failure: LookupFailure::Provider { status: Some(503) },
            })],
            track_count: 11,
        };
        let IdentifyState::Failed { context, .. } = verdict
            .resume_state(Some(&stored_signals()), &|result| {
                LibraryStatus::absent(&result.release_id)
            })
        else {
            panic!("a failed verdict resumes as Failed");
        };
        assert!(context.has_inputs());
        assert_eq!(context.disc.signal, stored_signals().disc_id);
        assert_eq!(context.barcode.code_values(), vec!["5099969394522"]);
        assert_eq!(
            context.barcode.codes[0].origin_path.as_deref(),
            Some("back.jpg")
        );
        assert!(context.barcode.had_source);
        assert_eq!(context.catalog.number_values(), vec!["LBL-1"]);
        assert!(context.catalog.chosen.is_empty());
        // The provider that could not answer stopped at the first code it was
        // asked about; the one that answered ran out of codes.
        assert_eq!(
            context.barcode.walks,
            vec![
                RecordedWalk {
                    source: MetadataSource::MusicBrainz,
                    end: WalkEnd::Exhausted,
                },
                RecordedWalk {
                    source: MetadataSource::Discogs,
                    end: WalkEnd::Failed {
                        code: "5099969394522".to_string(),
                    },
                },
            ]
        );
    }

    /// The run's provider list is not stored, so a resumed run asks what the
    /// verdict names: MusicBrainz answers every run, and Discogs is a column
    /// only where the verdict names it.
    #[test]
    fn a_resumed_run_lists_the_providers_the_verdict_names() {
        let musicbrainz_only = TerminalVerdict::Failed {
            failures: vec![IdentifyFailure::DiscId(LookupFailure::Network)],
            track_count: 11,
        };
        let IdentifyState::Failed { context, .. } = musicbrainz_only
            .resume_state(Some(&stored_signals()), &|result| {
                LibraryStatus::absent(&result.release_id)
            })
        else {
            panic!("a failed verdict resumes as Failed");
        };
        assert_eq!(context.providers, vec![MetadataSource::MusicBrainz]);

        let found_on_discogs = TerminalVerdict::Found {
            matches: vec![MetadataResult::for_test(
                MetadataSource::Discogs,
                "dg-1",
                Some("g"),
            )],
            track_count: 11,
            provenance: vec![ResultProvenance {
                by_disc_id: false,
                by_barcode: true,
                by_catalog: false,
            }],
            matched_barcode: Some("5099969394522".to_string()),
        };
        let IdentifyState::Found { context, .. } = found_on_discogs
            .resume_state(Some(&stored_signals()), &|result| {
                LibraryStatus::absent(&result.release_id)
            })
        else {
            panic!("a found verdict resumes as Found");
        };
        assert_eq!(
            context.providers,
            vec![MetadataSource::MusicBrainz, MetadataSource::Discogs]
        );
    }

    /// The reducer exposes a failed lookup directly, and verdict conversion
    /// preserves that terminal state.
    #[test]
    fn a_recorded_discid_failure_derives_and_stores_as_failed() {
        let mut context = mk_context(11);
        context.disc.failure = Some(crate::signals::LookupFailure::Provider { status: None });
        let state = crate::identify::state::re_derive_for_tests(context);
        assert!(matches!(state, IdentifyState::Failed { .. }));
        let verdict = TerminalVerdict::try_from(state).unwrap();
        assert!(matches!(
            verdict,
            TerminalVerdict::Failed {
                failures,
                track_count: 11,
            } if failures == vec![IdentifyFailure::DiscId(crate::signals::LookupFailure::Provider {
                    status: None
                })]
        ));
    }

    /// Signals that share no result settle as one `Found` over their union, so
    /// what stores is a single match list — not two sections. Both
    /// Neither signal recorded a failure here, so this also stands as the
    /// positive case for a union-shaped `Found`.
    #[test]
    fn a_union_of_disagreeing_signals_stores_as_one_match_list() {
        let context = SignalsContext {
            disc: DiscIdEvidence {
                signal: crate::signals::DiscIdSignal::Absent { track_count: 9 },
                results: vec![(mk_result("rel-a"), LibraryStatus::absent("rel-a"))],
                ..Default::default()
            },
            barcode: BarcodeEvidence {
                had_source: true,
                results: vec![(mk_result("rel-b"), LibraryStatus::absent("rel-b"))],
                matched: Some("012345".to_string()),
                ..Default::default()
            },
            ..mk_context(9)
        };
        let state = crate::identify::state::re_derive_for_tests(context);
        let verdict = TerminalVerdict::try_from(state).unwrap();
        assert_eq!(
            verdict,
            TerminalVerdict::Found {
                matches: vec![mk_result("rel-a"), mk_result("rel-b")],
                track_count: 9,
                provenance: vec![
                    ResultProvenance {
                        by_disc_id: true,
                        by_barcode: false,
                        by_catalog: false,
                    },
                    ResultProvenance {
                        by_disc_id: false,
                        by_barcode: true,
                        by_catalog: false,
                    },
                ],
                matched_barcode: Some("012345".to_string()),
            }
        );
    }

    /// A union reached where the disc-ID lookup failed rather than genuinely
    /// disagreeing: had it succeeded with a release the barcode side also
    /// returned, the intersection would have narrowed to one release. A
    /// missing intersection partner is exactly what can manufacture a longer
    /// match list, so this stores as a failure rather than that partial list.
    #[test]
    fn a_union_reached_with_a_recorded_discid_failure_is_failed() {
        let context = SignalsContext {
            disc: DiscIdEvidence {
                signal: crate::signals::DiscIdSignal::Absent { track_count: 9 },
                failure: Some(crate::signals::LookupFailure::Network),
                ..Default::default()
            },
            barcode: BarcodeEvidence {
                had_source: true,
                results: vec![
                    (mk_result("rel-1"), LibraryStatus::absent("rel-1")),
                    (mk_result("rel-2"), LibraryStatus::absent("rel-2")),
                ],
                ..Default::default()
            },
            ..mk_context(9)
        };
        let state = crate::identify::state::re_derive_for_tests(context);
        let verdict = TerminalVerdict::try_from(state).unwrap();
        assert!(matches!(
            verdict,
            TerminalVerdict::Failed {
                failures,
                track_count: 9,
            } if failures == vec![IdentifyFailure::DiscId(crate::signals::LookupFailure::Network)]
        ));
    }

    /// A genuinely empty search — both signals ran, neither failed, neither
    /// found anything — is a real answer and must convert.
    #[test]
    fn clean_not_found_anywhere_is_terminal() {
        let context = mk_context(7);
        let verdict = TerminalVerdict::try_from(IdentifyState::NotFoundAnywhere { context });
        assert!(matches!(verdict, Ok(TerminalVerdict::NotFoundAnywhere)));
    }

    /// A disc-ID lookup failure derives to `Failed`, never to no-match.
    #[test]
    fn discid_failure_derives_to_failed() {
        let mut context = mk_context(7);
        context.disc.failure = Some(crate::signals::LookupFailure::Network);
        let state = crate::identify::state::re_derive_for_tests(context);
        assert!(matches!(state, IdentifyState::Failed { .. }));
        let verdict = TerminalVerdict::try_from(state).unwrap();
        assert!(matches!(
            verdict,
            TerminalVerdict::Failed {
                failures,
                track_count: 7,
            } if failures == vec![IdentifyFailure::DiscId(crate::signals::LookupFailure::Network)]
        ));
    }

    /// Same for the barcode side, naming the provider that failed.
    #[test]
    fn barcode_failure_derives_to_failed() {
        let mut context = mk_context(7);
        context.barcode.failures = vec![SourceFailure {
            source: MetadataSource::Discogs,
            failure: crate::signals::LookupFailure::Timeout,
        }];
        let state = crate::identify::state::re_derive_for_tests(context);
        assert!(matches!(state, IdentifyState::Failed { .. }));
        let verdict = TerminalVerdict::try_from(state).unwrap();
        assert!(matches!(
            verdict,
            TerminalVerdict::Failed {
                failures,
                track_count: 7,
            } if failures == vec![IdentifyFailure::Barcode(SourceFailure {
                source: MetadataSource::Discogs,
                failure: crate::signals::LookupFailure::Timeout,
            })]
        ));
    }

    /// One provider failing on the barcode while the other answered is still a
    /// failed verdict — but the live state keeps the answering provider's
    /// match, so the pane shows it instead of blanking.
    #[test]
    fn a_partial_barcode_answer_keeps_its_matches_on_a_failed_state() {
        let mut context = mk_context(7);
        context.barcode.had_source = true;
        context.barcode.results = vec![(mk_result("rel-mb"), LibraryStatus::absent("rel-mb"))];
        context.barcode.failures = vec![SourceFailure {
            source: MetadataSource::Discogs,
            failure: crate::signals::LookupFailure::Network,
        }];
        let state = crate::identify::state::re_derive_for_tests(context);
        let IdentifyState::Failed {
            matches, failures, ..
        } = &state
        else {
            panic!("a provider failure is a failed state");
        };
        assert_eq!(matches.len(), 1, "the other provider's match still stands");
        assert_eq!(
            failures,
            &vec![IdentifyFailure::Barcode(SourceFailure {
                source: MetadataSource::Discogs,
                failure: crate::signals::LookupFailure::Network,
            })]
        );
        // What stores is the failure: the partial match is live evidence, and
        // re-running is what turns it into an answer.
        assert!(matches!(
            TerminalVerdict::try_from(state).unwrap(),
            TerminalVerdict::Failed { .. }
        ));
    }

    /// Both providers answering the barcode is an ordinary `Found`, with no
    /// failure recorded.
    #[test]
    fn both_providers_answering_the_barcode_is_found() {
        let mut context = mk_context(7);
        context.barcode.had_source = true;
        context.barcode.results = vec![
            (mk_result("rel-mb"), LibraryStatus::absent("rel-mb")),
            (mk_result("rel-dg"), LibraryStatus::absent("rel-dg")),
        ];
        let state = crate::identify::state::re_derive_for_tests(context);
        assert!(matches!(state, IdentifyState::Found { .. }));
    }

    /// A provider failing with nothing from anyone leaves a failure and no
    /// matches at all.
    #[test]
    fn a_barcode_failure_with_no_results_carries_no_matches() {
        let mut context = mk_context(7);
        context.barcode.had_source = true;
        context.barcode.failures = vec![SourceFailure {
            source: MetadataSource::Discogs,
            failure: crate::signals::LookupFailure::Network,
        }];
        let IdentifyState::Failed { matches, .. } =
            crate::identify::state::re_derive_for_tests(context)
        else {
            panic!("a provider failure is a failed state");
        };
        assert!(matches.is_empty());
    }

    #[test]
    fn chosen_catalog_failure_derives_to_failed() {
        let mut context = mk_context(7);
        context.catalog.chosen = vec![crate::identify::state::ChosenCatalog {
            value: "CAT-7".to_string(),
            results: Vec::new(),
            failures: vec![SourceFailure {
                source: MetadataSource::MusicBrainz,
                failure: crate::signals::LookupFailure::Network,
            }],
        }];
        let state = crate::identify::state::re_derive_for_tests(context);
        assert!(matches!(state, IdentifyState::Failed { .. }));
        let verdict = TerminalVerdict::try_from(state).unwrap();
        assert!(matches!(
            verdict,
            TerminalVerdict::Failed {
                failures,
                track_count: 7,
            } if failures == vec![IdentifyFailure::Catalog(SourceFailure {
                source: MetadataSource::MusicBrainz,
                failure: crate::signals::LookupFailure::Network,
            })]
        ));
    }
}

//! The identify pipeline's terminal outcome — what
//! [`crate::db::DbImportCandidateState`] persists, as the identify columns of
//! `import_candidate_state` and the `import_candidate_match` rows that hang
//! off them.
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
use super::state::{IdentifyState, SignalsContext};
use crate::import::search::{MetadataResult, SourceFailure};
use crate::signals::LookupFailure;

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
                matched_barcode: context.matched_barcode,
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
    /// The matches and their provenance are the stored ones, and so is the
    /// barcode that matched. The raw signal inputs (the disc ID value, the
    /// barcode codes, the catalog candidates) are deliberately not: they are
    /// local, recomputable facts a re-run re-extracts, and the verdict never
    /// stored them — so the context carries none of those, and a resumed state
    /// has no signals toolbar. `status_of`
    /// is the live library check for a release id the verdict names, never a
    /// stored copy (see the module doc).
    pub fn resume_state(
        self,
        status_of: &impl Fn(&MetadataResult) -> crate::db::LibraryStatus,
    ) -> IdentifyState {
        // `disc_id: Absent` here means "not retained by the stored verdict",
        // not "the folder had no disc artifact" — the distinction never
        // leaves core: the bridge crosses matches and result sections, and
        // the resumed state's toolbar is empty rather than derived from this.
        let empty_context = |track_count: u32| SignalsContext {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count },
            barcode_codes: Vec::new(),
            had_barcode_source: false,
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
            track_count,
        };
        match self {
            Self::Found {
                matches,
                track_count,
                provenance,
                matched_barcode,
            } => {
                let library_statuses = matches.iter().map(status_of).collect();
                IdentifyState::Found {
                    matches,
                    library_statuses,
                    track_count,
                    provenance,
                    context: SignalsContext {
                        matched_barcode,
                        ..empty_context(track_count)
                    },
                }
            }
            Self::NotFoundAnywhere => IdentifyState::NotFoundAnywhere {
                context: empty_context(0),
            },
            Self::ManualOnly { track_count } => IdentifyState::ManualOnly {
                track_count,
                context: empty_context(track_count),
            },
            // A stored failure resumes with no matches: what one source found
            // before the other failed was never stored, so a resumed failure
            // offers the re-run rather than a partial list.
            Self::Failed {
                failures,
                track_count,
            } => IdentifyState::Failed {
                failures,
                track_count,
                matches: Vec::new(),
                library_statuses: Vec::new(),
                provenance: Vec::new(),
                context: empty_context(track_count),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::LibraryStatus;
    use crate::identify::state::{BarcodeProgress, DiscidProgress};
    use crate::import::MetadataSource;

    fn mk_result(release_id: &str) -> MetadataResult {
        MetadataResult {
            source: MetadataSource::MusicBrainz,
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
            source_group_id: Some("group-1".to_string()),
            source_tracks: None,
        }
    }

    fn mk_status(release_id: &str) -> LibraryStatus {
        LibraryStatus {
            release_id: release_id.to_string(),
            release_in_library: false,
            album_in_library: false,
            album_title: None,
            album_id: None,
        }
    }

    /// A bare context, standing in for whatever the reducer would have
    /// accumulated by this point — its contents don't matter to these tests,
    /// only that `Idle`/`Triangulating` carry one and still aren't terminal.
    fn mk_context(track_count: u32) -> SignalsContext {
        SignalsContext {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count },
            barcode_codes: vec![],
            had_barcode_source: false,
            catalogs: vec![],
            chosen_catalog: None,
            disc_excluded: false,
            barcode_excluded: false,
            discid_results: vec![],
            barcode_results: vec![],
            catalog_results: vec![],
            discid_failure: None,
            barcode_failures: Vec::new(),
            barcode_scan_failure: None,
            catalog_failures: Vec::new(),
            matched_barcode: None,
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
            library_statuses: vec![mk_status("rel-1")],
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

    /// A stored verdict stands back up with its matches, and with the barcode
    /// that found them: the barcode rows say which image each was read off, so
    /// without this pointer a resumed candidate could not tell which of several
    /// images the release was identified from.
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
            verdict.resume_state(&|result| mk_status(&result.release_id))
        else {
            panic!("a found verdict resumes as Found");
        };
        assert_eq!(context.matched_barcode.as_deref(), Some("5099969394522"));
        // The signal inputs themselves are not retained — a re-run re-extracts
        // them, and the resumed state draws no signals toolbar.
        assert!(context.barcode_codes.is_empty());
    }

    /// The reducer exposes a failed lookup directly, and verdict conversion
    /// preserves that terminal state.
    #[test]
    fn a_recorded_discid_failure_derives_and_stores_as_failed() {
        let mut context = mk_context(11);
        context.discid_failure = Some(crate::signals::LookupFailure::Provider { status: None });
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
    /// `discid_failure` and `barcode_failure` are `None` here, so this also
    /// stands as the positive case for a union-shaped `Found`.
    #[test]
    fn a_union_of_disagreeing_signals_stores_as_one_match_list() {
        let context = SignalsContext {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count: 9 },
            barcode_codes: vec![],
            had_barcode_source: true,
            catalogs: vec![],
            chosen_catalog: None,
            disc_excluded: false,
            barcode_excluded: false,
            discid_results: vec![(mk_result("rel-a"), mk_status("rel-a"))],
            barcode_results: vec![(mk_result("rel-b"), mk_status("rel-b"))],
            catalog_results: vec![],
            discid_failure: None,
            barcode_failures: Vec::new(),
            barcode_scan_failure: None,
            catalog_failures: Vec::new(),
            matched_barcode: Some("012345".to_string()),
            track_count: 9,
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
            disc_id: crate::signals::DiscIdSignal::Absent { track_count: 9 },
            barcode_codes: vec![],
            had_barcode_source: true,
            catalogs: vec![],
            chosen_catalog: None,
            disc_excluded: false,
            barcode_excluded: false,
            discid_results: vec![],
            barcode_results: vec![
                (mk_result("rel-1"), mk_status("rel-1")),
                (mk_result("rel-2"), mk_status("rel-2")),
            ],
            catalog_results: vec![],
            discid_failure: Some(crate::signals::LookupFailure::Network),
            barcode_failures: Vec::new(),
            barcode_scan_failure: None,
            catalog_failures: Vec::new(),
            matched_barcode: None,
            track_count: 9,
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
        context.discid_failure = Some(crate::signals::LookupFailure::Network);
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
        context.barcode_failures = vec![SourceFailure {
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
        context.had_barcode_source = true;
        context.barcode_results = vec![(mk_result("rel-mb"), mk_status("rel-mb"))];
        context.barcode_failures = vec![SourceFailure {
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
        context.had_barcode_source = true;
        context.barcode_results = vec![
            (mk_result("rel-mb"), mk_status("rel-mb")),
            (mk_result("rel-dg"), mk_status("rel-dg")),
        ];
        let state = crate::identify::state::re_derive_for_tests(context);
        assert!(matches!(state, IdentifyState::Found { .. }));
    }

    /// A provider failing with nothing from anyone leaves a failure and no
    /// matches at all.
    #[test]
    fn a_barcode_failure_with_no_results_carries_no_matches() {
        let mut context = mk_context(7);
        context.had_barcode_source = true;
        context.barcode_failures = vec![SourceFailure {
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
        context.chosen_catalog = Some("CAT-7".to_string());
        context.catalog_failures = vec![SourceFailure {
            source: MetadataSource::MusicBrainz,
            failure: crate::signals::LookupFailure::Network,
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

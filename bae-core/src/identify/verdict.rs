//! The identify pipeline's terminal outcome — what
//! [`crate::db::DbImportCandidateState`] persists, JSON-encoded into its
//! `verdict` column.
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
//! `IdentifyState` calling a state "terminal" only means nothing is in
//! flight — it says nothing about whether a lookup that fed it actually
//! answered. `TryFrom` checks that separately, over every variant: a `Found`
//! or `Conflict` can be reached with one signal's lookup having failed just
//! as easily as `NotFoundAnywhere` can, and in every case what failed is
//! evidence half-missing, not evidence against. Only a state whose context
//! carries no recorded failure on either signal converts; everything else —
//! including a state that would otherwise satisfy one of the four terminal
//! shapes — is handed back as `Err`, same as `Idle`/`Triangulating`. The
//! ordinary, common case (both lookups ran to completion, whatever they
//! found) is what actually reaches `Ok` and is what the sweep persists —
//! this gate narrows *which* terminal states are storable, it doesn't make
//! the terminal states themselves rarer.

use super::combine::{GroupKey, ResultProvenance};
use super::state::{IdentifyState, SignalsContext};
use crate::db::DbImportCandidateState;
use crate::import::search::MetadataResult;

/// The identify pipeline's outcome once it can no longer change without new
/// input from the user or a re-run. Built from [`IdentifyState`]'s four
/// terminal variants (`Found`, `Conflict`, `NotFoundAnywhere`, `ManualOnly`);
/// `Idle` and `Triangulating` have no terminal verdict, hence the fallible
/// conversion below.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TerminalVerdict {
    /// One or more results, all in one release group — what `combine` produced
    /// and the sidebar and Ready rule both work from directly.
    Found {
        matches: Vec<MetadataResult>,
        track_count: u32,
        group: GroupKey,
        /// Index-aligned with `matches`: which signal(s) produced or confirmed
        /// each one, for the sidebar's "matched on disc ID / barcode / text"
        /// evidence line.
        provenance: Vec<ResultProvenance>,
    },
    /// The signals disagreed: an empty intersection, or results spanning
    /// several groups. Both signals' own result sets are kept so a reader can
    /// show both sections without re-fetching.
    Conflict {
        discid_results: Vec<MetadataResult>,
        barcode_results: Vec<MetadataResult>,
        /// Which barcode produced `barcode_results`; `None` when nothing
        /// matched.
        matched_barcode: Option<String>,
        track_count: u32,
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
}

impl TryFrom<IdentifyState> for TerminalVerdict {
    /// The state handed back unchanged when it isn't terminal yet (`Idle` or
    /// `Triangulating`) — nothing to store, and the caller may want it back
    /// rather than have it silently discarded.
    type Error = IdentifyState;

    fn try_from(state: IdentifyState) -> Result<Self, Self::Error> {
        // Checked before the per-variant match, uniformly, because a lookup
        // failure can shape *any* terminal outcome, not just settle it empty:
        //
        // - `Found` from the surviving signal alone. `combine::combine_results`
        //   takes whichever side is non-empty when the other is — and a
        //   `Failed` lookup contributes zero results exactly like a clean
        //   `Skipped` does. A single result reached this way passes the
        //   queue's Ready rule ("exactly one result") on evidence where half
        //   the cross-check never ran.
        // - `Conflict` from a missing intersection partner. Two disc-ID
        //   results and one barcode result that would have intersected to one
        //   release instead reads as an unresolved multi-group disagreement
        //   because the disc-ID lookup that would have narrowed it failed.
        // - `NotFoundAnywhere` from both sides settling empty — a genuine
        //   double-empty search and a failed lookup are indistinguishable
        //   once `combine_results` sees two empty sets (see
        //   `identify::state::re_derive`).
        //
        // In every case the state machine still calls it "terminal" — nothing
        // is in flight — but "terminal" and "safe to persist forever" are not
        // the same question, and only the context's own failure fields can
        // answer the second one.
        if has_lookup_failure(&state) {
            return Err(state);
        }

        match state {
            // Both lookups completed (this arm's whole precondition, enforced
            // above) — a genuine result, safe to store.
            IdentifyState::Found {
                matches,
                track_count,
                group,
                provenance,
                // A live per-release check at read time, not a stored copy —
                // see the module doc.
                library_statuses: _,
                context: _,
            } => Ok(Self::Found {
                matches,
                track_count,
                group,
                provenance,
            }),

            IdentifyState::Conflict { context } => {
                let SignalsContext {
                    discid_results,
                    barcode_results,
                    matched_barcode,
                    track_count,
                    // The raw signal inputs, the user's exclusions, and the
                    // settled lookup failures (already consulted above) drive
                    // re-triangulation in the reducer; a stored verdict
                    // carries neither.
                    disc_id: _,
                    barcode_codes: _,
                    had_barcode_source: _,
                    catalogs: _,
                    excluded: _,
                    discid_failure: _,
                    barcode_failure: _,
                } = context;
                Ok(Self::Conflict {
                    discid_results: drop_library_status(discid_results),
                    barcode_results: drop_library_status(barcode_results),
                    matched_barcode,
                    track_count,
                })
            }

            IdentifyState::NotFoundAnywhere { context: _ } => Ok(Self::NotFoundAnywhere),

            IdentifyState::ManualOnly {
                track_count,
                context: _,
            } => Ok(Self::ManualOnly { track_count }),

            other @ (IdentifyState::Idle | IdentifyState::Triangulating { .. }) => Err(other),
        }
    }
}

/// The verdict a stored `import_candidate_state` row holds, or `None` when this
/// build can no longer read it.
///
/// One implementation because there is one rule — the sweep decides what to
/// re-identify by it and the sidebar decides what to render by it, and the two
/// disagreeing would leave a candidate the sweep considers answered rendering
/// as unanswered forever.
pub fn decode_stored(row: &DbImportCandidateState) -> Result<Option<TerminalVerdict>, String> {
    // No identify result at all: a candidate nobody has answered, either never
    // or not since a sheet binding changed what the folder is and cleared the
    // answer it had.
    let Some(identify) = row.identify.as_ref() else {
        return Ok(None);
    };
    serde_json::from_str::<TerminalVerdict>(&identify.verdict)
        .map(Some)
        .map_err(|error| {
            format!(
                "stored verdict for {} does not decode: {error}",
                row.content_hash
            )
        })
}

fn drop_library_status(
    pairs: Vec<(MetadataResult, crate::db::LibraryStatus)>,
) -> Vec<MetadataResult> {
    pairs.into_iter().map(|(result, _)| result).collect()
}

impl TerminalVerdict {
    /// Every release this verdict names — what a resumer checks live library
    /// status for before standing the state back up.
    pub fn named_releases(&self) -> Vec<&MetadataResult> {
        match self {
            Self::Found { matches, .. } => matches.iter().collect(),
            Self::Conflict {
                discid_results,
                barcode_results,
                ..
            } => discid_results
                .iter()
                .chain(barcode_results.iter())
                .collect(),
            Self::NotFoundAnywhere | Self::ManualOnly { .. } => Vec::new(),
        }
    }

    /// The identify state this stored verdict stands back up as — what opening
    /// an answered candidate shows without running anything.
    ///
    /// The matches, provenance, and per-signal result sections are the stored
    /// ones. The raw signal inputs (the disc ID value, barcode codes, catalog
    /// candidates) are deliberately not: they are local, recomputable facts a
    /// re-run re-extracts, and the verdict never stored them — so the context
    /// carries none, and a resumed state has no signals toolbar. `status_of`
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
            excluded: std::collections::HashSet::new(),
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            discid_failure: None,
            barcode_failure: None,
            matched_barcode: None,
            track_count,
        };
        match self {
            Self::Found {
                matches,
                track_count,
                group,
                provenance,
            } => {
                let library_statuses = matches.iter().map(status_of).collect();
                IdentifyState::Found {
                    matches,
                    library_statuses,
                    track_count,
                    group,
                    provenance,
                    context: empty_context(track_count),
                }
            }
            Self::Conflict {
                discid_results,
                barcode_results,
                matched_barcode,
                track_count,
            } => {
                let with_status = |results: Vec<MetadataResult>| {
                    results
                        .into_iter()
                        .map(|result| {
                            let status = status_of(&result);
                            (result, status)
                        })
                        .collect::<Vec<_>>()
                };
                let barcode_results = with_status(barcode_results);
                IdentifyState::Conflict {
                    context: SignalsContext {
                        // The stored disagreement is the two result sections;
                        // `had_barcode_source` is re-derived from them because
                        // barcode results can only have come from a barcode.
                        had_barcode_source: !barcode_results.is_empty()
                            || matched_barcode.is_some(),
                        discid_results: with_status(discid_results),
                        barcode_results,
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
        }
    }
}

/// Whether the state's carried context recorded a lookup failure on either
/// signal, checked uniformly across every terminal variant (`Idle` and
/// `Triangulating` carry no verdict either way, so they read as `false` here
/// — the earlier arms in `try_from` already reject them for that reason).
fn has_lookup_failure(state: &IdentifyState) -> bool {
    let context = match state {
        IdentifyState::Found { context, .. }
        | IdentifyState::Conflict { context }
        | IdentifyState::NotFoundAnywhere { context }
        | IdentifyState::ManualOnly { context, .. } => context,
        IdentifyState::Idle | IdentifyState::Triangulating { .. } => return false,
    };
    context.discid_failure.is_some() || context.barcode_failure.is_some()
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
            excluded: Default::default(),
            discid_results: vec![],
            barcode_results: vec![],
            discid_failure: None,
            barcode_failure: None,
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
            context: mk_context(0),
        })
        .is_err());
    }

    fn found_state() -> IdentifyState {
        IdentifyState::Found {
            matches: vec![mk_result("rel-1")],
            library_statuses: vec![mk_status("rel-1")],
            track_count: 11,
            group: GroupKey {
                source: MetadataSource::MusicBrainz,
                source_group_id: "group-1".to_string(),
            },
            provenance: vec![ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                matches_catalog: false,
            }],
            context: mk_context(11),
        }
    }

    /// `Found` keeps its matches, group, and provenance, and drops
    /// `library_statuses` — that's re-checked live, not stored. `mk_context`
    /// carries no recorded failure, so this also stands as the positive case:
    /// a `Found` reached with both lookups completing converts and is what
    /// the sweep actually persists in the ordinary case — the failure gate
    /// below narrows which terminal states are storable, it doesn't make
    /// `Ok` rare.
    #[test]
    fn found_drops_library_status_and_keeps_the_rest() {
        let verdict = TerminalVerdict::try_from(found_state()).unwrap();
        assert_eq!(
            verdict,
            TerminalVerdict::Found {
                matches: vec![mk_result("rel-1")],
                track_count: 11,
                group: GroupKey {
                    source: MetadataSource::MusicBrainz,
                    source_group_id: "group-1".to_string(),
                },
                provenance: vec![ResultProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    matches_catalog: false,
                }],
            }
        );
    }

    /// A `Found` reached with the barcode side surviving alone because the
    /// disc-ID lookup died — `combine::combine_results` takes the non-empty
    /// side whenever the other is empty, and a `Failed` lookup contributes
    /// zero results exactly like a clean `Skipped` one. This single result
    /// would satisfy the queue's Ready rule ("exactly one result") on
    /// evidence where the disc-ID cross-check never ran, so it must not
    /// convert. This is the regression a `Found`-is-always-safe assumption
    /// would slip past if the failure gate were narrowed back to
    /// `NotFoundAnywhere` alone.
    #[test]
    fn found_reached_with_a_recorded_discid_failure_is_rejected() {
        let mut state = found_state();
        let IdentifyState::Found { context, .. } = &mut state else {
            unreachable!()
        };
        context.discid_failure = Some(crate::signals::LookupFailure::Provider { status: None });
        let verdict = TerminalVerdict::try_from(state);
        assert!(
            verdict.is_err(),
            "a Found built from a failed lookup on the other signal must not store"
        );
    }

    /// `Conflict` keeps both signals' result sets (minus library status) and
    /// the matched barcode, dropping the raw inputs the reducer needs to
    /// re-triangulate but a stored verdict does not. Both `discid_failure` and
    /// `barcode_failure` are `None` here, so this also stands as the positive
    /// case for `Conflict`.
    #[test]
    fn conflict_drops_library_status_and_raw_inputs() {
        let context = SignalsContext {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count: 9 },
            barcode_codes: vec![],
            had_barcode_source: true,
            catalogs: vec![],
            excluded: Default::default(),
            discid_results: vec![(mk_result("rel-a"), mk_status("rel-a"))],
            barcode_results: vec![(mk_result("rel-b"), mk_status("rel-b"))],
            discid_failure: None,
            barcode_failure: None,
            matched_barcode: Some("012345".to_string()),
            track_count: 9,
        };
        let verdict = TerminalVerdict::try_from(IdentifyState::Conflict { context }).unwrap();
        assert_eq!(
            verdict,
            TerminalVerdict::Conflict {
                discid_results: vec![mk_result("rel-a")],
                barcode_results: vec![mk_result("rel-b")],
                matched_barcode: Some("012345".to_string()),
                track_count: 9,
            }
        );
    }

    /// A `Conflict` reached where the disc-ID lookup failed rather than
    /// genuinely disagreeing: had it succeeded with a release the barcode
    /// side also returned, the intersection would have narrowed to one
    /// release and this would have been `Found`, not `Conflict`. A missing
    /// intersection partner is exactly what can manufacture a conflict, so
    /// this must not convert either — this is the regression the deleted "a
    /// `Failed` signal never causes the conflict" reasoning would slip past.
    #[test]
    fn conflict_reached_with_a_recorded_discid_failure_is_rejected() {
        let context = SignalsContext {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count: 9 },
            barcode_codes: vec![],
            had_barcode_source: true,
            catalogs: vec![],
            excluded: Default::default(),
            discid_results: vec![],
            barcode_results: vec![
                (mk_result("rel-1"), mk_status("rel-1")),
                (mk_result("rel-2"), mk_status("rel-2")),
            ],
            discid_failure: Some(crate::signals::LookupFailure::Network),
            barcode_failure: None,
            matched_barcode: None,
            track_count: 9,
        };
        let verdict = TerminalVerdict::try_from(IdentifyState::Conflict { context });
        assert!(
            verdict.is_err(),
            "a Conflict shaped by a failed lookup on the other signal must not store"
        );
    }

    /// A genuinely empty search — both signals ran, neither failed, neither
    /// found anything — is a real answer and must convert.
    #[test]
    fn clean_not_found_anywhere_is_terminal() {
        let context = mk_context(7);
        let verdict = TerminalVerdict::try_from(IdentifyState::NotFoundAnywhere { context });
        assert!(matches!(verdict, Ok(TerminalVerdict::NotFoundAnywhere)));
    }

    /// A disc-ID lookup that failed over the network, with no barcode source to
    /// fall back on, settles the reducer into `NotFoundAnywhere` — see
    /// `identify::state::re_derive`, which cannot tell "searched, found
    /// nothing" apart from "never got an answer" by the time it combines. The
    /// conversion must catch this from `discid_failure` and refuse to store it
    /// — this is the regression a stored-failure-as-NotFoundAnywhere bug would
    /// slip past if this check were ever removed.
    #[test]
    fn not_found_anywhere_masking_a_discid_failure_is_rejected() {
        let mut context = mk_context(7);
        context.discid_failure = Some(crate::signals::LookupFailure::Network);
        let verdict = TerminalVerdict::try_from(IdentifyState::NotFoundAnywhere { context });
        assert!(
            verdict.is_err(),
            "a failed lookup must not store as not-found"
        );
    }

    /// Same for the barcode side.
    #[test]
    fn not_found_anywhere_masking_a_barcode_failure_is_rejected() {
        let mut context = mk_context(7);
        context.barcode_failure = Some(crate::signals::LookupFailure::Timeout);
        let verdict = TerminalVerdict::try_from(IdentifyState::NotFoundAnywhere { context });
        assert!(
            verdict.is_err(),
            "a failed lookup must not store as not-found"
        );
    }
}

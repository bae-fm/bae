//! Pure state machine for the identify pipeline.
//!
//! Triangulation: the disc-ID and barcode signals run in parallel, each reporting
//! progress live so the UI can render both side by side ("Computing disc-id ✓ ·
//! Looking up barcode 2 of 3…"). Once both settle, the reducer hands their results
//! to `combine` and lands on `Found`, `Conflict`, or `NotFoundAnywhere`.
//!
//! `step` takes a state and an event and returns the next state plus the side
//! effects for the service to run. No I/O, no async, nothing outside itself.

use super::combine::{
    catalog_matches_candidate, combine_results, CombineOutcome, GroupKey, ResultProvenance,
};
use super::toolbar::{SignalKind, SignalRole, SignalState, ToolbarSignal};
use crate::db::LibraryStatus;
use crate::import::search::MetadataResult;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue,
};
use std::collections::HashSet;

/// The disc-ID signal's progress. `Done` / `Skipped` / `Failed` are the settled
/// variants — combine fires once both signals are settled.
#[derive(Clone, Debug)]
pub enum DiscidProgress {
    Computing,
    LookingUp,
    Done {
        results: Vec<(MetadataResult, LibraryStatus)>,
        track_count: u32,
    },
    /// No LOG/CUE to derive a disc ID from. Still carries the local track count,
    /// so a barcode match can report "N tracks here vs. M on the matched release."
    Skipped {
        track_count: u32,
    },
    Failed {
        failure: LookupFailure,
        track_count: u32,
    },
}

impl DiscidProgress {
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            DiscidProgress::Done { .. }
                | DiscidProgress::Skipped { .. }
                | DiscidProgress::Failed { .. }
        )
    }

    pub fn results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        match self {
            DiscidProgress::Done { results, .. } => results.clone(),
            _ => Vec::new(),
        }
    }
}

/// The barcode signal's progress. `LookingUp` carries position + total so the UI
/// can show "Looking up barcode 2 of 3."
#[derive(Clone, Debug)]
pub enum BarcodeProgress {
    Scanning,
    LookingUp {
        current: String,
        position: u32,
        total: u32,
        remaining: Vec<String>,
    },
    Done {
        /// Which barcode produced the results; `None` when the queue drained
        /// without a match, or held no codes at all. Carried so the UI can name
        /// the value: "Barcode 5051961234567 matched 1 release."
        matched: Option<String>,
        results: Vec<(MetadataResult, LibraryStatus)>,
    },
    Failed {
        failure: LookupFailure,
    },
    /// No barcode source at all. Combine treats it like `Done { results: [] }`.
    Skipped,
}

impl BarcodeProgress {
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            BarcodeProgress::Done { .. }
                | BarcodeProgress::Failed { .. }
                | BarcodeProgress::Skipped
        )
    }

    pub fn results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        match self {
            BarcodeProgress::Done { results, .. } => results.clone(),
            _ => Vec::new(),
        }
    }

    /// Which barcode produced the matched results, if any.
    pub fn matched_barcode(&self) -> Option<&str> {
        match self {
            BarcodeProgress::Done { matched, .. } => matched.as_deref(),
            _ => None,
        }
    }
}

/// Which signal(s) backed a terminal `Found` state. Drives source-specific
/// banner copy in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentifySource {
    Discid,
    Barcode,
    /// Both signals contributed to the result via intersection.
    Combined,
}

/// A signal the user has toggled off. The disc ID and barcode are singletons; a
/// catalog candidate is named by its value, since there can be several.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExcludedSignal {
    Disc,
    Barcode,
    Catalog(String),
}

/// Everything a settled state needs to re-derive its outcome when the user toggles
/// a signal or re-runs — carried unchanged through every non-`Idle` state.
///
/// The inputs (`disc_id`, `barcode_codes`, `catalogs`) drive the toolbar badges and
/// the `ReRun` re-dispatch; the settled results (`discid_results`,
/// `barcode_results`, `matched_barcode`) let a toggle re-combine without
/// re-fetching. `excluded` survives every new snapshot.
#[derive(Clone, Debug)]
pub struct SignalsContext {
    /// The disc-ID signal (value + its inherent `DiscToc` origin).
    pub disc_id: DiscIdSignal,
    /// The barcode code payloads with their origins.
    pub barcode_codes: Vec<SourcedValue>,
    /// The catalog-number candidates with their origins.
    pub catalogs: Vec<SourcedValue>,
    /// Signals the user has toggled off.
    pub excluded: HashSet<ExcludedSignal>,
    /// The disc-ID lookup's results, once settled. Empty while looking up or
    /// when the disc-ID pipe was skipped / found nothing.
    pub discid_results: Vec<(MetadataResult, LibraryStatus)>,
    /// The barcode lookup's results, once settled.
    pub barcode_results: Vec<(MetadataResult, LibraryStatus)>,
    /// The barcode lookup failure, once settled by an error.
    pub barcode_failure: Option<LookupFailure>,
    /// Which barcode produced `barcode_results`. `None` until matched.
    pub matched_barcode: Option<String>,
    /// The candidate's local track count.
    pub track_count: u32,
}

impl SignalsContext {
    /// No signals known yet — the context on entry to `Triangulating`, before the
    /// first `SignalsUpdated`.
    fn empty() -> Self {
        Self {
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            barcode_codes: Vec::new(),
            catalogs: Vec::new(),
            excluded: HashSet::new(),
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            barcode_failure: None,
            matched_barcode: None,
            track_count: 0,
        }
    }

    /// Take the inputs from a new snapshot, keeping the user's exclusions. Results
    /// aren't touched — they're recorded as the lookups settle.
    fn refresh_inputs(&mut self, signals: &Signals) {
        self.disc_id = signals.disc_id.clone();
        self.barcode_codes = signals.barcode.codes().to_vec();
        self.catalogs = signals.text.catalogs().to_vec();
        self.track_count = signals.disc_id.track_count();
    }

    fn record_results(
        &mut self,
        discid_results: Vec<(MetadataResult, LibraryStatus)>,
        barcode_results: Vec<(MetadataResult, LibraryStatus)>,
        barcode_failure: Option<LookupFailure>,
        matched_barcode: Option<String>,
    ) {
        self.discid_results = discid_results;
        self.barcode_results = barcode_results;
        self.barcode_failure = barcode_failure;
        self.matched_barcode = matched_barcode;
    }

    /// The catalog candidates the user hasn't excluded — what `combine_results`
    /// filters by.
    fn active_catalogs(&self) -> Vec<SourcedValue> {
        self.catalogs
            .iter()
            .filter(|c| {
                !self
                    .excluded
                    .contains(&ExcludedSignal::Catalog(c.value.clone()))
            })
            .cloned()
            .collect()
    }

    /// The disc-ID results combine sees — empty when the signal is excluded.
    fn active_discid_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.excluded.contains(&ExcludedSignal::Disc) {
            Vec::new()
        } else {
            self.discid_results.clone()
        }
    }

    /// The barcode results combine sees — empty when the signal is excluded.
    fn active_barcode_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.excluded.contains(&ExcludedSignal::Barcode) {
            Vec::new()
        } else {
            self.barcode_results.clone()
        }
    }

    fn toggle(&mut self, signal: ExcludedSignal) {
        if !self.excluded.remove(&signal) {
            self.excluded.insert(signal);
        }
    }
}

/// One candidate's identify state.
///
/// Every state but `Idle` carries a [`SignalsContext`], so the toolbar projection
/// always has its signal values and the user can toggle or re-run from any settled
/// state.
#[derive(Clone, Debug)]
pub enum IdentifyState {
    Idle,

    /// Both signals in flight. Each progresses independently; `settle_if_ready`
    /// combines them into a terminal state once both are settled.
    Triangulating {
        discid: DiscidProgress,
        barcode: BarcodeProgress,
        context: SignalsContext,
    },

    Found {
        matches: Vec<MetadataResult>,
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// All matches share this group — UI can render
        /// "N pressings of one release group" copy.
        group: GroupKey,
        source: IdentifySource,
        /// Per-match provenance (which signals produced/confirmed each row),
        /// index-aligned with `matches` — drives the per-row signal badges.
        provenance: Vec<ResultProvenance>,
        context: SignalsContext,
    },

    /// The signals disagree: an empty intersection, or a combined set spanning
    /// several groups. The UI renders each signal's section from `context`'s
    /// settled results; toggling one off re-combines over the rest.
    Conflict {
        context: SignalsContext,
    },

    NotFoundAnywhere {
        context: SignalsContext,
    },

    /// Nothing to look up: no disc-ID artifact (LOG/CUE) and no barcode source
    /// (artwork, CUE `CATALOG`). Distinct from `NotFoundAnywhere`, where signals
    /// ran and matched nothing — here none ran, so the UI offers manual search.
    ManualOnly {
        track_count: u32,
        context: SignalsContext,
    },
}

impl IdentifyState {
    /// The carried context; `None` only for `Idle`. One access path shared by
    /// `step`, the toolbar projection, and the signal actions.
    fn context(&self) -> Option<&SignalsContext> {
        match self {
            IdentifyState::Triangulating { context, .. }
            | IdentifyState::Found { context, .. }
            | IdentifyState::Conflict { context, .. }
            | IdentifyState::NotFoundAnywhere { context }
            | IdentifyState::ManualOnly { context, .. } => Some(context),
            IdentifyState::Idle => None,
        }
    }

    /// Apply a signal toggle. Mid-`Triangulating` it is only recorded — the
    /// in-flight lookups keep running, and `re_derive` applies the exclusion when
    /// they settle. From a settled state, re-combine in place.
    fn with_toggled(self, signal: ExcludedSignal) -> IdentifyState {
        match self {
            IdentifyState::Triangulating {
                discid,
                barcode,
                mut context,
            } => {
                context.toggle(signal);
                IdentifyState::Triangulating {
                    discid,
                    barcode,
                    context,
                }
            }
            IdentifyState::Found { mut context, .. }
            | IdentifyState::Conflict { mut context }
            | IdentifyState::NotFoundAnywhere { mut context }
            | IdentifyState::ManualOnly { mut context, .. } => {
                context.toggle(signal);
                re_derive(context)
            }
            IdentifyState::Idle => IdentifyState::Idle,
        }
    }

    /// The flat badge list the UI renders: disc ID, barcode, then one per catalog
    /// candidate. `Idle` has no toolbar.
    pub fn toolbar(&self) -> Vec<ToolbarSignal> {
        let Some(context) = self.context() else {
            return Vec::new();
        };
        let mut signals = Vec::new();
        signals.push(self.disc_badge(context));
        signals.push(self.barcode_badge(context));
        for catalog in &context.catalogs {
            signals.push(self.catalog_badge(catalog, context));
        }
        signals
    }

    /// State comes from the live `DiscidProgress` while triangulating, else from
    /// the context's settled results.
    fn disc_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let state = match self {
            IdentifyState::Triangulating { discid, .. } => discid_progress_state(discid),
            _ => settled_identity_state(&context.disc_id, &context.discid_results),
        };
        ToolbarSignal {
            kind: SignalKind::DiscId,
            role: SignalRole::Identity,
            value: context.disc_id.discid_value(),
            origin: SignalOrigin::DiscToc,
            state,
            excluded: context.excluded.contains(&ExcludedSignal::Disc),
        }
    }

    /// The badge shows the matched code, or the first one when nothing has matched
    /// yet, and takes its origin from that code.
    fn barcode_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let code = context
            .matched_barcode
            .as_ref()
            .and_then(|v| context.barcode_codes.iter().find(|c| &c.value == v))
            .or_else(|| context.barcode_codes.first());
        let state = match self {
            IdentifyState::Triangulating { barcode, .. } => barcode_progress_state(barcode),
            _ => barcode_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Barcode,
            role: SignalRole::Identity,
            value: code.map(|c| c.value.clone()),
            origin: code.map_or(SignalOrigin::Artwork, |c| c.origin),
            state,
            excluded: context.excluded.contains(&ExcludedSignal::Barcode),
        }
    }

    /// How many of the matched releases carry this catno. Only `Found` has a
    /// settled match set, so every other state counts zero.
    fn catalog_badge(&self, catalog: &SourcedValue, context: &SignalsContext) -> ToolbarSignal {
        let count = match self {
            IdentifyState::Found { matches, .. } => matches
                .iter()
                .filter(|m| catalog_matches_candidate(m.catalog_number.as_deref(), catalog))
                .count() as u32,
            _ => 0,
        };
        ToolbarSignal {
            kind: SignalKind::Catalog,
            role: SignalRole::Filter,
            value: Some(catalog.value.clone()),
            origin: catalog.origin,
            state: SignalState::Confirms { count },
            excluded: context
                .excluded
                .contains(&ExcludedSignal::Catalog(catalog.value.clone())),
        }
    }
}

fn discid_progress_state(progress: &DiscidProgress) -> SignalState {
    match progress {
        DiscidProgress::Computing | DiscidProgress::LookingUp => SignalState::LookingUp,
        DiscidProgress::Done { results, .. } => found_or_no_match(results.len() as u32),
        DiscidProgress::Skipped { .. } => SignalState::Skipped,
        DiscidProgress::Failed { failure, .. } => SignalState::Failed {
            failure: failure.clone(),
        },
    }
}

fn barcode_progress_state(progress: &BarcodeProgress) -> SignalState {
    match progress {
        BarcodeProgress::Scanning | BarcodeProgress::LookingUp { .. } => SignalState::LookingUp,
        BarcodeProgress::Done { results, .. } => found_or_no_match(results.len() as u32),
        BarcodeProgress::Failed { failure } => SignalState::Failed {
            failure: failure.clone(),
        },
        BarcodeProgress::Skipped => SignalState::Skipped,
    }
}

/// The disc-ID badge once the pipeline has left `Triangulating`: read off the
/// signal itself plus its recorded results.
fn settled_identity_state(
    disc_id: &DiscIdSignal,
    results: &[(MetadataResult, LibraryStatus)],
) -> SignalState {
    match disc_id {
        DiscIdSignal::Absent { .. } => SignalState::Skipped,
        DiscIdSignal::Failed { failure, .. } => SignalState::Failed {
            failure: failure.clone(),
        },
        DiscIdSignal::Computed { .. } => found_or_no_match(results.len() as u32),
    }
}

fn barcode_settled_state(context: &SignalsContext) -> SignalState {
    if let Some(failure) = &context.barcode_failure {
        SignalState::Failed {
            failure: failure.clone(),
        }
    } else if context.barcode_codes.is_empty() {
        SignalState::Skipped
    } else {
        found_or_no_match(context.barcode_results.len() as u32)
    }
}

fn found_or_no_match(count: u32) -> SignalState {
    if count == 0 {
        SignalState::NoMatch
    } else {
        SignalState::Found { count }
    }
}

/// What feeds the reducer: the external triggers, plus the completions of the
/// lookup effects the service ran on the previous step.
#[derive(Debug, Clone)]
pub enum IdentifyEvent {
    /// Begin. Enters `Triangulating` and waits for the first `SignalsUpdated` —
    /// extraction owns scanning and OCR, not the reducer.
    Started,
    Cancelled,

    /// The candidate's latest signals. The reducer dispatches the disc-ID lookup
    /// once the disc ID is `Computed` and the barcode lookups once the codes have
    /// `Settled`, and refreshes the catalog filter from every snapshot. Snapshots
    /// stream, so this is idempotent: each signal's progress guards its own lookup
    /// against being dispatched twice.
    SignalsUpdated {
        signals: Signals,
    },

    // ── DiscID lookup completion ────────────────────────────────────
    DiscidLookupCompleted {
        results: Vec<(MetadataResult, LibraryStatus)>,
        track_count: u32,
    },
    DiscidLookupFailed {
        failure: LookupFailure,
        track_count: u32,
    },

    // ── Barcode lookup completion ───────────────────────────────────
    BarcodeLookupMatched {
        for_barcode: String,
        results: Vec<(MetadataResult, LibraryStatus)>,
    },
    BarcodeLookupMissed {
        for_barcode: String,
    },
    BarcodeLookupFailed {
        for_barcode: String,
        failure: LookupFailure,
    },

    /// The user included or excluded a signal. The reducer flips it in
    /// `context.excluded` and re-combines over the rest.
    SignalToggled {
        signal: ExcludedSignal,
    },

    /// The user asked to replay the lookups. The reducer resets to `Triangulating`
    /// and re-dispatches from the retained signals, keeping the exclusions.
    ReRun,
}

/// The side effects the service performs — the two network lookups. Each one
/// finishing feeds an `IdentifyEvent` back into `step`. Scanning, OCR, and
/// disc-ID derivation belong to the extraction service, not here.
#[derive(Debug, Clone)]
pub enum Effect {
    LookupDiscid { disc_id: String, track_count: u32 },
    LookupBarcode { barcode: String },
}

/// Drive the state machine one step. `Cancelled` always resets to `Idle`.
pub fn step(state: IdentifyState, event: IdentifyEvent) -> (IdentifyState, Vec<Effect>) {
    if matches!(event, IdentifyEvent::Cancelled) {
        return (IdentifyState::Idle, vec![]);
    }

    // Toggle and re-run both act on the carried `SignalsContext`, so they're
    // handled once here rather than per state. `ReRun` is ignored during
    // triangulation — those lookups are already in flight.
    match &event {
        IdentifyEvent::SignalToggled { signal } if state.context().is_some() => {
            return (state.with_toggled(signal.clone()), vec![]);
        }
        IdentifyEvent::ReRun if !matches!(state, IdentifyState::Triangulating { .. }) => {
            if let Some(context) = state.context() {
                return rerun(context.clone());
            }
        }
        _ => {}
    }

    match (state, event) {
        (IdentifyState::Idle, IdentifyEvent::Started) => (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing,
                barcode: BarcodeProgress::Scanning,
                context: SignalsContext::empty(),
            },
            vec![],
        ),

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                context,
            },
            IdentifyEvent::SignalsUpdated { signals },
        ) => apply_signals(discid, barcode, context, signals),

        // ── DiscID lookup completion ───────────────────────────────────
        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing | DiscidProgress::LookingUp,
                barcode,
                context,
            },
            IdentifyEvent::DiscidLookupFailed {
                failure,
                track_count,
            },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Failed {
                failure,
                track_count,
            },
            barcode,
            context,
        }),

        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::LookingUp,
                barcode,
                context,
            },
            IdentifyEvent::DiscidLookupCompleted {
                results,
                track_count,
            },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Done {
                results,
                track_count,
            },
            barcode,
            context,
        }),

        // ── Barcode lookup iteration ───────────────────────────────────
        // First match wins. The `for_barcode == current` guard on each arm drops
        // a stale response from a code the queue has already moved past.
        (
            IdentifyState::Triangulating {
                discid,
                barcode:
                    BarcodeProgress::LookingUp {
                        current,
                        position: _,
                        total: _,
                        remaining: _,
                    },
                context,
            },
            IdentifyEvent::BarcodeLookupMatched {
                for_barcode,
                results,
            },
        ) if for_barcode == current => settle_if_ready(IdentifyState::Triangulating {
            discid,
            barcode: BarcodeProgress::Done {
                matched: Some(for_barcode),
                results,
            },
            context,
        }),

        // A miss advances the queue: the barcode signal settles empty only once
        // every code has been tried.
        (
            IdentifyState::Triangulating {
                discid,
                barcode:
                    BarcodeProgress::LookingUp {
                        current,
                        position,
                        total,
                        mut remaining,
                    },
                context,
            },
            IdentifyEvent::BarcodeLookupMissed { for_barcode },
        ) if for_barcode == current => {
            if remaining.is_empty() {
                settle_if_ready(IdentifyState::Triangulating {
                    discid,
                    barcode: BarcodeProgress::Done {
                        matched: None,
                        results: vec![],
                    },
                    context,
                })
            } else {
                let next = remaining.remove(0);
                (
                    IdentifyState::Triangulating {
                        discid,
                        barcode: BarcodeProgress::LookingUp {
                            current: next.clone(),
                            position: position + 1,
                            total,
                            remaining,
                        },
                        context,
                    },
                    vec![Effect::LookupBarcode { barcode: next }],
                )
            }
        }

        (
            IdentifyState::Triangulating {
                discid,
                barcode:
                    BarcodeProgress::LookingUp {
                        current,
                        position: _,
                        total: _,
                        remaining: _,
                    },
                context,
            },
            IdentifyEvent::BarcodeLookupFailed {
                for_barcode,
                failure,
            },
        ) if for_barcode == current => settle_if_ready(IdentifyState::Triangulating {
            discid,
            barcode: BarcodeProgress::Failed { failure },
            context,
        }),

        // An event Triangulating doesn't act on — a stale barcode response, say.
        (state @ IdentifyState::Triangulating { .. }, _) => (state, vec![]),

        // Any other (state, event) pair leaves the state alone.
        (state, _) => (state, vec![]),
    }
}

/// Fold the latest snapshot into the two signals.
///
/// Idempotent under streaming, because each signal's own progress guards it: the
/// disc ID dispatches `LookupDiscid` only while still `Computing`, and the barcode
/// queue is seeded only while still `Scanning` *and* once the codes have `Settled`
/// — so first-match-wins iterates over a complete, stable queue. The catalog filter
/// refreshes from every snapshot.
fn apply_signals(
    discid: DiscidProgress,
    barcode: BarcodeProgress,
    mut context: SignalsContext,
    signals: Signals,
) -> (IdentifyState, Vec<Effect>) {
    let mut effects = Vec::new();
    context.refresh_inputs(&signals);

    let discid = match (discid, &signals.disc_id) {
        (DiscidProgress::Computing, signal) => start_discid_progress(signal, &mut effects),
        // Past Computing: the lookup is in flight or settled.
        (discid, _) => discid,
    };

    let barcode = match (barcode, &signals.barcode) {
        (BarcodeProgress::Scanning, BarcodeSignal::Settled { codes }) => start_barcode_progress(
            codes,
            BarcodeProgress::Done {
                matched: None,
                results: Vec::new(),
            },
            &mut effects,
        ),
        (BarcodeProgress::Scanning, BarcodeSignal::Absent) => BarcodeProgress::Skipped,
        (BarcodeProgress::Scanning, BarcodeSignal::Failed { failure, .. }) => {
            BarcodeProgress::Failed {
                failure: failure.clone(),
            }
        }
        // Codes not settled yet, or already iterating/settled.
        (barcode, _) => barcode,
    };

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        context,
    };

    // A dispatched lookup means nothing can have settled this step.
    if effects.is_empty() {
        settle_if_ready(next)
    } else {
        (next, effects)
    }
}

fn start_discid_progress(signal: &DiscIdSignal, effects: &mut Vec<Effect>) -> DiscidProgress {
    match signal {
        DiscIdSignal::Computed {
            disc_id,
            track_count,
        } => {
            effects.push(Effect::LookupDiscid {
                disc_id: disc_id.clone(),
                track_count: *track_count,
            });
            DiscidProgress::LookingUp
        }
        DiscIdSignal::Absent { track_count } => DiscidProgress::Skipped {
            track_count: *track_count,
        },
        DiscIdSignal::Failed {
            failure,
            track_count,
        } => DiscidProgress::Failed {
            failure: failure.clone(),
            track_count: *track_count,
        },
    }
}

fn start_barcode_progress(
    codes: &[SourcedValue],
    empty_progress: BarcodeProgress,
    effects: &mut Vec<Effect>,
) -> BarcodeProgress {
    if codes.is_empty() {
        return empty_progress;
    }

    let mut values: Vec<String> = codes.iter().map(|c| c.value.clone()).collect();
    let total = values.len() as u32;
    let current = values.remove(0);
    effects.push(Effect::LookupBarcode {
        barcode: current.clone(),
    });
    BarcodeProgress::LookingUp {
        current,
        position: 1,
        total,
        remaining: values,
    }
}

/// Once both signals have settled, record their results into the context and
/// combine into a terminal state. Until then, stay in `Triangulating`.
fn settle_if_ready(state: IdentifyState) -> (IdentifyState, Vec<Effect>) {
    let IdentifyState::Triangulating {
        discid,
        barcode,
        mut context,
    } = state
    else {
        return (state, vec![]);
    };
    if !discid.is_settled() || !barcode.is_settled() {
        return (
            IdentifyState::Triangulating {
                discid,
                barcode,
                context,
            },
            vec![],
        );
    }

    let track_count = settled_track_count(&discid);
    context.track_count = track_count;
    let barcode_failure = match &barcode {
        BarcodeProgress::Failed { failure } => Some(failure.clone()),
        _ => None,
    };
    context.record_results(
        discid.results(),
        barcode.results(),
        barcode_failure,
        barcode.matched_barcode().map(str::to_string),
    );

    // Neither signal had anything to run. Offer manual search rather than claim
    // we looked and found nothing.
    if matches!(discid, DiscidProgress::Skipped { .. })
        && matches!(barcode, BarcodeProgress::Skipped)
    {
        return (
            IdentifyState::ManualOnly {
                track_count,
                context,
            },
            vec![],
        );
    }

    (re_derive(context), vec![])
}

/// Re-combine over the non-excluded signals and lift the outcome into a state. The
/// one combine path: triangulation settle, signal toggle, and re-run completion all
/// arrive here once the results are in the context.
///
/// Both sides empty — because the lookups found nothing, or because the user
/// excluded the signals that did — lands on `NotFoundAnywhere`.
fn re_derive(context: SignalsContext) -> IdentifyState {
    let discid_results = context.active_discid_results();
    let barcode_results = context.active_barcode_results();

    let discid_had_results = !discid_results.is_empty();
    let barcode_had_results = !barcode_results.is_empty();
    let outcome = combine_results(discid_results, barcode_results, &context.active_catalogs());
    let track_count = context.track_count;
    match outcome {
        CombineOutcome::Found {
            matches,
            library_statuses,
            group,
            provenance,
            ..
        } => IdentifyState::Found {
            matches,
            library_statuses,
            track_count,
            group,
            source: source_for_found(discid_had_results, barcode_had_results),
            provenance,
            context,
        },
        CombineOutcome::Conflict { .. } => IdentifyState::Conflict { context },
        CombineOutcome::NotFoundAnywhere => IdentifyState::NotFoundAnywhere { context },
    }
}

/// Reset to `Triangulating` and re-dispatch the lookups from the retained signals,
/// keeping the user's exclusions.
fn rerun(mut context: SignalsContext) -> (IdentifyState, Vec<Effect>) {
    // The prior results go; the new lookups replace them as they land.
    context.discid_results = Vec::new();
    context.barcode_results = Vec::new();
    context.barcode_failure = None;
    context.matched_barcode = None;

    let mut effects = Vec::new();

    let discid = start_discid_progress(&context.disc_id, &mut effects);
    let barcode = start_barcode_progress(
        &context.barcode_codes,
        BarcodeProgress::Skipped,
        &mut effects,
    );

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        context,
    };
    if effects.is_empty() {
        settle_if_ready(next)
    } else {
        (next, effects)
    }
}

/// The track count is whatever the disc-ID signal reported — every one of its
/// settled variants carries the local count, whether or not a disc ID was derived.
fn settled_track_count(discid: &DiscidProgress) -> u32 {
    match discid {
        DiscidProgress::Done { track_count, .. } => *track_count,
        DiscidProgress::Skipped { track_count } => *track_count,
        DiscidProgress::Failed { track_count, .. } => *track_count,
        DiscidProgress::Computing | DiscidProgress::LookingUp => 0,
    }
}

fn source_for_found(discid_had_results: bool, barcode_had_results: bool) -> IdentifySource {
    match (discid_had_results, barcode_had_results) {
        (true, true) => IdentifySource::Combined,
        (true, false) => IdentifySource::Discid,
        (false, true) => IdentifySource::Barcode,
        // `Found` carries at least one result by construction, so reaching this is
        // a logic bug upstream.
        (false, false) => unreachable!("Found with no results from either signal"),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

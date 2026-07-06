//! Pure state machine for the identify pipeline.
//!
//! Triangulation: disc-ID and barcode signals run in parallel. Per-signal
//! progress is reported live so the UI can render both pipes side by side
//! ("Computing disc-id ✓ · Looking up barcode 2 of 3..."). Once both
//! signals settle, the reducer hands their results to `combine` and lands
//! on a terminal state — `Found`, `Conflict`, or `NotFoundAnywhere`.
//!
//! `step` takes the current state plus an event and returns the next state
//! and a list of side effects for the service to perform. No I/O, no async,
//! no references to the outside world.

use super::combine::{
    combine_results, normalize_catalog, CombineOutcome, GroupKey, ResultProvenance,
};
use super::toolbar::{SignalKind, SignalRole, SignalState, ToolbarSignal};
use crate::db::LibraryStatus;
use crate::import::search::MetadataResult;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue,
};
use std::collections::HashSet;

/// Per-signal progress for disc-ID. The UI reads this to render
/// "Computing disc-id…", "Looking up disc-id…", or a final
/// "Disc-id: N matches / no match / failed / skipped."
///
/// Settled variants (`Done`, `Skipped`, `Failed`) tell the reducer this
/// signal pipe has finished — combine fires once both pipes are settled.
#[derive(Clone, Debug)]
pub enum DiscidProgress {
    Computing,
    LookingUp,
    Done {
        results: Vec<(MetadataResult, LibraryStatus)>,
        track_count: u32,
    },
    /// No disc-ID artifacts (LOG/CUE) available. Carries the local track
    /// count so barcode lookups can still report "N tracks here vs. M on
    /// the matched release."
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

/// Per-signal progress for barcode lookup. `LookingUp` carries position +
/// total so the UI can show "Looking up barcode 2 of 3."
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
        /// Which barcode produced the results. `None` when the queue
        /// drained without a match (or scanning yielded zero codes).
        /// Carried so the conflict surface can identify the value to the
        /// user — e.g., "Barcode 5051961234567 matched 1 release."
        matched: Option<String>,
        results: Vec<(MetadataResult, LibraryStatus)>,
    },
    Failed {
        failure: LookupFailure,
    },
    /// No artwork to scan. The signal pipe is "absent" rather than empty —
    /// combine treats it the same as `Done { results: [] }`.
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

    /// Which barcode produced the matched results, if any. `None` for
    /// other states or for `Done` queues that drained without a hit.
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

/// A signal the user has toggled off — excluded from triangulation. The disc
/// ID and barcode are singletons; a catalog candidate is named by its value
/// (there can be several). The reducer re-combines with the excluded signals
/// masked.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExcludedSignal {
    Disc,
    Barcode,
    Catalog(String),
}

/// Everything a post-settle state needs to re-derive its outcome when the user
/// toggles a signal or re-runs the lookups — carried unchanged through every
/// non-`Idle` state.
///
/// The *inputs* (`disc_id`, `barcode_codes`, `catalogs`) drive the toolbar
/// badge values and the `ReRun` re-dispatch. The settled per-signal *results*
/// (`discid_results`, `barcode_results`, `matched_barcode`) let `SignalToggled`
/// re-combine without re-fetching. `excluded` is preserved across snapshots.
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
    /// Seed an empty context — no signals known yet. Used on entry to
    /// `Triangulating` before the first `SignalsUpdated`.
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

    /// Refresh the signal *inputs* from a new `Signals` snapshot, preserving
    /// the user's exclusions. Results are not touched here — they're recorded
    /// as the lookups settle.
    fn refresh_inputs(&mut self, signals: &Signals) {
        self.disc_id = signals.disc_id.clone();
        self.barcode_codes = signals.barcode.codes().to_vec();
        self.catalogs = signals.text.catalogs().to_vec();
        self.track_count = signals.disc_id.track_count();
    }

    /// Record the settled per-signal results, for re-combine on toggle.
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

    /// Catalog candidates minus the ones the user excluded — the filter
    /// `combine_results` applies.
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

    /// The disc-ID results to feed combine — empty when the disc signal is
    /// excluded.
    fn active_discid_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.excluded.contains(&ExcludedSignal::Disc) {
            Vec::new()
        } else {
            self.discid_results.clone()
        }
    }

    /// The barcode results to feed combine — empty when the barcode signal is
    /// excluded.
    fn active_barcode_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.excluded.contains(&ExcludedSignal::Barcode) {
            Vec::new()
        } else {
            self.barcode_results.clone()
        }
    }

    /// Flip a signal's exclusion: drop it if present, add it otherwise.
    fn toggle(&mut self, signal: ExcludedSignal) {
        if !self.excluded.remove(&signal) {
            self.excluded.insert(signal);
        }
    }
}

/// The identify pipeline's current state for a single candidate.
///
/// Every state after `Idle` carries a [`SignalsContext`] — the inputs and
/// settled results behind the toolbar — so the user can toggle a signal or
/// re-run the lookups from any post-settle state, and the toolbar projection
/// has the signal values it needs in every state.
#[derive(Clone, Debug)]
pub enum IdentifyState {
    Idle,

    /// Both signals running in parallel. Each progresses independently and
    /// emits its own events; the reducer transitions to a terminal state
    /// once both `discid` and `barcode` are settled (combine fires inside
    /// `settle_if_ready`).
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

    /// Signals disagree: empty intersection, or combined set spans multiple
    /// groups. UI renders the per-signal sections (from `context`'s settled
    /// results). The user can toggle a signal off — the reducer re-combines
    /// over the surviving signals.
    Conflict {
        context: SignalsContext,
    },

    NotFoundAnywhere {
        context: SignalsContext,
    },

    /// Nothing to look up: neither a disc-ID artifact (LOG/CUE) nor a barcode
    /// source (artwork, CUE `CATALOG`) was present. Distinct from
    /// `NotFoundAnywhere`, where signals ran and matched nothing — here there
    /// was no signal to run, so the UI offers manual search. A folder with no
    /// signals lands here.
    ManualOnly {
        track_count: u32,
        context: SignalsContext,
    },
}

impl IdentifyState {
    /// The carried `SignalsContext` for the states that own one.
    /// `Idle` has no context. Lets `step`, toolbar projection, and signal
    /// actions share one access path.
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

    /// Apply a signal toggle. During `Triangulating` the exclusion is only
    /// recorded — the in-flight lookups keep running and the exclusion is
    /// applied when they settle (via `active_*` in `re_derive`). From a settled
    /// state, re-combine over the surviving signals in place.
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

    /// Project the current state into the flat list of toolbar badges the UI
    /// renders: the disc-ID badge, the barcode badge, then one badge per
    /// catalog candidate. Each carries its value, origin, live state, and
    /// whether the user has excluded it. `Idle` has no toolbar.
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

    /// The disc-ID badge. Its value is the disc-ID hash; its state comes from
    /// the live `DiscidProgress` while triangulating, else from the context's
    /// settled disc-ID results.
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

    /// The barcode badge. Its value is the matched code (or the first code
    /// when none matched yet); its origin comes from that code; its state
    /// from the live `BarcodeProgress` while triangulating, else the context.
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

    /// One catalog filter badge. Its state confirms how many of the currently
    /// matched releases carry this catno (only `Found` has a settled matched
    /// set; elsewhere the count is 0 — the candidate hasn't pinpointed
    /// anything yet).
    fn catalog_badge(&self, catalog: &SourcedValue, context: &SignalsContext) -> ToolbarSignal {
        let count = match self {
            IdentifyState::Found { matches, .. } => matches
                .iter()
                .filter(|m| catalog_matches_value(m.catalog_number.as_deref(), catalog))
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

/// Live disc-ID badge state from its in-flight progress.
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

/// Live barcode badge state from its in-flight progress.
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

/// Settled disc-ID badge state from the disc-ID signal + its recorded results
/// (used once the pipeline has left `Triangulating`).
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

/// Settled barcode badge state from the context's recorded barcode results.
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

/// Whether a release's catalog number matches a candidate's value, after
/// `normalize_catalog`. The per-badge confirm test.
fn catalog_matches_value(catalog_number: Option<&str>, candidate: &SourcedValue) -> bool {
    candidate.origin.can_confirm_catalog()
        && catalog_number
            .is_some_and(|c| normalize_catalog(c) == normalize_catalog(&candidate.value))
}

/// Events feeding the reducer. External triggers (`Started`, `Cancelled`,
/// `SignalsUpdated`) plus completions of the lookup effects the service ran on
/// the previous step.
#[derive(Debug, Clone)]
pub enum IdentifyEvent {
    /// Begin identifying. Enters `Triangulating` and waits for the first
    /// `SignalsUpdated` — extraction owns scanning/OCR, not the reducer.
    Started,
    Cancelled,

    /// Latest extracted signals for this candidate. The reducer dispatches the
    /// disc-ID lookup (once the disc ID is `Computed`) and the barcode lookups
    /// (once the barcode codes have `Settled`), and refreshes the catalog
    /// filter from the text signal. Snapshots stream, so this is idempotent:
    /// each signal drives its lookup exactly once (guarded by the pipe's
    /// current progress).
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

    /// User toggled a signal in the toolbar — included or excluded it from
    /// triangulation. The reducer flips the signal in `context.excluded` and
    /// re-combines over the non-excluded signals, landing on whichever
    /// terminal state results. Valid in every post-settle state and
    /// `Conflict`.
    SignalToggled {
        signal: ExcludedSignal,
    },

    /// User asked to replay the lookups. The reducer resets to `Triangulating`
    /// and re-dispatches the disc-ID / barcode lookups from the retained
    /// signals, preserving the user's exclusions. Valid in every post-settle
    /// state and `Conflict`.
    ReRun,
}

/// Side effects the service must perform — the two network lookups. Each
/// finished effect feeds an `IdentifyEvent` back into `step`. Scanning, OCR,
/// and disc-ID derivation belong to the extraction service, not here.
#[derive(Debug, Clone)]
pub enum Effect {
    LookupDiscid { disc_id: String, track_count: u32 },
    LookupBarcode { barcode: String },
}

/// Drive the state machine one step.
///
/// Terminal states ignore further events except `Cancelled`, which always
/// resets to `Idle`.
pub fn step(state: IdentifyState, event: IdentifyEvent) -> (IdentifyState, Vec<Effect>) {
    if matches!(event, IdentifyEvent::Cancelled) {
        return (IdentifyState::Idle, vec![]);
    }

    // ── Toggle a signal or re-run ──────────────────────────────────────
    // Both act on the carried `SignalsContext`, uniformly across every state
    // that owns one. A toggle during triangulation only records the exclusion
    // (the lookups keep running; it's applied when they settle); from a settled
    // state it re-combines in place. `ReRun` is ignored mid-triangulation — the
    // lookups are already in flight.
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
        // ── Started: enter triangulation and await the first signals ────
        // Extraction owns scanning/OCR; the reducer reacts to `SignalsUpdated`.
        (IdentifyState::Idle, IdentifyEvent::Started) => (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing,
                barcode: BarcodeProgress::Scanning,
                context: SignalsContext::empty(),
            },
            vec![],
        ),

        // ── Signals snapshot drives the lookups + catalog refinement ────
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

        // ── Barcode lookup iteration (first match wins) ────────────────
        // First match wins — the barcode signal is "Done with N results."
        // Stale responses (for_barcode != current) are dropped via the
        // guard.
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

        // A miss advances the queue. The barcode phase only settles empty
        // once every candidate code has been tried.
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

        // Unhandled event in Triangulating (stale barcode response, or an
        // event this state doesn't act on) — keep state.
        (state @ IdentifyState::Triangulating { .. }, _) => (state, vec![]),

        // Unknown (state, event) pair — state unchanged, no effects.
        (state, _) => (state, vec![]),
    }
}

/// Fold the latest `Signals` snapshot into the disc-ID and barcode pipes.
///
/// Streaming-idempotent: each pipe acts on its signal exactly once, guarded by
/// its current progress. The disc ID drives `LookupDiscid` only while the pipe
/// is still `Computing`; the barcode queue is seeded only while still
/// `Scanning` and only once the codes have `Settled` (so the first-match-wins
/// iteration runs over a stable, complete queue, exactly as before). The
/// catalog filter is refreshed from every snapshot. If a snapshot dispatches a
/// lookup the pipeline can't settle yet; otherwise we check for settle.
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
        // Already past Computing — the disc-ID lookup is in flight or settled.
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
        // Still scanning (codes not settled), or already iterating/settled.
        (barcode, _) => barcode,
    };

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        context,
    };

    // A dispatched lookup means the pipeline can't have settled this step;
    // otherwise both pipes may now be settled — check.
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

/// Check whether both signals have settled. If so, record the per-signal
/// results into the context and combine into a terminal state. Otherwise stay
/// in `Triangulating`.
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

    // No signal ran at all — no LOG/CUE for disc-ID, no artwork/CATALOG for
    // barcode. Offer manual search rather than reporting "found nothing."
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

/// Re-combine over the context's non-excluded signals and lift the outcome
/// into the resulting state. The single combine path for triangulation
/// settle, signal-toggle, and re-run-completion — they all hand off here once
/// the per-signal results are recorded in the context.
///
/// Both-empty (whether the lookups found nothing, or the user excluded the
/// signals that did) lands on `NotFoundAnywhere` via `combine_results`.
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

/// Reset to `Triangulating` and re-dispatch the lookups from the retained
/// signals, preserving the user's exclusions.
fn rerun(mut context: SignalsContext) -> (IdentifyState, Vec<Effect>) {
    // A fresh run discards prior results; they're recomputed as lookups land.
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

/// Track count is whichever the disc-ID phase reported. Disc-ID's resolution
/// always carries the local count; barcode reports zero if disc-ID was
/// `Skipped` without one, but in practice every entry path sets it.
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
        // Found requires at least one result by construction. The only way
        // here is a logic bug upstream.
        (false, false) => unreachable!("Found with no results from either signal"),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

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
use crate::signals::{BarcodeSignal, DiscIdSignal, SignalOrigin, Signals, SourcedValue};
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
        message: String,
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
    /// No artwork to scan. The signal pipe is "absent" rather than empty —
    /// combine treats it the same as `Done { results: [] }`.
    Skipped,
}

impl BarcodeProgress {
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            BarcodeProgress::Done { .. } | BarcodeProgress::Skipped
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
        matched_barcode: Option<String>,
    ) {
        self.discid_results = discid_results;
        self.barcode_results = barcode_results;
        self.matched_barcode = matched_barcode;
    }

    /// Catalog-candidate values minus the ones the user excluded — the filter
    /// `combine_results` applies.
    fn active_catalog_values(&self) -> Vec<String> {
        self.catalogs
            .iter()
            .filter(|c| {
                !self
                    .excluded
                    .contains(&ExcludedSignal::Catalog(c.value.clone()))
            })
            .map(|c| c.value.clone())
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
    /// The carried `SignalsContext`, cloned, for the states that own one.
    /// `Idle` has no context. Lets `step` handle `SignalToggled` / `ReRun`
    /// uniformly without matching each variant.
    fn context_cloned(&self) -> Option<SignalsContext> {
        match self {
            IdentifyState::Triangulating { context, .. }
            | IdentifyState::Found { context, .. }
            | IdentifyState::Conflict { context, .. }
            | IdentifyState::NotFoundAnywhere { context }
            | IdentifyState::ManualOnly { context, .. } => Some(context.clone()),
            IdentifyState::Idle => None,
        }
    }

    /// Whether this state carries a [`SignalsContext`] — everything but `Idle`.
    /// Gates the toggle / re-run handling without a clone.
    fn has_context(&self) -> bool {
        !matches!(self, IdentifyState::Idle)
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
            // Settled states re-combine immediately; `Idle`/`Error` (no
            // context) are unreachable here behind `has_context` but stay total.
            other => match other.context_cloned() {
                Some(mut context) => {
                    context.toggle(signal);
                    re_derive(context)
                }
                None => other,
            },
        }
    }

    /// Project the current state into the flat list of toolbar badges the UI
    /// renders: the disc-ID badge, the barcode badge, then one badge per
    /// catalog candidate. Each carries its value, origin, live state, and
    /// whether the user has excluded it. `Idle` and `Error` have no toolbar.
    pub fn toolbar(&self) -> Vec<ToolbarSignal> {
        let Some(context) = self.context_cloned() else {
            return Vec::new();
        };
        let mut signals = Vec::new();
        signals.push(self.disc_badge(&context));
        signals.push(self.barcode_badge(&context));
        for catalog in &context.catalogs {
            signals.push(self.catalog_badge(catalog, &context));
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
                .filter(|m| catalog_matches_value(m.catalog_number.as_deref(), &catalog.value))
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
        DiscidProgress::Failed { message } => SignalState::Failed {
            message: message.clone(),
        },
    }
}

/// Live barcode badge state from its in-flight progress.
fn barcode_progress_state(progress: &BarcodeProgress) -> SignalState {
    match progress {
        BarcodeProgress::Scanning | BarcodeProgress::LookingUp { .. } => SignalState::LookingUp,
        BarcodeProgress::Done { results, .. } => found_or_no_match(results.len() as u32),
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
        DiscIdSignal::Failed { message, .. } => SignalState::Failed {
            message: message.clone(),
        },
        DiscIdSignal::Computed { .. } => found_or_no_match(results.len() as u32),
    }
}

/// Settled barcode badge state from the context's recorded barcode results.
fn barcode_settled_state(context: &SignalsContext) -> SignalState {
    if context.barcode_codes.is_empty() {
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
fn catalog_matches_value(catalog_number: Option<&str>, candidate: &str) -> bool {
    catalog_number.is_some_and(|c| normalize_catalog(c) == normalize_catalog(candidate))
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
        message: String,
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
        message: String,
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
        IdentifyEvent::SignalToggled { signal } if state.has_context() => {
            return (state.with_toggled(signal.clone()), vec![]);
        }
        IdentifyEvent::ReRun
            if state.has_context() && !matches!(state, IdentifyState::Triangulating { .. }) =>
        {
            return rerun(
                state
                    .context_cloned()
                    .expect("has_context implies a context"),
            );
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
            IdentifyEvent::DiscidLookupFailed { message },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Failed { message },
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

        // Per-barcode failure behaves as a miss — a single provider outage
        // shouldn't kill the iteration. Terminal failure only when the
        // queue runs out (settled as `Done { results: [] }`).
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
            IdentifyEvent::BarcodeLookupFailed {
                for_barcode,
                message: _,
            },
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
        (
            DiscidProgress::Computing,
            DiscIdSignal::Computed {
                disc_id,
                track_count,
            },
        ) => {
            effects.push(Effect::LookupDiscid {
                disc_id: disc_id.clone(),
                track_count: *track_count,
            });
            DiscidProgress::LookingUp
        }
        (DiscidProgress::Computing, DiscIdSignal::Absent { track_count }) => {
            DiscidProgress::Skipped {
                track_count: *track_count,
            }
        }
        (DiscidProgress::Computing, DiscIdSignal::Failed { message, .. }) => {
            DiscidProgress::Failed {
                message: message.clone(),
            }
        }
        // Already past Computing — the disc-ID lookup is in flight or settled.
        (discid, _) => discid,
    };

    let barcode = match (barcode, &signals.barcode) {
        (BarcodeProgress::Scanning, BarcodeSignal::Settled { codes }) => {
            if codes.is_empty() {
                BarcodeProgress::Done {
                    matched: None,
                    results: Vec::new(),
                }
            } else {
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
        }
        (BarcodeProgress::Scanning, BarcodeSignal::Absent) => BarcodeProgress::Skipped,
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
    context.record_results(
        discid.results(),
        barcode.results(),
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
    let outcome = combine_results(
        discid_results,
        barcode_results,
        &context.active_catalog_values(),
    );
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
/// signals, preserving the user's exclusions. Disc-ID dispatches when computed
/// (else skips); the barcode queue re-seeds from the retained codes. Mirrors
/// `apply_signals`' dispatch shape but starts from the known-settled inputs.
fn rerun(mut context: SignalsContext) -> (IdentifyState, Vec<Effect>) {
    // A fresh run discards prior results; they're recomputed as lookups land.
    context.discid_results = Vec::new();
    context.barcode_results = Vec::new();
    context.matched_barcode = None;

    let mut effects = Vec::new();

    let discid = match &context.disc_id {
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
        DiscIdSignal::Failed { message, .. } => DiscidProgress::Failed {
            message: message.clone(),
        },
    };

    let barcode = if context.barcode_codes.is_empty() {
        BarcodeProgress::Skipped
    } else {
        let mut values: Vec<String> = context
            .barcode_codes
            .iter()
            .map(|c| c.value.clone())
            .collect();
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
    };

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
        _ => 0,
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
mod tests {
    use super::*;
    use crate::import::MetadataSource;
    use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};

    fn mk_result(release_id: &str, group_id: Option<&str>) -> MetadataResult {
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
            source_group_id: group_id.map(str::to_string),
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

    fn pair(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
        (mk_result(release_id, group_id), mk_status(release_id))
    }

    /// Drive `Idle` → `Triangulating` via `Started` (no effects yet — the
    /// reducer waits for `SignalsUpdated`).
    fn started() -> IdentifyState {
        let (state, effects) = step(IdentifyState::Idle, IdentifyEvent::Started);
        assert!(effects.is_empty(), "Started dispatches no effects");
        state
    }

    fn update(state: IdentifyState, signals: Signals) -> (IdentifyState, Vec<Effect>) {
        step(state, IdentifyEvent::SignalsUpdated { signals })
    }

    /// Build barcode code payloads with an arbitrary `Artwork` origin — the
    /// state machine only reads `.value`, so the origin is immaterial here.
    fn artwork_codes(values: &[&str]) -> Vec<SourcedValue> {
        values
            .iter()
            .map(|v| SourcedValue::new(v.to_string(), SignalOrigin::Artwork))
            .collect()
    }

    fn signals(disc_id: DiscIdSignal, barcode: BarcodeSignal, catalogs: &[&str]) -> Signals {
        Signals {
            disc_id,
            barcode,
            text: TextSignal::Settled {
                catalogs: catalogs
                    .iter()
                    .map(|s| SourcedValue::new(s.to_string(), SignalOrigin::FolderName))
                    .collect(),
                free_text: vec![],
            },
        }
    }

    #[test]
    fn started_enters_triangulating_awaiting_signals() {
        match started() {
            IdentifyState::Triangulating {
                discid,
                barcode,
                context,
            } => {
                assert!(matches!(discid, DiscidProgress::Computing));
                assert!(matches!(barcode, BarcodeProgress::Scanning));
                assert!(context.catalogs.is_empty());
            }
            other => panic!("expected Triangulating, got {other:?}"),
        }
    }

    /// The disc-ID lookup is dispatched exactly once, even as snapshots stream.
    #[test]
    fn disc_computed_dispatches_lookup_idempotently() {
        let snapshot = || {
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Scanning { codes: vec![] },
                &[],
            )
        };
        let (state, effects) = update(started(), snapshot());
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LookupDiscid { .. })));
        // A repeated snapshot must not re-dispatch the lookup.
        let (_, effects) = update(state, snapshot());
        assert!(
            effects.is_empty(),
            "disc-id lookup dispatched only once, got {effects:?}"
        );
    }

    #[test]
    fn no_disc_no_barcode_is_manual_only() {
        let (state, effects) = update(
            started(),
            signals(
                DiscIdSignal::Absent { track_count: 7 },
                BarcodeSignal::Absent,
                &[],
            ),
        );
        assert!(effects.is_empty());
        match state {
            IdentifyState::ManualOnly { track_count, .. } => assert_eq!(track_count, 7),
            other => panic!("expected ManualOnly, got {other:?}"),
        }
    }

    /// The barcode queue is seeded only once the codes settle — never from a
    /// still-`Scanning` snapshot — so first-match-wins runs over a stable list.
    #[test]
    fn barcode_queue_seeded_only_from_settled() {
        let (state, effects) = update(
            started(),
            signals(
                DiscIdSignal::Absent { track_count: 0 },
                BarcodeSignal::Scanning {
                    codes: artwork_codes(&["A"]),
                },
                &[],
            ),
        );
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::LookupBarcode { .. })),
            "no barcode lookup while still scanning"
        );
        let (_, effects) = update(
            state,
            signals(
                DiscIdSignal::Absent { track_count: 0 },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["A", "B"]),
                },
                &[],
            ),
        );
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "A")));
    }

    #[test]
    fn disc_only_resolves_to_found_with_provenance() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Absent,
                &[],
            ),
        );
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![pair("rel-1", Some("g-x"))],
                track_count: 5,
            },
        );
        match state {
            IdentifyState::Found {
                matches,
                source,
                provenance,
                ..
            } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(source, IdentifySource::Discid);
                assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn both_signals_intersect_to_found_combined() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 10,
                },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["BAR"]),
                },
                &[],
            ),
        );
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![pair("rel-1", Some("g-x")), pair("rel-2", Some("g-x"))],
                track_count: 10,
            },
        );
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMatched {
                for_barcode: "BAR".to_string(),
                results: vec![pair("rel-2", Some("g-x"))],
            },
        );
        match state {
            IdentifyState::Found {
                matches,
                source,
                provenance,
                ..
            } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].release_id, "rel-2");
                assert_eq!(source, IdentifySource::Combined);
                assert!(provenance[0].by_disc_id && provenance[0].by_barcode);
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn empty_intersection_is_conflict() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["BAR"]),
                },
                &[],
            ),
        );
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![pair("rel-1", Some("g-x"))],
                track_count: 5,
            },
        );
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMatched {
                for_barcode: "BAR".to_string(),
                results: vec![pair("rel-2", Some("g-y"))],
            },
        );
        match state {
            IdentifyState::Conflict { context } => {
                // The conflict retains each signal's results for the surface.
                assert_eq!(context.discid_results.len(), 1);
                assert_eq!(context.barcode_results.len(), 1);
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn barcode_iteration_first_match_wins() {
        let (state, effects) = update(
            started(),
            signals(
                DiscIdSignal::Absent { track_count: 0 },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["A", "B", "C"]),
                },
                &[],
            ),
        );
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "A")));
        let (state, effects) = step(
            state,
            IdentifyEvent::BarcodeLookupMissed {
                for_barcode: "A".to_string(),
            },
        );
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "B")));
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMatched {
                for_barcode: "B".to_string(),
                results: vec![pair("rel-1", Some("g-x"))],
            },
        );
        match state {
            IdentifyState::Found { source, .. } => assert_eq!(source, IdentifySource::Barcode),
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn stale_barcode_response_is_ignored() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Absent { track_count: 0 },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["A", "B"]),
                },
                &[],
            ),
        );
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMissed {
                for_barcode: "A".to_string(),
            },
        );
        // A late duplicate "A" arrives; current is "B", so it's dropped.
        let (state, effects) = step(
            state,
            IdentifyEvent::BarcodeLookupMissed {
                for_barcode: "A".to_string(),
            },
        );
        assert!(effects.is_empty());
        match state {
            IdentifyState::Triangulating {
                barcode: BarcodeProgress::LookingUp { current, .. },
                ..
            } => assert_eq!(current, "B"),
            other => panic!("expected LookingUp B, got {other:?}"),
        }
    }

    #[test]
    fn barcode_lookup_failure_advances_like_a_miss() {
        let (state, effects) = update(
            started(),
            signals(
                DiscIdSignal::Absent { track_count: 0 },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["A", "B"]),
                },
                &[],
            ),
        );
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "A")));

        // The first code's lookup fails (a provider outage) — iteration should
        // continue to "B" exactly as a miss would, not abort the whole pipe.
        let (state, effects) = step(
            state,
            IdentifyEvent::BarcodeLookupFailed {
                for_barcode: "A".to_string(),
                message: "provider 503".to_string(),
            },
        );
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "B")));
        match &state {
            IdentifyState::Triangulating {
                barcode: BarcodeProgress::LookingUp { current, .. },
                ..
            } => assert_eq!(current, "B"),
            other => panic!("expected LookingUp B, got {other:?}"),
        }

        // The last code also fails with nothing left in the queue — the barcode
        // pipe settles as a clean miss rather than stranding the pipeline.
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupFailed {
                for_barcode: "B".to_string(),
                message: "provider 503".to_string(),
            },
        );
        assert!(
            !matches!(state, IdentifyState::Triangulating { .. }),
            "pipeline should have settled after the last barcode failed, got {state:?}"
        );
    }

    #[test]
    fn catalog_filter_narrows_and_flags_provenance() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Absent,
                &["LBL 001"],
            ),
        );
        let mut r_a = mk_result("rel-a", Some("g-x"));
        r_a.catalog_number = Some("LBL-001".to_string());
        let mut r_b = mk_result("rel-b", Some("g-y"));
        r_b.catalog_number = Some("LBL-002".to_string());
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![(r_a, mk_status("rel-a")), (r_b, mk_status("rel-b"))],
                track_count: 5,
            },
        );
        match state {
            IdentifyState::Found {
                matches,
                provenance,
                ..
            } => {
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].release_id, "rel-a");
                assert!(provenance[0].matches_catalog);
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn both_lookups_empty_is_not_found_anywhere() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["BAR"]),
                },
                &[],
            ),
        );
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![],
                track_count: 5,
            },
        );
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMissed {
                for_barcode: "BAR".to_string(),
            },
        );
        assert!(matches!(state, IdentifyState::NotFoundAnywhere { .. }));
    }

    #[test]
    fn cancellation_returns_to_idle() {
        let (state, effects) = step(started(), IdentifyEvent::Cancelled);
        assert!(matches!(state, IdentifyState::Idle));
        assert!(effects.is_empty());
    }

    /// Drive the reducer through triangulation into a conflict: disc-id and
    /// barcode each match a single release, but on different groups. Returns
    /// the `Conflict` state.
    fn driven_conflict() -> IdentifyState {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 7,
                },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["BAR"]),
                },
                &[],
            ),
        );
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![pair("rel-1", Some("g-x"))],
                track_count: 7,
            },
        );
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMatched {
                for_barcode: "BAR".to_string(),
                results: vec![pair("rel-2", Some("g-y")), pair("rel-3", Some("g-y"))],
            },
        );
        assert!(
            matches!(state, IdentifyState::Conflict { .. }),
            "expected Conflict, got {state:?}",
        );
        state
    }

    /// Excluding the disc-ID signal from a conflict collapses to the barcode
    /// side's single coherent group; re-including it restores the conflict.
    #[test]
    fn toggle_excludes_discid_then_re_includes() {
        let conflict = driven_conflict();

        let (state, effects) = step(
            conflict,
            IdentifyEvent::SignalToggled {
                signal: ExcludedSignal::Disc,
            },
        );
        assert!(
            effects.is_empty(),
            "toggle re-combines in place, no lookups"
        );
        match &state {
            IdentifyState::Found {
                matches, source, ..
            } => {
                assert_eq!(matches.len(), 2);
                assert_eq!(*source, IdentifySource::Barcode);
            }
            other => panic!("expected Found, got {other:?}"),
        }
        // The disc-ID badge reports its exclusion.
        let disc = state
            .toolbar()
            .into_iter()
            .find(|s| s.kind == SignalKind::DiscId)
            .expect("disc badge");
        assert!(disc.excluded);

        // Re-including the disc-ID signal restores the empty-intersection
        // conflict.
        let (restored, _) = step(
            state,
            IdentifyEvent::SignalToggled {
                signal: ExcludedSignal::Disc,
            },
        );
        assert!(
            matches!(restored, IdentifyState::Conflict { .. }),
            "expected Conflict after re-include, got {restored:?}",
        );
    }

    /// Toggling a signal *during* triangulation records the exclusion without
    /// collapsing the in-flight lookups: the state stays `Triangulating`, the
    /// badge reads excluded, and the exclusion is honored once the lookups
    /// settle (disc excluded → barcode-only Found).
    #[test]
    fn toggle_during_triangulation_keeps_looking_up() {
        // Disc-ID computed + one barcode → both lookups dispatched, still
        // Triangulating.
        let (state, effects) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["BAR"]),
                },
                &[],
            ),
        );
        assert!(matches!(state, IdentifyState::Triangulating { .. }));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LookupDiscid { .. })));

        // Toggle the disc signal off mid-lookup: must NOT collapse to a
        // terminal state, and must dispatch nothing.
        let (state, effects) = step(
            state,
            IdentifyEvent::SignalToggled {
                signal: ExcludedSignal::Disc,
            },
        );
        assert!(
            matches!(state, IdentifyState::Triangulating { .. }),
            "toggling mid-lookup must stay Triangulating, got {state:?}",
        );
        assert!(effects.is_empty(), "toggle dispatches no lookups");
        let disc = state
            .toolbar()
            .into_iter()
            .find(|s| s.kind == SignalKind::DiscId)
            .expect("disc badge");
        assert!(disc.excluded, "disc badge reads excluded after toggle");

        // Both lookups settle; the exclusion is honored → barcode-only Found.
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![pair("rel-1", Some("g-x"))],
                track_count: 5,
            },
        );
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMatched {
                for_barcode: "BAR".to_string(),
                results: vec![pair("rel-2", Some("g-y"))],
            },
        );
        match state {
            IdentifyState::Found {
                source, matches, ..
            } => {
                assert_eq!(source, IdentifySource::Barcode);
                assert_eq!(matches.len(), 1);
                assert_eq!(matches[0].release_id, "rel-2");
            }
            other => panic!("expected barcode-only Found, got {other:?}"),
        }
    }

    /// `ReRun` from a settled state resets to `Triangulating` and re-dispatches
    /// both lookups from the retained signals.
    #[test]
    fn rerun_re_dispatches_lookups() {
        // A plain disc-only Found, then re-run.
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "d".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["BAR"]),
                },
                &[],
            ),
        );
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![pair("rel-1", Some("g-x"))],
                track_count: 5,
            },
        );
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMatched {
                for_barcode: "BAR".to_string(),
                results: vec![pair("rel-1", Some("g-x"))],
            },
        );
        assert!(matches!(state, IdentifyState::Found { .. }));

        let (rerun_state, effects) = step(state, IdentifyEvent::ReRun);
        assert!(
            matches!(rerun_state, IdentifyState::Triangulating { .. }),
            "expected Triangulating after re-run, got {rerun_state:?}",
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::LookupDiscid { disc_id, .. } if disc_id == "d")),
            "re-run re-dispatches the disc-ID lookup, got {effects:?}",
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "BAR")),
            "re-run re-dispatches the barcode lookup, got {effects:?}",
        );
    }

    /// `ReRun` from a skip-only candidate (disc Absent, no barcodes) dispatches
    /// no lookups, so rerun must settle immediately via settle_if_ready rather
    /// than park in `Triangulating` waiting for results that never arrive.
    #[test]
    fn rerun_with_nothing_to_look_up_settles() {
        let (settled, effects) = update(
            started(),
            signals(
                DiscIdSignal::Absent { track_count: 5 },
                BarcodeSignal::Absent,
                &[],
            ),
        );
        assert!(effects.is_empty(), "skip-only signals dispatch no lookups");
        assert!(
            !matches!(settled, IdentifyState::Triangulating { .. }),
            "skip-only signals settle on first snapshot, got {settled:?}",
        );
        let settled_kind = std::mem::discriminant(&settled);

        let (rerun_state, rerun_effects) = step(settled, IdentifyEvent::ReRun);
        assert!(
            rerun_effects.is_empty(),
            "re-run of a skip-only candidate dispatches no lookups, got {rerun_effects:?}",
        );
        assert!(
            !matches!(rerun_state, IdentifyState::Triangulating { .. }),
            "re-run must settle, not park in Triangulating, got {rerun_state:?}",
        );
        assert_eq!(
            std::mem::discriminant(&rerun_state),
            settled_kind,
            "re-run re-settles to the same terminal state",
        );
    }

    /// `ReRun` preserves the user's exclusions: a re-run after excluding the
    /// disc still does not re-include it (no disc lookup dispatched once the
    /// re-derive masks it). Here we exclude the barcode, re-run, and confirm
    /// the barcode lookup is still dispatched (re-run replays the lookups
    /// regardless) but the settled re-derive masks the excluded side.
    #[test]
    fn rerun_preserves_exclusions() {
        let conflict = driven_conflict();
        // Exclude the disc-ID signal: lands on barcode-only Found.
        let (excluded, _) = step(
            conflict,
            IdentifyEvent::SignalToggled {
                signal: ExcludedSignal::Disc,
            },
        );
        assert!(matches!(excluded, IdentifyState::Found { .. }));

        // Re-run, then re-settle both lookups. The disc exclusion survives,
        // so the re-derived state is barcode-only Found again, not a conflict.
        let (state, _) = step(excluded, IdentifyEvent::ReRun);
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![pair("rel-1", Some("g-x"))],
                track_count: 7,
            },
        );
        let (state, _) = step(
            state,
            IdentifyEvent::BarcodeLookupMatched {
                for_barcode: "BAR".to_string(),
                results: vec![pair("rel-2", Some("g-y")), pair("rel-3", Some("g-y"))],
            },
        );
        match state {
            IdentifyState::Found { source, .. } => assert_eq!(source, IdentifySource::Barcode),
            other => panic!("expected barcode-only Found after re-run, got {other:?}"),
        }
    }

    // MARK: - Toolbar projection

    #[test]
    fn toolbar_while_triangulating_shows_spinners() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "disc-hash".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["012345678905"]),
                },
                &["LBL-001"],
            ),
        );
        let toolbar = state.toolbar();
        // disc, barcode, one catalog.
        assert_eq!(toolbar.len(), 3);

        let disc = &toolbar[0];
        assert_eq!(disc.kind, SignalKind::DiscId);
        assert_eq!(disc.role, SignalRole::Identity);
        assert_eq!(disc.value.as_deref(), Some("disc-hash"));
        assert_eq!(disc.origin, SignalOrigin::DiscToc);
        assert_eq!(disc.state, SignalState::LookingUp);

        let barcode = &toolbar[1];
        assert_eq!(barcode.kind, SignalKind::Barcode);
        assert_eq!(barcode.value.as_deref(), Some("012345678905"));
        assert_eq!(barcode.state, SignalState::LookingUp);

        let catalog = &toolbar[2];
        assert_eq!(catalog.kind, SignalKind::Catalog);
        assert_eq!(catalog.role, SignalRole::Filter);
        assert_eq!(catalog.value.as_deref(), Some("LBL-001"));
    }

    #[test]
    fn toolbar_found_reports_counts_and_catalog_confirms() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "disc-hash".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Absent,
                &["LBL 001", "LBL 999"],
            ),
        );
        let mut r_a = mk_result("rel-a", Some("g-x"));
        r_a.catalog_number = Some("LBL-001".to_string());
        let mut r_b = mk_result("rel-b", Some("g-x"));
        r_b.catalog_number = Some("LBL-002".to_string());
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupCompleted {
                results: vec![(r_a, mk_status("rel-a")), (r_b, mk_status("rel-b"))],
                track_count: 5,
            },
        );
        assert!(matches!(state, IdentifyState::Found { .. }));

        let toolbar = state.toolbar();
        let disc = toolbar
            .iter()
            .find(|s| s.kind == SignalKind::DiscId)
            .expect("disc badge");
        assert_eq!(disc.state, SignalState::Found { count: 2 });

        let barcode = toolbar
            .iter()
            .find(|s| s.kind == SignalKind::Barcode)
            .expect("barcode badge");
        assert_eq!(barcode.state, SignalState::Skipped);
        assert_eq!(barcode.value, None);

        let catalogs: Vec<&ToolbarSignal> = toolbar
            .iter()
            .filter(|s| s.kind == SignalKind::Catalog)
            .collect();
        assert_eq!(catalogs.len(), 2);
        // `LBL 001` confirms rel-a (catno LBL-001 normalizes equal).
        let confirming = catalogs
            .iter()
            .find(|c| c.value.as_deref() == Some("LBL 001"))
            .expect("confirming catalog");
        assert_eq!(confirming.state, SignalState::Confirms { count: 1 });
        // `LBL 999` confirms nothing.
        let noise = catalogs
            .iter()
            .find(|c| c.value.as_deref() == Some("LBL 999"))
            .expect("noise catalog");
        assert_eq!(noise.state, SignalState::Confirms { count: 0 });
    }

    #[test]
    fn toolbar_shows_failed_disc_id_lookup() {
        // The disc-id lookup fails while the barcode is still in flight, so the
        // pipeline stays in Triangulating and the disc badge reports the
        // failure rather than a spinner or a count.
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Computed {
                    disc_id: "disc-hash".to_string(),
                    track_count: 5,
                },
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["012345678905"]),
                },
                &[],
            ),
        );
        let (state, _) = step(
            state,
            IdentifyEvent::DiscidLookupFailed {
                message: "network error".to_string(),
            },
        );
        let disc = state
            .toolbar()
            .into_iter()
            .find(|s| s.kind == SignalKind::DiscId)
            .expect("disc badge");
        assert_eq!(
            disc.state,
            SignalState::Failed {
                message: "network error".to_string()
            }
        );
    }

    #[test]
    fn toolbar_skipped_disc_and_barcode_in_manual_only() {
        let (state, _) = update(
            started(),
            signals(
                DiscIdSignal::Absent { track_count: 7 },
                BarcodeSignal::Absent,
                &[],
            ),
        );
        assert!(matches!(state, IdentifyState::ManualOnly { .. }));
        let toolbar = state.toolbar();
        let disc = &toolbar[0];
        assert_eq!(disc.state, SignalState::Skipped);
        assert_eq!(disc.value, None);
        let barcode = &toolbar[1];
        assert_eq!(barcode.state, SignalState::Skipped);
    }

    #[test]
    fn idle_has_empty_toolbar() {
        assert!(IdentifyState::Idle.toolbar().is_empty());
    }
}

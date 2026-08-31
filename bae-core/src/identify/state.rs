//! Pure state machine for the identify pipeline.
//!
//! Triangulation: the disc-ID and barcode signals run in parallel, each reporting
//! progress live so the UI can render both side by side ("Computing disc-id ✓ ·
//! Looking up barcode 2 of 3…"). Once both settle, the reducer hands their results
//! to `combine` and lands on `Found` or `NotFoundAnywhere`.
//!
//! `step` takes a state and an event and returns the next state plus the side
//! effects for the service to run. No I/O, no async, nothing outside itself.

use super::combine::{combine_results, CombineOutcome, ResultProvenance};
use super::toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
use crate::db::LibraryStatus;
use crate::import::search::MetadataResult;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue,
};
/// A signal the user acted on in the toolbar. The disc ID and barcode are
/// checked by default and toggle off; the catalog is off until one of the
/// extracted numbers is chosen, and choosing another replaces it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SignalToggle {
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
#[derive(Clone, Debug, PartialEq)]
pub struct SignalsContext {
    /// The disc-ID signal (value + its inherent `DiscToc` origin).
    pub disc_id: DiscIdSignal,
    /// The barcode code payloads with their origins.
    pub barcode_codes: Vec<SourcedValue>,
    /// Whether there was a barcode source at all. Empty `barcode_codes` is
    /// ambiguous on its own — artwork scanned that held no barcode, and nothing to
    /// scan, both produce an empty vec but settle differently — so the distinction
    /// `BarcodeSignal` draws between `Settled { codes: [] }` and `Absent` has to be
    /// carried, not re-derived.
    pub had_barcode_source: bool,
    /// Every catalog number extracted from the candidate, with its origin —
    /// what the catalog badge's list offers.
    pub catalogs: Vec<SourcedValue>,
    /// Which of `catalogs` the run is using. `None` — the resting state — keeps
    /// the catalog out of the combine entirely.
    pub chosen_catalog: Option<String>,
    /// Whether the user unchecked the disc ID.
    pub disc_excluded: bool,
    /// Whether the user unchecked the barcode.
    pub barcode_excluded: bool,
    /// The disc-ID lookup's results, once settled. Empty while looking up or
    /// when the disc-ID pipe was skipped / found nothing.
    pub discid_results: Vec<(MetadataResult, LibraryStatus)>,
    /// The barcode lookup's results, once settled.
    pub barcode_results: Vec<(MetadataResult, LibraryStatus)>,
    /// The chosen catalog number's lookup results, once settled.
    pub catalog_results: Vec<(MetadataResult, LibraryStatus)>,
    /// Whatever failure settled the disc-ID pipe into `DiscidProgress::Failed`
    /// (see `record_results`), if any. Two things can put it there:
    /// `start_discid_progress` copies it straight from
    /// [`crate::signals::DiscIdSignal::Failed`] when the disc ID itself
    /// couldn't be computed (no readable TOC — reachable only on the
    /// re-identify path today, `signals/service.rs`, not on the import-scan
    /// path that feeds this pipeline), or a `DiscidLookupFailed` event closes
    /// out a lookup that ran against a disc ID that computed fine. Either way
    /// `discid_results` is left empty exactly as it would be for a clean
    /// no-match, so this is what lets a caller lifting a settled state into a
    /// stored verdict tell "nothing was learned" apart from "the lookup ran
    /// and found nothing" (see [`super::verdict::TerminalVerdict`]).
    pub discid_failure: Option<LookupFailure>,
    /// The barcode lookup failure, once settled by an error.
    pub barcode_failure: Option<LookupFailure>,
    /// The catalog lookup failure, once settled by an error.
    pub catalog_failure: Option<LookupFailure>,
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
            had_barcode_source: false,
            catalogs: Vec::new(),
            chosen_catalog: None,
            disc_excluded: false,
            barcode_excluded: false,
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            catalog_results: Vec::new(),
            discid_failure: None,
            barcode_failure: None,
            catalog_failure: None,
            matched_barcode: None,
            track_count: 0,
        }
    }

    /// Take the inputs from a new snapshot, keeping what the user checked.
    /// Results aren't touched — they're recorded as the lookups settle. A
    /// chosen catalog number that the new snapshot no longer offers is dropped;
    /// the choice has to be one of the values on the list.
    fn refresh_inputs(&mut self, signals: &Signals) {
        self.disc_id = signals.disc_id.clone();
        self.barcode_codes = signals.barcode.codes().to_vec();
        self.had_barcode_source = !matches!(signals.barcode, BarcodeSignal::Absent);
        self.catalogs = signals.text.catalogs().to_vec();
        if let Some(chosen) = &self.chosen_catalog {
            if !self.catalogs.iter().any(|c| &c.value == chosen) {
                self.chosen_catalog = None;
                self.catalog_results.clear();
                self.catalog_failure = None;
            }
        }
        self.track_count = signals.disc_id.track_count();
    }

    fn record_results(
        &mut self,
        discid: &DiscidProgress,
        barcode: &BarcodeProgress,
        catalog: &CatalogProgress,
    ) {
        self.discid_results = discid.results();
        self.barcode_results = barcode.results();
        self.catalog_results = catalog.results();
        self.discid_failure = match discid {
            DiscidProgress::Failed { failure, .. } => Some(failure.clone()),
            _ => None,
        };
        self.barcode_failure = match barcode {
            BarcodeProgress::Failed { failure } => Some(failure.clone()),
            _ => None,
        };
        self.catalog_failure = match catalog {
            CatalogProgress::Failed { failure } => Some(failure.clone()),
            _ => None,
        };
        self.matched_barcode = barcode.matched_barcode().map(str::to_string);
    }

    /// The disc-ID results combine sees — empty when the signal is unchecked.
    fn active_discid_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.disc_excluded {
            Vec::new()
        } else {
            self.discid_results.clone()
        }
    }

    /// The barcode results combine sees — empty when the signal is unchecked.
    fn active_barcode_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.barcode_excluded {
            Vec::new()
        } else {
            self.barcode_results.clone()
        }
    }

    /// The catalog results combine sees. Nothing chosen means nothing ran, so
    /// they are empty and the catalog takes no part.
    fn active_catalog_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.chosen_catalog.is_none() {
            Vec::new()
        } else {
            self.catalog_results.clone()
        }
    }

    /// Clear what the last catalog lookup left, for a choice that replaces it.
    fn clear_catalog_lookup(&mut self) {
        self.catalog_results.clear();
        self.catalog_failure = None;
    }

    /// Failures belonging to evidence the current selection still uses.
    /// Lookups already in flight are allowed to finish after exclusion, but
    /// their answer no longer participates in the derived state.
    fn active_failures(&self) -> Vec<super::IdentifyFailure> {
        let mut failures = Vec::new();
        if !self.disc_excluded {
            if let Some(failure) = &self.discid_failure {
                failures.push(super::IdentifyFailure::DiscId(failure.clone()));
            }
        }
        if !self.barcode_excluded {
            if let Some(failure) = &self.barcode_failure {
                failures.push(super::IdentifyFailure::Barcode(failure.clone()));
            }
        }
        if self.chosen_catalog.is_some() {
            if let Some(failure) = &self.catalog_failure {
                failures.push(super::IdentifyFailure::Catalog(failure.clone()));
            }
        }
        failures
    }
}

/// One candidate's identify state.
///
/// Every state but `Idle` carries a [`SignalsContext`], so the toolbar projection
/// always has its signal values and the user can toggle or re-run from any settled
/// state.
#[derive(Clone, Debug, PartialEq)]
pub enum IdentifyState {
    Idle,

    /// Lookups in flight. Each signal progresses independently; `settle_if_ready`
    /// combines them into a terminal state once all three are settled. The
    /// catalog pipe rests at `Skipped` until a number is chosen — which is also
    /// what puts a settled state back here, with the other two standing back up
    /// from their stored results rather than running again.
    Triangulating {
        discid: DiscidProgress,
        barcode: BarcodeProgress,
        catalog: CatalogProgress,
        context: SignalsContext,
    },

    Found {
        matches: Vec<MetadataResult>,
        library_statuses: Vec<LibraryStatus>,
        track_count: u32,
        /// Per-match provenance (which signals produced/confirmed each row),
        /// index-aligned with `matches` — drives the per-row signal badges, and
        /// says which signal produced any given match.
        provenance: Vec<ResultProvenance>,
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

    /// An automatic lookup failed, either in the live reducer or resumed from
    /// its stored verdict after the run ended.
    Failed {
        failures: Vec<super::IdentifyFailure>,
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
            | IdentifyState::NotFoundAnywhere { context }
            | IdentifyState::ManualOnly { context, .. }
            | IdentifyState::Failed { context, .. } => Some(context),
            IdentifyState::Idle => None,
        }
    }

    /// Whether the machine has stopped moving on its own: nothing is in flight,
    /// so only the user (a toggle, a re-run) can change it now.
    ///
    /// A lookup failure is terminal too; conversion preserves it as a failed
    /// verdict rather than misclassifying its partial evidence.
    pub fn is_terminal(&self) -> bool {
        match self {
            IdentifyState::Found { .. }
            | IdentifyState::NotFoundAnywhere { .. }
            | IdentifyState::ManualOnly { .. }
            | IdentifyState::Failed { .. } => true,
            IdentifyState::Idle | IdentifyState::Triangulating { .. } => false,
        }
    }

    /// The badge list the UI renders: the disc ID, the barcode, and the
    /// catalog — three, whatever the candidate turned up. `Idle` has no
    /// toolbar.
    pub fn toolbar(&self) -> Vec<ToolbarSignal> {
        let Some(context) = self.context() else {
            return Vec::new();
        };
        vec![
            self.disc_badge(context),
            self.barcode_badge(context),
            self.catalog_badge(context),
        ]
    }

    /// State comes from the live `DiscidProgress` while triangulating, else from
    /// the context's settled results.
    fn disc_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let state = match self {
            IdentifyState::Triangulating { discid, .. } => discid_progress_state(discid),
            _ => settled_identity_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::DiscId,
            value: context.disc_id.discid_value(),
            origin: SignalOrigin::DiscToc,
            state,
            excluded: context.disc_excluded,
            options: Vec::new(),
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
            value: code.map(|c| c.value.clone()),
            origin: code.map_or(SignalOrigin::Artwork, |c| c.origin),
            state,
            excluded: context.barcode_excluded,
            options: Vec::new(),
        }
    }

    /// One badge for the catalog, whatever the candidate turned up: the chosen
    /// number and how its lookup went, with every extracted number behind it as
    /// the list to choose from. Nothing chosen means nothing ran.
    fn catalog_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let chosen = context
            .chosen_catalog
            .as_ref()
            .and_then(|v| context.catalogs.iter().find(|c| &c.value == v));
        let state = match self {
            IdentifyState::Triangulating { catalog, .. } => catalog_progress_state(catalog),
            _ => catalog_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Catalog,
            value: chosen.map(|c| c.value.clone()),
            origin: chosen.map_or(SignalOrigin::CueSheet, |c| c.origin),
            state,
            excluded: false,
            options: context
                .catalogs
                .iter()
                .map(|c| SignalOption {
                    value: c.value.clone(),
                    origin: c.origin,
                    chosen: context.chosen_catalog.as_deref() == Some(c.value.as_str()),
                })
                .collect(),
        }
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

    // ── Catalog lookup completion ───────────────────────────────────
    CatalogLookupCompleted {
        for_catalog: String,
        results: Vec<(MetadataResult, LibraryStatus)>,
    },
    CatalogLookupFailed {
        for_catalog: String,
        failure: LookupFailure,
    },

    /// The user checked or unchecked a signal. Unchecking the disc ID or the
    /// barcode re-combines over the rest; choosing a catalog number runs its
    /// lookup, and choosing the one already chosen clears it.
    SignalToggled {
        signal: SignalToggle,
    },

    /// The user asked to replay the lookups. The reducer resets to `Triangulating`
    /// and re-dispatches from the retained signals, keeping what the user
    /// checked.
    ReRun,
}

/// The side effects the service performs — the provider lookups. Each one
/// finishing feeds an `IdentifyEvent` back into `step`. Scanning, OCR, and
/// disc-ID derivation belong to the extraction service, not here.
#[derive(Debug, Clone)]
pub enum Effect {
    LookupDiscid { disc_id: String, track_count: u32 },
    LookupBarcode { barcode: String },
    LookupCatalog { catalog: String },
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
            return apply_toggle(state, signal.clone());
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
                catalog: CatalogProgress::Skipped,
                context: SignalsContext::empty(),
            },
            vec![],
        ),

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                context,
            },
            IdentifyEvent::SignalsUpdated { signals },
        ) => apply_signals(discid, barcode, catalog, context, signals),

        // ── DiscID lookup completion ───────────────────────────────────
        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing | DiscidProgress::LookingUp,
                barcode,
                catalog,
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
            catalog,
            context,
        }),

        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::LookingUp,
                barcode,
                catalog,
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
            catalog,
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
                catalog,
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
            catalog,
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
                catalog,
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
                    catalog,
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
                        catalog,
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
                catalog,
                context,
            },
            IdentifyEvent::BarcodeLookupFailed {
                for_barcode,
                failure,
            },
        ) if for_barcode == current => settle_if_ready(IdentifyState::Triangulating {
            discid,
            barcode: BarcodeProgress::Failed { failure },
            catalog,
            context,
        }),

        // ── Catalog lookup completion ──────────────────────────────────
        // The `for_catalog` guard drops a response for a number the user has
        // already moved off.
        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::LookingUp,
                context,
            },
            IdentifyEvent::CatalogLookupCompleted {
                for_catalog,
                results,
            },
        ) if context.chosen_catalog.as_deref() == Some(for_catalog.as_str()) => {
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Done { results },
                context,
            })
        }

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::LookingUp,
                context,
            },
            IdentifyEvent::CatalogLookupFailed {
                for_catalog,
                failure,
            },
        ) if context.chosen_catalog.as_deref() == Some(for_catalog.as_str()) => {
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Failed { failure },
                context,
            })
        }

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
    catalog: CatalogProgress,
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
        (BarcodeProgress::Scanning, BarcodeSignal::Settled { codes }) => {
            start_barcode_progress(codes, true, &mut effects)
        }
        (BarcodeProgress::Scanning, BarcodeSignal::Absent) => BarcodeProgress::Skipped,
        (BarcodeProgress::Scanning, BarcodeSignal::Failed { failure, .. }) => {
            BarcodeProgress::Failed {
                failure: failure.clone(),
            }
        }
        // Codes not settled yet, or already iterating/settled.
        (barcode, _) => barcode,
    };

    // A snapshot that no longer offers the chosen number clears the choice, so
    // the pipe waiting on it has nothing left to wait for.
    let catalog = if context.chosen_catalog.is_none() {
        CatalogProgress::Skipped
    } else {
        catalog
    };

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        context,
    };

    // A dispatched lookup means nothing can have settled this step.
    if effects.is_empty() {
        settle_if_ready(next)
    } else {
        (next, effects)
    }
}

/// Once every signal has settled, record their results into the context and
/// combine into a terminal state. Until then, stay in `Triangulating`.
fn settle_if_ready(state: IdentifyState) -> (IdentifyState, Vec<Effect>) {
    let IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        mut context,
    } = state
    else {
        return (state, vec![]);
    };
    if !discid.is_settled() || !barcode.is_settled() || !catalog.is_settled() {
        return (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                context,
            },
            vec![],
        );
    }

    let track_count = settled_track_count(&discid);
    context.track_count = track_count;
    context.record_results(&discid, &barcode, &catalog);

    // Nothing had anything to run. Offer manual search rather than claim we
    // looked and found nothing.
    if matches!(discid, DiscidProgress::Skipped { .. })
        && matches!(barcode, BarcodeProgress::Skipped)
        && matches!(catalog, CatalogProgress::Skipped)
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

/// Apply a toolbar toggle.
///
/// Unchecking the disc ID or the barcode changes nothing that has to be
/// fetched, so it re-combines in place. Choosing a catalog number does: its
/// lookup has to run, which puts the state back in `Triangulating` with the
/// other two signals standing back up from what they already found.
///
/// Mid-`Triangulating` an exclusion is only recorded — the in-flight lookups
/// keep running, and the settle applies it.
fn apply_toggle(state: IdentifyState, signal: SignalToggle) -> (IdentifyState, Vec<Effect>) {
    let SignalToggle::Catalog(value) = signal else {
        let exclude_disc = matches!(signal, SignalToggle::Disc);
        return match state {
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                mut context,
            } => {
                toggle_exclusion(&mut context, exclude_disc);
                (
                    IdentifyState::Triangulating {
                        discid,
                        barcode,
                        catalog,
                        context,
                    },
                    vec![],
                )
            }
            IdentifyState::Found { mut context, .. }
            | IdentifyState::NotFoundAnywhere { mut context }
            | IdentifyState::ManualOnly { mut context, .. }
            | IdentifyState::Failed { mut context, .. } => {
                toggle_exclusion(&mut context, exclude_disc);
                (re_derive(context), vec![])
            }
            IdentifyState::Idle => (IdentifyState::Idle, vec![]),
        };
    };
    choose_catalog(state, value)
}

fn toggle_exclusion(context: &mut SignalsContext, disc: bool) {
    if disc {
        context.disc_excluded = !context.disc_excluded;
    } else {
        context.barcode_excluded = !context.barcode_excluded;
    }
}

/// Check one extracted catalog number, or uncheck the one already checked.
/// At most one is ever chosen: choosing another replaces it.
fn choose_catalog(state: IdentifyState, value: String) -> (IdentifyState, Vec<Effect>) {
    let (discid, barcode, mut context) = match state {
        IdentifyState::Triangulating {
            discid,
            barcode,
            context,
            ..
        } => (discid, barcode, context),
        IdentifyState::Found { context, .. }
        | IdentifyState::NotFoundAnywhere { context }
        | IdentifyState::ManualOnly { context, .. }
        | IdentifyState::Failed { context, .. } => (
            settled_discid_progress(&context),
            settled_barcode_progress(&context),
            context,
        ),
        IdentifyState::Idle => return (IdentifyState::Idle, vec![]),
    };

    context.clear_catalog_lookup();
    if context.chosen_catalog.as_deref() == Some(value.as_str()) {
        context.chosen_catalog = None;
        return settle_if_ready(IdentifyState::Triangulating {
            discid,
            barcode,
            catalog: CatalogProgress::Skipped,
            context,
        });
    }

    context.chosen_catalog = Some(value.clone());
    (
        IdentifyState::Triangulating {
            discid,
            barcode,
            catalog: CatalogProgress::LookingUp,
            context,
        },
        vec![Effect::LookupCatalog { catalog: value }],
    )
}

/// Re-combine over the non-excluded signals and lift the outcome into a state. The
/// one combine path: triangulation settle, signal toggle, and re-run completion all
/// arrive here once the results are in the context.
///
/// Both sides empty — because the lookups found nothing, or because the user
/// excluded the signals that did — lands on `NotFoundAnywhere`.
fn re_derive(context: SignalsContext) -> IdentifyState {
    let failures = context.active_failures();
    if !failures.is_empty() {
        return IdentifyState::Failed {
            failures,
            track_count: context.track_count,
            context,
        };
    }
    let outcome = combine_results(
        context.active_discid_results(),
        context.active_barcode_results(),
        context.active_catalog_results(),
    );
    let track_count = context.track_count;
    match outcome {
        CombineOutcome::Found {
            matches,
            library_statuses,
            provenance,
        } => IdentifyState::Found {
            matches,
            library_statuses,
            track_count,
            provenance,
            context,
        },
        CombineOutcome::NotFoundAnywhere => IdentifyState::NotFoundAnywhere { context },
    }
}

/// Reset to `Triangulating` and re-dispatch the lookups from the retained
/// signals, keeping what the user checked.
fn rerun(mut context: SignalsContext) -> (IdentifyState, Vec<Effect>) {
    // The prior results go; the new lookups replace them as they land.
    context.discid_results = Vec::new();
    context.barcode_results = Vec::new();
    context.discid_failure = None;
    context.barcode_failure = None;
    context.matched_barcode = None;
    context.clear_catalog_lookup();

    let mut effects = Vec::new();

    let discid = start_discid_progress(&context.disc_id, &mut effects);
    let barcode = start_barcode_progress(
        &context.barcode_codes,
        context.had_barcode_source,
        &mut effects,
    );
    let catalog = match &context.chosen_catalog {
        Some(value) => {
            effects.push(Effect::LookupCatalog {
                catalog: value.clone(),
            });
            CatalogProgress::LookingUp
        }
        None => CatalogProgress::Skipped,
    };

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        context,
    };
    if effects.is_empty() {
        settle_if_ready(next)
    } else {
        (next, effects)
    }
}

mod progress;

use progress::{
    barcode_progress_state, barcode_settled_state, catalog_progress_state, catalog_settled_state,
    discid_progress_state, settled_barcode_progress, settled_discid_progress,
    settled_identity_state, settled_track_count, start_barcode_progress, start_discid_progress,
};
pub use progress::{BarcodeProgress, CatalogProgress, DiscidProgress};

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

/// `re_derive` for tests in sibling modules: the one path that turns a settled
/// context into a terminal state, so a test can build the state a real run
/// would reach rather than hand-assembling one.
#[cfg(test)]
pub(crate) fn re_derive_for_tests(context: SignalsContext) -> IdentifyState {
    re_derive(context)
}

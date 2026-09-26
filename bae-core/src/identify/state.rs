//! Pure state machine for the identify pipeline.
//!
//! Triangulation: the disc-ID, barcode and catalog signals run in parallel,
//! each reporting progress live so the UI can render them side by side. The
//! barcode and catalog lookups ask every provider in the run, and each provider
//! answers for itself — MusicBrainz landing never waits on Discogs, and one
//! failing leaves the other's matches standing.
//!
//! The identifiers naming nothing is not the end of the run. A release whose
//! identifiers no catalog holds — a box set listed with neither a barcode nor
//! a disc ID — is still found by its title, so once the three settle empty the
//! run asks every provider the candidate's own album title and artist. That
//! search is the fourth step, and it runs last because it is the weakest
//! claim: an exact code beats a name, so a name is only asked when no code
//! answered.
//!
//! Once every step settles, and what they found holds both catalogs'
//! releases, the run reads what its MusicBrainz albums are on Discogs — the
//! statements that put the two catalogs' albums on one card, and the Discogs
//! releases a MusicBrainz release names where that is the statement (see
//! [`crate::import::album_links`]). Then the reducer hands the results to
//! `combine` and lands on `Found`, `NotFoundAnywhere`, or `Failed`.
//!
//! Settling also records the run's ledger — the layout every surface has been
//! drawing while it ran — onto the terminal state, so what the run showed
//! survives the run.
//!
//! `step` takes a state and an event and returns the next state plus the side
//! effects for the service to run. No I/O, no async, nothing outside itself.

use super::combine::{combine_results, Findings, LibraryStatuses};
use super::toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
use super::view::{run_view, IdentifyRunView};
use crate::db::LibraryStatus;
use crate::import::album_links::{self, GroupReading, ToRead};
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::{Catalog, LookupChoices};
use crate::signals::{
    ArtworkScan, BarcodeSignal, LookupFailure, SignalOrigin, Signals, SourcedValue,
};

/// One candidate's identify state.
///
/// Every state but `Idle` carries a [`SignalsContext`], so the toolbar
/// projection always has its signal values.
///
/// Every settled state carries the ledger its run recorded as it ended — the
/// last frame the run showed, with the lookups that were still in flight
/// settled onto it. `None` when extraction handed the run nothing to lay out,
/// and for a state stood back up from a verdict whose row records none.
#[derive(Clone, Debug, PartialEq)]
pub enum IdentifyState {
    Idle,

    /// Lookups in flight. Each identifier signal progresses independently;
    /// `settle_if_ready` combines them into a terminal state once all three
    /// are settled and the title search they leave has settled too, and
    /// records the ledger they settled as. The catalog pipe rests at `Skipped`
    /// until a number is chosen, and the search rests at `Pending` until the
    /// three say whether it is needed.
    Triangulating {
        discid: DiscidProgress,
        barcode: BarcodeProgress,
        catalog: CatalogProgress,
        search: SearchProgress,
        context: SignalsContext,
    },

    /// The run settled on its findings, every lookup having answered.
    Found {
        findings: Findings,
        /// The live library check of every release `findings` names.
        library_statuses: LibraryStatuses,
        track_count: u32,
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },

    NotFoundAnywhere {
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },

    /// Nothing to look up: no disc-ID artifact (LOG/CUE) and no barcode source
    /// (artwork, CUE `CATALOG`). Distinct from `NotFoundAnywhere`, where signals
    /// ran and matched nothing — here none ran, so the UI offers manual search.
    ManualOnly {
        track_count: u32,
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },

    /// An automatic lookup failed, either in the live reducer or resumed from
    /// its stored verdict after the run ended.
    ///
    /// It carries what the lookups that did answer found, which is usually not
    /// nothing: one provider failing on the title search leaves the other
    /// provider's results standing, and a person looking at the pane should see
    /// them rather than an empty result area. They are stored with the
    /// failures, so a resumed failure shows what the live one did.
    Failed {
        failures: Vec<super::IdentifyFailure>,
        findings: Findings,
        library_statuses: LibraryStatuses,
        track_count: u32,
        ledger: Option<IdentifyRunView>,
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
            | IdentifyState::NotFoundAnywhere { context, .. }
            | IdentifyState::ManualOnly { context, .. }
            | IdentifyState::Failed { context, .. } => Some(context),
            IdentifyState::Idle => None,
        }
    }

    /// The candidate's own text this run judged its results against. `Idle`
    /// carries no context and judged nothing.
    ///
    /// A terminal state is projected into a [`super::TerminalVerdict`], which
    /// keeps the matches and the lookups that returned them but not the text —
    /// that is stored on the candidate. A caller that must re-judge those
    /// matches, as the sweep's settle step does to find the record a row leads
    /// with, takes the text from here before the projection drops it.
    pub fn candidate_text(&self) -> super::CandidateText {
        self.context()
            .map(|context| context.text.clone())
            .unwrap_or_default()
    }

    /// Whether the machine has stopped moving on its own: nothing is in
    /// flight, so this run has nothing left to do. The driver ends here, and
    /// what a person asks for next is a run of its own.
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
            value: context.disc.signal.discid_value(),
            origin: SignalOrigin::DiscToc,
            state,
            excluded: context.disc.excluded,
            options: Vec::new(),
        }
    }

    /// The badge shows the matched code, or the first one when nothing has
    /// matched yet, and takes its origin from that code. Every code the folder
    /// carries is behind it as the list to choose from, each marked when the
    /// run asks about it; the badge reads as left out only once no code is
    /// asked about.
    fn barcode_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let code = context
            .barcode
            .matched
            .as_ref()
            .and_then(|v| context.barcode.codes.iter().find(|c| &c.value == v))
            .or_else(|| context.barcode.codes.first());
        let state = match self {
            IdentifyState::Triangulating { barcode, .. } => barcode_progress_state(barcode),
            _ => barcode_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Barcode,
            value: code.map(|c| c.value.clone()),
            origin: code.map_or(SignalOrigin::Artwork, |c| c.origin),
            state,
            excluded: context.barcode.every_code_excluded(),
            options: signal_options(&context.barcode.codes, |value| {
                !context.barcode.excluded.iter().any(|left| left == value)
            }),
        }
    }

    /// One badge for the catalog, whatever the candidate turned up: the first
    /// number chosen and how the chosen numbers' lookups went together, with
    /// every extracted number behind it as the list to choose from, each
    /// marked when it is chosen. Nothing chosen means nothing ran.
    fn catalog_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let first_chosen = context.catalog.chosen.first().and_then(|chosen| {
            context
                .catalog
                .numbers
                .iter()
                .find(|c| c.value == chosen.value)
        });
        let state = match self {
            IdentifyState::Triangulating { catalog, .. } => catalog_progress_state(catalog),
            _ => catalog_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Catalog,
            value: first_chosen.map(|c| c.value.clone()),
            origin: first_chosen.map_or(SignalOrigin::CueSheet, |c| c.origin),
            state,
            excluded: false,
            options: signal_options(&context.catalog.numbers, |value| {
                context.catalog.is_chosen(value)
            }),
        }
    }
}

/// The values one signal offers, each once, in the order they were first seen,
/// with `chosen` saying whether the run asks about each. A value seen in two
/// places is one option and names where it was first seen.
fn signal_options(sightings: &[SourcedValue], chosen: impl Fn(&str) -> bool) -> Vec<SignalOption> {
    let mut options: Vec<SignalOption> = Vec::new();
    for sighting in sightings {
        if options.iter().any(|option| option.value == sighting.value) {
            continue;
        }
        options.push(SignalOption {
            value: sighting.value.clone(),
            origin: sighting.origin,
            chosen: chosen(&sighting.value),
        });
    }
    options
}

/// One provider's answer to one lookup, with each match paired with its
/// library status.
pub type LookupOutcome = Result<LookupResults, LookupFailure>;

/// What feeds the reducer: the external triggers, plus the completions of the
/// lookup effects the service ran on the previous step.
#[derive(Debug, Clone)]
pub enum IdentifyEvent {
    /// Begin. Enters `Triangulating` and waits for the first `SignalsUpdated` —
    /// extraction owns scanning and OCR, not the reducer. `providers` is what
    /// this run asks: MusicBrainz, and Discogs when it is configured.
    ///
    /// `choices` is what the person decided this candidate's identification
    /// asks about, read from the candidate when the run started. It is the
    /// only way a choice enters a run: nothing changes one while the run is
    /// going, so a different decision is a different run.
    ///
    /// `title_search` is what the candidate's draft says about the release —
    /// read once at the start, like the choices — which the run falls back on
    /// when the identifiers name nothing. `None` when the draft states no
    /// title.
    Started {
        providers: Vec<Catalog>,
        choices: LookupChoices,
        title_search: Option<TitleSearch>,
    },
    Cancelled,

    /// The candidate's latest signals, and where the artwork pass feeding
    /// them has got to. The reducer dispatches the disc-ID lookup once the
    /// disc ID is `Computed` and the barcode lookups once the codes have
    /// `Settled`, and refreshes the catalog filter from every snapshot.
    /// Snapshots stream, so this is idempotent: each signal's progress guards
    /// its own lookup against being dispatched twice.
    SignalsUpdated {
        signals: Signals,
        artwork: ArtworkScan,
    },

    // ── DiscID lookup completion ────────────────────────────────────
    DiscidLookupCompleted {
        results: LookupResults,
        track_count: u32,
    },
    DiscidLookupFailed {
        failure: LookupFailure,
        track_count: u32,
    },

    /// One provider answered about one barcode: matches, none, or why not.
    /// The reducer moves that provider's walk on and leaves the others alone.
    BarcodeLookupAnswered {
        source: Catalog,
        for_barcode: String,
        outcome: LookupOutcome,
    },

    /// One provider answered about one chosen catalog number.
    CatalogLookupAnswered {
        source: Catalog,
        for_catalog: String,
        outcome: LookupOutcome,
    },

    /// One provider answered the title search. There is one query and one
    /// lookup per provider, so this names no value to land on.
    SearchAnswered {
        source: Catalog,
        outcome: LookupOutcome,
    },

    /// What reading the groups `Effect::ReadAlbumLinks` named answered, each
    /// read or unread.
    AlbumLinksRead {
        read: Vec<GroupReading>,
    },
}

/// The side effects the service performs — the provider lookups, one per
/// provider. Each one finishing feeds an `IdentifyEvent` back into `step`.
/// Scanning, OCR, and disc-ID derivation belong to the extraction service, not
/// here.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    LookupDiscid {
        disc_id: String,
        track_count: u32,
    },
    LookupBarcode {
        source: Catalog,
        barcode: String,
    },
    LookupCatalog {
        source: Catalog,
        catalog: String,
    },
    /// Ask one provider the same query the Search section's General tab asks.
    SearchTitle {
        source: Catalog,
        query: TitleSearch,
    },
    /// Read what these MusicBrainz release groups are on the other catalog.
    ReadAlbumLinks {
        to_read: ToRead,
    },
}

/// Drive the state machine one step. `Cancelled` always resets to `Idle`.
pub fn step(state: IdentifyState, event: IdentifyEvent) -> (IdentifyState, Vec<Effect>) {
    if matches!(event, IdentifyEvent::Cancelled) {
        return (IdentifyState::Idle, vec![]);
    }

    match (state, event) {
        (
            IdentifyState::Idle,
            IdentifyEvent::Started {
                providers,
                choices,
                title_search,
            },
        ) => {
            let context = SignalsContext::started(providers, choices, title_search);
            // The chosen numbers are the person's decision about this
            // candidate, not something read off a snapshot, so their lookups
            // go out with the run rather than waiting for extraction to offer
            // the numbers again. A number the settled snapshot no longer
            // offers loses its lookup then, in `apply_signals`.
            let mut effects = Vec::new();
            let catalog = start_catalog_progress(
                &context.catalog.chosen_values(),
                &context.providers,
                &mut effects,
            );
            (
                IdentifyState::Triangulating {
                    discid: DiscidProgress::Computing,
                    barcode: BarcodeProgress::Scanning,
                    catalog,
                    search: SearchProgress::Pending,
                    context,
                },
                effects,
            )
        }

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search,
                context,
            },
            IdentifyEvent::SignalsUpdated { signals, artwork },
        ) => apply_signals(discid, barcode, catalog, search, context, signals, artwork),

        // ── DiscID lookup completion ───────────────────────────────────
        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing | DiscidProgress::LookingUp,
                barcode,
                catalog,
                search,
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
            search,
            context,
        }),

        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::LookingUp,
                barcode,
                catalog,
                search,
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
            search,
            context,
        }),

        // ── One provider's barcode answer ──────────────────────────────
        // The provider's own walk moves on; the others are untouched. An
        // answer for a code the walk has already moved past is stale and
        // dropped.
        (
            IdentifyState::Triangulating {
                discid,
                barcode:
                    BarcodeProgress::Lookups {
                        codes,
                        mut providers,
                    },
                catalog,
                search,
                context,
            },
            IdentifyEvent::BarcodeLookupAnswered {
                source,
                for_barcode,
                outcome,
            },
        ) => {
            let mut effects = Vec::new();
            if let Some(provider) = providers.iter_mut().find(|p| p.source == source) {
                if let BarcodeLookupState::Trying { index } = provider.state {
                    if codes.get(index) == Some(&for_barcode) {
                        provider.state =
                            advance_barcode_walk(source, index, &codes, outcome, &mut effects);
                    }
                }
            }
            let next = IdentifyState::Triangulating {
                discid,
                barcode: BarcodeProgress::Lookups { codes, providers },
                catalog,
                search,
                context,
            };
            if effects.is_empty() {
                settle_if_ready(next)
            } else {
                (next, effects)
            }
        }

        // ── One provider's catalog answer ──────────────────────────────
        // The answer lands on the lookup of the number it was asked about. A
        // number the user has since taken out of the run has no lookup any
        // more, so its late answer lands nowhere.
        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Lookups { mut values },
                search,
                context,
            },
            IdentifyEvent::CatalogLookupAnswered {
                source,
                for_catalog,
                outcome,
            },
        ) => {
            let asked = values
                .iter_mut()
                .find(|lookup| lookup.value == for_catalog)
                .and_then(|lookup| {
                    lookup
                        .providers
                        .iter_mut()
                        .find(|l| l.source == source && l.state == LookupState::LookingUp)
                });
            if let Some(lookup) = asked {
                lookup.state = match outcome {
                    Ok(results) => LookupState::Done { results },
                    Err(failure) => LookupState::Failed { failure },
                };
            }
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Lookups { values },
                search,
                context,
            })
        }

        // ── One provider's answer to the title search ──────────────────
        // One query, so the answer lands on that provider's one lookup.
        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search: SearchProgress::Lookups { mut providers },
                context,
            },
            IdentifyEvent::SearchAnswered { source, outcome },
        ) => {
            let asked = providers
                .iter_mut()
                .find(|l| l.source == source && l.state == LookupState::LookingUp);
            if let Some(lookup) = asked {
                lookup.state = match outcome {
                    Ok(results) => LookupState::Done { results },
                    Err(failure) => LookupState::Failed { failure },
                };
            }
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search: SearchProgress::Lookups { providers },
                context,
            })
        }

        // ── The album links, read once every lookup settled ────────────
        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search,
                mut context,
            },
            IdentifyEvent::AlbumLinksRead { read },
        ) if context.album_links == AlbumLinkReading::Reading => {
            context.album_links = AlbumLinkReading::Read(read);
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search,
                context,
            })
        }

        // An event Triangulating doesn't act on — a stale barcode response, say.
        (state @ IdentifyState::Triangulating { .. }, _) => (state, vec![]),

        // Any other (state, event) pair leaves the state alone.
        (state, _) => (state, vec![]),
    }
}

/// Where one provider's walk goes after answering about `codes[index]`: a
/// match ends it, a miss asks about the next code (or ends it exhausted), a
/// failure leaves it failed.
fn advance_barcode_walk(
    source: Catalog,
    index: usize,
    codes: &[String],
    outcome: LookupOutcome,
    effects: &mut Vec<Effect>,
) -> BarcodeLookupState {
    match outcome {
        Ok(results) if !results.is_empty() => BarcodeLookupState::Matched {
            code: codes[index].clone(),
            results,
        },
        Ok(_) => match codes.get(index + 1) {
            Some(next) => {
                effects.push(Effect::LookupBarcode {
                    source,
                    barcode: next.clone(),
                });
                BarcodeLookupState::Trying { index: index + 1 }
            }
            None => BarcodeLookupState::Exhausted,
        },
        Err(failure) => BarcodeLookupState::Failed { failure, index },
    }
}

/// Fold the latest snapshot into the two signals.
///
/// Idempotent under streaming, because each signal's own progress guards it: the
/// disc ID dispatches `LookupDiscid` only while still `Computing`, and the barcode
/// walks are started only while still `Scanning` *and* once the codes have
/// `Settled` — so every provider walks a complete, stable list. The catalog
/// filter refreshes from every snapshot.
#[allow(clippy::too_many_arguments)]
fn apply_signals(
    discid: DiscidProgress,
    barcode: BarcodeProgress,
    catalog: CatalogProgress,
    search: SearchProgress,
    mut context: SignalsContext,
    signals: Signals,
    artwork: ArtworkScan,
) -> (IdentifyState, Vec<Effect>) {
    let mut effects = Vec::new();
    context.refresh_inputs(&signals, artwork);

    let discid = match (discid, &signals.disc_id) {
        (DiscidProgress::Computing, signal) => start_discid_progress(
            signal,
            context.disc.excluded,
            &context.providers,
            &mut effects,
        ),
        // Past Computing: the lookup is in flight or settled.
        (discid, _) => discid,
    };

    let barcode = match (barcode, &signals.barcode) {
        (BarcodeProgress::Scanning, BarcodeSignal::Settled { .. }) => start_barcode_progress(
            context.barcode.code_values(),
            &context.barcode.excluded,
            true,
            None,
            &context.providers,
            &mut effects,
        ),
        (BarcodeProgress::Scanning, BarcodeSignal::Absent) => BarcodeProgress::Skipped,
        (BarcodeProgress::Scanning, BarcodeSignal::Failed { failure, .. }) => {
            BarcodeProgress::ScanFailed {
                failure: failure.clone(),
            }
        }
        // Codes not settled yet, or already iterating/settled.
        (barcode, _) => barcode,
    };

    // A snapshot that no longer offers a chosen number drops the choice, so
    // the lookup waiting on it has nothing left to wait for.
    let catalog = catalog.keeping(|lookup| context.catalog.is_chosen(&lookup.value));

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        search,
        context,
    };

    // A dispatched lookup means nothing can have settled this step.
    if effects.is_empty() {
        settle_if_ready(next)
    } else {
        (next, effects)
    }
}

/// Once every step has settled, record its results into the context and
/// combine into a terminal state. Until then, stay in `Triangulating`.
///
/// The three identifiers settle first, and what they found decides the fourth:
/// naming nothing between them is what sends the candidate's own title to
/// every provider, and the run stays in flight until that answers too.
fn settle_if_ready(state: IdentifyState) -> (IdentifyState, Vec<Effect>) {
    let IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        search,
        mut context,
    } = state
    else {
        return (state, vec![]);
    };
    // The text is an input too: results are judged against it, and the
    // verdict stores the snapshot it settled on. A run with nothing to look up
    // still waits for the settled snapshot rather than answering on the first.
    if !context.text_settled
        || !discid.is_settled()
        || !barcode.is_settled()
        || !catalog.is_settled()
    {
        return (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search,
                context,
            },
            vec![],
        );
    }

    let track_count = settled_track_count(&discid);
    context.track_count = track_count;
    context.record_results(&discid, &barcode, &catalog);

    // The fourth step, decided once the three that feed it are in: either it
    // goes out now and the run waits on it, or it never runs at all.
    let search = if matches!(search, SearchProgress::Pending) {
        let mut effects = Vec::new();
        let started = start_search_progress(&context, &mut effects);
        if !effects.is_empty() {
            return (
                IdentifyState::Triangulating {
                    discid,
                    barcode,
                    catalog,
                    search: started,
                    context,
                },
                effects,
            );
        }
        started
    } else if !search.is_settled() {
        return (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search,
                context,
            },
            vec![],
        );
    } else {
        search
    };
    context.record_search(&search);

    // Every lookup is in, so what the run found is final: read its
    // MusicBrainz albums' links when it holds both catalogs' releases, and
    // settle once they are read.
    match context.album_links {
        AlbumLinkReading::Pending => {
            let found = context.lookup_results();
            let to_read =
                album_links::to_read(found.iter().flatten().map(|(result, _)| result), |_| false);
            if to_read.is_empty() {
                context.album_links = AlbumLinkReading::Read(Vec::new());
            } else {
                context.album_links = AlbumLinkReading::Reading;
                return (
                    IdentifyState::Triangulating {
                        discid,
                        barcode,
                        catalog,
                        search,
                        context,
                    },
                    vec![Effect::ReadAlbumLinks { to_read }],
                );
            }
        }
        AlbumLinkReading::Reading => {
            return (
                IdentifyState::Triangulating {
                    discid,
                    barcode,
                    catalog,
                    search,
                    context,
                },
                vec![],
            )
        }
        AlbumLinkReading::Read(_) => {}
    }

    // The ledger this run showed, as its last frame showed it: the same
    // layout the driver has been publishing, with every lookup now settled.
    // It is computed here and nowhere else — every later reader shows what
    // was recorded rather than standing the pipes back up.
    let ledger = context
        .has_inputs()
        .then(|| run_view(&discid, &barcode, &catalog, &search, &context));

    // Nothing had anything to run, the title included. Offer manual search
    // rather than claim we looked and found nothing.
    if matches!(discid, DiscidProgress::Skipped { .. })
        && matches!(barcode, BarcodeProgress::Skipped)
        && matches!(catalog, CatalogProgress::Skipped)
        && matches!(search, SearchProgress::Skipped)
    {
        return (
            IdentifyState::ManualOnly {
                track_count,
                ledger,
                context,
            },
            vec![],
        );
    }

    (re_derive(context, ledger), vec![])
}

/// Re-combine over the evidence the run's choices still admit and lift the
/// outcome into a state. The one combine path: every way triangulation settles
/// arrives here once the results are in the context.
///
/// Every side empty — because the lookups found nothing, or because the run was
/// told to ask about nothing — lands on `NotFoundAnywhere`.
fn re_derive(context: SignalsContext, ledger: Option<IdentifyRunView>) -> IdentifyState {
    // Combine first, whatever failed: a provider that did not answer never
    // invalidates what the others found, and a failed state that hid those
    // matches would leave a person looking at an empty pane while one source
    // had the answer.
    let [discid_results, barcode_results, catalog_results, search_results] =
        context.lookup_results();
    let (findings, library_statuses) = combine_results(
        discid_results,
        barcode_results,
        catalog_results,
        search_results,
        context.twins(),
        &context.text,
    );
    let track_count = context.track_count;
    let failures = context.active_failures();
    if !failures.is_empty() {
        return IdentifyState::Failed {
            failures,
            findings,
            library_statuses,
            track_count,
            ledger,
            context,
        };
    }
    if findings.is_empty() {
        return IdentifyState::NotFoundAnywhere { ledger, context };
    }
    IdentifyState::Found {
        findings,
        library_statuses,
        track_count,
        ledger,
        context,
    }
}

mod context;
mod progress;

pub use context::{
    AlbumLinkReading, BarcodeEvidence, CatalogEvidence, ChosenCatalog, DiscIdEvidence,
    SearchEvidence, SignalsContext, TitleSearch,
};
use progress::{
    barcode_progress_state, barcode_settled_state, catalog_progress_state, catalog_settled_state,
    discid_progress_state, settled_identity_state, settled_track_count, start_barcode_progress,
    start_catalog_progress, start_discid_progress, start_search_progress,
};
pub use progress::{
    BarcodeLookupState, BarcodeProgress, CatalogLookup, CatalogProgress, DiscidProgress,
    LookupResults, LookupState, ProviderBarcodeLookup, ProviderLookup, SearchProgress,
};

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

/// `re_derive` for tests in sibling modules: the one path that turns a settled
/// context into a terminal state, so a test can build the state a real run
/// would reach rather than hand-assembling one. It records no ledger — a test
/// that wants one drives the pipes through [`step`].
#[cfg(test)]
pub(crate) fn re_derive_for_tests(context: SignalsContext) -> IdentifyState {
    re_derive(context, None)
}

/// The terminal state these settled pipes land on, ledger and all — the one
/// path a run ends by, for tests in sibling modules that want the run
/// recorded as a real one would record it.
#[cfg(test)]
pub(crate) fn settle_for_tests(
    discid: DiscidProgress,
    barcode: BarcodeProgress,
    catalog: CatalogProgress,
    search: SearchProgress,
    context: SignalsContext,
) -> IdentifyState {
    settle_if_ready(IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        search,
        context,
    })
    .0
}

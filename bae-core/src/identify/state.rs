//! The identify pipeline's pure state machine: `step` takes a state and an
//! event and returns the next state and the lookups for the service to run.
//!
//! The disc-ID, barcode, catalog and ISRC lookups run in parallel, each
//! provider answering for itself. When the first three name nothing, the run
//! searches by the candidate's title. Once every lookup settles, the run
//! reads the full documents of the rows it offers, and of the Discogs
//! releases their MusicBrainz documents name as themselves — ranking again
//! after each read, until every offered row is read (see
//! [`super::documents`]). What those documents state its MusicBrainz albums
//! are on Discogs joins the albums (see [`crate::import::album_links`]); the
//! run combines the results into a terminal state, keeps what it read each
//! album to be, and records the ledger it showed.

use super::combine::{combine_results, Findings, LibraryStatuses};
use super::documents::{DocumentReading, ReleaseReading, Twin, TwinToRead};
use super::toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
use super::view::{run_view, IdentifyRunView};
use crate::config::IdentificationSteps;
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::{Catalog, LookupChoices};
use crate::signals::{
    ArtworkScan, AudioFacts, BarcodeSignal, InternalFailure, LookupFailure, Signals,
};

/// One candidate's identify state. Every settled state carries the ledger its
/// run recorded as it ended, `None` when there was nothing to lay out.
#[derive(Clone, Debug, PartialEq)]
pub enum IdentifyState {
    Idle,

    /// Lookups in flight.
    Triangulating {
        discid: DiscidProgress,
        barcode: BarcodeProgress,
        catalog: CatalogProgress,
        isrc: IsrcProgress,
        search: SearchProgress,
        context: SignalsContext,
    },

    /// The run settled on its findings.
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

    /// No lookup ran — nothing to look up, or nobody was asked about what
    /// there was — so the UI offers manual search rather than "not found".
    ManualOnly {
        track_count: u32,
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },

    /// A lookup failed; carries what the lookups that did answer found.
    Failed {
        failures: Vec<super::IdentifyFailure>,
        findings: Findings,
        library_statuses: LibraryStatuses,
        track_count: u32,
        ledger: Option<IdentifyRunView>,
        context: SignalsContext,
    },

    /// bae broke on its own side — reading the folder's files, its store, the
    /// library check — and the run ended there. Nothing it had found stands:
    /// what it would have concluded is unknown.
    Error {
        failure: InternalFailure,
        context: SignalsContext,
    },
}

impl IdentifyState {
    /// The carried context; `None` only for `Idle`.
    fn context(&self) -> Option<&SignalsContext> {
        match self {
            IdentifyState::Triangulating { context, .. }
            | IdentifyState::Found { context, .. }
            | IdentifyState::NotFoundAnywhere { context, .. }
            | IdentifyState::ManualOnly { context, .. }
            | IdentifyState::Failed { context, .. }
            | IdentifyState::Error { context, .. } => Some(context),
            IdentifyState::Idle => None,
        }
    }

    /// The audio this state was identified over; `None` only for `Idle`.
    pub fn audio(&self) -> Option<&AudioFacts> {
        self.context().map(|context| &context.audio)
    }

    /// The candidate's own text this run judged its results against, which a
    /// [`super::TerminalVerdict`] does not keep.
    pub fn candidate_text(&self) -> super::CandidateText {
        self.context()
            .map(|context| context.text.clone())
            .unwrap_or_default()
    }

    /// Whether the run has nothing left in flight.
    pub fn is_terminal(&self) -> bool {
        match self {
            IdentifyState::Found { .. }
            | IdentifyState::NotFoundAnywhere { .. }
            | IdentifyState::ManualOnly { .. }
            | IdentifyState::Failed { .. }
            | IdentifyState::Error { .. } => true,
            IdentifyState::Idle | IdentifyState::Triangulating { .. } => false,
        }
    }

    /// The disc-ID, barcode, catalog and ISRC badges; none for `Idle`.
    pub fn toolbar(&self) -> Vec<ToolbarSignal> {
        let Some(context) = self.context() else {
            return Vec::new();
        };
        vec![
            self.disc_badge(context),
            self.barcode_badge(context),
            self.catalog_badge(context),
            self.isrc_badge(context),
        ]
    }

    /// Shows the first code, with every code as an option: all of them are
    /// asked about, in one search.
    fn isrc_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let codes = context.isrc.codes();
        let state = match self {
            IdentifyState::Triangulating { isrc, .. } => isrc_progress_state(isrc),
            _ => isrc_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Isrc,
            shown: codes.first().cloned(),
            state,
            excluded: false,
            options: signal_options(&codes, |_| true),
        }
    }

    fn disc_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let state = match self {
            IdentifyState::Triangulating { discid, .. } => discid_progress_state(discid),
            _ => settled_identity_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::DiscId,
            shown: context.disc.signal.discid_value(),
            state,
            excluded: context.disc.excluded,
            options: Vec::new(),
        }
    }

    /// Shows the matched code, else the first, with every code as an option.
    fn barcode_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let codes = context.barcode.code_values();
        let shown = context
            .barcode
            .matched
            .clone()
            .or_else(|| codes.first().cloned());
        let state = match self {
            IdentifyState::Triangulating { barcode, .. } => barcode_progress_state(barcode),
            _ => barcode_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Barcode,
            shown,
            state,
            excluded: context.barcode.every_code_excluded(),
            options: signal_options(&codes, |value| {
                !context.barcode.excluded.iter().any(|left| left == value)
            }),
        }
    }

    /// Shows the first chosen number, with every extracted number as an option.
    fn catalog_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let first_chosen = context
            .catalog
            .chosen
            .first()
            .map(|chosen| chosen.value.clone())
            .filter(|value| context.catalog.numbers.contains(value));
        let state = match self {
            IdentifyState::Triangulating { catalog, .. } => catalog_progress_state(catalog),
            _ => catalog_settled_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::Catalog,
            shown: first_chosen,
            state,
            excluded: false,
            options: signal_options(&context.catalog.numbers, |value| {
                context.catalog.is_chosen(value)
            }),
        }
    }
}

/// The values one signal offers, in first-seen order, each marked when chosen.
fn signal_options(values: &[String], chosen: impl Fn(&str) -> bool) -> Vec<SignalOption> {
    values
        .iter()
        .map(|value| SignalOption {
            value: value.clone(),
            chosen: chosen(value),
        })
        .collect()
}

/// One provider's answer to one lookup.
pub type LookupOutcome = Result<LookupResults, LookupFailure>;

/// What feeds the reducer: triggers, and the answers to its effects.
#[derive(Debug, Clone)]
pub enum IdentifyEvent {
    /// Begin a run. Its catalogs, steps, choices and title are fixed here; a
    /// different one is a different run.
    Started {
        providers: Vec<Catalog>,
        steps: IdentificationSteps,
        choices: LookupChoices,
        title_search: Option<TitleSearch>,
    },
    Cancelled,

    /// The candidate's latest signals, the audio they were read beside, and
    /// where the artwork pass has got to. Snapshots stream, so handling one is
    /// idempotent.
    SignalsUpdated {
        signals: Signals,
        audio: AudioFacts,
        artwork: ArtworkScan,
    },

    DiscidLookupCompleted {
        results: LookupResults,
    },
    DiscidLookupFailed {
        failure: LookupFailure,
    },

    /// One provider answered about one barcode.
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

    /// MusicBrainz answered the search by the audio's ISRCs.
    IsrcLookupAnswered {
        outcome: LookupOutcome,
    },

    /// One provider answered the title search.
    SearchAnswered {
        source: Catalog,
        outcome: LookupOutcome,
    },

    /// The documents `Effect::ReadReleases` asked for, record by record and
    /// twin by twin, with each twin that could be read.
    ReleasesRead {
        read: Vec<ReleaseReading>,
        twins: Vec<Twin>,
    },

    /// bae broke carrying out one of the run's effects.
    Broke {
        failure: InternalFailure,
    },
}

/// What the service does for a run: the lookups, each answering with an
/// `IdentifyEvent`, and keeping what they found beyond it.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    LookupDiscid {
        disc_id: String,
    },
    LookupBarcode {
        source: Catalog,
        barcode: String,
    },
    LookupCatalog {
        source: Catalog,
        catalog: String,
    },
    /// Ask MusicBrainz for the recordings these ISRCs are registered to.
    LookupIsrcs {
        isrcs: Vec<String>,
    },
    /// Ask one provider for the title.
    SearchTitle {
        source: Catalog,
        query: TitleSearch,
    },
    /// Fetch and store these records' and twins' full documents, reading
    /// each one's tracklist against `track_lengths_ms`, and check each twin
    /// against the library.
    ReadReleases {
        releases: Vec<crate::import::MetadataRef>,
        twins: Vec<TwinToRead>,
        track_lengths_ms: Vec<u64>,
    },
    /// Keep what these release groups were read to be beyond the run. Nothing
    /// answers it.
    KeepAlbumLinks {
        kept: Vec<(String, Vec<crate::import::album_links::AlbumLink>)>,
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
                steps,
                choices,
                title_search,
            },
        ) => {
            let context = SignalsContext::started(providers, steps, choices, title_search);
            // Chosen numbers are looked up at once, without waiting for a
            // snapshot to offer them again.
            let mut effects = Vec::new();
            let catalog = start_catalog_progress(
                &context.catalog.chosen_values(),
                &context.providers,
                &mut effects,
            );
            let search = search_progress_at_start(context.steps.search_by_title);
            (
                IdentifyState::Triangulating {
                    discid: DiscidProgress::Computing,
                    barcode: BarcodeProgress::Scanning,
                    catalog,
                    isrc: IsrcProgress::Reading,
                    search,
                    context,
                },
                effects,
            )
        }

        (IdentifyState::Triangulating { context, .. }, IdentifyEvent::Broke { failure }) => {
            (IdentifyState::Error { failure, context }, vec![])
        }

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc,
                search,
                context,
            },
            IdentifyEvent::SignalsUpdated {
                signals,
                audio,
                artwork,
            },
        ) => apply_signals(
            discid, barcode, catalog, isrc, search, context, signals, audio, artwork,
        ),

        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing | DiscidProgress::LookingUp,
                barcode,
                catalog,
                isrc,
                search,
                context,
            },
            IdentifyEvent::DiscidLookupFailed { failure },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Failed { failure },
            barcode,
            catalog,
            isrc,
            search,
            context,
        }),

        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::LookingUp,
                barcode,
                catalog,
                isrc,
                search,
                context,
            },
            IdentifyEvent::DiscidLookupCompleted { results },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Done { results },
            barcode,
            catalog,
            isrc,
            search,
            context,
        }),

        (
            IdentifyState::Triangulating {
                discid,
                barcode: BarcodeProgress::Lookups { mut codes },
                catalog,
                isrc,
                search,
                context,
            },
            IdentifyEvent::BarcodeLookupAnswered {
                source,
                for_barcode,
                outcome,
            },
        ) => {
            if let Some(lookup) = codes.iter_mut().find(|lookup| lookup.value == for_barcode) {
                lookup.answer(source, outcome);
            }
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode: BarcodeProgress::Lookups { codes },
                catalog,
                isrc,
                search,
                context,
            })
        }

        // A number since taken out of the run has no lookup for its answer.
        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Lookups { mut values },
                isrc,
                search,
                context,
            },
            IdentifyEvent::CatalogLookupAnswered {
                source,
                for_catalog,
                outcome,
            },
        ) => {
            if let Some(lookup) = values.iter_mut().find(|lookup| lookup.value == for_catalog) {
                lookup.answer(source, outcome);
            }
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog: CatalogProgress::Lookups { values },
                isrc,
                search,
                context,
            })
        }

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc,
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
                isrc,
                search: SearchProgress::Lookups { providers },
                context,
            })
        }

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc: IsrcProgress::LookingUp,
                search,
                context,
            },
            IdentifyEvent::IsrcLookupAnswered { outcome },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid,
            barcode,
            catalog,
            isrc: match outcome {
                Ok(results) => IsrcProgress::Done { results },
                Err(failure) => IsrcProgress::Failed { failure },
            },
            search,
            context,
        }),

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc,
                search,
                mut context,
            },
            IdentifyEvent::ReleasesRead { read, twins },
        ) if matches!(context.documents, DocumentReading::Reading(_)) => {
            let mut documents = context.documents.documents();
            documents.releases.extend(read);
            documents.twins.extend(twins);
            context.documents = DocumentReading::Read(documents);
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc,
                search,
                context,
            })
        }

        // A stale answer, or an event this state does not act on.
        (state @ IdentifyState::Triangulating { .. }, _) => (state, vec![]),
        (state, _) => (state, vec![]),
    }
}

/// Fold the latest snapshot in. Each lookup starts once, from its first
/// settled input: the disc ID while `Computing`, the barcodes once `Settled`.
#[allow(clippy::too_many_arguments)]
fn apply_signals(
    discid: DiscidProgress,
    barcode: BarcodeProgress,
    catalog: CatalogProgress,
    isrc: IsrcProgress,
    search: SearchProgress,
    mut context: SignalsContext,
    signals: Signals,
    audio: AudioFacts,
    artwork: ArtworkScan,
) -> (IdentifyState, Vec<Effect>) {
    // Reading the folder's files broke: nothing it would have read can be
    // looked up.
    if let Some(failure) = signals.failure() {
        return (
            IdentifyState::Error {
                failure: failure.clone(),
                context,
            },
            vec![],
        );
    }
    let mut effects = Vec::new();
    context.refresh_inputs(&signals, audio, artwork);

    let discid = match (discid, &signals.disc_id) {
        (DiscidProgress::Computing, signal) => start_discid_progress(
            signal,
            context.disc.excluded,
            context.steps.look_up_disc_ids,
            &context.providers,
            &mut effects,
        ),
        (discid, _) => discid,
    };

    let barcode = match (barcode, &signals.barcode) {
        (BarcodeProgress::Scanning, BarcodeSignal::Settled { .. }) => start_barcode_progress(
            context.barcode.code_values(),
            &context.barcode.excluded,
            true,
            context.steps.look_up_barcodes,
            &context.providers,
            &mut effects,
        ),
        (BarcodeProgress::Scanning, BarcodeSignal::Absent) => BarcodeProgress::Skipped,
        (barcode, _) => barcode,
    };

    // A chosen number the snapshot no longer offers loses its lookup.
    let catalog = catalog.keeping(|lookup| context.catalog.is_chosen(&lookup.value));

    let isrc = match isrc {
        IsrcProgress::Reading => {
            start_isrc_progress(context.isrc.codes(), &context.providers, &mut effects)
        }
        isrc => isrc,
    };

    let next = IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        isrc,
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

/// Once every step has settled, record the results and combine them into a
/// terminal state; until then, stay in `Triangulating`.
fn settle_if_ready(state: IdentifyState) -> (IdentifyState, Vec<Effect>) {
    let IdentifyState::Triangulating {
        discid,
        barcode,
        catalog,
        isrc,
        search,
        mut context,
    } = state
    else {
        return (state, vec![]);
    };
    // Results are judged against the text, so wait for its final snapshot.
    if !context.text_settled
        || !discid.is_settled()
        || !barcode.is_settled()
        || !catalog.is_settled()
        || !isrc.is_settled()
    {
        return (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc,
                search,
                context,
            },
            vec![],
        );
    }

    context.record_results(&discid, &barcode, &catalog, &isrc);

    // The title search goes out now or never.
    let search = if matches!(search, SearchProgress::Pending) {
        let mut effects = Vec::new();
        let started = start_search_progress(&context, &mut effects);
        if !effects.is_empty() {
            return (
                IdentifyState::Triangulating {
                    discid,
                    barcode,
                    catalog,
                    isrc,
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
                isrc,
                search,
                context,
            },
            vec![],
        );
    } else {
        search
    };
    context.record_search(&search);

    // Every lookup is in: fetch every offered record's document and rank
    // once more with what they state. A row whose tracklist holds other
    // tracks than the folder drops out, and the rows below it move up; a row
    // the documents raise to the top is read in turn, and so is each twin an
    // offered MusicBrainz record's document names. This goes on until every
    // offered row's records and twins are read, or no row is left.
    if matches!(context.documents, DocumentReading::Reading(_)) {
        return (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc,
                search,
                context,
            },
            vec![],
        );
    }
    let documents = context.documents.documents();
    let offered: Vec<crate::import::MetadataRef> =
        offered_rows(&context).into_iter().flatten().collect();
    let releases: Vec<crate::import::MetadataRef> = offered
        .iter()
        .filter(|release| {
            !documents
                .releases
                .iter()
                .any(|reading| reading.release == **release)
        })
        .cloned()
        .collect();
    let twins = context.twins_to_read(&offered);
    if !releases.is_empty() || !twins.is_empty() {
        context.documents = DocumentReading::Reading(documents);
        let effects = vec![Effect::ReadReleases {
            releases,
            twins,
            track_lengths_ms: context.audio.track_lengths_ms.clone(),
        }];
        return (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                isrc,
                search,
                context,
            },
            effects,
        );
    }
    context.documents = DocumentReading::Read(documents);
    let kept = context.album_links_to_keep();
    let effects = if kept.is_empty() {
        Vec::new()
    } else {
        vec![Effect::KeepAlbumLinks { kept }]
    };

    // The only place a ledger is recorded; later readers show this one.
    let ledger = context
        .has_inputs()
        .then(|| run_view(&discid, &barcode, &catalog, &isrc, &search, &context));

    // Nothing was asked of anyone, so nothing was found wanting.
    if matches!(
        discid,
        DiscidProgress::Skipped | DiscidProgress::NotAsked { .. }
    ) && matches!(
        barcode,
        BarcodeProgress::Skipped | BarcodeProgress::NotAsked { .. }
    ) && matches!(catalog, CatalogProgress::Skipped)
        && matches!(isrc, IsrcProgress::Skipped | IsrcProgress::NotAsked { .. })
        && matches!(
            search,
            SearchProgress::Skipped | SearchProgress::NotAsked { .. }
        )
    {
        return (
            IdentifyState::ManualOnly {
                track_count: context.audio.track_count,
                ledger,
                context,
            },
            effects,
        );
    }

    (re_derive(context, ledger), effects)
}

/// The recorded results, combined: what the run offers and sets aside.
fn combined(context: &SignalsContext) -> (Findings, LibraryStatuses) {
    combine_results(
        context.lookup_results(),
        context.twins(),
        &context.text,
        context.folder_audio(),
    )
}

/// The rows the run offers as its results stand, each as its records.
fn offered_rows(context: &SignalsContext) -> Vec<Vec<crate::import::MetadataRef>> {
    let findings = combined(context).0;
    let mut rows: Vec<(u32, Vec<crate::import::MetadataRef>)> = Vec::new();
    for (result, row) in findings.matches.iter().zip(&findings.pressings) {
        let record = crate::import::MetadataRef::new(result.source, result.release_id.clone());
        match rows.iter_mut().find(|(numbered, _)| numbered == row) {
            Some((_, records)) => records.push(record),
            None => rows.push((*row, vec![record])),
        }
    }
    rows.into_iter().map(|(_, records)| records).collect()
}

/// Combine the recorded results into `Failed`, `NotFoundAnywhere` or `Found`.
fn re_derive(context: SignalsContext, ledger: Option<IdentifyRunView>) -> IdentifyState {
    // Combine whatever failed: one provider failing leaves the others' matches.
    let (findings, library_statuses) = combined(&context);
    let track_count = context.audio.track_count;
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
    BarcodeEvidence, CatalogEvidence, ChosenCatalog, DiscIdEvidence, IsrcEvidence, SearchEvidence,
    SignalsContext, TitleSearch,
};
use progress::{
    barcode_progress_state, barcode_settled_state, catalog_progress_state, catalog_settled_state,
    discid_progress_state, isrc_progress_state, isrc_settled_state, search_progress_at_start,
    settled_identity_state, start_barcode_progress, start_catalog_progress, start_discid_progress,
    start_isrc_progress, start_search_progress,
};
pub use progress::{
    BarcodeProgress, CatalogProgress, DiscidProgress, IsrcProgress, LookupResults, LookupState,
    ProviderLookup, SearchProgress, ValueLookup,
};

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

/// `re_derive` for tests in sibling modules; records no ledger.
#[cfg(test)]
pub(crate) fn re_derive_for_tests(context: SignalsContext) -> IdentifyState {
    re_derive(context, None)
}

/// The terminal state these settled pipes land on, ledger and all, for tests
/// in sibling modules. The audio's tags carry no ISRC.
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
        isrc: IsrcProgress::Skipped,
        search,
        context,
    })
    .0
}

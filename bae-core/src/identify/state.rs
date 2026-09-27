//! The identify pipeline's pure state machine: `step` takes a state and an
//! event and returns the next state and the lookups for the service to run.
//!
//! The disc-ID, barcode and catalog lookups run in parallel, each provider
//! answering for itself. When they name nothing, the run searches by the
//! candidate's title. Once every lookup settles, the run reads its MusicBrainz
//! albums' links to Discogs (see [`crate::import::album_links`]), then the
//! full documents of the rows it offers, then what the list's releases print
//! for the albums no link joins; it combines the results into a terminal
//! state, and records the ledger it showed.

use super::combine::{combine_results, Findings, LibraryStatuses};
use super::documents::{DocumentReading, ReleaseReading};
use super::toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
use super::view::{run_view, IdentifyRunView};
use crate::config::IdentificationSteps;
use crate::db::LibraryStatus;
use crate::import::album_links::{self, GroupReading, ToRead};
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::{Catalog, LookupChoices};
use crate::signals::{ArtworkScan, AudioFacts, BarcodeSignal, LookupFailure, Signals};

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
}

impl IdentifyState {
    /// The carried context; `None` only for `Idle`.
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
            | IdentifyState::Failed { .. } => true,
            IdentifyState::Idle | IdentifyState::Triangulating { .. } => false,
        }
    }

    /// The disc-ID, barcode and catalog badges; none for `Idle`.
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

    /// One provider answered the title search.
    SearchAnswered {
        source: Catalog,
        outcome: LookupOutcome,
    },

    /// What reading the groups `Effect::ReadAlbumLinks` named answered.
    AlbumLinksRead {
        read: Vec<GroupReading>,
    },

    /// The documents `Effect::ReadReleases` asked for, record by record.
    ReleasesRead {
        read: Vec<ReleaseReading>,
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
    /// Ask one provider for the title.
    SearchTitle {
        source: Catalog,
        query: TitleSearch,
    },
    /// Read what these MusicBrainz release groups are on the other catalog.
    ReadAlbumLinks {
        to_read: ToRead,
    },
    /// Fetch and store these records' full documents, reading each one's
    /// tracklist against `track_lengths_ms`.
    ReadReleases {
        releases: Vec<crate::import::MetadataRef>,
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
                    search,
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
            IdentifyEvent::SignalsUpdated {
                signals,
                audio,
                artwork,
            },
        ) => apply_signals(
            discid, barcode, catalog, search, context, signals, audio, artwork,
        ),

        (
            IdentifyState::Triangulating {
                discid: DiscidProgress::Computing | DiscidProgress::LookingUp,
                barcode,
                catalog,
                search,
                context,
            },
            IdentifyEvent::DiscidLookupFailed { failure },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Failed { failure },
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
            IdentifyEvent::DiscidLookupCompleted { results },
        ) => settle_if_ready(IdentifyState::Triangulating {
            discid: DiscidProgress::Done { results },
            barcode,
            catalog,
            search,
            context,
        }),

        (
            IdentifyState::Triangulating {
                discid,
                barcode: BarcodeProgress::Lookups { mut codes },
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
            if let Some(lookup) = codes.iter_mut().find(|lookup| lookup.value == for_barcode) {
                lookup.answer(source, outcome);
            }
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode: BarcodeProgress::Lookups { codes },
                catalog,
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
                search,
                context,
            })
        }

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
            context.album_links = AlbumLinkReading::LinksRead(read);
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search,
                context,
            })
        }

        (
            IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
                search,
                mut context,
            },
            IdentifyEvent::ReleasesRead { read },
        ) if matches!(context.documents, DocumentReading::Reading(_)) => {
            let mut documents = context.documents.read().to_vec();
            documents.extend(read);
            context.documents = DocumentReading::Read(documents);
            settle_if_ready(IdentifyState::Triangulating {
                discid,
                barcode,
                catalog,
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
    search: SearchProgress,
    mut context: SignalsContext,
    signals: Signals,
    audio: AudioFacts,
    artwork: ArtworkScan,
) -> (IdentifyState, Vec<Effect>) {
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
            None,
            context.steps.look_up_barcodes,
            &context.providers,
            &mut effects,
        ),
        (BarcodeProgress::Scanning, BarcodeSignal::Absent) => BarcodeProgress::Skipped,
        (BarcodeProgress::Scanning, BarcodeSignal::Failed { failure, .. }) => {
            BarcodeProgress::ScanFailed {
                failure: failure.clone(),
            }
        }
        (barcode, _) => barcode,
    };

    // A chosen number the snapshot no longer offers loses its lookup.
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

/// Once every step has settled, record the results and combine them into a
/// terminal state; until then, stay in `Triangulating`.
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
    // Results are judged against the text, so wait for its final snapshot.
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

    context.record_results(&discid, &barcode, &catalog);

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

    // Every lookup is in: read the albums' links, where there are any to read.
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
        AlbumLinkReading::LinksRead(_)
        | AlbumLinkReading::Read(_)
        | AlbumLinkReading::NotAsked { .. } => {}
    }

    // The albums are read: fetch every offered record's document and rank
    // once more with what they state, until every offered row's records are
    // read — a row the documents raise to the top is read in turn — or more
    // rows are offered than a run reads, which then leaves them to the
    // person. Then read what the list's releases print for the albums no
    // catalog's document links, once for the run, keep what each group was
    // read to be, and read any row those joins raise.
    let mut effects = Vec::new();
    if matches!(context.documents, DocumentReading::Reading(_)) {
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
    loop {
        let read = context.documents.read().to_vec();
        let offered = offered_rows(&context);
        let releases: Vec<crate::import::MetadataRef> = offered
            .iter()
            .flatten()
            .filter(|release| !read.iter().any(|reading| reading.release == **release))
            .cloned()
            .collect();
        if !releases.is_empty() && offered.len() <= super::documents::MOST_ROWS_READ {
            context.documents = DocumentReading::Reading(read);
            effects.push(Effect::ReadReleases {
                releases,
                track_lengths_ms: context.audio.track_lengths_ms.clone(),
            });
            return (
                IdentifyState::Triangulating {
                    discid,
                    barcode,
                    catalog,
                    search,
                    context,
                },
                effects,
            );
        }
        context.documents = DocumentReading::Read(read);
        let AlbumLinkReading::LinksRead(links) = &context.album_links else {
            break;
        };
        let found = context.lookup_results();
        let twins = context.twins();
        let list: Vec<&crate::import::search::MetadataResult> = found
            .iter()
            .flatten()
            .map(|(result, _)| result)
            .chain(twins.iter().map(|twin| &twin.result))
            .collect();
        let links = album_links::read_the_list(links.clone(), &list);
        effects.push(Effect::KeepAlbumLinks {
            kept: album_links::to_keep(&links),
        });
        context.album_links = AlbumLinkReading::Read(links);
    }

    // The only place a ledger is recorded; later readers show this one.
    let ledger = context
        .has_inputs()
        .then(|| run_view(&discid, &barcode, &catalog, &search, &context));

    // Nothing was asked of anyone, so nothing was found wanting.
    if matches!(
        discid,
        DiscidProgress::Skipped | DiscidProgress::NotAsked { .. }
    ) && matches!(
        barcode,
        BarcodeProgress::Skipped | BarcodeProgress::NotAsked { .. }
    ) && matches!(catalog, CatalogProgress::Skipped)
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
    let [discid_results, barcode_results, catalog_results, search_results] =
        context.lookup_results();
    combine_results(
        discid_results,
        barcode_results,
        catalog_results,
        search_results,
        context.twins(),
        &context.text,
        super::medium::FolderAudio {
            rip: &context.rip,
            mono: context.audio.mono,
            track_count: context.audio.track_count,
        },
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
    AlbumLinkReading, BarcodeEvidence, CatalogEvidence, ChosenCatalog, DiscIdEvidence,
    SearchEvidence, SignalsContext, TitleSearch,
};
use progress::{
    barcode_progress_state, barcode_settled_state, catalog_progress_state, catalog_settled_state,
    discid_progress_state, search_progress_at_start, settled_identity_state,
    start_barcode_progress, start_catalog_progress, start_discid_progress, start_search_progress,
};
pub use progress::{
    BarcodeProgress, CatalogProgress, DiscidProgress, LookupResults, LookupState, ProviderLookup,
    SearchProgress, ValueLookup,
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
/// in sibling modules.
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

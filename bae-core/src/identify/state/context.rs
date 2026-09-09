//! What a settled identify run carries forward: the raw signals it ran
//! against, the user's exclusions, and every provider's answer.
//!
//! Held apart from the reducer because it is what makes a re-combine free:
//! each answer that lands folds into it instead of re-fetching what the others
//! found, and it is what tells "nothing was learned" apart from "the lookup
//! ran and found nothing".
//!
//! It belongs to the run. A state stood back up from a stored verdict carries
//! an empty one: that run is over, and what it showed is the ledger the state
//! carries.
//!
//! One type per signal, each holding that signal's input, whether the current
//! selection uses it, what its lookup returned, and how it failed — the four
//! facts every caller here reads together. The three are not the same shape and
//! so are not one type parameterised over the signal: only the barcode can fail
//! before any provider is asked and names which of several codes matched, only
//! the disc ID has a single provider and so a single failure, and the catalog
//! has no checkbox at all — choosing a number is what turns it on.

use super::{
    BarcodeProgress, CatalogProgress, DiscidProgress, LibraryStatus, MetadataResult, SourceFailure,
};
use crate::identify::agreements::CandidateText;
use crate::identify::IdentifyFailure;
use crate::import::{LookupChoices, MetadataSource};
use crate::signals::{
    ArtworkScan, BarcodeSignal, DiscIdSignal, LookupFailure, Signals, SourcedValue, TextSignal,
};

/// The disc-ID signal and what asking about it produced. The disc-ID endpoint
/// is MusicBrainz's alone, so there is one failure here, not a per-provider
/// list.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscIdEvidence {
    /// The disc-ID signal (value + its inherent `DiscToc` origin).
    pub signal: DiscIdSignal,
    /// Whether the user unchecked the disc ID.
    pub excluded: bool,
    /// The lookup's results, once settled. Empty while looking up or when the
    /// disc-ID pipe was skipped / found nothing.
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    /// Whatever failure settled the disc-ID pipe into `DiscidProgress::Failed`
    /// (see `record`), if any. Two things can put it there:
    /// `start_discid_progress` copies it straight from
    /// [`crate::signals::DiscIdSignal::Failed`] when the disc ID itself
    /// couldn't be computed (no readable TOC — reachable only on the
    /// re-identify path today, `signals/service.rs`, not on the import-scan
    /// path that feeds this pipeline), or a `DiscidLookupFailed` event closes
    /// out a lookup that ran against a disc ID that computed fine. Either way
    /// `results` is left empty exactly as it would be for a clean no-match, so
    /// this is what lets a caller lifting a settled state into a stored verdict
    /// tell "nothing was learned" apart from "the lookup ran and found nothing"
    /// (see [`crate::identify::TerminalVerdict`]).
    pub failure: Option<LookupFailure>,
}

impl Default for DiscIdEvidence {
    /// Nothing known yet: no disc artifact seen, nothing excluded, nothing
    /// asked.
    fn default() -> Self {
        Self {
            signal: DiscIdSignal::Absent { track_count: 0 },
            excluded: false,
            results: Vec::new(),
            failure: None,
        }
    }
}

impl DiscIdEvidence {
    /// Take the input from a new snapshot. The exclusion is the user's and the
    /// results are the lookup's; neither is an input, so both stand.
    fn refresh_input(&mut self, signal: &DiscIdSignal) {
        self.signal = signal.clone();
    }

    /// Record what the settled pipe found.
    fn record(&mut self, progress: &DiscidProgress) {
        self.results = progress.results();
        self.failure = match progress {
            DiscidProgress::Failed { failure, .. } => Some(failure.clone()),
            _ => None,
        };
    }

    /// `results` as the current selection sees them: nothing when the user
    /// unchecked the disc ID. Takes the results rather than reading `self.results`
    /// so a lookup still in flight can be combined under the same rule.
    pub(crate) fn active<T>(&self, results: Vec<T>) -> Vec<T> {
        if self.excluded {
            Vec::new()
        } else {
            results
        }
    }

    /// The results combine sees — empty when the signal is unchecked.
    pub(super) fn active_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        self.active(self.results.clone())
    }

    /// A failure belonging to evidence the current selection still uses. A
    /// lookup already in flight is allowed to finish after exclusion, but its
    /// answer no longer participates in the derived state.
    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
        if self.excluded {
            return;
        }
        if let Some(failure) = &self.failure {
            into.push(IdentifyFailure::DiscId(failure.clone()));
        }
    }
}

/// The candidate's barcodes and what asking about them produced. Every
/// configured provider walks the codes on its own, so failures are per
/// provider — and reading the codes off the artwork can itself fail, before any
/// provider is asked.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarcodeEvidence {
    /// Every sighting of a barcode in the candidate's files, with its origin.
    /// One code read off two images is two entries; the walks ask each code
    /// once.
    pub codes: Vec<SourcedValue>,
    /// Whether there was a barcode source at all. Empty `codes` is ambiguous on
    /// its own — artwork scanned that held no barcode, and nothing to scan,
    /// both produce an empty vec but settle differently — so the distinction
    /// `BarcodeSignal` draws between `Settled { codes: [] }` and `Absent` has
    /// to be carried, not re-derived.
    pub had_source: bool,
    /// Whether the user unchecked the barcode.
    pub excluded: bool,
    /// The lookup's results, once settled.
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    /// The providers that did not answer. Independent of `results`: one
    /// provider can answer while another fails, and the pane shows what was
    /// found while naming what did not.
    pub failures: Vec<SourceFailure>,
    /// Why reading the candidate's barcodes failed, where it did — an artwork
    /// analysis that did not finish, not a provider's answer. No lookup ran, so
    /// this is not one of `failures`.
    pub scan_failure: Option<LookupFailure>,
    /// Which barcode produced `results`. `None` until matched.
    pub matched: Option<String>,
}

impl BarcodeEvidence {
    /// Take the inputs from a new snapshot: the codes, whether there was
    /// anything to read them off, and why reading them failed where it did.
    fn refresh_input(&mut self, signal: &BarcodeSignal) {
        self.codes = signal.codes().to_vec();
        self.had_source = !matches!(signal, BarcodeSignal::Absent);
        self.scan_failure = match signal {
            BarcodeSignal::Failed { failure, .. } => Some(failure.clone()),
            BarcodeSignal::Scanning { .. }
            | BarcodeSignal::Settled { .. }
            | BarcodeSignal::Absent => None,
        };
    }

    /// Record what the settled pipe found. Both settled shapes carry provider
    /// failures: a lookup one provider answered and another failed is `Done`
    /// with failures on it.
    fn record(&mut self, progress: &BarcodeProgress) {
        self.results = progress.results();
        self.failures = progress.failures();
        self.scan_failure = progress.scan_failure().cloned();
        self.matched = progress.matched_barcode();
    }

    /// The codes the walks ask, each once, in the order they were first seen.
    pub fn code_values(&self) -> Vec<String> {
        unique_values(&self.codes)
    }

    /// `results` as the current selection sees them: nothing when the user
    /// unchecked the barcode. Takes the results rather than reading
    /// `self.results` so a lookup still in flight can be combined under the same
    /// rule.
    pub(crate) fn active<T>(&self, results: Vec<T>) -> Vec<T> {
        if self.excluded {
            Vec::new()
        } else {
            results
        }
    }

    /// The results combine sees — empty when the signal is unchecked.
    pub(super) fn active_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        self.active(self.results.clone())
    }

    /// Failures belonging to evidence the current selection still uses.
    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
        if self.excluded {
            return;
        }
        if let Some(failure) = &self.scan_failure {
            into.push(IdentifyFailure::BarcodeScan(failure.clone()));
        }
        into.extend(self.failures.iter().cloned().map(IdentifyFailure::Barcode));
    }
}

/// One catalog number the run looks up, and what asking every provider about
/// it produced.
#[derive(Clone, Debug, PartialEq)]
pub struct ChosenCatalog {
    pub value: String,
    /// The number's lookup results, once settled.
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    /// The providers that did not answer about it.
    pub failures: Vec<SourceFailure>,
}

impl ChosenCatalog {
    /// A number the run looks up, before its lookup has run.
    pub(crate) fn new(value: String) -> Self {
        Self {
            value,
            results: Vec::new(),
            failures: Vec::new(),
        }
    }
}

/// The catalog numbers extracted from the candidate and what asking about the
/// chosen ones produced. There is no checkbox: choosing a number is what turns
/// it on, and choosing it again turns it back off. Several can be on at once,
/// each with its own lookup, because one number can name thirty releases and
/// the next one none — which of them is the disc's is the person's to see.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogEvidence {
    /// Every sighting of a catalog number in the candidate's files, with its
    /// origin. One number read off two images is two entries.
    pub numbers: Vec<SourcedValue>,
    /// The numbers the run looks up, in the order they were chosen. Empty — the
    /// resting state — keeps the catalog out of the combine entirely.
    pub chosen: Vec<ChosenCatalog>,
    /// The numbers the person struck out of the candidate's text, so that a
    /// result carrying one of them earns no catalog agreement from it. Read
    /// at the run's start beside the chosen ones, and applied to the text
    /// every snapshot rebuilds.
    pub struck_out: Vec<String>,
}

impl CatalogEvidence {
    /// Take the extracted numbers from a new snapshot. A choice has to be one
    /// of the values on the list, so once the list is final a chosen number it
    /// does not offer is dropped along with its lookup.
    ///
    /// Only once it is final. The numbers stream out of the artwork pass, so a
    /// snapshot taken while it is still reading offers only what has been read
    /// so far — and dropping a stored choice against a half-read list would
    /// take a number out of the run because the OCR had not reached it yet.
    fn refresh_input(&mut self, text: &TextSignal) {
        self.numbers = text.catalogs().to_vec();
        if matches!(text, TextSignal::Scanning { .. }) {
            return;
        }
        self.chosen
            .retain(|chosen| self.numbers.iter().any(|c| c.value == chosen.value));
    }

    /// Record what the settled pipe found, number by number.
    fn record(&mut self, progress: &CatalogProgress) {
        for chosen in &mut self.chosen {
            chosen.results = progress.results_for(&chosen.value);
            chosen.failures = progress.failures_for(&chosen.value);
        }
    }

    /// Whether the run looks `value` up.
    pub fn is_chosen(&self, value: &str) -> bool {
        self.chosen.iter().any(|chosen| chosen.value == value)
    }

    /// The values the run looks up, each once, in the order they were chosen.
    pub fn chosen_values(&self) -> Vec<String> {
        self.chosen
            .iter()
            .map(|chosen| chosen.value.clone())
            .collect()
    }

    /// The numbers offered, each once, in the order they were first seen —
    /// a number's sightings fold into one tile.
    pub fn number_values(&self) -> Vec<String> {
        unique_values(&self.numbers)
    }

    /// `results` as the current selection sees them. Nothing chosen means
    /// nothing ran, so the catalog takes no part. Takes the results rather than
    /// reading the recorded ones so a lookup still in flight can be combined
    /// under the same rule.
    pub(crate) fn active<T>(&self, results: Vec<T>) -> Vec<T> {
        if self.chosen.is_empty() {
            Vec::new()
        } else {
            results
        }
    }

    /// The results combine sees: every chosen number's, in chosen order.
    /// Nothing chosen means nothing ran, so they are empty and the catalog
    /// takes no part.
    pub(super) fn active_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        self.chosen
            .iter()
            .flat_map(|chosen| chosen.results.iter().cloned())
            .collect()
    }

    /// Every chosen number's provider failures, in chosen order.
    pub(super) fn recorded_failures(&self) -> Vec<SourceFailure> {
        self.chosen
            .iter()
            .flat_map(|chosen| chosen.failures.iter().cloned())
            .collect()
    }

    /// Failures belonging to evidence the current selection still uses: every
    /// chosen number's.
    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
        for chosen in &self.chosen {
            into.extend(
                chosen
                    .failures
                    .iter()
                    .cloned()
                    .map(IdentifyFailure::Catalog),
            );
        }
    }
}

/// Each value among `sightings` once, in the order it was first seen.
fn unique_values(sightings: &[SourcedValue]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for sighting in sightings {
        if !out.contains(&sighting.value) {
            out.push(sighting.value.clone());
        }
    }
    out
}

/// Everything a state needs to re-derive its outcome as answers land —
/// carried unchanged through every non-`Idle` state.
///
/// The three signals' evidence drives the toolbar badges, and the results each
/// one recorded let a re-combine happen without re-fetching. What the run was
/// told to ask about survives every new snapshot: it is the person's decision
/// about the candidate, not something a snapshot states.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalsContext {
    /// The providers this run asks — MusicBrainz, and Discogs when it is
    /// configured. Fixed when the run starts, so a lookup that starts later,
    /// like a chosen catalog number's, asks the same ones.
    pub providers: Vec<MetadataSource>,
    /// Where the artwork pass has got to, from the latest snapshot. Progress
    /// a surface shows, not an input the lookups read; a context stood up
    /// from a stored verdict never saw a pass and reads `Absent`.
    pub artwork: ArtworkScan,
    pub disc: DiscIdEvidence,
    pub barcode: BarcodeEvidence,
    pub catalog: CatalogEvidence,
    /// The candidate's own text, normalized for lookup — what a result is
    /// judged against. The candidate's, not the run's: it is read off the
    /// folder rather than produced by anything the run asked, and a state
    /// stood back up from a stored verdict carries the stored pool so its rows
    /// badge and order exactly as they did while the run went.
    pub text: CandidateText,
    /// The candidate's local track count.
    pub track_count: u32,
}

impl Default for SignalsContext {
    /// Nothing read, nobody asked. Two states hold it: a run before its first
    /// snapshot, and a state stood back up from a stored verdict — that run is
    /// over, and what it showed is the ledger the state carries, not anything
    /// re-derived from here.
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            artwork: ArtworkScan::Absent,
            disc: DiscIdEvidence::default(),
            barcode: BarcodeEvidence::default(),
            catalog: CatalogEvidence::default(),
            text: CandidateText::default(),
            track_count: 0,
        }
    }
}

impl SignalsContext {
    /// No signals known yet — the context on entry to `Triangulating`, before
    /// the first `SignalsUpdated`. `providers` is what the run will ask, and
    /// `choices` is what the person decided it asks about: the exclusions are
    /// set and every chosen catalog number is chosen, with nothing found for
    /// any of them yet.
    pub(super) fn started(providers: Vec<MetadataSource>, choices: LookupChoices) -> Self {
        Self {
            providers,
            disc: DiscIdEvidence {
                excluded: choices.disc_id_excluded,
                ..Default::default()
            },
            barcode: BarcodeEvidence {
                excluded: choices.barcode_excluded,
                ..Default::default()
            },
            catalog: CatalogEvidence {
                numbers: Vec::new(),
                chosen: choices
                    .chosen_catalogs
                    .into_iter()
                    .map(ChosenCatalog::new)
                    .collect(),
                struck_out: choices.discounted_catalogs,
            },
            ..Default::default()
        }
    }

    /// Take the inputs from a new snapshot, keeping what the run was told to
    /// ask about. Results aren't touched — they're recorded as the lookups
    /// settle.
    pub(super) fn refresh_inputs(&mut self, signals: &Signals, artwork: ArtworkScan) {
        self.artwork = artwork;
        self.disc.refresh_input(&signals.disc_id);
        self.barcode.refresh_input(&signals.barcode);
        self.catalog.refresh_input(&signals.text);
        self.text = CandidateText::of(&signals.text_pool, &self.catalog.struck_out);
        self.track_count = signals.disc_id.track_count();
    }

    pub(super) fn record_results(
        &mut self,
        discid: &DiscidProgress,
        barcode: &BarcodeProgress,
        catalog: &CatalogProgress,
    ) {
        self.disc.record(discid);
        self.barcode.record(barcode);
        self.catalog.record(catalog);
    }

    /// Failures belonging to evidence the current selection still uses.
    pub(super) fn active_failures(&self) -> Vec<IdentifyFailure> {
        let mut failures = Vec::new();
        self.disc.active_failures(&mut failures);
        self.barcode.active_failures(&mut failures);
        self.catalog.active_failures(&mut failures);
        failures
    }

    /// Whether extraction handed this run anything at all: a disc ID it read
    /// or failed to read, a barcode source, a catalog number. A context with
    /// none was stood up from a stored verdict, or belongs to a folder that
    /// carries nothing to look up — either way there is no run to lay out.
    pub fn has_inputs(&self) -> bool {
        !matches!(self.disc.signal, DiscIdSignal::Absent { .. })
            || self.barcode.had_source
            || !self.barcode.codes.is_empty()
            || self.barcode.scan_failure.is_some()
            || !self.catalog.numbers.is_empty()
    }
}

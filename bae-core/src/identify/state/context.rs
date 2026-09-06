//! What a settled identify run carries forward: the raw signals it ran
//! against, the user's exclusions, and every provider's answer.
//!
//! Held apart from the reducer because it is what makes a re-combine free: a
//! toggle or a re-run reads it instead of re-fetching, and lifting a settled
//! state into a stored verdict reads it to tell "nothing was learned" apart
//! from "the lookup ran and found nothing".
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
use crate::identify::IdentifyFailure;
use crate::import::MetadataSource;
use crate::signals::{
    ArtworkScan, BarcodeSignal, DiscIdSignal, LookupFailure, Signals, SourcedValue,
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
    /// (see [`super::verdict::TerminalVerdict`]).
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

    /// Drop what the last lookup left, for a re-run that replaces it.
    pub(super) fn clear_lookup(&mut self) {
        self.results.clear();
        self.failure = None;
    }

    /// The results combine sees — empty when the signal is unchecked.
    pub(super) fn active_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.excluded {
            Vec::new()
        } else {
            self.results.clone()
        }
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
    /// The barcode code payloads with their origins.
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
    /// with failures on it. The matched code competes against the one already
    /// recorded, so a pipe stood back up from this evidence keeps it.
    fn record(&mut self, progress: &BarcodeProgress) {
        self.results = progress.results();
        self.failures = progress.failures();
        self.scan_failure = progress.scan_failure().cloned();
        self.matched = progress.matched_barcode(self.matched.as_deref());
    }

    /// Drop what the last lookup left, for a re-run that replaces it. The scan
    /// failure stays: it says the codes were never readable, which the re-run
    /// has to honour rather than settle as a no-match.
    pub(super) fn clear_lookup(&mut self) {
        self.results.clear();
        self.failures.clear();
        self.matched = None;
    }

    /// The results combine sees — empty when the signal is unchecked.
    pub(super) fn active_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.excluded {
            Vec::new()
        } else {
            self.results.clone()
        }
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

/// The catalog numbers extracted from the candidate and what asking about the
/// chosen one produced. There is no checkbox: choosing a number is what turns
/// the signal on, and choosing it again turns it back off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogEvidence {
    /// Every catalog number extracted from the candidate, with its origin —
    /// what the catalog badge's list offers.
    pub numbers: Vec<SourcedValue>,
    /// Which of `numbers` the run is using. `None` — the resting state — keeps
    /// the catalog out of the combine entirely.
    pub chosen: Option<String>,
    /// The chosen number's lookup results, once settled.
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    /// The providers that did not answer.
    pub failures: Vec<SourceFailure>,
}

impl CatalogEvidence {
    /// Take the extracted numbers from a new snapshot. A chosen number the new
    /// snapshot no longer offers is dropped along with its lookup; the choice
    /// has to be one of the values on the list.
    fn refresh_input(&mut self, numbers: &[SourcedValue]) {
        self.numbers = numbers.to_vec();
        if let Some(chosen) = &self.chosen {
            if !self.numbers.iter().any(|c| &c.value == chosen) {
                self.chosen = None;
                self.clear_lookup();
            }
        }
    }

    /// Record what the settled pipe found.
    fn record(&mut self, progress: &CatalogProgress) {
        self.results = progress.results();
        self.failures = progress.failures();
    }

    /// Clear what the last catalog lookup left, for a choice that replaces it.
    pub(super) fn clear_lookup(&mut self) {
        self.results.clear();
        self.failures.clear();
    }

    /// The results combine sees. Nothing chosen means nothing ran, so they are
    /// empty and the catalog takes no part.
    pub(super) fn active_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.chosen.is_none() {
            Vec::new()
        } else {
            self.results.clone()
        }
    }

    /// Failures belonging to evidence the current selection still uses.
    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
        if self.chosen.is_some() {
            into.extend(self.failures.iter().cloned().map(IdentifyFailure::Catalog));
        }
    }
}

/// Everything a settled state needs to re-derive its outcome when the user toggles
/// a signal or re-runs — carried unchanged through every non-`Idle` state.
///
/// The three signals' evidence drives the toolbar badges and the `ReRun`
/// re-dispatch, and the results each one recorded let a toggle re-combine
/// without re-fetching. What the user checked survives every new snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalsContext {
    /// The providers this run asks — MusicBrainz, and Discogs when it is
    /// configured. Fixed when the run starts (or re-runs), so a lookup that
    /// starts later, like a chosen catalog number's, asks the same ones.
    pub providers: Vec<MetadataSource>,
    /// Where the artwork pass has got to, from the latest snapshot. Progress
    /// a surface shows, not an input the lookups read; a context stood up
    /// from a stored verdict never saw a pass and reads `Absent`.
    pub artwork: ArtworkScan,
    pub disc: DiscIdEvidence,
    pub barcode: BarcodeEvidence,
    pub catalog: CatalogEvidence,
    /// The candidate's local track count.
    pub track_count: u32,
}

impl SignalsContext {
    /// No signals known yet — the context on entry to `Triangulating`, before the
    /// first `SignalsUpdated`. `providers` is what the run will ask.
    pub(super) fn empty(providers: Vec<MetadataSource>) -> Self {
        Self {
            providers,
            artwork: ArtworkScan::Absent,
            disc: DiscIdEvidence::default(),
            barcode: BarcodeEvidence::default(),
            catalog: CatalogEvidence::default(),
            track_count: 0,
        }
    }

    /// Take the inputs from a new snapshot, keeping what the user checked.
    /// Results aren't touched — they're recorded as the lookups settle.
    pub(super) fn refresh_inputs(&mut self, signals: &Signals, artwork: ArtworkScan) {
        self.artwork = artwork;
        self.disc.refresh_input(&signals.disc_id);
        self.barcode.refresh_input(&signals.barcode);
        self.catalog.refresh_input(signals.text.catalogs());
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
}

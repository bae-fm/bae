//! What a settled identify run carries forward: the raw signals it ran
//! against, the user's exclusions, and every provider's answer.
//!
//! Held apart from the reducer because it is what makes a re-combine free: a
//! toggle or a re-run reads it instead of re-fetching, and lifting a settled
//! state into a stored verdict reads it to tell "nothing was learned" apart
//! from "the lookup ran and found nothing".

use super::{
    BarcodeProgress, CatalogProgress, DiscidProgress, LibraryStatus, MetadataResult, SourceFailure,
};
use crate::import::MetadataSource;
use crate::signals::{
    ArtworkScan, BarcodeSignal, DiscIdSignal, LookupFailure, Signals, SourcedValue,
};

/// Everything a settled state needs to re-derive its outcome when the user toggles
/// a signal or re-runs — carried unchanged through every non-`Idle` state.
///
/// The inputs (`disc_id`, `barcode_codes`, `catalogs`) drive the toolbar badges and
/// the `ReRun` re-dispatch; the settled results (`discid_results`,
/// `barcode_results`, `matched_barcode`) let a toggle re-combine without
/// re-fetching. `excluded` survives every new snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalsContext {
    /// The providers this run asks — MusicBrainz, and Discogs when it is
    /// configured. Fixed when the run starts (or re-runs), so a lookup that
    /// starts later, like a chosen catalog number's, asks the same ones.
    pub providers: Vec<MetadataSource>,
    /// The disc-ID signal (value + its inherent `DiscToc` origin).
    pub disc_id: DiscIdSignal,
    /// Where the artwork pass has got to, from the latest snapshot. Progress
    /// a surface shows, not an input the lookups read; a context stood up
    /// from a stored verdict never saw a pass and reads `Absent`.
    pub artwork: ArtworkScan,
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
    /// The providers that did not answer the barcode lookup. Independent of
    /// `barcode_results`: one provider can answer while another fails, and the
    /// pane shows what was found while naming what did not.
    pub barcode_failures: Vec<SourceFailure>,
    /// Why reading the candidate's barcodes failed, where it did — an artwork
    /// analysis that did not finish, not a provider's answer. No lookup ran, so
    /// this is not one of `barcode_failures`.
    pub barcode_scan_failure: Option<LookupFailure>,
    /// The providers that did not answer the catalog lookup.
    pub catalog_failures: Vec<SourceFailure>,
    /// Which barcode produced `barcode_results`. `None` until matched.
    pub matched_barcode: Option<String>,
    /// The candidate's local track count.
    pub track_count: u32,
}

impl SignalsContext {
    /// No signals known yet — the context on entry to `Triangulating`, before the
    /// first `SignalsUpdated`. `providers` is what the run will ask.
    pub(super) fn empty(providers: Vec<MetadataSource>) -> Self {
        Self {
            providers,
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            artwork: ArtworkScan::Absent,
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
            barcode_failures: Vec::new(),
            barcode_scan_failure: None,
            catalog_failures: Vec::new(),
            matched_barcode: None,
            track_count: 0,
        }
    }

    /// Take the inputs from a new snapshot, keeping what the user checked.
    /// Results aren't touched — they're recorded as the lookups settle. A
    /// chosen catalog number that the new snapshot no longer offers is dropped;
    /// the choice has to be one of the values on the list.
    pub(super) fn refresh_inputs(&mut self, signals: &Signals, artwork: ArtworkScan) {
        self.disc_id = signals.disc_id.clone();
        self.artwork = artwork;
        self.barcode_codes = signals.barcode.codes().to_vec();
        self.had_barcode_source = !matches!(signals.barcode, BarcodeSignal::Absent);
        self.barcode_scan_failure = match &signals.barcode {
            BarcodeSignal::Failed { failure, .. } => Some(failure.clone()),
            BarcodeSignal::Scanning { .. }
            | BarcodeSignal::Settled { .. }
            | BarcodeSignal::Absent => None,
        };
        self.catalogs = signals.text.catalogs().to_vec();
        if let Some(chosen) = &self.chosen_catalog {
            if !self.catalogs.iter().any(|c| &c.value == chosen) {
                self.chosen_catalog = None;
                self.catalog_results.clear();
                self.catalog_failures.clear();
            }
        }
        self.track_count = signals.disc_id.track_count();
    }

    pub(super) fn record_results(
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
        // Both settled shapes carry provider failures: a lookup one provider
        // answered and another failed is `Done` with failures on it.
        self.barcode_failures = barcode.failures();
        self.barcode_scan_failure = barcode.scan_failure().cloned();
        self.catalog_failures = catalog.failures();
        self.matched_barcode = barcode.matched_barcode(self.matched_barcode.as_deref());
    }

    /// The disc-ID results combine sees — empty when the signal is unchecked.
    pub(super) fn active_discid_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.disc_excluded {
            Vec::new()
        } else {
            self.discid_results.clone()
        }
    }

    /// The barcode results combine sees — empty when the signal is unchecked.
    pub(super) fn active_barcode_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.barcode_excluded {
            Vec::new()
        } else {
            self.barcode_results.clone()
        }
    }

    /// The catalog results combine sees. Nothing chosen means nothing ran, so
    /// they are empty and the catalog takes no part.
    pub(super) fn active_catalog_results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        if self.chosen_catalog.is_none() {
            Vec::new()
        } else {
            self.catalog_results.clone()
        }
    }

    /// Clear what the last catalog lookup left, for a choice that replaces it.
    pub(super) fn clear_catalog_lookup(&mut self) {
        self.catalog_results.clear();
        self.catalog_failures.clear();
    }

    /// Failures belonging to evidence the current selection still uses.
    /// Lookups already in flight are allowed to finish after exclusion, but
    /// their answer no longer participates in the derived state.
    pub(super) fn active_failures(&self) -> Vec<crate::identify::IdentifyFailure> {
        let mut failures = Vec::new();
        if !self.disc_excluded {
            if let Some(failure) = &self.discid_failure {
                failures.push(crate::identify::IdentifyFailure::DiscId(failure.clone()));
            }
        }
        if !self.barcode_excluded {
            if let Some(failure) = &self.barcode_scan_failure {
                failures.push(crate::identify::IdentifyFailure::BarcodeScan(
                    failure.clone(),
                ));
            }
            failures.extend(
                self.barcode_failures
                    .iter()
                    .cloned()
                    .map(crate::identify::IdentifyFailure::Barcode),
            );
        }
        if self.chosen_catalog.is_some() {
            failures.extend(
                self.catalog_failures
                    .iter()
                    .cloned()
                    .map(crate::identify::IdentifyFailure::Catalog),
            );
        }
        failures
    }
}

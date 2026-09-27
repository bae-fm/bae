//! What a run carries forward: the signals it ran against, what the person
//! left out or chose, and every provider's answer, so answers re-combine
//! without re-fetching. A state stood back up from a stored verdict carries an
//! empty one.

use super::{
    BarcodeProgress, CatalogProgress, DiscidProgress, LibraryStatus, MetadataResult,
    SearchProgress, SourceFailure,
};
use crate::config::IdentificationSteps;
use crate::identify::agreements::CandidateText;
use crate::identify::documents::DocumentReading;
use crate::identify::{IdentifyFailure, NotAskedReason};
use crate::import::album_links::{self, GroupReading, Twin};
use crate::import::{Catalog, LookupChoices};
use crate::signals::{
    ArtworkScan, AudioFacts, BarcodeSignal, DiscIdSignal, LookupFailure, RipEvidence, Signals,
    SourcedValue, TextSignal,
};

/// The disc-ID signal and what asking about it produced; one provider, so one
/// failure.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscIdEvidence {
    pub signal: DiscIdSignal,
    /// Whether the person left the disc ID out.
    pub excluded: bool,
    /// The lookup's results, once settled.
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    /// Why the disc ID could not be computed or looked up, which tells
    /// "nothing was learned" from an empty `results`.
    pub failure: Option<LookupFailure>,
    /// Why nobody was asked about the disc ID, where nobody was.
    pub not_asked: Option<NotAskedReason>,
}

impl Default for DiscIdEvidence {
    fn default() -> Self {
        Self {
            signal: DiscIdSignal::Absent,
            excluded: false,
            results: Vec::new(),
            failure: None,
            not_asked: None,
        }
    }
}

impl DiscIdEvidence {
    /// Take the input from a new snapshot, keeping the exclusion and results.
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
        self.not_asked = match progress {
            DiscidProgress::NotAsked { reason, .. } => Some(*reason),
            _ => None,
        };
    }

    /// The disc ID's failure, where it has one.
    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
        if let Some(failure) = &self.failure {
            into.push(IdentifyFailure::DiscId(failure.clone()));
        }
    }
}

/// The candidate's barcodes and what asking every provider about them
/// produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarcodeEvidence {
    /// Every sighting of a barcode in the candidate's files; a code seen twice
    /// is two entries but asked about once.
    pub codes: Vec<SourcedValue>,
    /// Whether there was a barcode source at all, which tells "found none"
    /// from "nothing to read" when `codes` is empty.
    pub had_source: bool,
    /// The codes the person left out of the run.
    pub excluded: Vec<String>,
    /// The lookup's results, once settled.
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    /// The providers that failed; others may still have answered.
    pub failures: Vec<SourceFailure>,
    /// Why reading the barcodes off the artwork failed, before any provider
    /// was asked.
    pub scan_failure: Option<LookupFailure>,
    /// Which barcode produced `results`. `None` until matched.
    pub matched: Option<String>,
    /// Why nobody was asked about any of the codes, where nobody was.
    pub not_asked: Option<NotAskedReason>,
}

impl BarcodeEvidence {
    /// Take the inputs from a new snapshot.
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

    /// Record what the settled pipe found.
    fn record(&mut self, progress: &BarcodeProgress) {
        self.results = progress.results();
        self.failures = progress.failures();
        self.scan_failure = progress.scan_failure().cloned();
        self.matched = progress.matched_barcode();
        self.not_asked = match progress {
            BarcodeProgress::NotAsked { reason, .. } => Some(*reason),
            _ => None,
        };
    }

    /// The codes the candidate carries, each once, in first-seen order.
    pub fn code_values(&self) -> Vec<String> {
        SourcedValue::values(&self.codes)
    }

    /// `code_values` less the ones the person left out.
    pub fn asked_code_values(&self) -> Vec<String> {
        self.code_values()
            .into_iter()
            .filter(|code| !self.excluded.contains(code))
            .collect()
    }

    /// Whether the candidate carries codes and the person left out every one.
    pub fn every_code_excluded(&self) -> bool {
        !self.codes.is_empty() && self.asked_code_values().is_empty()
    }

    /// The scan failure and every provider failure.
    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
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
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    pub failures: Vec<SourceFailure>,
}

impl ChosenCatalog {
    pub(crate) fn new(value: String) -> Self {
        Self {
            value,
            results: Vec::new(),
            failures: Vec::new(),
        }
    }
}

/// The catalog numbers extracted from the candidate and what asking about the
/// chosen ones produced; a number is looked up only once the person chooses it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogEvidence {
    /// The numbers extracted, each once, in first-seen order.
    pub numbers: Vec<String>,
    /// The numbers the run looks up, in the order they were chosen.
    pub chosen: Vec<ChosenCatalog>,
    /// The numbers the person struck out of the candidate's text, so a result
    /// carrying one earns no catalog agreement from it.
    pub struck_out: Vec<String>,
}

impl CatalogEvidence {
    /// Take the extracted numbers from a new snapshot, dropping a chosen
    /// number the list no longer offers — but only once the list is final.
    fn refresh_input(&mut self, text: &TextSignal) {
        self.numbers = text.catalogs().to_vec();
        if matches!(text, TextSignal::Scanning { .. }) {
            return;
        }
        self.chosen
            .retain(|chosen| self.numbers.contains(&chosen.value));
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

    /// The values the run looks up, in the order they were chosen.
    pub fn chosen_values(&self) -> Vec<String> {
        self.chosen
            .iter()
            .map(|chosen| chosen.value.clone())
            .collect()
    }

    /// Every chosen number's results, in chosen order.
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

    /// Every chosen number's failures.
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

/// The album title and artist a run searches by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleSearch {
    pub album: String,
    /// The first album artist's name, or blank.
    pub artist: String,
}

impl TitleSearch {
    /// The search for these words; `None` when there is no title.
    pub fn of(album: &str, artist: &str) -> Option<Self> {
        let album = album.trim();
        (!album.is_empty()).then(|| Self {
            album: album.to_string(),
            artist: artist.trim().to_string(),
        })
    }

    /// The search for a draft's own title, with trailing bracketed parts such
    /// as `[MR2002]` taken off, since catalogs match nothing with them.
    pub fn of_draft(album: &str, artist: &str) -> Option<Self> {
        let words = crate::signals::candidate_text::strip_trailing_brackets(album);
        let album = if words.is_empty() { album } else { &words };
        Self::of(album, artist)
    }
}

/// The title the run can search by, and what asking every provider produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchEvidence {
    /// What the draft offered when the run started.
    pub query: Option<TitleSearch>,
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    /// The providers that failed; others may still have answered.
    pub failures: Vec<SourceFailure>,
}

impl SearchEvidence {
    fn record(&mut self, progress: &SearchProgress) {
        self.results = progress.results();
        self.failures = progress.failures();
    }

    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
        into.extend(self.failures.iter().cloned().map(IdentifyFailure::Search));
    }
}

/// Everything a state needs to re-derive its outcome as answers land, carried
/// through every non-`Idle` state.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalsContext {
    /// The catalogs this run asks, fixed when it starts.
    pub providers: Vec<Catalog>,
    /// The steps this run takes, fixed when it starts.
    pub steps: IdentificationSteps,
    /// Where the artwork pass has got to, from the latest snapshot.
    pub artwork: ArtworkScan,
    /// What the candidate's files say about the medium its audio was ripped
    /// from.
    pub rip: RipEvidence,
    /// The audio being identified, read off its files.
    pub audio: AudioFacts,
    pub disc: DiscIdEvidence,
    pub barcode: BarcodeEvidence,
    pub catalog: CatalogEvidence,
    pub search: SearchEvidence,
    /// The candidate's own text, normalized — what a result is judged against.
    pub text: CandidateText,
    /// Whether `text` is final; a run does not settle before it is.
    pub text_settled: bool,
    /// What the run's MusicBrainz albums are on Discogs, read once every
    /// lookup has settled.
    pub album_links: AlbumLinkReading,
    /// The full documents of the rows the run offers, read once the album
    /// links are.
    pub documents: DocumentReading,
}

/// Where a run is with the album links of what its lookups returned.
#[derive(Clone, Debug, PartialEq)]
pub enum AlbumLinkReading {
    /// Waiting for every lookup to settle.
    Pending,
    /// The links of these groups are being read.
    Reading,
    /// The catalogs' documents are read, group by group; what the list's
    /// releases print is read once the offered rows' own documents are in.
    LinksRead(Vec<GroupReading>),
    /// Read, the list too, group by group; empty when there was nothing to
    /// join.
    Read(Vec<GroupReading>),
    /// Not read, for `reason`.
    NotAsked { reason: NotAskedReason },
}

impl Default for SignalsContext {
    /// Nothing read, nobody asked.
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            steps: IdentificationSteps::default(),
            artwork: ArtworkScan::Absent,
            rip: RipEvidence::Unproven,
            audio: AudioFacts::default(),
            disc: DiscIdEvidence::default(),
            barcode: BarcodeEvidence::default(),
            catalog: CatalogEvidence::default(),
            search: SearchEvidence::default(),
            text: CandidateText::default(),
            text_settled: false,
            album_links: AlbumLinkReading::Pending,
            documents: DocumentReading::Pending,
        }
    }
}

impl SignalsContext {
    /// The context a run starts with, before its first snapshot.
    pub(super) fn started(
        providers: Vec<Catalog>,
        steps: IdentificationSteps,
        choices: LookupChoices,
        title_search: Option<TitleSearch>,
    ) -> Self {
        Self {
            providers,
            steps,
            album_links: if steps.follow_catalog_links {
                AlbumLinkReading::Pending
            } else {
                AlbumLinkReading::NotAsked {
                    reason: NotAskedReason::SwitchedOff,
                }
            },
            search: SearchEvidence {
                query: title_search,
                ..Default::default()
            },
            disc: DiscIdEvidence {
                excluded: choices.disc_id_excluded,
                ..Default::default()
            },
            barcode: BarcodeEvidence {
                excluded: choices.excluded_barcodes,
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

    /// Take the inputs from a new snapshot, keeping choices and results.
    pub(super) fn refresh_inputs(
        &mut self,
        signals: &Signals,
        audio: AudioFacts,
        artwork: ArtworkScan,
    ) {
        self.artwork = artwork;
        self.rip = signals.rip.clone();
        self.audio = audio;
        self.disc.refresh_input(&signals.disc_id);
        self.barcode.refresh_input(&signals.barcode);
        self.catalog.refresh_input(&signals.text);
        self.text = CandidateText::of(&signals.text_pool, &self.catalog.struck_out);
        self.text_settled = !matches!(signals.text, TextSignal::Scanning { .. });
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

    /// Record what the title search found; it settles after the identifiers.
    pub(super) fn record_search(&mut self, search: &SearchProgress) {
        self.search.record(search);
    }

    /// What reading the run's albums answered, once it has.
    fn album_readings(&self) -> &[GroupReading] {
        match &self.album_links {
            AlbumLinkReading::LinksRead(read) | AlbumLinkReading::Read(read) => read,
            AlbumLinkReading::Pending
            | AlbumLinkReading::Reading
            | AlbumLinkReading::NotAsked { .. } => &[],
        }
    }

    /// Every lookup's results with their album links and documents applied,
    /// in the order combine takes them.
    pub(super) fn lookup_results(&self) -> [Vec<(MetadataResult, LibraryStatus)>; 4] {
        let read = self.album_readings();
        [
            self.disc.results.clone(),
            self.barcode.results.clone(),
            self.catalog.active_results(),
            self.search.results.clone(),
        ]
        .map(|results| {
            results
                .into_iter()
                .map(|(mut result, status)| {
                    album_links::apply(&mut result, read);
                    self.documents.apply(&mut result);
                    (result, status)
                })
                .collect()
        })
    }

    /// The releases reading the albums found that no lookup returned, with
    /// their documents applied.
    pub(super) fn twins(&self) -> Vec<Twin> {
        self.album_readings()
            .iter()
            .filter_map(|reading| reading.twin.clone())
            .map(|mut twin| {
                self.documents.apply(&mut twin.result);
                twin
            })
            .collect()
    }

    pub(super) fn active_failures(&self) -> Vec<IdentifyFailure> {
        let mut failures = Vec::new();
        self.disc.active_failures(&mut failures);
        self.barcode.active_failures(&mut failures);
        self.catalog.active_failures(&mut failures);
        self.search.active_failures(&mut failures);
        failures
    }

    /// Whether extraction gave this run anything to lay out: a disc ID, a
    /// barcode source, or a catalog number.
    pub fn has_inputs(&self) -> bool {
        !matches!(self.disc.signal, DiscIdSignal::Absent)
            || self.barcode.had_source
            || !self.barcode.codes.is_empty()
            || self.barcode.scan_failure.is_some()
            || !self.catalog.numbers.is_empty()
    }
}

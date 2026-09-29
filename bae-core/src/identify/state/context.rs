//! What a run carries forward: the signals it ran against, what the person
//! left out or chose, and every provider's answer, so answers re-combine
//! without re-fetching. A state stood back up from a stored verdict carries an
//! empty one.

use super::{
    BarcodeProgress, CatalogProgress, DiscidProgress, IsrcProgress, LibraryStatus, MetadataResult,
    SearchProgress, SourceFailure,
};
use crate::config::IdentificationSteps;
use crate::identify::agreements::CandidateText;
use crate::identify::combine::LookupAnswers;
use crate::identify::documents::{DocumentReading, Twin, TwinToRead};
use crate::identify::{IdentifyFailure, NotAskedReason};
use crate::import::album_links::{self, AlbumLink, GroupLinks};
use crate::import::MetadataRef;
use crate::import::{Catalog, LookupChoices};
use crate::signals::{
    ArtworkScan, AudioFacts, AudioOrigin, BarcodeSignal, DiscIdSignal, LookupFailure, Signals,
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
    }

    /// Record what the settled pipe found.
    fn record(&mut self, progress: &BarcodeProgress) {
        self.results = progress.results();
        self.failures = progress.failures();
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

    /// Every provider failure.
    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
        into.extend(self.failures.iter().cloned().map(IdentifyFailure::Barcode));
    }
}

/// The ISRCs the audio's tags carry and what asking MusicBrainz about them
/// produced; one provider, so one failure.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IsrcEvidence {
    /// Each audio file's ISRC, in the files' order, as the signals carry them.
    pub tagged: Vec<String>,
    /// The lookup's results, once settled.
    pub results: Vec<(MetadataResult, LibraryStatus)>,
    /// Why the lookup failed, which tells "nothing was learned" from an empty
    /// `results`.
    pub failure: Option<LookupFailure>,
    /// Why nobody was asked about the codes, where nobody was.
    pub not_asked: Option<NotAskedReason>,
}

impl IsrcEvidence {
    /// Each code once, in first-tagged order: what the lookup asks about.
    pub fn codes(&self) -> Vec<String> {
        let mut codes: Vec<String> = Vec::new();
        for code in &self.tagged {
            if !codes.contains(code) {
                codes.push(code.clone());
            }
        }
        codes
    }

    /// Where most of the audio's recordings were registered — see
    /// [`crate::isrc::registered_in`].
    fn registered_in(&self) -> Option<crate::pressing::ReleaseArea> {
        crate::isrc::registered_in(self.tagged.iter().map(String::as_str))
    }

    /// Record what the settled lookup found.
    fn record(&mut self, progress: &IsrcProgress) {
        self.results = progress.results();
        self.failure = match progress {
            IsrcProgress::Failed { failure } => Some(failure.clone()),
            _ => None,
        };
        self.not_asked = match progress {
            IsrcProgress::NotAsked { reason } => Some(*reason),
            _ => None,
        };
    }

    /// The lookup's failure, where it has one.
    fn active_failures(&self, into: &mut Vec<IdentifyFailure>) {
        if let Some(failure) = &self.failure {
            into.push(IdentifyFailure::Isrc(failure.clone()));
        }
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
    /// as `[MR2002]` taken off, since catalogs match nothing with them — see
    /// `text_match::bare_album_title`.
    pub fn of_draft(album: &str, artist: &str) -> Option<Self> {
        Self::of(&crate::text_match::bare_album_title(album), artist)
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
    /// What the candidate's files say about where its audio came from.
    pub origin: AudioOrigin,
    /// The audio being identified, read off its files.
    pub audio: AudioFacts,
    pub disc: DiscIdEvidence,
    pub barcode: BarcodeEvidence,
    pub catalog: CatalogEvidence,
    pub isrc: IsrcEvidence,
    /// Each track's title as the folder gives it, from the latest snapshot.
    pub track_titles: Vec<String>,
    pub search: SearchEvidence,
    /// The candidate's own text, normalized — what a result is judged against.
    pub text: CandidateText,
    /// Whether `text` is final; a run does not settle before it is.
    pub text_settled: bool,
    /// The full documents of the rows the run offers, and of their twins,
    /// read once every lookup has settled.
    pub documents: DocumentReading,
}

impl Default for SignalsContext {
    /// Nothing read, nobody asked.
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            steps: IdentificationSteps::default(),
            artwork: ArtworkScan::Absent,
            origin: AudioOrigin::default(),
            audio: AudioFacts::default(),
            disc: DiscIdEvidence::default(),
            barcode: BarcodeEvidence::default(),
            catalog: CatalogEvidence::default(),
            isrc: IsrcEvidence::default(),
            track_titles: Vec::new(),
            search: SearchEvidence::default(),
            text: CandidateText::default(),
            text_settled: false,
            documents: DocumentReading::Pending,
        }
    }
}

impl SignalsContext {
    /// What the folder's files say about its audio, as combine reads it.
    pub(crate) fn folder_audio(&self) -> crate::identify::medium::FolderAudio<'_> {
        crate::identify::medium::FolderAudio {
            origin: &self.origin,
            mono: self.audio.mono,
            track_count: self.audio.track_count,
            track_titles: &self.track_titles,
            registered_in: self.isrc.registered_in(),
        }
    }

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
        self.origin = signals.origin.clone();
        self.isrc.tagged = signals.isrcs.clone();
        self.track_titles = signals.track_titles.clone();
        self.audio = audio;
        self.disc.refresh_input(&signals.disc_id);
        self.barcode.refresh_input(&signals.barcode);
        self.catalog.refresh_input(&signals.text);
        self.text = CandidateText::of(
            &signals.text_pool,
            &self.catalog.struck_out,
            signals.barcode.codes(),
        );
        self.text_settled = !matches!(signals.text, TextSignal::Scanning { .. });
    }

    pub(super) fn record_results(
        &mut self,
        discid: &DiscidProgress,
        barcode: &BarcodeProgress,
        catalog: &CatalogProgress,
        isrc: &IsrcProgress,
    ) {
        self.disc.record(discid);
        self.barcode.record(barcode);
        self.catalog.record(catalog);
        self.isrc.record(isrc);
    }

    /// Record what the title search found; it settles after the identifiers.
    pub(super) fn record_search(&mut self, search: &SearchProgress) {
        self.search.record(search);
    }

    /// Every lookup's results with their documents applied, and what each
    /// MusicBrainz album on the list was read to be, in the order combine
    /// takes them.
    pub(super) fn lookup_results(&self) -> LookupAnswers {
        let read = |results: Vec<(MetadataResult, LibraryStatus)>| -> Vec<_> {
            results
                .into_iter()
                .map(|(mut result, status)| {
                    self.documents.apply(&mut result);
                    (result, status)
                })
                .collect()
        };
        let mut answers = LookupAnswers {
            disc_id: read(self.disc.results.clone()),
            barcode: read(self.barcode.results.clone()),
            catalog: read(self.catalog.active_results()),
            isrc: read(self.isrc.results.clone()),
            search: read(self.search.results.clone()),
        };
        let groups = self.album_groups(&answers);
        for (result, _) in [
            &mut answers.disc_id,
            &mut answers.barcode,
            &mut answers.catalog,
            &mut answers.isrc,
            &mut answers.search,
        ]
        .into_iter()
        .flatten()
        {
            album_links::apply(result, &groups);
        }
        answers
    }

    /// The twins read, with their documents applied.
    pub(super) fn twins(&self) -> Vec<Twin> {
        self.documents
            .twins()
            .iter()
            .cloned()
            .map(|mut twin| {
                self.documents.apply(&mut twin.result);
                twin
            })
            .collect()
    }

    /// What each MusicBrainz album on the list — `answers` and the twins —
    /// was read to be, from the documents read so far. Nothing, for a run
    /// that does not join records across catalogs.
    fn album_groups(&self, answers: &LookupAnswers) -> Vec<GroupLinks> {
        if !self.steps.follow_catalog_links {
            return Vec::new();
        }
        let twins = self.twins();
        let list: Vec<&MetadataResult> = answers
            .all()
            .map(|(result, _)| result)
            .chain(twins.iter().map(|twin| &twin.result))
            .collect();
        album_links::read_groups(&list, |release| {
            self.documents
                .album_statements(&MetadataRef::new(Catalog::MusicBrainz, release))
        })
    }

    /// What the run's MusicBrainz albums were read to be, to keep beyond it
    /// (see [`album_links::to_keep`]).
    pub(super) fn album_links_to_keep(&self) -> Vec<(String, Vec<AlbumLink>)> {
        let answers = self.lookup_results();
        let twins = self.twins();
        let list: Vec<&MetadataResult> = answers
            .all()
            .map(|(result, _)| result)
            .chain(twins.iter().map(|twin| &twin.result))
            .collect();
        album_links::to_keep(&self.album_groups(&answers), &list)
    }

    /// The Discogs releases the documents of `offered` MusicBrainz records
    /// name as themselves that are still to be read as twins: none the list
    /// holds, and none read already. None where the run does not join records
    /// across catalogs or does not ask Discogs.
    pub(super) fn twins_to_read(&self, offered: &[MetadataRef]) -> Vec<TwinToRead> {
        if !self.steps.follow_catalog_links || !self.providers.contains(&Catalog::Discogs) {
            return Vec::new();
        }
        let answers = self.lookup_results();
        let listed = |release: &MetadataRef| {
            answers.all().any(|(result, _)| {
                result.source == release.catalog && result.release_id == release.key
            }) || self.documents.twins().iter().any(|twin| {
                twin.result.source == release.catalog && twin.result.release_id == release.key
            })
        };
        let read = |release: &MetadataRef| {
            self.documents
                .read()
                .iter()
                .any(|reading| reading.release == *release)
        };
        let mut twins: Vec<TwinToRead> = Vec::new();
        for named_by in offered
            .iter()
            .filter(|record| record.catalog == Catalog::MusicBrainz)
        {
            let Some(Ok(document)) = self
                .documents
                .read()
                .iter()
                .find(|reading| reading.release == *named_by)
                .map(|reading| &reading.document)
            else {
                continue;
            };
            for release in document
                .links
                .iter()
                .filter(|link| link.catalog == Catalog::Discogs)
            {
                if listed(release)
                    || read(release)
                    || twins.iter().any(|twin| twin.release == *release)
                {
                    continue;
                }
                twins.push(TwinToRead {
                    release: release.clone(),
                    named_by: named_by.clone(),
                });
            }
        }
        twins
    }

    pub(super) fn active_failures(&self) -> Vec<IdentifyFailure> {
        let mut failures = Vec::new();
        self.disc.active_failures(&mut failures);
        self.barcode.active_failures(&mut failures);
        self.catalog.active_failures(&mut failures);
        self.isrc.active_failures(&mut failures);
        self.search.active_failures(&mut failures);
        failures
    }

    /// Whether extraction gave this run anything to lay out: a disc ID, a
    /// barcode source, a catalog number, or an ISRC.
    pub fn has_inputs(&self) -> bool {
        !matches!(self.disc.signal, DiscIdSignal::Absent)
            || self.barcode.had_source
            || !self.barcode.codes.is_empty()
            || !self.catalog.numbers.is_empty()
            || !self.isrc.tagged.is_empty()
    }
}

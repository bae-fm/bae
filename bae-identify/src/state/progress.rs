//! How each step's lookup progresses.
//!
//! Every provider answers for itself, so a lookup holds one entry per provider
//! and settles once all of them have.

use super::{Effect, LookupOutcome};
use crate::signals::DiscIdSignal;
use crate::NotAskedReason;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::{MetadataResult, SourceFailure};
use bae_catalogs::LookupFailure;

/// What one lookup produced: each match paired with its library status.
pub type LookupResults = Vec<MetadataResult>;

/// What MusicBrainz answered about one Disc ID: the releases holding the
/// disc, and each one's document as the answer states it, which the run reads
/// the release from.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscIdAnswer {
    pub results: LookupResults,
    pub documents: Vec<bae_catalogs::payloads::DiscIdRelease>,
}

#[cfg(test)]
impl DiscIdAnswer {
    /// An answer of `results` whose documents a test does not read.
    pub(crate) fn for_test(results: LookupResults) -> Self {
        Self { results, documents: Vec::new() }
    }
}

/// The disc-ID lookups' progress. Only MusicBrainz answers disc IDs, so each
/// of the candidate's disc IDs has one lookup.
#[derive(Clone, Debug, PartialEq)]
pub enum DiscidProgress {
    Computing,
    /// One lookup per disc ID, in the signal's order.
    Lookups {
        discs: Vec<DiscLookup>,
    },
    /// No disc ID was derived.
    Skipped,
    /// Disc IDs were derived and nobody was asked about them, for `reason`.
    NotAsked {
        reason: NotAskedReason,
    },
}

/// One disc ID's lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscLookup {
    pub disc_id: String,
    pub state: LookupState,
}

impl DiscidProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            DiscidProgress::Computing => false,
            DiscidProgress::Lookups { discs } => discs.iter().all(|disc| disc.state.is_settled()),
            DiscidProgress::Skipped | DiscidProgress::NotAsked { .. } => true,
        }
    }

    /// What each disc ID's lookup found, in disc order; a lookup that has
    /// not answered, or failed, found nothing.
    pub fn answers(&self) -> Vec<LookupResults> {
        self.lookups()
            .iter()
            .map(|disc| match &disc.state {
                LookupState::Done { results } => results.clone(),
                LookupState::LookingUp | LookupState::Failed { .. } => Vec::new(),
            })
            .collect()
    }

    /// Every disc ID lookup's failure, each distinct one once.
    pub fn failures(&self) -> Vec<LookupFailure> {
        let mut failures: Vec<LookupFailure> = Vec::new();
        for disc in self.lookups() {
            if let LookupState::Failed { failure } = &disc.state {
                if !failures.contains(failure) {
                    failures.push(failure.clone());
                }
            }
        }
        failures
    }

    pub fn lookups(&self) -> &[DiscLookup] {
        match self {
            DiscidProgress::Lookups { discs } => discs,
            DiscidProgress::Computing
            | DiscidProgress::Skipped
            | DiscidProgress::NotAsked { .. } => &[],
        }
    }

    /// Land the answer about `disc_id`, where it is still being waited for,
    /// returning the documents it answered with; `None` where nothing was
    /// waiting for it.
    pub(super) fn answer(
        &mut self,
        disc_id: &str,
        outcome: Result<DiscIdAnswer, LookupFailure>,
    ) -> Option<Vec<bae_catalogs::payloads::DiscIdRelease>> {
        let DiscidProgress::Lookups { discs } = self else {
            return None;
        };
        let disc = discs
            .iter_mut()
            .find(|disc| disc.disc_id == disc_id && disc.state == LookupState::LookingUp)?;
        let (state, documents) = match outcome {
            Ok(DiscIdAnswer { results, documents }) => (LookupState::Done { results }, documents),
            Err(failure) => (LookupState::Failed { failure }, Vec::new()),
        };
        disc.state = state;
        Some(documents)
    }
}

/// The ISRC lookup's progress. Only MusicBrainz answers ISRCs, and every code
/// the audio's tags carry is asked in one search, so there is one lookup.
#[derive(Clone, Debug, PartialEq)]
pub enum IsrcProgress {
    /// Waiting for the first snapshot, which carries the tags' codes.
    Reading,
    LookingUp,
    Done {
        results: LookupResults,
    },
    /// No audio file's tags carry an ISRC.
    Skipped,
    /// The tags carry codes and nobody was asked about them, for `reason`.
    NotAsked {
        reason: NotAskedReason,
    },
    Failed {
        failure: LookupFailure,
    },
}

impl IsrcProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            IsrcProgress::Reading | IsrcProgress::LookingUp => false,
            IsrcProgress::Done { .. }
            | IsrcProgress::Skipped
            | IsrcProgress::NotAsked { .. }
            | IsrcProgress::Failed { .. } => true,
        }
    }

    pub fn results(&self) -> LookupResults {
        match self {
            IsrcProgress::Done { results } => results.clone(),
            IsrcProgress::Reading
            | IsrcProgress::LookingUp
            | IsrcProgress::Skipped
            | IsrcProgress::NotAsked { .. }
            | IsrcProgress::Failed { .. } => Vec::new(),
        }
    }
}

/// One provider's part of a lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderLookup {
    pub source: LookupCatalog,
    pub state: LookupState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LookupState {
    LookingUp,
    Done { results: LookupResults },
    Failed { failure: LookupFailure },
}

impl LookupState {
    pub(super) fn is_settled(&self) -> bool {
        !matches!(self, LookupState::LookingUp)
    }
}

/// The barcode signal's progress.
#[derive(Clone, Debug, PartialEq)]
pub enum BarcodeProgress {
    /// The artwork is still being read for codes.
    Scanning,
    /// There was a barcode source and it held no code.
    NoCodes,
    /// One lookup per code the run asks about, in first-seen order; every code
    /// is asked, since a folder may hold two releases.
    Lookups { codes: Vec<ValueLookup> },
    /// The candidate has barcodes and nobody was asked about any of them, for
    /// `reason`; the codes are still listed.
    NotAsked { codes: Vec<String>, reason: NotAskedReason },
    /// No barcode source at all.
    Skipped,
}

impl BarcodeProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            BarcodeProgress::Scanning => false,
            BarcodeProgress::Lookups { codes } => codes.iter().all(ValueLookup::is_settled),
            BarcodeProgress::NoCodes
            | BarcodeProgress::NotAsked { .. }
            | BarcodeProgress::Skipped => true,
        }
    }

    /// What every code's lookup found, in code then provider order.
    pub fn results(&self) -> LookupResults {
        self.lookups().iter().flat_map(ValueLookup::results).collect()
    }

    /// Every provider that failed any code's lookup.
    pub fn failures(&self) -> Vec<SourceFailure> {
        self.lookups().iter().flat_map(ValueLookup::failures).collect()
    }

    /// The codes' lookups, in first-seen order.
    pub fn lookups(&self) -> &[ValueLookup] {
        match self {
            BarcodeProgress::Lookups { codes } => codes,
            BarcodeProgress::Scanning
            | BarcodeProgress::NoCodes
            | BarcodeProgress::NotAsked { .. }
            | BarcodeProgress::Skipped => &[],
        }
    }
}

/// Every provider's lookup of one value: a barcode or a chosen catalog number.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueLookup {
    pub value: String,
    pub providers: Vec<ProviderLookup>,
}

impl ValueLookup {
    /// Ask every provider about `value`, each through the effect `ask` makes.
    fn started(
        value: &str,
        providers: &[LookupCatalog],
        ask: impl Fn(LookupCatalog, String) -> Effect,
        effects: &mut Vec<Effect>,
    ) -> Self {
        Self {
            value: value.to_string(),
            providers: ask_every(providers, |source| ask(source, value.to_string()), effects),
        }
    }

    /// Land `source`'s answer, where it is still being waited for.
    pub(super) fn answer(&mut self, source: LookupCatalog, outcome: LookupOutcome) {
        answer_provider(&mut self.providers, source, outcome);
    }

    fn is_settled(&self) -> bool {
        providers_settled(&self.providers)
    }

    fn results(&self) -> LookupResults {
        providers_results(&self.providers)
    }

    fn failures(&self) -> Vec<SourceFailure> {
        providers_failures(&self.providers)
    }
}

/// Ask every one of `providers`, each through the effect `ask` makes.
pub(super) fn ask_every(
    providers: &[LookupCatalog],
    ask: impl Fn(LookupCatalog) -> Effect,
    effects: &mut Vec<Effect>,
) -> Vec<ProviderLookup> {
    providers
        .iter()
        .map(|&source| {
            effects.push(ask(source));
            ProviderLookup { source, state: LookupState::LookingUp }
        })
        .collect()
}

/// Land `source`'s answer among `providers`, where it is still being waited
/// for.
pub(super) fn answer_provider(
    providers: &mut [ProviderLookup],
    source: LookupCatalog,
    outcome: LookupOutcome,
) {
    if let Some(lookup) =
        providers.iter_mut().find(|l| l.source == source && l.state == LookupState::LookingUp)
    {
        lookup.state = match outcome {
            Ok(results) => LookupState::Done { results },
            Err(failure) => LookupState::Failed { failure },
        };
    }
}

pub(super) fn providers_settled(providers: &[ProviderLookup]) -> bool {
    providers.iter().all(|l| l.state.is_settled())
}

/// What every provider that answered found, in provider order.
pub(super) fn providers_results(providers: &[ProviderLookup]) -> LookupResults {
    providers
        .iter()
        .filter_map(|l| match &l.state {
            LookupState::Done { results } => Some(results.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

pub(super) fn providers_failures(providers: &[ProviderLookup]) -> Vec<SourceFailure> {
    providers
        .iter()
        .filter_map(|l| match &l.state {
            LookupState::Failed { failure } => {
                Some(SourceFailure { source: l.source.catalog(), failure: failure.clone() })
            }
            _ => None,
        })
        .collect()
}

// ── Starting lookups ────────────────────────────────────────────────────────

pub(super) fn start_discid_progress(
    signal: &DiscIdSignal,
    excluded: bool,
    providers: &[LookupCatalog],
    effects: &mut Vec<Effect>,
) -> DiscidProgress {
    match signal {
        DiscIdSignal::Computed { discs } => {
            // The reason nearest the value wins: see `NotAskedReason`.
            let reason = if excluded {
                Some(NotAskedReason::LeftOut)
            } else if !providers.contains(&LookupCatalog::DISC_ID) {
                Some(NotAskedReason::NoCatalog)
            } else {
                None
            };
            if let Some(reason) = reason {
                return DiscidProgress::NotAsked { reason };
            }
            DiscidProgress::Lookups {
                discs: discs
                    .iter()
                    .map(|disc| {
                        effects.push(Effect::LookupDiscid { disc_id: disc.disc_id.clone() });
                        DiscLookup { disc_id: disc.disc_id.clone(), state: LookupState::LookingUp }
                    })
                    .collect(),
            }
        }
        DiscIdSignal::Absent | DiscIdSignal::NotCdAudio => DiscidProgress::Skipped,
        DiscIdSignal::Failed { .. } => {
            unreachable!("a run whose Disc ID could not be derived ended as an error")
        }
    }
}

/// Ask MusicBrainz about every code the audio's tags carry, in one search.
pub(super) fn start_isrc_progress(
    codes: Vec<String>,
    providers: &[LookupCatalog],
    effects: &mut Vec<Effect>,
) -> IsrcProgress {
    if codes.is_empty() {
        return IsrcProgress::Skipped;
    }
    if !providers.contains(&LookupCatalog::ISRC) {
        return IsrcProgress::NotAsked { reason: NotAskedReason::NoCatalog };
    }
    effects.push(Effect::LookupIsrcs { isrcs: codes });
    IsrcProgress::LookingUp
}

/// Ask every provider about every code in `codes` the person did not leave out.
pub(super) fn start_barcode_progress(
    codes: Vec<String>,
    excluded: &[String],
    had_source: bool,
    providers: &[LookupCatalog],
    effects: &mut Vec<Effect>,
) -> BarcodeProgress {
    if codes.is_empty() {
        // Whether there was a source is what tells "found none" from "never looked".
        return if had_source { BarcodeProgress::NoCodes } else { BarcodeProgress::Skipped };
    }
    let asked: Vec<String> =
        codes.iter().filter(|code| !excluded.contains(code)).cloned().collect();
    if asked.is_empty() {
        return BarcodeProgress::NotAsked { codes, reason: NotAskedReason::LeftOut };
    }
    BarcodeProgress::Lookups {
        codes: asked
            .iter()
            .map(|code| {
                ValueLookup::started(
                    code,
                    providers,
                    |source, barcode| Effect::LookupBarcode { source, barcode },
                    effects,
                )
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;

//! The toolbar's badges: what each signal shows while its lookup runs and
//! once the run has settled.

use super::{
    BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState, IsrcProgress, SignalsContext,
};
use crate::signals::DiscIdSignal;
use crate::toolbar::{SignalKind, SignalOption, SignalState, ToolbarSignal};
use bae_catalogs::search::SourceFailure;

impl IdentifyState {
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

    /// Shows the first disc ID, with every disc ID as an option: all of them
    /// are asked about, and left out together.
    fn disc_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let disc_ids = context.disc.signal.disc_ids();
        let state = match self {
            IdentifyState::Triangulating { discid, .. } => discid_progress_state(discid),
            _ => settled_identity_state(context),
        };
        ToolbarSignal {
            kind: SignalKind::DiscId,
            shown: disc_ids.first().cloned(),
            state,
            excluded: context.disc.excluded,
            options: signal_options(&disc_ids, |_| !context.disc.excluded),
        }
    }

    /// Shows the matched code, else the first, with every code as an option.
    fn barcode_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let codes = context.barcode.code_values();
        let shown = context.barcode.matched.clone().or_else(|| codes.first().cloned());
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

    /// Shows the first number in effect, with every offered number as an
    /// option.
    fn catalog_badge(&self, context: &SignalsContext) -> ToolbarSignal {
        let first_chosen = context.catalog.in_effect().first().cloned();
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
                context.catalog.is_in_effect(value)
            }),
        }
    }
}

/// The values one signal offers, in first-seen order, each marked when chosen.
fn signal_options(values: &[String], chosen: impl Fn(&str) -> bool) -> Vec<SignalOption> {
    values
        .iter()
        .map(|value| SignalOption { value: value.clone(), chosen: chosen(value) })
        .collect()
}

// ── Toolbar badge states ────────────────────────────────────────────────────

/// Results beat failures: only lookups that found nothing read as failed.
pub(super) fn discid_progress_state(progress: &DiscidProgress) -> SignalState {
    match progress {
        DiscidProgress::Computing => SignalState::LookingUp,
        DiscidProgress::Lookups { .. } if !progress.is_settled() => SignalState::LookingUp,
        DiscidProgress::Lookups { .. } => {
            let found = crate::combine::union_all(&progress.answers()).len();
            match progress.failures().first() {
                Some(failure) if found == 0 => SignalState::Failed { failure: failure.clone() },
                _ => found_or_no_match(found as u32),
            }
        }
        DiscidProgress::Skipped => SignalState::Skipped,
        DiscidProgress::NotAsked { reason } => SignalState::NotAsked { reason: *reason },
    }
}

pub(super) fn isrc_progress_state(progress: &IsrcProgress) -> SignalState {
    match progress {
        IsrcProgress::Reading | IsrcProgress::LookingUp => SignalState::LookingUp,
        IsrcProgress::Done { results } => found_or_no_match(results.len() as u32),
        IsrcProgress::Skipped => SignalState::Skipped,
        IsrcProgress::NotAsked { reason } => SignalState::NotAsked { reason: *reason },
        IsrcProgress::Failed { failure } => SignalState::Failed { failure: failure.clone() },
    }
}

/// Results beat failures: only a lookup that found nothing reads as failed.
pub(super) fn barcode_progress_state(progress: &BarcodeProgress) -> SignalState {
    match progress {
        BarcodeProgress::Scanning => SignalState::LookingUp,
        BarcodeProgress::Lookups { .. } if !progress.is_settled() => SignalState::LookingUp,
        BarcodeProgress::Lookups { .. } => {
            settled_lookup_state(progress.results().len(), &progress.failures())
        }
        BarcodeProgress::NoCodes => SignalState::NoMatch,
        BarcodeProgress::NotAsked { reason, .. } => SignalState::NotAsked { reason: *reason },
        BarcodeProgress::Skipped => SignalState::Skipped,
    }
}

pub(super) fn catalog_progress_state(progress: &CatalogProgress) -> SignalState {
    match progress {
        CatalogProgress::Skipped => SignalState::Skipped,
        CatalogProgress::Lookups { .. } if !progress.is_settled() => SignalState::LookingUp,
        CatalogProgress::Lookups { .. } => {
            settled_lookup_state(progress.results().len(), &progress.failures())
        }
    }
}

/// A settled lookup's badge: what it found, else the first provider's
/// failure, else no match.
fn settled_lookup_state(n_results: usize, failures: &[SourceFailure]) -> SignalState {
    if n_results > 0 {
        return found_or_no_match(n_results as u32);
    }
    match failures.first() {
        Some(first) => SignalState::Failed { failure: first.failure.clone() },
        None => SignalState::NoMatch,
    }
}

// ── The badges a settled run wears, read off what it recorded ───────────────

/// The disc-ID badge of a settled run. A lookup's failure comes first where
/// no disc ID found anything, since the signal only says whether disc IDs
/// could be computed.
pub(super) fn settled_identity_state(context: &SignalsContext) -> SignalState {
    let found = context.disc.results().len();
    if let Some(failure) = context.disc.failures.first().filter(|_| found == 0) {
        return SignalState::Failed { failure: failure.clone() };
    }
    match &context.disc.signal {
        DiscIdSignal::Absent | DiscIdSignal::NotCdAudio => SignalState::Skipped,
        DiscIdSignal::Failed { .. } => {
            unreachable!("a run whose Disc ID could not be derived ended as an error")
        }
        DiscIdSignal::Computed { .. } => match context.disc.not_asked {
            Some(reason) => SignalState::NotAsked { reason },
            None => found_or_no_match(found as u32),
        },
    }
}

/// The ISRC badge of a settled run.
pub(super) fn isrc_settled_state(context: &SignalsContext) -> SignalState {
    let isrc = &context.isrc;
    if let Some(failure) = &isrc.failure {
        return SignalState::Failed { failure: failure.clone() };
    }
    if isrc.tagged.is_empty() {
        return SignalState::Skipped;
    }
    match isrc.not_asked {
        Some(reason) => SignalState::NotAsked { reason },
        None => found_or_no_match(isrc.results.len() as u32),
    }
}

/// The barcode badge of a settled run.
pub(super) fn barcode_settled_state(context: &SignalsContext) -> SignalState {
    let barcode = &context.barcode;
    if barcode.codes.is_empty() {
        return if barcode.had_source { SignalState::NoMatch } else { SignalState::Skipped };
    }
    if let Some(reason) = barcode.not_asked {
        return SignalState::NotAsked { reason };
    }
    settled_lookup_state(barcode.results.len(), &barcode.failures)
}

/// The catalog badge of a settled run, over every number searched.
pub(super) fn catalog_settled_state(context: &SignalsContext) -> SignalState {
    if context.catalog.searched.is_empty() {
        return SignalState::Skipped;
    }
    settled_lookup_state(
        context.catalog.active_results().len(),
        &context.catalog.recorded_failures(),
    )
}

pub(super) fn found_or_no_match(count: u32) -> SignalState {
    if count == 0 {
        SignalState::NoMatch
    } else {
        SignalState::Found { count }
    }
}

#[cfg(test)]
#[path = "badges_tests.rs"]
mod tests;

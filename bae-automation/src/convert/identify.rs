//! Identify's states and progress, mirrored into the JSON shapes an MCP client
//! reads. Core's `IdentifyStateView` already made every domain decision — the
//! matches are folded into their album cards, provenance is keyed by release
//! id, and an in-flight payload is reduced to a count — so this is a field copy
//! per variant and nothing else.

use super::*;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_lookup_state(view: bae_core::identify::LookupView) -> AutomationLookupState {
    use bae_core::identify::LookupView;
    match view {
        LookupView::LookingUp => AutomationLookupState::LookingUp,
        LookupView::Found { count } => AutomationLookupState::Found { count },
        LookupView::NoMatch => AutomationLookupState::NoMatch,
        LookupView::Failed { failure } => AutomationLookupState::Failed {
            failure: automation_lookup_failure(failure),
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_disc_id_step(view: bae_core::identify::DiscIdStepView) -> AutomationDiscIdStep {
    use bae_core::identify::DiscIdStepView;
    match view {
        DiscIdStepView::Reading => AutomationDiscIdStep::Reading,
        DiscIdStepView::Absent => AutomationDiscIdStep::Absent,
        DiscIdStepView::ReadFailed { failure } => AutomationDiscIdStep::ReadFailed {
            failure: automation_lookup_failure(failure),
        },
        DiscIdStepView::Read {
            disc_id,
            source_file,
            lookup,
        } => AutomationDiscIdStep::Read {
            disc_id,
            source_file,
            lookup: automation_lookup_state(lookup),
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_barcode_step(view: bae_core::identify::BarcodeStepView) -> AutomationBarcodeStep {
    use bae_core::identify::{BarcodeLookupView, BarcodeStepView};
    match view {
        BarcodeStepView::AwaitingArtwork => AutomationBarcodeStep::AwaitingArtwork,
        BarcodeStepView::Absent => AutomationBarcodeStep::Absent,
        BarcodeStepView::NoCodes => AutomationBarcodeStep::NoCodes,
        BarcodeStepView::ScanFailed { failure } => AutomationBarcodeStep::ScanFailed {
            failure: automation_lookup_failure(failure),
        },
        BarcodeStepView::Lookups { codes, providers } => AutomationBarcodeStep::Lookups {
            codes,
            providers: providers
                .into_iter()
                .map(|provider| AutomationProviderBarcodeLookup {
                    source: provider.source.into(),
                    state: match provider.state {
                        BarcodeLookupView::Trying {
                            barcode,
                            position,
                            total,
                        } => AutomationBarcodeLookupState::Trying {
                            barcode,
                            position,
                            total,
                        },
                        BarcodeLookupView::Matched { barcode, count } => {
                            AutomationBarcodeLookupState::Matched { barcode, count }
                        }
                        BarcodeLookupView::Exhausted => AutomationBarcodeLookupState::Exhausted,
                        BarcodeLookupView::Failed { failure } => {
                            AutomationBarcodeLookupState::Failed {
                                failure: automation_lookup_failure(failure),
                            }
                        }
                    },
                })
                .collect(),
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_catalog_step(view: bae_core::identify::CatalogStepView) -> AutomationCatalogStep {
    use bae_core::identify::CatalogStepView;
    match view {
        CatalogStepView::NoneFound => AutomationCatalogStep::NoneFound,
        CatalogStepView::Unchosen { available } => AutomationCatalogStep::Unchosen { available },
        CatalogStepView::Chosen { value, lookups } => AutomationCatalogStep::Chosen {
            value,
            lookups: lookups
                .into_iter()
                .map(|lookup| AutomationProviderLookup {
                    source: lookup.source.into(),
                    state: automation_lookup_state(lookup.state),
                })
                .collect(),
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_identify_run(view: bae_core::identify::IdentifyRunView) -> AutomationIdentifyRun {
    let bae_core::identify::IdentifyRunView {
        disc_id,
        barcode,
        catalog,
    } = view;
    AutomationIdentifyRun {
        disc_id: automation_disc_id_step(disc_id),
        barcode: automation_barcode_step(barcode),
        catalog: automation_catalog_step(catalog),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn automation_result_provenance(
    release_id: String,
    provenance: bae_core::identify::ResultProvenance,
) -> AutomationResultProvenance {
    let bae_core::identify::ResultProvenance {
        by_disc_id,
        by_barcode,
        by_catalog,
    } = provenance;
    AutomationResultProvenance {
        release_id,
        by_disc_id,
        by_barcode,
        by_catalog,
    }
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the JSON enum. Core has
/// already folded the matches into their group cards, keyed the provenance,
/// reduced the in-flight payloads to counts, and dropped what must not cross —
/// this is a field copy per variant and nothing else.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn automation_identify_state(
    state: bae_core::identify::IdentifyState,
) -> AutomationIdentifyState {
    use bae_core::identify::IdentifyStateView;
    match IdentifyStateView::from(state) {
        IdentifyStateView::Idle => AutomationIdentifyState::Idle,
        IdentifyStateView::Triangulating {
            run,
            groups,
            library_statuses,
            provenance,
        } => AutomationIdentifyState::Triangulating {
            run: automation_identify_run(run),
            groups: groups.into_iter().map(automation_release_group).collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(automation_library_status)
                .collect(),
            provenance: provenance
                .into_iter()
                .map(|(release_id, p)| automation_result_provenance(release_id, p))
                .collect(),
        },
        IdentifyStateView::Found {
            groups,
            library_statuses,
            track_count,
            provenance,
        } => AutomationIdentifyState::Found {
            groups: groups.into_iter().map(automation_release_group).collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(automation_library_status)
                .collect(),
            track_count,
            provenance: provenance
                .into_iter()
                .map(|(release_id, p)| automation_result_provenance(release_id, p))
                .collect(),
        },
        IdentifyStateView::NotFoundAnywhere => AutomationIdentifyState::NotFoundAnywhere,
        IdentifyStateView::ManualOnly { track_count } => {
            AutomationIdentifyState::ManualOnly { track_count }
        }
        IdentifyStateView::Failed {
            failures,
            groups,
            library_statuses,
            provenance,
        } => AutomationIdentifyState::Failed {
            failures: failures
                .into_iter()
                .map(automation_identify_failure)
                .collect(),
            groups: groups.into_iter().map(automation_release_group).collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(automation_library_status)
                .collect(),
            provenance: provenance
                .into_iter()
                .map(|(release_id, p)| automation_result_provenance(release_id, p))
                .collect(),
        },
    }
}

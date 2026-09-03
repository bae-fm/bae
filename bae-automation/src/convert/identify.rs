//! Identify's states and progress, mirrored into the JSON shapes an MCP client
//! reads. Core's `IdentifyStateView` already made every domain decision — the
//! matches are folded into their album cards, provenance is keyed by release
//! id, and an in-flight payload is reduced to a count — so this is a field copy
//! per variant and nothing else.

use super::*;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn automation_discid_progress(
    progress: bae_core::identify::DiscidProgressView,
) -> AutomationDiscidProgress {
    use bae_core::identify::DiscidProgressView;
    match progress {
        DiscidProgressView::Computing => AutomationDiscidProgress::Computing,
        DiscidProgressView::LookingUp => AutomationDiscidProgress::LookingUp,
        DiscidProgressView::Done { n_results } => AutomationDiscidProgress::Done { n_results },
        DiscidProgressView::Skipped => AutomationDiscidProgress::Skipped,
        DiscidProgressView::Failed { failure } => AutomationDiscidProgress::Failed {
            failure: automation_lookup_failure(failure),
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn automation_barcode_progress(
    progress: bae_core::identify::BarcodeProgressView,
) -> AutomationBarcodeProgress {
    use bae_core::identify::BarcodeProgressView;
    match progress {
        BarcodeProgressView::Scanning => AutomationBarcodeProgress::Scanning,
        BarcodeProgressView::LookingUp {
            current,
            position,
            total,
        } => AutomationBarcodeProgress::LookingUp {
            current,
            position,
            total,
        },
        BarcodeProgressView::Done { n_results } => AutomationBarcodeProgress::Done { n_results },
        BarcodeProgressView::Failed { failures } => AutomationBarcodeProgress::Failed {
            failures: failures
                .into_iter()
                .map(automation_source_failure)
                .collect(),
        },
        BarcodeProgressView::ScanFailed { failure } => AutomationBarcodeProgress::ScanFailed {
            failure: automation_lookup_failure(failure),
        },
        BarcodeProgressView::Skipped => AutomationBarcodeProgress::Skipped,
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
        IdentifyStateView::Triangulating { discid, barcode } => {
            AutomationIdentifyState::Triangulating {
                discid: automation_discid_progress(discid),
                barcode: automation_barcode_progress(barcode),
            }
        }
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

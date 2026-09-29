//! Identify's states and progress as the JSON an MCP client reads: a field
//! copy of core's `IdentifyStateView`. Not `Serialize` derives on core's
//! types, because some of their leaves already serialize as the stored
//! `failures_json` format, which differs from this one.

use super::*;

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationNotAskedReason = bae_core::identify::NotAskedReason,
    from_core: pub(crate) fn,
    variants: { LeftOut, SwitchedOff, NoCatalog },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationLookupState = bae_core::identify::LookupView,
    from_core: pub(crate) fn,
    variants: {
        Queued,
        NotAsked { reason: (AutomationNotAskedReason) },
        LookingUp,
        Found { count, groups: (each AutomationReleaseGroup) },
        NoMatch,
        Failed { failure: (AutomationLookupFailure) },
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationMediumConflict = bae_core::identify::MediumConflict,
    from_core: pub(crate) fn,
    variants: { CdRip, NotCdAudio },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationFolderCheck = bae_core::identify::FolderCheck,
    from_core: pub(crate) fn,
    variants: {
        TrackCountDisagrees { local, source },
        SourceTracksUnknown,
        MediumDisagrees { folder: (AutomationMediumConflict) },
    },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationProviderCell = bae_core::identify::ProviderCell,
    from_core: pub(crate) fn,
    fields: { source: (into), lookup: (AutomationLookupState) },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSignalValueRow = bae_core::identify::SignalValueRow,
    from_core: pub(crate) fn,
    fields: {
        value,
        excluded,
        cells: (each AutomationProviderCell),
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationDiscIdStep = bae_core::identify::DiscIdStepView,
    from_core: pub(crate) fn,
    variants: {
        Reading,
        Absent,
        NotCdAudio,
        Read { disc_id, lookup: (AutomationLookupState) },
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationBarcodeStep = bae_core::identify::BarcodeStepView,
    from_core: pub(crate) fn,
    variants: {
        Absent,
        CoverArtOff,
        NoCodes,
        Rows { scanning, rows: (each AutomationSignalValueRow) },
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationIsrcStep = bae_core::identify::IsrcStepView,
    from_core: pub(crate) fn,
    variants: {
        Reading,
        Absent,
        Read { isrcs, lookup: (AutomationLookupState) },
    },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationCatalogCandidate = bae_core::identify::CatalogCandidateView,
    from_core: pub(crate) fn,
    fields: { value },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationCatalogStep = bae_core::identify::CatalogStepView,
    from_core: pub(crate) fn,
    variants: {
        NoneFound,
        CoverArtOff,
        Numbers {
            scanning,
            rows: (each AutomationSignalValueRow),
            candidates: (each AutomationCatalogCandidate),
        },
    },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationCatalogAgreement = bae_core::identify::CatalogAgreementView,
    from_core: pub(crate) fn,
    fields: { value, discounted },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSearchStep = bae_core::identify::SearchStepView,
    from_core: pub(crate) fn,
    variants: {
        NotAsked { reason: (AutomationNotAskedReason) },
        NotNeeded,
        NoTitle,
        Waiting { album, artist },
        Searched { album, artist, cells: (each AutomationProviderCell) },
    },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationIdentifyRun = bae_core::identify::IdentifyRunView,
    from_core: pub(crate) fn,
    fields: {
        providers: (each into),
        disc_id: (AutomationDiscIdStep),
        barcode: (AutomationBarcodeStep),
        catalog: (AutomationCatalogStep),
        isrc: (AutomationIsrcStep),
        search: (AutomationSearchStep),
    },
}

impl AutomationIdentifyFailure {
    /// Names core's one `SourceFailure` payload as two fields.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(failure: bae_core::identify::IdentifyFailure) -> Self {
        use bae_core::identify::IdentifyFailure;
        match failure {
            IdentifyFailure::DiscId(failure) => Self::DiscId {
                failure: AutomationLookupFailure::from_core(failure),
            },
            IdentifyFailure::Barcode(failure) => Self::Barcode {
                source: failure.source.into(),
                failure: AutomationLookupFailure::from_core(failure.failure),
            },
            IdentifyFailure::Catalog(failure) => Self::Catalog {
                source: failure.source.into(),
                failure: AutomationLookupFailure::from_core(failure.failure),
            },
            IdentifyFailure::Search(failure) => Self::Search {
                source: failure.source.into(),
                failure: AutomationLookupFailure::from_core(failure.failure),
            },
            IdentifyFailure::Isrc(failure) => Self::Isrc {
                failure: AutomationLookupFailure::from_core(failure),
            },
            IdentifyFailure::ReleaseDetails(failure) => Self::ReleaseDetails {
                failure: AutomationLookupFailure::from_core(failure),
            },
        }
    }
}

/// Moves each pair's release id inside its entry.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_agreements(
    agreements: Vec<(String, bae_core::identify::RowAgreements)>,
) -> Vec<AutomationAgreements> {
    agreements
        .into_iter()
        .map(|(release_id, agreements)| {
            let bae_core::identify::RowAgreements { fields, notes } = agreements;
            let bae_core::identify::Agreements {
                disc_id,
                barcode,
                catalog,
                label,
                year,
                country,
                title,
                artist,
            } = fields;
            AutomationAgreements {
                release_id,
                disc_id,
                barcode,
                catalog,
                label,
                year,
                country,
                title,
                artist,
                notes,
            }
        })
        .collect()
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the JSON enum.
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
            agreements,
            narrowed_out_count,
        } => AutomationIdentifyState::Triangulating {
            run: AutomationIdentifyRun::from_core(run),
            groups: groups
                .into_iter()
                .map(AutomationReleaseGroup::from_core)
                .collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(AutomationLibraryStatus::from_core)
                .collect(),
            agreements: automation_agreements(agreements),
            narrowed_out_count,
        },
        IdentifyStateView::Found {
            run,
            groups,
            library_statuses,
            track_count,
            agreements,
            narrowed_out_count,
            catalog_agreements,
            folder_check,
            picks_unattended,
            offers_shared_album,
        } => AutomationIdentifyState::Found {
            run: run.map(AutomationIdentifyRun::from_core),
            groups: groups
                .into_iter()
                .map(AutomationReleaseGroup::from_core)
                .collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(AutomationLibraryStatus::from_core)
                .collect(),
            track_count,
            agreements: automation_agreements(agreements),
            narrowed_out_count,
            catalog_agreements: catalog_agreements
                .into_iter()
                .map(AutomationCatalogAgreement::from_core)
                .collect(),
            folder_check: folder_check.map(AutomationFolderCheck::from_core),
            picks_unattended,
            offers_shared_album,
        },
        IdentifyStateView::NotFoundAnywhere { run } => AutomationIdentifyState::NotFoundAnywhere {
            run: run.map(AutomationIdentifyRun::from_core),
        },
        IdentifyStateView::ManualOnly { track_count, run } => AutomationIdentifyState::ManualOnly {
            track_count,
            run: run.map(AutomationIdentifyRun::from_core),
        },
        IdentifyStateView::Error { failure } => AutomationIdentifyState::Error {
            failure: AutomationInternalFailure::from_core(failure),
        },
        IdentifyStateView::Failed {
            run,
            failures,
            groups,
            library_statuses,
            agreements,
            narrowed_out_count,
            catalog_agreements,
            offers_shared_album,
        } => AutomationIdentifyState::Failed {
            run: run.map(AutomationIdentifyRun::from_core),
            failures: failures
                .into_iter()
                .map(AutomationIdentifyFailure::from_core)
                .collect(),
            groups: groups
                .into_iter()
                .map(AutomationReleaseGroup::from_core)
                .collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(AutomationLibraryStatus::from_core)
                .collect(),
            agreements: automation_agreements(agreements),
            narrowed_out_count,
            catalog_agreements: catalog_agreements
                .into_iter()
                .map(AutomationCatalogAgreement::from_core)
                .collect(),
            offers_shared_album,
        },
    }
}

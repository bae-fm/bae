//! Identify's states and progress, mirrored into the JSON shapes an MCP client
//! reads. Core's `IdentifyStateView` already made every domain decision — the
//! matches are folded into their album cards, provenance is keyed by release
//! id, and an in-flight payload is reduced to a count — so the variants here
//! are a field copy.
//!
//! The copy cannot be replaced by `Serialize` derives on core's view types.
//! The leaves this tree is built from — `signals::LookupFailure`,
//! `import::SourceFailure`, `import::MetadataSource` — already carry a
//! `Serialize`, and it is the on-disk format of `import_candidate_verdict`'s
//! `failures_json`: externally tagged, `[{"Barcode":{"source":"MusicBrainz",
//! "failure":{"Provider":{"status":503}}}}]`. This JSON is internally tagged
//! and snake_case, and a type gets one `Serialize`, so giving core's the MCP
//! shape would leave every stored failed verdict unreadable. Two shapes, two
//! types. What is genuinely not a field copy: provenance, which core keys as
//! `(release_id, ResultProvenance)` pairs and this names inside each entry.

use super::*;

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationLookupState = bae_core::identify::LookupView,
    from_core: pub(crate) fn,
    variants: {
        Queued,
        NotAsked,
        LookingUp,
        Found { count, groups: (each AutomationReleaseGroup) },
        NoMatch,
        Failed { failure: (AutomationLookupFailure) },
    },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationValueSource = bae_core::identify::ValueSource,
    from_core: pub(crate) fn,
    fields: {
        origin: (AutomationSignalOrigin),
        file,
        region: (opt AutomationImageRegion),
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
        sources: (each AutomationValueSource),
        cells: (each AutomationProviderCell),
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationDiscIdFileKind = bae_core::identify::DiscIdFileKind,
    from_core: pub(crate) fn,
    variants: { Log, Cue },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationDiscIdFile = bae_core::identify::DiscIdFile,
    from_core: pub(crate) fn,
    fields: { kind: (AutomationDiscIdFileKind), file },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationDiscIdStep = bae_core::identify::DiscIdStepView,
    from_core: pub(crate) fn,
    variants: {
        Reading,
        Absent,
        ReadFailed { failure: (AutomationLookupFailure) },
        Read {
            disc_id,
            source: (opt AutomationDiscIdFile),
            lookup: (AutomationLookupState),
        },
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationBarcodeStep = bae_core::identify::BarcodeStepView,
    from_core: pub(crate) fn,
    variants: {
        Absent,
        NoCodes,
        ScanFailed { failure: (AutomationLookupFailure) },
        Rows { scanning, rows: (each AutomationSignalValueRow) },
    },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationCatalogCandidate = bae_core::identify::CatalogCandidateView,
    from_core: pub(crate) fn,
    fields: { value, sources: (each AutomationValueSource) },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationCatalogStep = bae_core::identify::CatalogStepView,
    from_core: pub(crate) fn,
    variants: {
        NoneFound,
        Numbers {
            scanning,
            rows: (each AutomationSignalValueRow),
            candidates: (each AutomationCatalogCandidate),
        },
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
    },
}

impl AutomationIdentifyFailure {
    /// Not a copy: core carries the source and the failure of a per-provider
    /// lookup as one `SourceFailure` payload, which this names as two fields.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(failure: bae_core::identify::IdentifyFailure) -> Self {
        use bae_core::identify::IdentifyFailure;
        match failure {
            IdentifyFailure::DiscId(failure) => Self::DiscId {
                failure: AutomationLookupFailure::from_core(failure),
            },
            IdentifyFailure::BarcodeScan(failure) => Self::BarcodeScan {
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
            IdentifyFailure::ReleaseDetails(failure) => Self::ReleaseDetails {
                failure: AutomationLookupFailure::from_core(failure),
            },
        }
    }
}

/// Not a copy: core keys provenance as `(release_id, ResultProvenance)` pairs,
/// which this names inside each entry.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_provenance(
    provenance: Vec<(String, bae_core::identify::ResultProvenance)>,
) -> Vec<AutomationResultProvenance> {
    provenance
        .into_iter()
        .map(|(release_id, provenance)| {
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
        })
        .collect()
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the JSON enum. Core has
/// already folded the matches into their group cards, keyed the provenance,
/// reduced the in-flight payloads to counts, and dropped what must not cross,
/// so every variant is a field copy — except provenance, whose pairs this names.
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
            run: AutomationIdentifyRun::from_core(run),
            groups: groups
                .into_iter()
                .map(AutomationReleaseGroup::from_core)
                .collect(),
            library_statuses: library_statuses
                .into_iter()
                .map(AutomationLibraryStatus::from_core)
                .collect(),
            provenance: automation_provenance(provenance),
        },
        IdentifyStateView::Found {
            run,
            groups,
            library_statuses,
            track_count,
            provenance,
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
            provenance: automation_provenance(provenance),
        },
        IdentifyStateView::NotFoundAnywhere { run } => AutomationIdentifyState::NotFoundAnywhere {
            run: run.map(AutomationIdentifyRun::from_core),
        },
        IdentifyStateView::ManualOnly { track_count, run } => AutomationIdentifyState::ManualOnly {
            track_count,
            run: run.map(AutomationIdentifyRun::from_core),
        },
        IdentifyStateView::Failed {
            run,
            failures,
            groups,
            library_statuses,
            provenance,
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
            provenance: automation_provenance(provenance),
        },
    }
}

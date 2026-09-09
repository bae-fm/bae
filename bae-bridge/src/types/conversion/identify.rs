use super::super::*;

impl BridgeMetadataResult {
    pub(crate) fn from_core(r: bae_core::import::search::MetadataResult) -> Self {
        let bae_core::import::search::MetadataResult {
            source,
            release_id,
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
            source_group_id,
            // Dropped: the card carries the album's title/artist/cover, so a
            // pressing projection keeps only pressing-distinguishing fields.
            title: _,
            artist: _,
            cover_art: _,
            // The source's own tracklist is Ready-rule evidence, not something
            // a pressing row renders; the sidebar reads the classification the
            // rule produced from it.
            source_tracks: _,
        } = r;
        BridgeMetadataResult {
            source: BridgeMetadataSource::from_core(source),
            release_id,
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
            source_group_id,
        }
    }
}
impl BridgeRemoteCover {
    pub(crate) fn from_core(c: bae_core::import::cover_art::RemoteCover) -> Self {
        let bae_core::import::cover_art::RemoteCover {
            url,
            thumbnail_url,
            label,
            source,
        } = c;
        let selection = bridge_remote_cover_selection(url, source);
        let cover_choice = remote_cover_choice_to_bridge(&selection, &thumbnail_url);
        BridgeRemoteCover {
            cover_choice,
            label,
        }
    }
}

fn bridge_remote_cover_selection(
    url: String,
    source: bae_core::import::MetadataSource,
) -> BridgeRemoteCoverSelection {
    BridgeRemoteCoverSelection {
        url,
        source: BridgeMetadataSource::from_core(source),
    }
}

fn remote_cover_choice_to_bridge(
    selection: &BridgeRemoteCoverSelection,
    thumbnail_url: &str,
) -> BridgeCoverChoice {
    BridgeCoverChoice {
        selection: BridgeCoverSelection::RemoteCover {
            selection: selection.clone(),
        },
        preview_source: BridgeCoverImageSource::Remote {
            url: selection.url.clone(),
        },
        thumbnail_source: BridgeCoverImageSource::Remote {
            url: thumbnail_url.to_string(),
        },
    }
}

impl BridgeReleaseDetail {
    pub(crate) fn from_core(d: bae_core::import::search::ImportSearchReleaseDetail) -> Self {
        // Derived values borrow `&d`; compute them before destructuring `d`.
        let default_cover = d
            .default_cover()
            .cloned()
            .map(BridgeRemoteCover::from_core)
            .map(|c| c.cover_choice);
        let bae_core::import::search::ImportSearchReleaseDetail {
            release_id,
            source,
            source_group_id,
            title,
            artist,
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
            track_count,
            tracks,
            cover_art,
        } = d;
        BridgeReleaseDetail {
            release_id,
            source: BridgeMetadataSource::from_core(source),
            source_group_id,
            title,
            artist,
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
            track_count,
            tracks: tracks
                .into_iter()
                .map(BridgeReleaseTrack::from_core)
                .collect(),
            cover_art: cover_art
                .into_iter()
                .map(BridgeRemoteCover::from_core)
                .collect(),
            default_cover,
        }
    }
}

mirror_struct! {
    BridgeReleaseTrack = bae_core::import::search::ReleaseTrack,
    from_core: pub(crate) fn,
    fields: { title, artist, duration_ms, position, side },
}

impl BridgeCoverChoice {
    pub(crate) fn from_core(choice: bae_core::import::CoverChoice) -> Self {
        let bae_core::import::CoverChoice {
            selection,
            preview,
            thumbnail,
        } = choice;
        Self {
            selection: match selection {
                bae_core::import::CoverSelection::Local(file_id) => {
                    BridgeCoverSelection::ReleaseImage { file_id }
                }
                bae_core::import::CoverSelection::Remote(url, source) => {
                    BridgeCoverSelection::RemoteCover {
                        selection: bridge_remote_cover_selection(url, source),
                    }
                }
                bae_core::import::CoverSelection::Embedded(source_file_id) => {
                    BridgeCoverSelection::EmbeddedCover { source_file_id }
                }
            },
            preview_source: BridgeCoverImageSource::from_core(preview),
            thumbnail_source: BridgeCoverImageSource::from_core(thumbnail),
        }
    }
}

impl BridgeCoverImageSource {
    pub(crate) fn from_core(source: bae_core::import::CoverImageSource) -> Self {
        match source {
            bae_core::import::CoverImageSource::Remote { url } => Self::Remote { url },
            bae_core::import::CoverImageSource::Local { path } => Self::Local {
                path: path.to_string_lossy().into_owned(),
            },
            bae_core::import::CoverImageSource::Bytes { data } => Self::Bytes { data },
        }
    }
}

mirror_enum! {
    BridgeEvidenceSignal = bae_core::import::EvidenceSignal,
    from_core: fn,
    variants: { Barcode, DiscId },
}

mirror_struct! {
    BridgeFileEvidence = bae_core::import::FileEvidence,
    from_core: pub(crate) fn,
    fields: { signal: (BridgeEvidenceSignal), value, file_id },
}

mirror_enum! {
    BridgeLookupState = bae_core::identify::LookupView,
    from_core: fn,
    variants: {
        Queued,
        NotAsked,
        LookingUp,
        Found { count, groups: (each BridgeReleaseGroup) },
        NoMatch,
        Failed { failure: (BridgeLookupFailure) },
    },
}

mirror_struct! {
    BridgeValueSource = bae_core::identify::ValueSource,
    from_core: fn,
    fields: {
        origin: (BridgeSignalOrigin),
        file,
        region: (opt BridgeImageRegion),
    },
}

mirror_struct! {
    BridgeProviderCell = bae_core::identify::ProviderCell,
    from_core: fn,
    fields: { source: (BridgeMetadataSource), lookup: (BridgeLookupState) },
}

mirror_struct! {
    BridgeSignalValueRow = bae_core::identify::SignalValueRow,
    from_core: fn,
    fields: {
        value,
        sources: (each BridgeValueSource),
        cells: (each BridgeProviderCell),
    },
}

mirror_enum! {
    BridgeDiscIdFileKind = bae_core::identify::DiscIdFileKind,
    from_core: fn,
    variants: { Log, Cue },
}

mirror_struct! {
    BridgeDiscIdFile = bae_core::identify::DiscIdFile,
    from_core: fn,
    fields: { kind: (BridgeDiscIdFileKind), file },
}

mirror_enum! {
    BridgeDiscIdStep = bae_core::identify::DiscIdStepView,
    from_core: fn,
    variants: {
        Reading,
        Absent,
        ReadFailed { failure: (BridgeLookupFailure) },
        Read {
            disc_id,
            source: (opt BridgeDiscIdFile),
            lookup: (BridgeLookupState),
        },
        ReadNotAsked {
            disc_id,
            source: (opt BridgeDiscIdFile),
        },
    },
}

mirror_enum! {
    BridgeBarcodeStep = bae_core::identify::BarcodeStepView,
    from_core: fn,
    variants: {
        Absent,
        NoCodes,
        ScanFailed { failure: (BridgeLookupFailure) },
        Rows { scanning, rows: (each BridgeSignalValueRow) },
    },
}

mirror_struct! {
    BridgeCatalogCandidate = bae_core::identify::CatalogCandidateView,
    from_core: fn,
    fields: { value, sources: (each BridgeValueSource) },
}

mirror_enum! {
    BridgeCatalogStep = bae_core::identify::CatalogStepView,
    from_core: fn,
    variants: {
        NoneFound,
        Numbers {
            scanning,
            rows: (each BridgeSignalValueRow),
            candidates: (each BridgeCatalogCandidate),
        },
    },
}

mirror_struct! {
    BridgeCatalogAgreement = bae_core::identify::CatalogAgreementView,
    from_core: fn,
    fields: { value, discounted },
}

mirror_struct! {
    BridgeIdentifyRun = bae_core::identify::IdentifyRunView,
    from_core: fn,
    fields: {
        providers: (each BridgeMetadataSource),
        disc_id: (BridgeDiscIdStep),
        barcode: (BridgeBarcodeStep),
        catalog: (BridgeCatalogStep),
    },
}

mirror_struct! {
    BridgeReleaseGroupSource = bae_core::import::release_group::ReleaseGroupSource,
    from_core: fn,
    fields: { source: (BridgeMetadataSource), group_url },
}

impl BridgeReleaseGroup {
    pub(crate) fn from_core(g: bae_core::import::release_group::ReleaseGroup) -> Self {
        let bae_core::import::release_group::ReleaseGroup {
            id,
            title,
            artist,
            label,
            cover_art,
            sources,
            year_min,
            year_max,
            pressings,
        } = g;
        BridgeReleaseGroup {
            id,
            title,
            artist,
            label,
            cover_art: cover_art.map(BridgeRemoteCover::from_core),
            sources: sources
                .into_iter()
                .map(BridgeReleaseGroupSource::from_core)
                .collect(),
            year_min,
            year_max,
            pressings: pressings
                .into_iter()
                .map(|pressing| BridgePressing {
                    pick: crate::types::BridgeMetadataProvenance::from_core(pressing.pick()),
                    releases: pressing
                        .releases
                        .into_iter()
                        .map(BridgeMetadataResult::from_core)
                        .collect(),
                })
                .collect(),
        }
    }
}

mirror_enum! {
    BridgeDiscIdSignal = bae_core::signals::DiscIdSignal,
    from_core: fn,
    variants: {
        Computed { disc_id, track_count, source_file },
        Absent { track_count },
        Failed { failure: (BridgeLookupFailure), track_count },
    },
}

mirror_enum! {
    BridgeBarcodeSignal = bae_core::signals::BarcodeSignal,
    from_core: fn,
    variants: {
        Scanning { codes: (each BridgeSourcedValue) },
        Settled { codes: (each BridgeSourcedValue) },
        Failed {
            failure: (BridgeLookupFailure),
            codes: (each BridgeSourcedValue),
        },
        Absent,
    },
}

mirror_enum! {
    BridgeTextSignal = bae_core::signals::TextSignal,
    from_core: fn,
    variants: {
        Scanning { catalogs: (each BridgeSourcedValue), free_text },
        Settled { catalogs: (each BridgeSourcedValue), free_text },
        Failed {
            failure: (BridgeLookupFailure),
            catalogs: (each BridgeSourcedValue),
            free_text,
        },
    },
}

/// Not a `mirror_struct`: neither the measured durations nor the candidate's
/// own text lines cross. The durations are a Ready-rule input and the mapping
/// table's lengths, and the pane reads them through its own record; the text
/// pool is what core judges and orders the rows by, and what it concluded is
/// already on every row as its badges.
impl BridgeSignals {
    pub(crate) fn from_core(s: bae_core::signals::Signals) -> Self {
        let bae_core::signals::Signals {
            disc_id,
            barcode,
            text,
            text_pool: _,
            durations: _,
        } = s;
        BridgeSignals {
            disc_id: BridgeDiscIdSignal::from_core(disc_id),
            barcode: BridgeBarcodeSignal::from_core(barcode),
            text: BridgeTextSignal::from_core(text),
        }
    }
}

mirror_struct! {
    BridgeAgreements = bae_core::identify::Agreements,
    from_core: fn,
    fields: { disc_id, barcode, catalog, label, year, country },
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the uniffi enum. Core has
/// already folded the matches into their group cards, ranked them, keyed the
/// agreements, reduced the in-flight payloads to counts, and dropped what must
/// not cross — this is a field copy per variant and nothing else.
impl BridgeIdentifyState {
    pub(crate) fn from_core(s: bae_core::identify::IdentifyState) -> Self {
        use bae_core::identify::IdentifyStateView;
        match IdentifyStateView::from(s) {
            IdentifyStateView::Idle => BridgeIdentifyState::Idle,
            IdentifyStateView::Triangulating {
                run,
                groups,
                library_statuses,
                agreements,
                narrowed_out,
            } => BridgeIdentifyState::Triangulating {
                run: BridgeIdentifyRun::from_core(run),
                groups: groups
                    .into_iter()
                    .map(BridgeReleaseGroup::from_core)
                    .collect(),
                library_statuses: status_map(library_statuses),
                agreements: agreements
                    .into_iter()
                    .map(|(release_id, a)| (release_id, BridgeAgreements::from_core(a)))
                    .collect(),
                narrowed_out: BridgeNarrowedOut::from_core(narrowed_out),
            },
            IdentifyStateView::Found {
                run,
                groups,
                library_statuses,
                track_count,
                agreements,
                narrowed_out,
                catalog_agreements,
            } => BridgeIdentifyState::Found {
                run: run.map(BridgeIdentifyRun::from_core),
                groups: groups
                    .into_iter()
                    .map(BridgeReleaseGroup::from_core)
                    .collect(),
                library_statuses: status_map(library_statuses),
                track_count,
                agreements: agreements
                    .into_iter()
                    .map(|(release_id, a)| (release_id, BridgeAgreements::from_core(a)))
                    .collect(),
                narrowed_out: BridgeNarrowedOut::from_core(narrowed_out),
                catalog_agreements: catalog_agreements
                    .into_iter()
                    .map(BridgeCatalogAgreement::from_core)
                    .collect(),
            },
            IdentifyStateView::NotFoundAnywhere { run } => BridgeIdentifyState::NotFoundAnywhere {
                run: run.map(BridgeIdentifyRun::from_core),
            },
            IdentifyStateView::ManualOnly { track_count, run } => BridgeIdentifyState::ManualOnly {
                track_count,
                run: run.map(BridgeIdentifyRun::from_core),
            },
            IdentifyStateView::Failed {
                run,
                failures,
                groups,
                library_statuses,
                agreements,
                narrowed_out,
                catalog_agreements,
            } => BridgeIdentifyState::Failed {
                run: run.map(BridgeIdentifyRun::from_core),
                failures: failures.into_iter().map(identify_failure).collect(),
                groups: groups
                    .into_iter()
                    .map(BridgeReleaseGroup::from_core)
                    .collect(),
                library_statuses: status_map(library_statuses),
                agreements: agreements
                    .into_iter()
                    .map(|(release_id, a)| (release_id, BridgeAgreements::from_core(a)))
                    .collect(),
                narrowed_out: BridgeNarrowedOut::from_core(narrowed_out),
                catalog_agreements: catalog_agreements
                    .into_iter()
                    .map(BridgeCatalogAgreement::from_core)
                    .collect(),
            },
        }
    }
}

impl BridgeNarrowedOut {
    fn from_core(view: bae_core::identify::NarrowedOutView) -> Self {
        Self {
            groups: view
                .groups
                .into_iter()
                .map(BridgeReleaseGroup::from_core)
                .collect(),
            library_statuses: status_map(view.library_statuses),
            agreements: view
                .agreements
                .into_iter()
                .map(|(release_id, a)| (release_id, BridgeAgreements::from_core(a)))
                .collect(),
        }
    }
}

fn identify_failure(
    failure: bae_core::identify::IdentifyFailure,
) -> crate::types::BridgeIdentifyFailure {
    use bae_core::identify::IdentifyFailure;
    match failure {
        IdentifyFailure::DiscId(failure) => crate::types::BridgeIdentifyFailure::DiscId {
            failure: BridgeLookupFailure::from_core(failure),
        },
        IdentifyFailure::BarcodeScan(failure) => crate::types::BridgeIdentifyFailure::BarcodeScan {
            failure: BridgeLookupFailure::from_core(failure),
        },
        IdentifyFailure::Barcode(failure) => crate::types::BridgeIdentifyFailure::Barcode {
            source: BridgeMetadataSource::from_core(failure.source),
            failure: BridgeLookupFailure::from_core(failure.failure),
        },
        IdentifyFailure::Catalog(failure) => crate::types::BridgeIdentifyFailure::Catalog {
            source: BridgeMetadataSource::from_core(failure.source),
            failure: BridgeLookupFailure::from_core(failure.failure),
        },
        IdentifyFailure::ReleaseDetails(failure) => {
            crate::types::BridgeIdentifyFailure::ReleaseDetails {
                failure: BridgeLookupFailure::from_core(failure),
            }
        }
    }
}

/// Key library statuses by release id — the UI looks a row's status up by id
/// rather than re-indexing a flat list. Each status carries its own id, so this
/// is a re-container, not a re-pairing.
fn status_map(
    statuses: Vec<bae_core::db::LibraryStatus>,
) -> std::collections::HashMap<String, BridgeLibraryStatus> {
    statuses
        .into_iter()
        .map(|s| (s.release_id.clone(), BridgeLibraryStatus::from_core(s)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_core::identify::state::{BarcodeEvidence, DiscIdEvidence, SignalsContext};
    use bae_core::identify::{
        BarcodeLookupState, BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState,
        ProviderBarcodeLookup,
    };
    use bae_core::import::MetadataSource;
    use bae_core::signals::{DiscIdSignal, LookupFailure, SignalOrigin, SourcedValue};

    fn in_flight(barcode: BarcodeProgress) -> IdentifyState {
        IdentifyState::Triangulating {
            discid: DiscidProgress::Skipped { track_count: 9 },
            barcode,
            catalog: CatalogProgress::Skipped,
            context: SignalsContext {
                providers: vec![MetadataSource::MusicBrainz, MetadataSource::Discogs],
                artwork: bae_core::signals::ArtworkScan::Absent,
                disc: DiscIdEvidence {
                    signal: DiscIdSignal::Absent { track_count: 9 },
                    ..Default::default()
                },
                barcode: BarcodeEvidence {
                    codes: vec![SourcedValue::in_file(
                        "0123456789012".to_string(),
                        SignalOrigin::Artwork,
                        "back.jpg".to_string(),
                    )],
                    had_source: true,
                    ..Default::default()
                },
                catalog: Default::default(),
                text: Default::default(),
                text_settled: true,
                track_count: 9,
            },
        }
    }

    fn barcode_step(state: IdentifyState) -> BridgeBarcodeStep {
        match BridgeIdentifyState::from_core(state) {
            BridgeIdentifyState::Triangulating { run, .. } => run.barcode,
            other => panic!("expected a run in flight, got {other:?}"),
        }
    }

    /// A code crosses as one row: where it was read, and one cell per
    /// provider — the one still looking beside the one that failed, so a
    /// surface can say which to retry while the other keeps going.
    #[test]
    fn a_code_crosses_as_a_row_with_one_cell_per_provider() {
        let step = barcode_step(in_flight(BarcodeProgress::Lookups {
            codes: vec!["0123456789012".to_string()],
            providers: vec![
                ProviderBarcodeLookup {
                    source: MetadataSource::MusicBrainz,
                    state: BarcodeLookupState::Trying { index: 0 },
                },
                ProviderBarcodeLookup {
                    source: MetadataSource::Discogs,
                    state: BarcodeLookupState::Failed {
                        failure: LookupFailure::Diagnostic {
                            detail: "provider lookup failed".to_string(),
                        },
                        index: 0,
                    },
                },
            ],
        }));
        let BridgeBarcodeStep::Rows { scanning, rows } = step else {
            panic!("a walk in flight crosses as rows, got {step:?}");
        };
        assert!(!scanning);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, "0123456789012");
        assert_eq!(
            rows[0].sources,
            vec![BridgeValueSource {
                origin: BridgeSignalOrigin::Artwork,
                file: Some("back.jpg".to_string()),
                region: None,
            }]
        );
        assert_eq!(rows[0].cells.len(), 2);
        assert_eq!(rows[0].cells[0].source, BridgeMetadataSource::MusicBrainz);
        assert!(matches!(
            rows[0].cells[0].lookup,
            BridgeLookupState::LookingUp
        ));
        assert_eq!(rows[0].cells[1].source, BridgeMetadataSource::Discogs);
        assert!(matches!(
            &rows[0].cells[1].lookup,
            BridgeLookupState::Failed {
                failure: BridgeLookupFailure::Diagnostic { detail }
            } if detail == "provider lookup failed"
        ));
    }

    /// Both halves of what a candidate's identification asks about cross, and
    /// cross back: the numbers a run looks up, and the numbers struck out of
    /// the candidate's own text so they rank nothing.
    #[test]
    fn both_halves_of_what_identification_asks_about_cross() {
        let choices = bae_core::import::LookupChoices {
            disc_id_excluded: true,
            barcode_excluded: false,
            chosen_catalogs: vec!["WPCR-80001".to_string()],
            discounted_catalogs: vec!["LBL-9".to_string()],
        };
        let crossed = crate::types::BridgeLookupChoices::from_core(choices.clone());
        assert_eq!(crossed.chosen_catalogs, vec!["WPCR-80001".to_string()]);
        assert_eq!(crossed.discounted_catalogs, vec!["LBL-9".to_string()]);
        assert_eq!(crossed.into_core(), choices);
    }

    /// Reading the candidate's barcodes failing is not a provider's failure,
    /// and crosses as its own variant rather than as an unattributed one.
    #[test]
    fn a_failed_barcode_scan_crosses_as_its_own_variant() {
        let step = barcode_step(in_flight(BarcodeProgress::ScanFailed {
            failure: LookupFailure::ArtworkAnalysis,
        }));
        assert!(matches!(
            step,
            BridgeBarcodeStep::ScanFailed {
                failure: BridgeLookupFailure::ArtworkAnalysis
            }
        ));
    }
}

use super::super::*;

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
fn bridge_remote_cover_selection(
    url: String,
    source: bae_core::import::MetadataSource,
) -> BridgeRemoteCoverSelection {
    BridgeRemoteCoverSelection {
        url,
        source: BridgeMetadataSource::from_core(source),
    }
}

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
impl BridgeReleaseTrack {
    pub(crate) fn from_core(t: bae_core::import::search::ReleaseTrack) -> Self {
        let bae_core::import::search::ReleaseTrack {
            title,
            artist,
            duration_ms,
            position,
            side,
        } = t;
        Self {
            title,
            artist,
            duration_ms,
            position,
            side,
        }
    }
}

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
impl BridgeFileEvidence {
    pub(crate) fn from_core(evidence: bae_core::import::FileEvidence) -> Self {
        use bae_core::import::EvidenceSignal;
        let bae_core::import::FileEvidence {
            signal,
            value,
            file_id,
        } = evidence;
        BridgeFileEvidence {
            signal: match signal {
                EvidenceSignal::Barcode => BridgeEvidenceSignal::Barcode,
                EvidenceSignal::DiscId => BridgeEvidenceSignal::DiscId,
            },
            value,
            file_id,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeLookupState {
    fn from_view(v: bae_core::identify::LookupView) -> Self {
        use bae_core::identify::LookupView;
        match v {
            LookupView::LookingUp => BridgeLookupState::LookingUp,
            LookupView::Found { count } => BridgeLookupState::Found { count },
            LookupView::NoMatch => BridgeLookupState::NoMatch,
            LookupView::Failed { failure } => BridgeLookupState::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeDiscIdStep {
    fn from_view(v: bae_core::identify::DiscIdStepView) -> Self {
        use bae_core::identify::DiscIdStepView;
        match v {
            DiscIdStepView::Reading => BridgeDiscIdStep::Reading,
            DiscIdStepView::Absent => BridgeDiscIdStep::Absent,
            DiscIdStepView::ReadFailed { failure } => BridgeDiscIdStep::ReadFailed {
                failure: BridgeLookupFailure::from_core(failure),
            },
            DiscIdStepView::Read {
                disc_id,
                source_file,
                lookup,
            } => BridgeDiscIdStep::Read {
                disc_id,
                source_file,
                lookup: BridgeLookupState::from_view(lookup),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeBarcodeStep {
    fn from_view(v: bae_core::identify::BarcodeStepView) -> Self {
        use bae_core::identify::{BarcodeLookupView, BarcodeStepView};
        match v {
            BarcodeStepView::AwaitingArtwork => BridgeBarcodeStep::AwaitingArtwork,
            BarcodeStepView::Absent => BridgeBarcodeStep::Absent,
            BarcodeStepView::NoCodes => BridgeBarcodeStep::NoCodes,
            BarcodeStepView::ScanFailed { failure } => BridgeBarcodeStep::ScanFailed {
                failure: BridgeLookupFailure::from_core(failure),
            },
            BarcodeStepView::Lookups { codes, providers } => BridgeBarcodeStep::Lookups {
                codes,
                providers: providers
                    .into_iter()
                    .map(|provider| BridgeProviderBarcodeLookup {
                        source: BridgeMetadataSource::from_core(provider.source),
                        state: match provider.state {
                            BarcodeLookupView::Trying {
                                barcode,
                                position,
                                total,
                            } => BridgeBarcodeLookupState::Trying {
                                barcode,
                                position,
                                total,
                            },
                            BarcodeLookupView::Matched { barcode, count } => {
                                BridgeBarcodeLookupState::Matched { barcode, count }
                            }
                            BarcodeLookupView::Exhausted => BridgeBarcodeLookupState::Exhausted,
                            BarcodeLookupView::Failed { failure } => {
                                BridgeBarcodeLookupState::Failed {
                                    failure: BridgeLookupFailure::from_core(failure),
                                }
                            }
                        },
                    })
                    .collect(),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCatalogStep {
    fn from_view(v: bae_core::identify::CatalogStepView) -> Self {
        use bae_core::identify::CatalogStepView;
        match v {
            CatalogStepView::NoneFound => BridgeCatalogStep::NoneFound,
            CatalogStepView::Unchosen { available } => BridgeCatalogStep::Unchosen { available },
            CatalogStepView::Chosen { value, lookups } => BridgeCatalogStep::Chosen {
                value,
                lookups: lookups
                    .into_iter()
                    .map(|lookup| BridgeProviderLookup {
                        source: BridgeMetadataSource::from_core(lookup.source),
                        state: BridgeLookupState::from_view(lookup.state),
                    })
                    .collect(),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeIdentifyRun {
    fn from_view(v: bae_core::identify::IdentifyRunView) -> Self {
        let bae_core::identify::IdentifyRunView {
            disc_id,
            barcode,
            catalog,
        } = v;
        BridgeIdentifyRun {
            disc_id: BridgeDiscIdStep::from_view(disc_id),
            barcode: BridgeBarcodeStep::from_view(barcode),
            catalog: BridgeCatalogStep::from_view(catalog),
        }
    }
}

#[cfg(feature = "desktop")]
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
                .map(|source| BridgeReleaseGroupSource {
                    source: BridgeMetadataSource::from_core(source.source),
                    group_url: source.group_url,
                })
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

#[cfg(feature = "desktop")]
impl BridgeSignals {
    pub(crate) fn from_core(s: bae_core::signals::Signals) -> Self {
        use bae_core::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};

        fn sourced_values(values: Vec<bae_core::signals::SourcedValue>) -> Vec<BridgeSourcedValue> {
            values
                .into_iter()
                .map(BridgeSourcedValue::from_core)
                .collect()
        }

        let Signals {
            disc_id,
            barcode,
            text,
            // The measured durations are a Ready-rule input and the mapping
            // table's lengths, not a badge: the sidebar reads a candidate's
            // classification, and the pane reads the durations through its own
            // record. Neither wants them here, so they do not cross.
            durations: _,
        } = s;

        let disc_id = match disc_id {
            DiscIdSignal::Computed {
                disc_id,
                track_count,
                source_file,
            } => BridgeDiscIdSignal::Computed {
                disc_id,
                track_count,
                source_file,
            },
            DiscIdSignal::Absent { track_count } => BridgeDiscIdSignal::Absent { track_count },
            DiscIdSignal::Failed {
                failure,
                track_count,
            } => BridgeDiscIdSignal::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                track_count,
            },
        };

        let barcode = match barcode {
            BarcodeSignal::Scanning { codes } => BridgeBarcodeSignal::Scanning {
                codes: sourced_values(codes),
            },
            BarcodeSignal::Settled { codes } => BridgeBarcodeSignal::Settled {
                codes: sourced_values(codes),
            },
            BarcodeSignal::Failed { failure, codes } => BridgeBarcodeSignal::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                codes: sourced_values(codes),
            },
            BarcodeSignal::Absent => BridgeBarcodeSignal::Absent,
        };

        let text = match text {
            TextSignal::Scanning {
                catalogs,
                free_text,
            } => BridgeTextSignal::Scanning {
                catalogs: sourced_values(catalogs),
                free_text,
            },
            TextSignal::Settled {
                catalogs,
                free_text,
            } => BridgeTextSignal::Settled {
                catalogs: sourced_values(catalogs),
                free_text,
            },
            TextSignal::Failed {
                failure,
                catalogs,
                free_text,
            } => BridgeTextSignal::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                catalogs: sourced_values(catalogs),
                free_text,
            },
        };

        BridgeSignals {
            disc_id,
            barcode,
            text,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeResultProvenance {
    fn from_core(p: bae_core::identify::ResultProvenance) -> Self {
        let bae_core::identify::ResultProvenance {
            by_disc_id,
            by_barcode,
            by_catalog,
        } = p;
        BridgeResultProvenance {
            by_disc_id,
            by_barcode,
            by_catalog,
        }
    }
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the uniffi enum. Core has
/// already folded the matches into their group cards, keyed the provenance,
/// reduced the in-flight payloads to counts, and dropped what must not cross —
/// this is a field copy per variant and nothing else.
#[cfg(feature = "desktop")]
impl BridgeIdentifyState {
    pub(crate) fn from_core(s: bae_core::identify::IdentifyState) -> Self {
        use bae_core::identify::IdentifyStateView;
        match IdentifyStateView::from(s) {
            IdentifyStateView::Idle => BridgeIdentifyState::Idle,
            IdentifyStateView::Triangulating { run } => BridgeIdentifyState::Triangulating {
                run: BridgeIdentifyRun::from_view(run),
            },
            IdentifyStateView::Found {
                groups,
                library_statuses,
                track_count,
                provenance,
            } => BridgeIdentifyState::Found {
                groups: groups
                    .into_iter()
                    .map(BridgeReleaseGroup::from_core)
                    .collect(),
                library_statuses: status_map(library_statuses),
                track_count,
                provenance: provenance
                    .into_iter()
                    .map(|(release_id, p)| (release_id, BridgeResultProvenance::from_core(p)))
                    .collect(),
            },
            IdentifyStateView::NotFoundAnywhere => BridgeIdentifyState::NotFoundAnywhere,
            IdentifyStateView::ManualOnly { track_count } => {
                BridgeIdentifyState::ManualOnly { track_count }
            }
            IdentifyStateView::Failed {
                failures,
                groups,
                library_statuses,
                provenance,
            } => BridgeIdentifyState::Failed {
                failures: failures.into_iter().map(identify_failure).collect(),
                groups: groups
                    .into_iter()
                    .map(BridgeReleaseGroup::from_core)
                    .collect(),
                library_statuses: status_map(library_statuses),
                provenance: provenance
                    .into_iter()
                    .map(|(release_id, p)| (release_id, BridgeResultProvenance::from_core(p)))
                    .collect(),
            },
        }
    }
}

#[cfg(feature = "desktop")]
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
#[cfg(feature = "desktop")]
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
    use bae_core::identify::state::SignalsContext;
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
                disc_id: DiscIdSignal::Absent { track_count: 9 },
                barcode_codes: vec![SourcedValue::new(
                    "0123456789012".to_string(),
                    SignalOrigin::Artwork,
                )],
                had_barcode_source: true,
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
                track_count: 9,
            },
        }
    }

    fn barcode_step(state: IdentifyState) -> BridgeBarcodeStep {
        match BridgeIdentifyState::from_core(state) {
            BridgeIdentifyState::Triangulating { run } => run.barcode,
            other => panic!("expected a run in flight, got {other:?}"),
        }
    }

    /// A provider that failed its barcode walk crosses with its name on it
    /// and beside the provider still walking, so a surface can say which one
    /// to retry while the other keeps going.
    #[test]
    fn a_failed_provider_crosses_beside_the_one_still_looking() {
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
                    },
                },
            ],
        }));
        let BridgeBarcodeStep::Lookups { codes, providers } = step else {
            panic!("a walk in flight crosses as lookups, got {step:?}");
        };
        assert_eq!(codes, vec!["0123456789012".to_string()]);
        assert_eq!(
            providers,
            vec![
                BridgeProviderBarcodeLookup {
                    source: BridgeMetadataSource::MusicBrainz,
                    state: BridgeBarcodeLookupState::Trying {
                        barcode: "0123456789012".to_string(),
                        position: 1,
                        total: 1,
                    },
                },
                BridgeProviderBarcodeLookup {
                    source: BridgeMetadataSource::Discogs,
                    state: BridgeBarcodeLookupState::Failed {
                        failure: BridgeLookupFailure::Diagnostic {
                            detail: "provider lookup failed".to_string(),
                        },
                    },
                },
            ]
        );
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

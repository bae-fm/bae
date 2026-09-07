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
        LookingUp,
        Found { count },
        NoMatch,
        Failed { failure: (BridgeLookupFailure) },
    },
}

mirror_enum! {
    BridgeDiscIdStep = bae_core::identify::DiscIdStepView,
    from_core: fn,
    variants: {
        Reading,
        Absent,
        ReadFailed { failure: (BridgeLookupFailure) },
        Read { disc_id, source_file, lookup: (BridgeLookupState) },
    },
}

/// Not a `mirror_enum`: `Failed` renames core's `read` to `images_read`, which
/// is what a surface says the number is.
impl BridgeArtworkStep {
    fn from_core(v: bae_core::identify::ArtworkStepView) -> Self {
        use bae_core::identify::ArtworkStepView;
        match v {
            ArtworkStepView::Absent => BridgeArtworkStep::Absent,
            ArtworkStepView::Reading {
                current,
                position,
                total,
                barcodes,
                catalogs,
            } => BridgeArtworkStep::Reading {
                current,
                position,
                total,
                barcodes,
                catalogs,
            },
            ArtworkStepView::Read {
                images,
                barcodes,
                catalogs,
            } => BridgeArtworkStep::Read {
                images,
                barcodes,
                catalogs,
            },
            ArtworkStepView::Failed {
                failure,
                read,
                total,
            } => BridgeArtworkStep::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                images_read: read,
                total,
            },
        }
    }
}

mirror_enum! {
    BridgeBarcodeLookupState = bae_core::identify::BarcodeLookupView,
    from_core: fn,
    variants: {
        Trying { barcode, position, total },
        Matched { barcode, count },
        Exhausted,
        Failed { failure: (BridgeLookupFailure) },
    },
}

mirror_struct! {
    BridgeProviderBarcodeLookup = bae_core::identify::ProviderBarcodeLookupView,
    from_core: fn,
    fields: { source: (BridgeMetadataSource), state: (BridgeBarcodeLookupState) },
}

mirror_enum! {
    BridgeBarcodeStep = bae_core::identify::BarcodeStepView,
    from_core: fn,
    variants: {
        AwaitingArtwork,
        Absent,
        NoCodes,
        ScanFailed { failure: (BridgeLookupFailure) },
        Lookups { codes, providers: (each BridgeProviderBarcodeLookup) },
    },
}

mirror_struct! {
    BridgeProviderLookup = bae_core::identify::ProviderLookupView,
    from_core: fn,
    fields: { source: (BridgeMetadataSource), state: (BridgeLookupState) },
}

mirror_enum! {
    BridgeCatalogStep = bae_core::identify::CatalogStepView,
    from_core: fn,
    variants: {
        NoneFound,
        Unchosen { available },
        Chosen { value, lookups: (each BridgeProviderLookup) },
    },
}

mirror_struct! {
    BridgeIdentifyRun = bae_core::identify::IdentifyRunView,
    from_core: fn,
    fields: {
        providers: (each BridgeMetadataSource),
        disc_id: (BridgeDiscIdStep),
        artwork: (BridgeArtworkStep),
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

/// Not a `mirror_struct`: the measured durations are a Ready-rule input and the
/// mapping table's lengths, not a badge — the sidebar reads a candidate's
/// classification, and the pane reads the durations through its own record.
/// Neither wants them here, so they do not cross.
impl BridgeSignals {
    pub(crate) fn from_core(s: bae_core::signals::Signals) -> Self {
        let bae_core::signals::Signals {
            disc_id,
            barcode,
            text,
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
    BridgeResultProvenance = bae_core::identify::ResultProvenance,
    from_core: fn,
    fields: { by_disc_id, by_barcode, by_catalog },
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the uniffi enum. Core has
/// already folded the matches into their group cards, keyed the provenance,
/// reduced the in-flight payloads to counts, and dropped what must not cross —
/// this is a field copy per variant and nothing else.
impl BridgeIdentifyState {
    pub(crate) fn from_core(s: bae_core::identify::IdentifyState) -> Self {
        use bae_core::identify::IdentifyStateView;
        match IdentifyStateView::from(s) {
            IdentifyStateView::Idle => BridgeIdentifyState::Idle,
            IdentifyStateView::Triangulating {
                run,
                groups,
                library_statuses,
                provenance,
            } => BridgeIdentifyState::Triangulating {
                run: BridgeIdentifyRun::from_core(run),
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

    #[test]
    fn failed_artwork_preserves_the_number_of_images_read() {
        assert_eq!(
            BridgeArtworkStep::from_core(bae_core::identify::ArtworkStepView::Failed {
                failure: LookupFailure::ArtworkAnalysis,
                read: 2,
                total: 5,
            }),
            BridgeArtworkStep::Failed {
                failure: BridgeLookupFailure::ArtworkAnalysis,
                images_read: 2,
                total: 5,
            }
        );
    }

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
                    codes: vec![SourcedValue::new(
                        "0123456789012".to_string(),
                        SignalOrigin::Artwork,
                    )],
                    had_source: true,
                    ..Default::default()
                },
                catalog: Default::default(),
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

use super::super::*;

impl BridgeMetadataResult {
    pub(crate) fn from_core(r: bae_core::import::search::MetadataResult) -> Self {
        let facts = BridgePressingFacts::from_core(r.facts());
        let bae_core::import::search::MetadataResult {
            source,
            release_id,
            year,
            barcodes,
            source_group_id,
            // The row shows its records' labels as one list, and whether a
            // document could not be read.
            labels: _,
            document_failure: _,
            // The album card carries these.
            title: _,
            artist: _,
            cover_art: _,
            // auto-import evidence, not something a row renders.
            source_tracks: _,
            // Read into `facts` above, or pairing evidence the row already
            // reflects.
            area: _,
            status: _,
            packaging: _,
            discogs_details: _,
            media: _,
            links: _,
            // Already applied by the grouping.
            album_links: _,
        } = r;
        BridgeMetadataResult {
            source: BridgeCatalog::from_core(source),
            release_id,
            year,
            facts,
            barcodes,
            source_group_id,
        }
    }
}
impl BridgeRemoteCover {
    pub(crate) fn from_core(c: bae_core::import::cover_art::RemoteCover) -> Self {
        let bae_core::import::cover_art::RemoteCover {
            image,
            label,
            source,
            // Core already chose which covers to offer by it.
            standing: _,
        } = c;
        let image = BridgeRemoteImageSet::from_core(image);
        BridgeRemoteCover {
            cover_choice: BridgeCoverChoice {
                selection: BridgeCoverSelection::RemoteCover {
                    selection: BridgeRemoteCoverSelection {
                        image: image.clone(),
                        source: BridgeCatalog::from_core(source),
                    },
                },
                image: BridgeCoverImageSource::Remote { image },
            },
            label,
        }
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
            labels,
            barcode,
            facts,
            track_count,
            tracks,
            cover_art,
            // Pairing evidence; the picker renders the facts.
            media: _,
            links: _,
        } = d;
        BridgeReleaseDetail {
            release_id,
            source: BridgeCatalog::from_core(source),
            source_group_id,
            title,
            artist,
            year,
            labels: labels
                .into_iter()
                .map(BridgeReleaseLabel::from_core)
                .collect(),
            barcode,
            facts: BridgePressingFacts::from_core(facts),
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
        let bae_core::import::CoverChoice { selection, image } = choice;
        Self {
            selection: match selection {
                bae_core::import::CoverSelection::Local(file_id) => {
                    BridgeCoverSelection::ReleaseImage { file_id }
                }
                bae_core::import::CoverSelection::Remote(image, source) => {
                    BridgeCoverSelection::RemoteCover {
                        selection: BridgeRemoteCoverSelection {
                            image: BridgeRemoteImageSet::from_core(image),
                            source: BridgeCatalog::from_core(source),
                        },
                    }
                }
                bae_core::import::CoverSelection::Embedded(source_file_id) => {
                    BridgeCoverSelection::EmbeddedCover { source_file_id }
                }
            },
            image: BridgeCoverImageSource::from_core(image),
        }
    }
}

impl BridgeCoverImageSource {
    pub(crate) fn from_core(source: bae_core::import::CoverImageSource) -> Self {
        match source {
            bae_core::import::CoverImageSource::Remote { image } => Self::Remote {
                image: BridgeRemoteImageSet::from_core(image),
            },
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
    BridgeNotAskedReason = bae_core::identify::NotAskedReason,
    from_core: fn,
    variants: { LeftOut, SwitchedOff, NoCatalog },
}

mirror_enum! {
    BridgeLookupState = bae_core::identify::LookupView,
    from_core: fn,
    variants: {
        Queued,
        NotAsked { reason: (BridgeNotAskedReason) },
        LookingUp,
        Found { count, groups: (each BridgeReleaseGroup) },
        NoMatch,
        Failed { failure: (BridgeLookupFailure) },
    },
}

mirror_struct! {
    BridgeProviderCell = bae_core::identify::ProviderCell,
    from_core: fn,
    fields: { source: (BridgeCatalog), lookup: (BridgeLookupState) },
}

mirror_struct! {
    BridgeSignalValueRow = bae_core::identify::SignalValueRow,
    from_core: fn,
    fields: { value, excluded, cells: (each BridgeProviderCell) },
}

impl BridgeDiscIdStep {
    /// Not a copy: an unread sheet crosses with the rate of the audio it was
    /// run over, which is what says why it was not read.
    fn from_core(step: bae_core::identify::DiscIdStepView, rate: Option<u32>) -> Self {
        use bae_core::identify::DiscIdStepView;
        match step {
            DiscIdStepView::Reading => Self::Reading,
            DiscIdStepView::Absent => Self::Absent,
            DiscIdStepView::NotCdAudio => Self::NotCdAudio {
                sample_rate_hz: rate,
            },
            DiscIdStepView::ReadFailed { failure } => Self::ReadFailed {
                failure: BridgeLookupFailure::from_core(failure),
            },
            DiscIdStepView::Read { disc_id, lookup } => Self::Read {
                disc_id,
                lookup: BridgeLookupState::from_core(lookup),
            },
        }
    }
}

mirror_enum! {
    BridgeBarcodeStep = bae_core::identify::BarcodeStepView,
    from_core: fn,
    variants: {
        Absent,
        CoverArtOff,
        NoCodes,
        ScanFailed { failure: (BridgeLookupFailure) },
        Rows { scanning, rows: (each BridgeSignalValueRow) },
    },
}

mirror_struct! {
    BridgeCatalogCandidate = bae_core::identify::CatalogCandidateView,
    from_core: fn,
    fields: { value },
}

mirror_enum! {
    BridgeCatalogStep = bae_core::identify::CatalogStepView,
    from_core: fn,
    variants: {
        NoneFound,
        CoverArtOff,
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

mirror_enum! {
    BridgeSearchStep = bae_core::identify::SearchStepView,
    from_core: fn,
    variants: {
        NotAsked { reason: (BridgeNotAskedReason) },
        NotNeeded,
        NoTitle,
        Waiting { album, artist },
        Searched { album, artist, cells: (each BridgeProviderCell) },
    },
}

impl BridgeIdentifyRun {
    /// The ledger, with the rate of the audio the run was over for its
    /// disc-ID step.
    fn from_core(run: bae_core::identify::IdentifyRunView, rate: Option<u32>) -> Self {
        let bae_core::identify::IdentifyRunView {
            providers,
            disc_id,
            barcode,
            catalog,
            search,
        } = run;
        Self {
            providers: providers
                .into_iter()
                .map(BridgeCatalog::from_core)
                .collect(),
            disc_id: BridgeDiscIdStep::from_core(disc_id, rate),
            barcode: BridgeBarcodeStep::from_core(barcode),
            catalog: BridgeCatalogStep::from_core(catalog),
            search: BridgeSearchStep::from_core(search),
        }
    }
}

mirror_struct! {
    BridgeReleaseGroupSource = bae_core::import::release_group::ReleaseGroupSource,
    from_core: fn,
    fields: { source: (BridgeCatalog), group_url, album_links_unread },
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
            sections,
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
            sections: sections
                .into_iter()
                .map(BridgePressingSection::from_core)
                .collect(),
        }
    }
}

impl BridgePressingSection {
    fn from_core(section: bae_core::import::release_group::PressingSection) -> Self {
        let bae_core::import::release_group::PressingSection {
            album,
            pressings,
            narrowed_out,
        } = section;
        BridgePressingSection {
            album: album.map(BridgeAlbumHeading::from_core),
            pressings: pressings
                .into_iter()
                .map(BridgePressing::from_core)
                .collect(),
            narrowed_out: narrowed_out
                .into_iter()
                .map(BridgePressing::from_core)
                .collect(),
        }
    }
}

mirror_struct! {
    BridgeAlbumHeading = bae_core::import::release_group::AlbumHeading,
    from_core: fn,
    fields: { title, source: (BridgeReleaseGroupSource) },
}

impl BridgePressing {
    fn from_core(pressing: bae_core::import::release_group::Pressing) -> Self {
        let facts = BridgePressingFacts::from_core(pressing.facts());
        BridgePressing {
            document_failure: pressing
                .document_failure()
                .cloned()
                .map(BridgeLookupFailure::from_core),
            labels: pressing
                .label_lines()
                .into_iter()
                .map(BridgeLabelLine::from_core)
                .collect(),
            pick: crate::types::BridgeMetadataProvenance::from_core(pressing.pick()),
            summary: crate::types::bridge_pressing_summary(facts.clone()),
            details: crate::types::bridge_pressing_details(facts),
            releases: pressing
                .releases
                .into_iter()
                .map(BridgeMetadataResult::from_core)
                .collect(),
        }
    }
}

mirror_enum! {
    BridgeTextSignal = bae_core::signals::TextSignal,
    from_core: fn,
    variants: {
        Scanning { catalogs, free_text },
        Settled { catalogs, free_text },
        Failed { failure: (BridgeLookupFailure), catalogs, free_text },
    },
}

/// Not a `mirror_struct`: only the text pools cross; what a surface needs from
/// the rest reaches it through the run's ledger and the file evidence.
impl BridgeSignals {
    pub(crate) fn from_core(s: bae_core::signals::Signals) -> Self {
        let bae_core::signals::Signals {
            rip: _,
            disc_id: _,
            barcode: _,
            text,
            text_pool: _,
        } = s;
        BridgeSignals {
            text: BridgeTextSignal::from_core(text),
        }
    }
}

impl BridgeAgreements {
    /// The pressing's agreements the list shows as badges. The album's title
    /// and artist rank the rows but are no badge.
    fn from_core(agreements: bae_core::identify::Agreements) -> Self {
        let bae_core::identify::Agreements {
            disc_id,
            barcode,
            catalog,
            label,
            year,
            country,
            title: _,
            artist: _,
        } = agreements;
        Self {
            disc_id,
            barcode,
            catalog,
            label,
            year,
            country,
        }
    }
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the uniffi enum, a field
/// copy per variant with the rate of the state's audio on its run.
impl BridgeIdentifyState {
    pub(crate) fn from_core(s: bae_core::identify::IdentifyState) -> Self {
        use bae_core::identify::IdentifyStateView;
        let rate = s.audio().and_then(|audio| audio.rate_ruling_out_cd);
        match IdentifyStateView::from(s) {
            IdentifyStateView::Idle => BridgeIdentifyState::Idle,
            IdentifyStateView::Triangulating {
                run,
                groups,
                library_statuses,
                agreements,
                narrowed_out_count,
            } => BridgeIdentifyState::Triangulating {
                run: BridgeIdentifyRun::from_core(run, rate),
                groups: groups
                    .into_iter()
                    .map(BridgeReleaseGroup::from_core)
                    .collect(),
                library_statuses: status_map(library_statuses),
                agreements: agreements
                    .into_iter()
                    .map(|(release_id, a)| (release_id, BridgeAgreements::from_core(a)))
                    .collect(),
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
            } => BridgeIdentifyState::Found {
                run: run.map(|run| BridgeIdentifyRun::from_core(run, rate)),
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
                narrowed_out_count,
                catalog_agreements: catalog_agreements
                    .into_iter()
                    .map(BridgeCatalogAgreement::from_core)
                    .collect(),
            },
            IdentifyStateView::NotFoundAnywhere { run } => BridgeIdentifyState::NotFoundAnywhere {
                run: run.map(|run| BridgeIdentifyRun::from_core(run, rate)),
            },
            IdentifyStateView::ManualOnly { track_count, run } => BridgeIdentifyState::ManualOnly {
                track_count,
                run: run.map(|run| BridgeIdentifyRun::from_core(run, rate)),
            },
            IdentifyStateView::Failed {
                run,
                failures,
                groups,
                library_statuses,
                agreements,
                narrowed_out_count,
                catalog_agreements,
            } => BridgeIdentifyState::Failed {
                run: run.map(|run| BridgeIdentifyRun::from_core(run, rate)),
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
                narrowed_out_count,
                catalog_agreements: catalog_agreements
                    .into_iter()
                    .map(BridgeCatalogAgreement::from_core)
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
            source: BridgeCatalog::from_core(failure.source),
            failure: BridgeLookupFailure::from_core(failure.failure),
        },
        IdentifyFailure::Catalog(failure) => crate::types::BridgeIdentifyFailure::Catalog {
            source: BridgeCatalog::from_core(failure.source),
            failure: BridgeLookupFailure::from_core(failure.failure),
        },
        IdentifyFailure::Search(failure) => crate::types::BridgeIdentifyFailure::Search {
            source: BridgeCatalog::from_core(failure.source),
            failure: BridgeLookupFailure::from_core(failure.failure),
        },
        IdentifyFailure::ReleaseDetails(failure) => {
            crate::types::BridgeIdentifyFailure::ReleaseDetails {
                failure: BridgeLookupFailure::from_core(failure),
            }
        }
    }
}

/// Key library statuses by release id, which each status carries.
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
        BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState, LookupState,
        ProviderLookup, ValueLookup,
    };
    use bae_core::import::Catalog;
    use bae_core::signals::{DiscIdSignal, LookupFailure, SourcedValue};

    fn in_flight(barcode: BarcodeProgress) -> IdentifyState {
        IdentifyState::Triangulating {
            discid: DiscidProgress::Skipped,
            barcode,
            catalog: CatalogProgress::Skipped,
            search: bae_core::identify::SearchProgress::Skipped,
            context: SignalsContext {
                providers: vec![Catalog::MusicBrainz, Catalog::Discogs],
                steps: bae_core::config::IdentificationSteps::default(),
                artwork: bae_core::signals::ArtworkScan::Absent,
                rip: bae_core::signals::RipEvidence::Unproven,
                disc: DiscIdEvidence {
                    signal: DiscIdSignal::Absent,
                    ..Default::default()
                },
                barcode: BarcodeEvidence {
                    codes: vec![SourcedValue::in_file(
                        "0123456789012".to_string(),
                        "back.jpg".to_string(),
                    )],
                    had_source: true,
                    ..Default::default()
                },
                catalog: Default::default(),
                search: Default::default(),
                text: Default::default(),
                text_settled: true,
                audio: bae_core::signals::AudioFacts {
                    track_count: 9,
                    ..Default::default()
                },
                registered_in: None,
                album_links: bae_core::identify::state::AlbumLinkReading::Pending,
                documents: bae_core::identify::documents::DocumentReading::Pending,
            },
        }
    }

    fn barcode_step(state: IdentifyState) -> BridgeBarcodeStep {
        match BridgeIdentifyState::from_core(state) {
            BridgeIdentifyState::Triangulating { run, .. } => run.barcode,
            other => panic!("expected a run in flight, got {other:?}"),
        }
    }

    /// A code crosses as one row with one cell per provider, each its own.
    #[test]
    fn a_code_crosses_as_a_row_with_one_cell_per_provider() {
        let step = barcode_step(in_flight(BarcodeProgress::Lookups {
            codes: vec![ValueLookup {
                value: "0123456789012".to_string(),
                providers: vec![
                    ProviderLookup {
                        source: Catalog::MusicBrainz,
                        state: LookupState::LookingUp,
                    },
                    ProviderLookup {
                        source: Catalog::Discogs,
                        state: LookupState::Failed {
                            failure: LookupFailure::Diagnostic {
                                detail: "provider lookup failed".to_string(),
                            },
                        },
                    },
                ],
            }],
        }));
        let BridgeBarcodeStep::Rows { scanning, rows } = step else {
            panic!("a lookup in flight crosses as rows, got {step:?}");
        };
        assert!(!scanning);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, "0123456789012");
        assert!(!rows[0].excluded);
        assert_eq!(rows[0].cells.len(), 2);
        assert_eq!(rows[0].cells[0].source, BridgeCatalog::MusicBrainz);
        assert!(matches!(
            rows[0].cells[0].lookup,
            BridgeLookupState::LookingUp
        ));
        assert_eq!(rows[0].cells[1].source, BridgeCatalog::Discogs);
        assert!(matches!(
            &rows[0].cells[1].lookup,
            BridgeLookupState::Failed {
                failure: BridgeLookupFailure::Diagnostic { detail }
            } if detail == "provider lookup failed"
        ));
    }

    /// A code the person left out crosses as a row whose cells say so.
    #[test]
    fn a_code_left_out_crosses_as_a_row_that_says_so() {
        let mut state = in_flight(BarcodeProgress::NotAsked {
            codes: vec!["0123456789012".to_string()],
            reason: bae_core::identify::NotAskedReason::LeftOut,
        });
        let IdentifyState::Triangulating { context, .. } = &mut state else {
            panic!("a run in flight");
        };
        context.barcode.excluded = vec!["0123456789012".to_string()];
        let step = barcode_step(state);
        let BridgeBarcodeStep::Rows { rows, .. } = step else {
            panic!("a left-out code still crosses as a row, got {step:?}");
        };
        assert_eq!(rows.len(), 1);
        assert!(rows[0].excluded);
        assert!(rows[0].cells.iter().all(|cell| matches!(
            cell.lookup,
            BridgeLookupState::NotAsked {
                reason: BridgeNotAskedReason::LeftOut
            }
        )));
    }

    /// A disc ID nobody looked up crosses as read, with why.
    #[test]
    fn a_disc_id_nobody_looked_up_crosses_with_why() {
        use bae_core::identify::NotAskedReason;
        let step = |reason: NotAskedReason| {
            let mut state = in_flight(BarcodeProgress::Skipped);
            let IdentifyState::Triangulating {
                discid, context, ..
            } = &mut state
            else {
                panic!("a run in flight");
            };
            *discid = DiscidProgress::NotAsked { reason };
            context.disc.signal = DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                source_file: None,
            };
            match BridgeIdentifyState::from_core(state) {
                BridgeIdentifyState::Triangulating { run, .. } => run.disc_id,
                other => panic!("expected a run in flight, got {other:?}"),
            }
        };
        for (reason, crossed) in [
            (NotAskedReason::LeftOut, BridgeNotAskedReason::LeftOut),
            (
                NotAskedReason::SwitchedOff,
                BridgeNotAskedReason::SwitchedOff,
            ),
            (NotAskedReason::NoCatalog, BridgeNotAskedReason::NoCatalog),
        ] {
            assert!(matches!(
                step(reason),
                BridgeDiscIdStep::Read {
                    lookup: BridgeLookupState::NotAsked { reason },
                    ..
                } if reason == crossed
            ));
        }
    }

    /// An unread sheet crosses with the rate of the audio the run was over:
    /// the stored step states no number, the audio's files do.
    #[test]
    fn an_unread_sheet_crosses_with_the_audio_s_rate() {
        let mut state = in_flight(BarcodeProgress::Skipped);
        let IdentifyState::Triangulating { context, .. } = &mut state else {
            panic!("a run in flight");
        };
        context.rip = bae_core::signals::RipEvidence::NotCd;
        context.disc.signal = DiscIdSignal::NotCdAudio;
        context.audio.rate_ruling_out_cd = Some(96_000);
        match BridgeIdentifyState::from_core(state) {
            BridgeIdentifyState::Triangulating { run, .. } => assert!(matches!(
                run.disc_id,
                BridgeDiscIdStep::NotCdAudio {
                    sample_rate_hz: Some(96_000)
                }
            )),
            other => panic!("expected a run in flight, got {other:?}"),
        }
    }

    /// A failed barcode scan crosses as its own variant, not a provider's.
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

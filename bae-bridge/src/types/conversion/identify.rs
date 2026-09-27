use super::super::*;

impl BridgeMetadataResult {
    pub(crate) fn from_core(r: bae_core::import::search::MetadataResult) -> Self {
        let facts = BridgePressingFacts::from_core(r.facts());
        let bae_core::import::search::MetadataResult {
            source,
            release_id,
            year,
            labels,
            barcodes,
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
            // What the record said about the pressing is the facts read above;
            // its media in their own shape and its counterparts are pairing
            // evidence, and the pairing they fed is already the row.
            area: _,
            status: _,
            packaging: _,
            discogs_details: _,
            media: _,
            links: _,
            // Which cards the albums join is already the grouping's answer.
            album_links: _,
        } = r;
        BridgeMetadataResult {
            source: BridgeCatalog::from_core(source),
            release_id,
            year,
            labels: labels
                .into_iter()
                .map(BridgeReleaseLabel::from_core)
                .collect(),
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
            // Core already chose which covers to offer by it; a surface
            // draws the covers it is handed.
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
            // Pairing evidence the result a pick becomes carries; the picker
            // renders the facts.
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
    BridgeLookupState = bae_core::identify::LookupView,
    from_core: fn,
    variants: {
        Queued,
        NotAsked,
        LookingUp,
        Found { count, groups: (each BridgeReleaseGroup) },
        NoMatch,
        Failed { failure: (BridgeLookupFailure) },
        Off,
    },
}

mirror_struct! {
    BridgeProviderCell = bae_core::identify::ProviderCell,
    from_core: fn,
    fields: { source: (BridgeCatalog), lookup: (BridgeLookupState) },
}

impl BridgeSignalValueRow {
    fn from_core(row: bae_core::identify::SignalValueRow) -> Self {
        let bae_core::identify::SignalValueRow {
            value,
            // Where the value was read is automation's to report; no surface
            // shows it.
            sources: _,
            excluded,
            cells,
        } = row;
        Self {
            value,
            excluded,
            cells: cells
                .into_iter()
                .map(BridgeProviderCell::from_core)
                .collect(),
        }
    }
}

impl BridgeDiscIdStep {
    // The file a disc ID was read off is automation's to report; no surface
    // shows it.
    fn from_core(step: bae_core::identify::DiscIdStepView) -> Self {
        use bae_core::identify::DiscIdStepView;
        match step {
            DiscIdStepView::Reading => Self::Reading,
            DiscIdStepView::Absent => Self::Absent,
            DiscIdStepView::NotCdAudio { sample_rate_hz } => Self::NotCdAudio { sample_rate_hz },
            DiscIdStepView::ReadFailed { failure } => Self::ReadFailed {
                failure: BridgeLookupFailure::from_core(failure),
            },
            DiscIdStepView::Read {
                disc_id,
                source: _,
                lookup,
            } => Self::Read {
                disc_id,
                lookup: BridgeLookupState::from_core(lookup),
            },
            DiscIdStepView::ReadNotAsked { disc_id, source: _ } => Self::ReadNotAsked { disc_id },
            DiscIdStepView::LeftOut { disc_id, source: _ } => Self::LeftOut { disc_id },
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

impl BridgeCatalogCandidate {
    fn from_core(candidate: bae_core::identify::CatalogCandidateView) -> Self {
        let bae_core::identify::CatalogCandidateView {
            value,
            // Where the number was read is automation's to report; no surface
            // shows it.
            sources: _,
        } = candidate;
        Self { value }
    }
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
        Off,
        NotNeeded,
        NoTitle,
        Waiting { album, artist },
        Searched { album, artist, cells: (each BridgeProviderCell) },
    },
}

mirror_struct! {
    BridgeIdentifyRun = bae_core::identify::IdentifyRunView,
    from_core: fn,
    fields: {
        providers: (each BridgeCatalog),
        disc_id: (BridgeDiscIdStep),
        barcode: (BridgeBarcodeStep),
        catalog: (BridgeCatalogStep),
        search: (BridgeSearchStep),
        album_links: (BridgeAlbumLinksStep),
    },
}

mirror_enum! {
    BridgeAlbumLinksStep = bae_core::identify::AlbumLinksStepView,
    from_core: fn,
    variants: { Followed, Off },
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
    BridgeDiscIdSignal = bae_core::signals::DiscIdSignal,
    from_core: fn,
    variants: {
        Computed { disc_id, track_count, source_file },
        Absent { track_count },
        NotCdAudio { track_count, sample_rate_hz },
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

/// Not a `mirror_struct`: the rip evidence, the mono flag, the durations and
/// the text lines stay in core, which already shows what it concluded from
/// them.
impl BridgeSignals {
    pub(crate) fn from_core(s: bae_core::signals::Signals) -> Self {
        let bae_core::signals::Signals {
            rip: _,
            mono_audio: _,
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
/// copy per variant.
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

mirror_struct! {
    BridgeNarrowedOut = bae_core::identify::NarrowedOutView,
    from_core: fn,
    fields: { groups: (each BridgeReleaseGroup), count },
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
    use bae_core::signals::{DiscIdSignal, LookupFailure, SourcedValue, TextOrigin};

    fn in_flight(barcode: BarcodeProgress) -> IdentifyState {
        IdentifyState::Triangulating {
            discid: DiscidProgress::Skipped { track_count: 9 },
            barcode,
            catalog: CatalogProgress::Skipped,
            search: bae_core::identify::SearchProgress::Skipped,
            context: SignalsContext {
                providers: vec![Catalog::MusicBrainz, Catalog::Discogs],
                steps: bae_core::config::IdentificationSteps::default(),
                artwork: bae_core::signals::ArtworkScan::Absent,
                rip: bae_core::signals::RipEvidence::Unproven,
                mono_audio: false,
                disc: DiscIdEvidence {
                    signal: DiscIdSignal::Absent { track_count: 9 },
                    ..Default::default()
                },
                barcode: BarcodeEvidence {
                    codes: vec![SourcedValue::in_file(
                        "0123456789012".to_string(),
                        TextOrigin::Artwork,
                        "back.jpg".to_string(),
                    )],
                    had_source: true,
                    ..Default::default()
                },
                catalog: Default::default(),
                search: Default::default(),
                text: Default::default(),
                text_settled: true,
                track_count: 9,
                album_links: bae_core::identify::state::AlbumLinkReading::Pending,
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

    /// Every part of what a candidate's identification asks about crosses, and
    /// crosses back: the signals it leaves out, the numbers it looks up, and
    /// the numbers struck out of the candidate's own text so they rank nothing.
    #[test]
    fn every_part_of_what_identification_asks_about_crosses() {
        let choices = bae_core::import::LookupChoices {
            disc_id_excluded: true,
            excluded_barcodes: vec!["0123456789012".to_string(), "9999999999999".to_string()],
            chosen_catalogs: vec!["WPCR-80001".to_string()],
            search_words: None,
            discounted_catalogs: vec!["LBL-9".to_string()],
        };
        let crossed = crate::types::BridgeLookupChoices::from_core(choices.clone());
        assert_eq!(
            crossed.excluded_barcodes,
            vec!["0123456789012".to_string(), "9999999999999".to_string()]
        );
        assert_eq!(crossed.chosen_catalogs, vec!["WPCR-80001".to_string()]);
        assert_eq!(crossed.discounted_catalogs, vec!["LBL-9".to_string()]);
        assert_eq!(crossed.into_core(), choices);
    }

    /// A code the person left out crosses as a row that says so, beside the
    /// codes the run asked about — the surface draws it as the off chip it is
    /// rather than as a lookup that found nothing.
    #[test]
    fn a_code_left_out_crosses_as_a_row_that_says_so() {
        let mut state = in_flight(BarcodeProgress::NotAsked {
            codes: vec!["0123456789012".to_string()],
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
        assert!(rows[0]
            .cells
            .iter()
            .all(|cell| matches!(cell.lookup, BridgeLookupState::NotAsked)));
    }

    /// A disc ID nothing looked up crosses as which of the two it is: the
    /// person left it out, or no provider the run asks answers disc IDs.
    #[test]
    fn a_left_out_disc_id_crosses_apart_from_one_no_provider_answers() {
        let step = |excluded: bool| {
            let mut state = in_flight(BarcodeProgress::Skipped);
            let IdentifyState::Triangulating {
                discid, context, ..
            } = &mut state
            else {
                panic!("a run in flight");
            };
            *discid = DiscidProgress::NotAsked { track_count: 9 };
            context.disc.signal = DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 9,
                source_file: None,
            };
            context.disc.excluded = excluded;
            match BridgeIdentifyState::from_core(state) {
                BridgeIdentifyState::Triangulating { run, .. } => run.disc_id,
                other => panic!("expected a run in flight, got {other:?}"),
            }
        };
        assert!(matches!(step(true), BridgeDiscIdStep::LeftOut { .. }));
        assert!(matches!(step(false), BridgeDiscIdStep::ReadNotAsked { .. }));
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

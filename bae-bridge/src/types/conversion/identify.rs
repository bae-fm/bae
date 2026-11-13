use super::super::*;

impl BridgeMetadataResult {
    pub(crate) fn from_core(r: bae_catalogs::search::MetadataResult) -> Self {
        let facts = BridgePressingFacts::from_core(r.facts());
        let bae_catalogs::search::MetadataResult {
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
            // auto-import and ranking evidence, not something a row renders.
            source_tracks: _,
            album_first_year: _,
            track_titles: _,
            notes: _,
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
            source: BridgeLookupCatalog::from_core(source),
            release_id,
            year,
            facts,
            barcodes,
            source_group_id,
        }
    }
}
impl BridgeRemoteCover {
    pub(crate) fn from_core(c: bae_catalogs::remote_image::RemoteCover) -> Self {
        let bae_catalogs::remote_image::RemoteCover {
            image,
            label,
            source,
            // Core already chose which covers to offer by it.
            standing: _,
        } = c;
        BridgeRemoteCover {
            cover_choice: BridgeCoverChoice::from_core(bae_import::CoverChoice::Remote {
                image,
                source,
            }),
            source_name: source.cover_source_name().to_string(),
            label: BridgeCoverLabel::from_core(label),
        }
    }
}

impl BridgeCoverChoice {
    pub(crate) fn from_core(choice: bae_import::CoverChoice) -> Self {
        Self {
            selection: match choice.selection() {
                bae_import::CoverSelection::Local(file_id) => {
                    BridgeCoverSelection::ReleaseImage { file_id }
                }
                bae_import::CoverSelection::Remote(image, source) => {
                    BridgeCoverSelection::RemoteCover {
                        selection: BridgeRemoteCoverSelection {
                            image: BridgeRemoteImageSet::from_core(image),
                            source: BridgeCatalog::from_core(source),
                        },
                    }
                }
                bae_import::CoverSelection::Embedded(source_file_id) => {
                    BridgeCoverSelection::EmbeddedCover { source_file_id }
                }
            },
            image: BridgeCoverImageSource::from_core(choice.into_image()),
        }
    }
}

impl BridgeCoverImageSource {
    pub(crate) fn from_core(source: bae_import::CoverImageSource) -> Self {
        match source {
            bae_import::CoverImageSource::Library { image } => {
                Self::Library { image: crate::types::BridgeImageRef::from_core(image) }
            }
            bae_import::CoverImageSource::Remote { image } => {
                Self::Remote { image: BridgeRemoteImageSet::from_core(image) }
            }
            bae_import::CoverImageSource::Local { file } => {
                Self::Local { file: BridgeFileVersion::from_core(file) }
            }
            bae_import::CoverImageSource::Embedded { source, candidate, source_file_id } => {
                Self::Embedded {
                    source: BridgeFileVersion::from_core(source),
                    candidate_key: candidate.into(),
                    source_file_id,
                }
            }
        }
    }
}

impl BridgeFileVersion {
    pub(crate) fn from_core(file: bae_import::folder_scanner::FileVersion) -> Self {
        let bae_import::folder_scanner::FileVersion { path, size, modified_at_ns } = file;
        Self { path: coven::path_bytes(&path), size, modified_at_ns }
    }
}

mirror_enum! {
    BridgeNotAskedReason = bae_identify::NotAskedReason,
    from_core: fn,
    variants: { LeftOut, NoCatalog },
}

mirror_enum! {
    BridgeLookupState = bae_identify::LookupView,
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
    BridgeProviderCell = bae_identify::ProviderCell,
    from_core: fn,
    fields: { source: (BridgeLookupCatalog), lookup: (BridgeLookupState) },
}

mirror_struct! {
    BridgeDiscIdRow = bae_identify::DiscIdRow,
    from_core: fn,
    fields: { disc_id, lookup: (BridgeLookupState) },
}

mirror_struct! {
    BridgeSignalValueRow = bae_identify::SignalValueRow,
    from_core: fn,
    fields: { value, excluded, cells: (each BridgeProviderCell) },
}

impl BridgeDiscIdStep {
    /// Not a copy: an unread sheet crosses with the rate of the audio it was
    /// run over, which is what says why it was not read.
    fn from_core(step: bae_identify::DiscIdStepView, rate: Option<u32>) -> Self {
        use bae_identify::DiscIdStepView;
        match step {
            DiscIdStepView::Reading => Self::Reading,
            DiscIdStepView::Absent => Self::Absent,
            DiscIdStepView::NotCdAudio => Self::NotCdAudio { sample_rate_hz: rate },
            DiscIdStepView::Read { discs } => {
                Self::Read { discs: discs.into_iter().map(BridgeDiscIdRow::from_core).collect() }
            }
        }
    }
}

impl BridgeBarcodeStep {
    /// Not a copy: `scanning` crosses once for the run, as
    /// `BridgeIdentifyRun::reading_artwork`.
    fn from_core(step: bae_identify::BarcodeStepView) -> Self {
        use bae_identify::BarcodeStepView;
        match step {
            BarcodeStepView::Absent => Self::Absent,
            BarcodeStepView::NoCodes => Self::NoCodes,
            BarcodeStepView::Rows { scanning: _, rows } => {
                Self::Rows { rows: rows.into_iter().map(BridgeSignalValueRow::from_core).collect() }
            }
        }
    }
}

mirror_struct! {
    BridgeCatalogCandidate = bae_identify::CatalogCandidateView,
    from_core: fn,
    fields: { value },
}

impl BridgeCatalogStep {
    /// Not a copy: `scanning` crosses once for the run, as
    /// `BridgeIdentifyRun::reading_artwork`.
    fn from_core(step: bae_identify::CatalogStepView) -> Self {
        use bae_identify::CatalogStepView;
        match step {
            CatalogStepView::NoneFound => Self::NoneFound,
            CatalogStepView::Numbers { scanning: _, rows, candidates } => Self::Numbers {
                rows: rows.into_iter().map(BridgeSignalValueRow::from_core).collect(),
                candidates: candidates.into_iter().map(BridgeCatalogCandidate::from_core).collect(),
            },
        }
    }
}

mirror_enum! {
    BridgeSearchStep = bae_identify::SearchStepView,
    from_core: fn,
    variants: {
        NotNeeded,
        NoTitle,
        Waiting { album, artist },
        Searched { album, artist, cells: (each BridgeProviderCell) },
    },
}

mirror_enum! {
    BridgeIsrcStep = bae_identify::IsrcStepView,
    from_core: fn,
    variants: {
        Reading,
        Absent,
        Read { isrcs, lookup: (BridgeLookupState) },
    },
}

impl BridgeIdentifyRun {
    /// The ledger, with the rate of the audio the run was over for its
    /// disc-ID step.
    fn from_core(run: bae_identify::IdentifyRunView, rate: Option<u32>) -> Self {
        let reading_artwork = run.reading_artwork();
        let bae_identify::IdentifyRunView { providers, disc_id, barcode, catalog, isrc, search } =
            run;
        Self {
            providers: providers.into_iter().map(BridgeLookupCatalog::from_core).collect(),
            disc_id: BridgeDiscIdStep::from_core(disc_id, rate),
            barcode: BridgeBarcodeStep::from_core(barcode),
            catalog: BridgeCatalogStep::from_core(catalog),
            isrc: BridgeIsrcStep::from_core(isrc),
            search: BridgeSearchStep::from_core(search),
            reading_artwork,
        }
    }
}

mirror_struct! {
    BridgeReleaseGroupSource = bae_identify::release_group::ReleaseGroupSource,
    from_core: fn,
    fields: { source: (BridgeCatalog), group_url, album_links_unread },
}

impl BridgeReleaseGroup {
    pub(crate) fn from_core(g: bae_identify::release_group::ReleaseGroup) -> Self {
        let bae_identify::release_group::ReleaseGroup {
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
            artist: artist.map(BridgeCreditLine::from_core),
            label,
            cover_art: cover_art.map(BridgeRemoteCover::from_core),
            sources: sources.into_iter().map(BridgeReleaseGroupSource::from_core).collect(),
            year_min,
            year_max,
            sections: sections.into_iter().map(BridgePressingSection::from_core).collect(),
        }
    }
}

impl BridgePressingSection {
    fn from_core(section: bae_identify::release_group::PressingSection) -> Self {
        let bae_identify::release_group::PressingSection { album, pressings, narrowed_out } =
            section;
        BridgePressingSection {
            album: album.map(BridgeAlbumHeading::from_core),
            pressings: pressings.into_iter().map(BridgePressing::from_core).collect(),
            narrowed_out: narrowed_out.into_iter().map(BridgePressing::from_core).collect(),
        }
    }
}

mirror_struct! {
    BridgeAlbumHeading = bae_identify::release_group::AlbumHeading,
    from_core: fn,
    fields: { title, source: (BridgeReleaseGroupSource) },
}

impl BridgePressing {
    fn from_core(pressing: bae_identify::release_group::Pressing) -> Self {
        // The row's summary is what a pick applies: its lead record's facts.
        // Each record's own facts stay on it, in `releases`.
        let facts = BridgePressingFacts::from_core(pressing.lead().facts());
        BridgePressing {
            document_failure: pressing
                .document_failure()
                .cloned()
                .map(BridgeLookupFailure::from_core),
            labels: pressing.label_lines().into_iter().map(BridgeLabelLine::from_core).collect(),
            pick: crate::types::BridgePressingLink::from_core(pressing.pick()),
            sources: pressing.sources().into_iter().map(BridgeLookupCatalog::from_core).collect(),
            summary: crate::types::bridge_pressing_summary(facts.clone()),
            details: crate::types::bridge_pressing_details(facts),
            releases: pressing.releases.into_iter().map(BridgeMetadataResult::from_core).collect(),
        }
    }
}

mirror_enum! {
    BridgeTextSignal = bae_identify::signals::TextSignal,
    from_core: fn,
    variants: {
        Scanning { catalogs, free_text },
        Settled { catalogs, free_text },
        Failed { failure: (crate::types::BridgeInternalFailure), catalogs, free_text },
    },
}

/// Not a `mirror_struct`: only the text pools cross; what a surface needs from
/// the rest reaches it through the run's ledger.
impl BridgeSignals {
    pub(crate) fn from_core(s: bae_identify::signals::Signals) -> Self {
        let bae_identify::signals::Signals {
            origin: _,
            disc_id: _,
            barcode: _,
            text,
            text_pool: _,
            isrcs: _,
            track_titles: _,
        } = s;
        BridgeSignals { text: BridgeTextSignal::from_core(text) }
    }
}

impl BridgeAgreements {
    /// The row's agreements the list shows as badges. The album's title and
    /// artist rank the rows but are no badge.
    fn from_core(agreements: bae_identify::RowAgreements) -> Self {
        let bae_identify::RowAgreements { fields, notes } = agreements;
        let bae_identify::Agreements {
            disc_id,
            barcode,
            catalog,
            label,
            year,
            country,
            title: _,
            artist: _,
        } = fields;
        Self { disc_id, barcode, catalog, label, year, country, notes }
    }
}

/// Mirror [`bae_identify::IdentifyStateView`] into the uniffi enum, a field
/// copy per variant with the rate of the state's audio on its run.
impl BridgeIdentifyState {
    pub(crate) fn from_core(s: bae_identify::IdentifyState) -> Self {
        let rate = s.rate_ruling_out_cd();
        Self::from_view(bae_identify::IdentifyStateView::from(s), rate)
    }

    /// What a run published, as its rows hold it.
    pub(crate) fn from_published(published: bae_identify::PublishedRun) -> Self {
        Self::from_view(published.view(), published.rate_ruling_out_cd())
    }

    fn from_view(view: bae_identify::IdentifyStateView, rate: Option<u32>) -> Self {
        use bae_identify::IdentifyStateView;
        match view {
            IdentifyStateView::Idle => BridgeIdentifyState::Idle,
            IdentifyStateView::Error { failure } => BridgeIdentifyState::Error {
                failure: crate::types::BridgeInternalFailure::from_core(failure),
            },
            IdentifyStateView::Triangulating { run, groups, agreements, narrowed_out_count } => {
                BridgeIdentifyState::Triangulating {
                    run: BridgeIdentifyRun::from_core(run, rate),
                    groups: groups.into_iter().map(BridgeReleaseGroup::from_core).collect(),
                    agreements: agreements
                        .into_iter()
                        .map(|(release_id, a)| (release_id, BridgeAgreements::from_core(a)))
                        .collect(),
                    narrowed_out_count,
                }
            }
            IdentifyStateView::Found {
                run,
                groups,
                track_count,
                agreements,
                narrowed_out_count,
                folder_check,
                unattended_pick,
                offers_shared_album,
            } => BridgeIdentifyState::Found {
                run: run.map(|run| BridgeIdentifyRun::from_core(run, rate)),
                groups: groups.into_iter().map(BridgeReleaseGroup::from_core).collect(),
                track_count,
                agreements: agreements
                    .into_iter()
                    .map(|(release_id, a)| (release_id, BridgeAgreements::from_core(a)))
                    .collect(),
                narrowed_out_count,
                folder_check: folder_check
                    .map(|check| crate::types::BridgeFolderCheck::with_rate(check, rate)),
                unattended_pick,
                offers_shared_album,
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
                failure_lines,
                groups,
                agreements,
                narrowed_out_count,
                offers_shared_album,
            } => BridgeIdentifyState::Failed {
                run: run.map(|run| BridgeIdentifyRun::from_core(run, rate)),
                failure_lines: failure_lines.into_iter().map(identify_failure).collect(),
                groups: groups.into_iter().map(BridgeReleaseGroup::from_core).collect(),
                agreements: agreements
                    .into_iter()
                    .map(|(release_id, a)| (release_id, BridgeAgreements::from_core(a)))
                    .collect(),
                narrowed_out_count,
                offers_shared_album,
            },
        }
    }
}

fn identify_failure(failure: bae_identify::IdentifyFailure) -> crate::types::BridgeIdentifyFailure {
    use bae_identify::IdentifyFailure;
    match failure {
        IdentifyFailure::DiscId(failure) => crate::types::BridgeIdentifyFailure::DiscId {
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
        IdentifyFailure::Isrc(failure) => crate::types::BridgeIdentifyFailure::Isrc {
            failure: BridgeLookupFailure::from_core(failure),
        },
        IdentifyFailure::Pressing(failure) => crate::types::BridgeIdentifyFailure::Pressing {
            source: BridgeCatalog::from_core(failure.source),
            failure: BridgeLookupFailure::from_core(failure.failure),
        },
        IdentifyFailure::ReleaseDetails(failure) => {
            crate::types::BridgeIdentifyFailure::ReleaseDetails {
                failure: BridgeLookupFailure::from_core(failure),
            }
        }
        IdentifyFailure::Cover(failure) => crate::types::BridgeIdentifyFailure::Cover {
            source: BridgeCatalog::from_core(failure.source),
            failure: BridgeLookupFailure::from_core(failure.failure),
        },
    }
}

/// Key library statuses by release id, which each status carries.
#[cfg(test)]
#[path = "identify_tests.rs"]
mod tests;

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
            // Dropped: the card carries the album's title/artist/cover, so a
            // pressing projection keeps only pressing-distinguishing fields.
            title: _,
            artist: _,
            cover_art: _,
            source_group_id: _,
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
            },
            preview_source: cover_image_source(preview),
            thumbnail_source: cover_image_source(thumbnail),
        }
    }
}

#[cfg(feature = "desktop")]
fn cover_image_source(source: bae_core::import::CoverImageSource) -> BridgeCoverImageSource {
    match source {
        bae_core::import::CoverImageSource::Remote { url } => {
            BridgeCoverImageSource::Remote { url }
        }
        bae_core::import::CoverImageSource::Local { path } => BridgeCoverImageSource::Local {
            path: path.to_string_lossy().into_owned(),
        },
    }
}

#[cfg(feature = "desktop")]
impl BridgeClaimEvidence {
    pub(crate) fn from_core(evidence: bae_core::import::ClaimEvidence) -> Self {
        use bae_core::import::ClaimEvidence;
        match evidence {
            ClaimEvidence::DiscIdAlone => BridgeClaimEvidence::DiscIdAlone,
            ClaimEvidence::DiscIdShared { match_count } => {
                BridgeClaimEvidence::DiscIdShared { match_count }
            }
            ClaimEvidence::Barcode => BridgeClaimEvidence::Barcode,
            ClaimEvidence::Search => BridgeClaimEvidence::Search,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeDiscidProgress {
    fn from_view(p: bae_core::identify::DiscidProgressView) -> Self {
        use bae_core::identify::DiscidProgressView;
        match p {
            DiscidProgressView::Computing => BridgeDiscidProgress::Computing,
            DiscidProgressView::LookingUp => BridgeDiscidProgress::LookingUp,
            DiscidProgressView::Done { n_results } => BridgeDiscidProgress::Done { n_results },
            DiscidProgressView::Skipped => BridgeDiscidProgress::Skipped,
            DiscidProgressView::Failed { failure } => BridgeDiscidProgress::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeBarcodeProgress {
    fn from_view(p: bae_core::identify::BarcodeProgressView) -> Self {
        use bae_core::identify::BarcodeProgressView;
        match p {
            BarcodeProgressView::Scanning => BridgeBarcodeProgress::Scanning,
            BarcodeProgressView::LookingUp {
                current,
                position,
                total,
            } => BridgeBarcodeProgress::LookingUp {
                current,
                position,
                total,
            },
            BarcodeProgressView::Done { n_results } => BridgeBarcodeProgress::Done { n_results },
            BarcodeProgressView::Failed { failure } => BridgeBarcodeProgress::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
            BarcodeProgressView::Skipped => BridgeBarcodeProgress::Skipped,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseGroup {
    pub(crate) fn from_core(g: bae_core::import::release_group::ReleaseGroup) -> Self {
        let bae_core::import::release_group::ReleaseGroup {
            id,
            source_group_id,
            title,
            artist,
            cover_art,
            source_label,
            group_url,
            year_min,
            year_max,
            pressings,
        } = g;
        BridgeReleaseGroup {
            id,
            source_group_id,
            title,
            artist,
            cover_art: cover_art.map(BridgeRemoteCover::from_core),
            source_label,
            group_url,
            year_min,
            year_max,
            pressings: pressings
                .into_iter()
                .map(BridgeMetadataResult::from_core)
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
            } => BridgeDiscIdSignal::Computed {
                disc_id,
                track_count,
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
            matches_catalog,
        } = p;
        BridgeResultProvenance {
            by_disc_id,
            by_barcode,
            matches_catalog,
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
            IdentifyStateView::Triangulating { discid, barcode } => {
                BridgeIdentifyState::Triangulating {
                    discid: BridgeDiscidProgress::from_view(discid),
                    barcode: BridgeBarcodeProgress::from_view(barcode),
                }
            }
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

    #[test]
    fn barcode_progress_failure_crosses_bridge() {
        let progress = bae_core::identify::BarcodeProgress::Failed {
            failure: bae_core::signals::LookupFailure::Diagnostic {
                detail: "provider lookup failed".to_string(),
            },
        };

        let view = bae_core::identify::BarcodeProgressView::from(progress);
        match BridgeBarcodeProgress::from_view(view) {
            BridgeBarcodeProgress::Failed {
                failure: BridgeLookupFailure::Diagnostic { detail },
            } => assert_eq!(detail, "provider lookup failed"),
            other => panic!("expected failed barcode progress, got {other:?}"),
        }
    }
}

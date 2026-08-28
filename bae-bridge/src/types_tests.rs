use super::*;

#[test]
fn pairing_cancellation_crosses_the_bridge_as_cancellation() {
    assert!(matches!(
        BridgeError::from(bae_core::library::LibraryError::from(
            coven::ApproveDevicePairingError::Cancelled
        )),
        BridgeError::Cancelled
    ));
}

#[cfg(all(test, feature = "desktop"))]
mod triage_tests {
    use super::*;

    /// The stacking order the UIs iterate is core's, spelled once on this side
    /// so mobile builds carry it too. This is what keeps the two spellings from
    /// drifting — reorder either and it fails.
    #[test]
    fn group_order_mirrors_core() {
        let core: Vec<BridgeNeedsYouGroup> = bae_core::import::NeedsYouGroup::IN_ORDER
            .iter()
            .map(|group| BridgeNeedsYouGroup::from_core(*group))
            .collect();
        assert_eq!(bridge_needs_you_groups_in_order(), core);
    }

    /// The badge row is a projection of the identify state, so the two cross
    /// as one value rather than side by side: a state with signals carries its
    /// badges, and `Idle` carries none.
    #[test]
    fn the_toolbar_is_derived_from_the_state_it_crosses_with() {
        use bae_core::identify::IdentifyState;

        let context = bae_core::identify::state::SignalsContext {
            disc_id: bae_core::signals::DiscIdSignal::Absent { track_count: 9 },
            barcode_codes: Vec::new(),
            had_barcode_source: false,
            catalogs: Vec::new(),
            chosen_catalog: None,
            disc_excluded: false,
            barcode_excluded: false,
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            catalog_results: Vec::new(),
            discid_failure: None,
            barcode_failure: None,
            catalog_failure: None,
            matched_barcode: None,
            track_count: 9,
        };
        let live = IdentifyState::ManualOnly {
            track_count: 9,
            context,
        };
        let expected = live.toolbar().len();
        assert!(expected > 0, "the sample state has badges to project");

        let crossed =
            BridgeCandidateRuntimeSnapshot::from_core(bae_core::import::CandidateRuntimeSnapshot {
                identify: Some(live),
                import: None,
            });
        assert_eq!(crossed.signals_toolbar.signals.len(), expected);
        assert!(crossed.import.is_none());

        let idle =
            BridgeCandidateRuntimeSnapshot::from_core(bae_core::import::CandidateRuntimeSnapshot {
                identify: None,
                import: Some(bae_core::import::ImportInFlight {
                    progress_percent: 40,
                    step: None,
                }),
            });
        assert!(matches!(idle.identify_state, BridgeIdentifyState::Idle));
        assert!(idle.signals_toolbar.signals.is_empty());
        assert_eq!(idle.import.map(|import| import.progress_percent), Some(40));
    }

    /// A placement's tab is the one core's own projection gives it, for every
    /// variant — so `bridge_triage_tab` cannot become a second, divergent
    /// rule.
    #[test]
    fn tab_of_placement_mirrors_core() {
        use bae_core::import::{NeedsYouGroup, NeedsYouReason, TriagePlacement, TriageTab};
        for core in [
            TriagePlacement::Ready,
            TriagePlacement::NeedsYou {
                group: NeedsYouGroup::StillIdentifying,
                reason: NeedsYouReason::StillIdentifying {
                    phase: bae_core::import::IdentifyPhase::Queued,
                },
            },
            TriagePlacement::Importing,
            TriagePlacement::Done,
            TriagePlacement::Skipped,
        ] {
            let expected = match core.tab() {
                TriageTab::Pending => BridgeTriageTab::Pending,
                TriageTab::Done => BridgeTriageTab::Done,
                TriageTab::Skipped => BridgeTriageTab::Skipped,
            };
            let bridge = BridgeTriagePlacement::from_core(core);
            assert_eq!(bridge_triage_tab(&bridge), expected);
        }
    }
}

/// Round-trips a fully-populated sample through `from_core` then `into_core` and
/// asserts equality with the original. The one bug the exhaustive-destructure
/// compile checks can't catch is a transposed same-typed field introduced during
/// a rewrite; these catch it for both directions in one assertion (types without
/// `PartialEq` compare their `Debug` forms). Placeholder names only.
#[cfg(test)]
mod conversion_roundtrip {
    use super::*;

    #[cfg(feature = "desktop")]
    fn existing_artist() -> bae_core::import::ExistingArtist {
        bae_core::import::ExistingArtist {
            artist_id: "artist-1".to_string(),
            name: "Existing Artist".to_string(),
            sort_name: Some("Artist, Existing".to_string()),
            musicbrainz_artist_id: Some("musicbrainz-1".to_string()),
            discogs_artist_id: Some("discogs-1".to_string()),
        }
    }

    #[test]
    fn import_metadata_sources_cross_the_bridge_unchanged() {
        for core in [
            bae_core::config::DefaultImportMetadataSource::FindOnline,
            bae_core::config::DefaultImportMetadataSource::FileTags,
            bae_core::config::DefaultImportMetadataSource::None,
        ] {
            let bridge = BridgeDefaultImportMetadataSource::from_core(core);
            assert_eq!(bridge.into_core(), core);
        }
    }

    #[test]
    fn config_exposes_independent_default_source_and_automatic_policy() {
        use bae_core::config::{Config, DefaultImportMetadataSource as Source};

        for automatic_identification in [false, true] {
            for source in [Source::FindOnline, Source::FileTags, Source::None] {
                let mut config = Config::with_defaults(
                    "library".to_string(),
                    "device".to_string(),
                    std::path::PathBuf::from("/library"),
                    "Library".to_string(),
                );
                config.automatic_import_identification = automatic_identification;
                config.default_import_metadata_source = source;

                let bridge = BridgeConfig::from_core(&config);
                assert_eq!(
                    bridge.automatic_import_identification,
                    automatic_identification
                );
                assert_eq!(
                    bridge.default_import_metadata_source,
                    BridgeDefaultImportMetadataSource::from_core(source),
                );
            }
        }
    }

    #[test]
    fn cloud_setup_failure_reason_crosses_the_bridge_unchanged() {
        use bae_core::ui::UiErrorCategory;
        use coven::CloudHomeSetupFailure as Core;

        for (core, bridge) in [
            (
                Core::Authentication,
                BridgeCloudHomeSetupFailure::Authentication,
            ),
            (
                Core::PermissionDenied,
                BridgeCloudHomeSetupFailure::PermissionDenied,
            ),
            (
                Core::ContainerNotFound,
                BridgeCloudHomeSetupFailure::ContainerNotFound,
            ),
            (
                Core::RegionMismatch,
                BridgeCloudHomeSetupFailure::RegionMismatch,
            ),
            (
                Core::QuotaExceeded,
                BridgeCloudHomeSetupFailure::QuotaExceeded,
            ),
            (
                Core::InvalidConfiguration,
                BridgeCloudHomeSetupFailure::InvalidConfiguration,
            ),
            (
                Core::LocationOccupied,
                BridgeCloudHomeSetupFailure::LocationOccupied,
            ),
            (Core::Network, BridgeCloudHomeSetupFailure::Network),
            (
                Core::DeviceIdentityMissing,
                BridgeCloudHomeSetupFailure::DeviceIdentityMissing,
            ),
            (
                Core::SecureStorage,
                BridgeCloudHomeSetupFailure::SecureStorage,
            ),
            (Core::Internal, BridgeCloudHomeSetupFailure::Internal),
        ] {
            assert_eq!(
                BridgeErrorCategory::from_core(UiErrorCategory::CloudSetup(core)),
                BridgeErrorCategory::CloudSetup { failure: bridge },
            );
        }

        assert_eq!(
            BridgeErrorCategory::from_core(UiErrorCategory::DeviceIdentityMissing),
            BridgeErrorCategory::DeviceIdentityMissing,
        );
    }

    #[test]
    fn image_ref_round_trips() {
        let core = bae_core::album_detail::ImageRef {
            id: "rel-123".to_string(),
            version: "v1".to_string(),
            image_type: bae_core::db::LibraryImageType::Artist,
        };
        assert_eq!(core, BridgeImageRef::from_core(core.clone()).into_core());
    }

    #[test]
    fn artist_search_result_keeps_identity_fields() {
        let core = bae_core::album_detail::ArtistSearchResult {
            artist: bae_core::db::DbArtist {
                id: "artist-1".to_string(),
                name: "Artist Name".to_string(),
                sort_name: Some("Name, Artist".to_string()),
                discogs_artist_id: Some("discogs-1".to_string()),
                musicbrainz_artist_id: Some("musicbrainz-1".to_string()),
                created_at: "2026-01-01T00:00:00Z".parse().unwrap(),
            },
            image: Some(bae_core::album_detail::ImageRef {
                id: "artist-1".to_string(),
                version: "image-1".to_string(),
                image_type: bae_core::db::LibraryImageType::Artist,
            }),
        };

        let bridge = BridgeArtistSearchResult::from_core(core);
        assert_eq!(bridge.artist.artist_id, "artist-1");
        assert_eq!(bridge.artist.name, "Artist Name");
        assert_eq!(bridge.artist.sort_name.as_deref(), Some("Name, Artist"));
        assert_eq!(
            bridge.artist.discogs_artist_id.as_deref(),
            Some("discogs-1")
        );
        assert_eq!(
            bridge.artist.musicbrainz_artist_id.as_deref(),
            Some("musicbrainz-1")
        );
        assert_eq!(
            bridge.image.map(|image| image.id),
            Some("artist-1".to_string())
        );
    }

    #[test]
    fn export_preset_round_trips_and_re_derives_extension() {
        let core = bae_core::config::SavePreset {
            id: "preset-1".to_string(),
            name: "Preset One".to_string(),
            codec: bae_core::config::SaveCodec::Flac {
                bit_depth: bae_core::config::SaveBitDepth::Bits24,
            },
            filename_tokens: vec![
                bae_core::config::SaveFilenameToken::Artist,
                bae_core::config::SaveFilenameToken::Title,
            ],
            pregap_placement: bae_core::config::SavePregapPlacement::Exclude,
            applies_to_track: true,
            applies_to_release: false,
            embed_cover: false,
        };
        let bridge = BridgeSavePreset::from_core(&core);
        // `extension` is derived from the codec, not carried in the core preset.
        assert_eq!(bridge.extension, core.codec.extension());
        assert!(!bridge.embed_cover);
        assert_eq!(core, bridge.into_core());
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn release_user_edit_round_trips() {
        let core = bae_core::import::ReleaseUserEdit {
            album_title: "Album Title".to_string(),
            album_artist_assignments: vec![
                bae_core::import::ArtistAssignment::existing(existing_artist()),
                bae_core::import::ArtistAssignment::new("Artist Beta"),
            ],
            pressing: bae_core::import::PressingEdit {
                year: Some(1990),
                format: Some("CD".to_string()),
                label: Some("Label Name".to_string()),
                catalog_number: Some("CAT-1".to_string()),
                country: Some("US".to_string()),
                barcode: Some("012345678905".to_string()),
            },
            tracks: vec![bae_core::import::TrackUserEdit {
                title: "Track Title".to_string(),
                side: 1,
                track_number: Some(1),
                artist_assignments: bae_core::import::TrackArtistAssignments::Explicit(vec![
                    bae_core::import::ArtistAssignment::new("Track Artist"),
                ]),
                file: Some(bae_core::import::AudioFile::SheetSlice {
                    file_id: "CDImage.flac".to_string(),
                    sheet_id: "CDImage.cue".to_string(),
                    index: 0,
                }),
            }],
        };
        assert_eq!(
            core,
            BridgeReleaseUserEdit::from_core(core.clone()).into_core()
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn raw_release_edit_round_trips() {
        let core = bae_core::import::RawReleaseEdit {
            album_title: "Album Title".to_string(),
            album_artist_assignments: vec![
                bae_core::import::ArtistAssignment::new("Artist Name"),
                bae_core::import::ArtistAssignment::new("Artist Beta"),
            ],
            pressing: bae_core::import::RawPressingEdit {
                year: "1990".to_string(),
                format: "CD".to_string(),
                label: "Label Name".to_string(),
                catalog_number: "CAT-1".to_string(),
                country: "US".to_string(),
                barcode: "012345678905".to_string(),
            },
            tracks: vec![bae_core::import::RawTrackEdit {
                id: "row-1".to_string(),
                title: "Track Title".to_string(),
                artist_assignments: bae_core::import::TrackArtistAssignments::Explicit(vec![
                    bae_core::import::ArtistAssignment::new("Track Artist"),
                ]),
                side: 1,
                track_number: Some(1),
                // The audio binding is not a form field, so it has to survive
                // the editor's round trip untouched or a corrected pairing is
                // lost between the slot table and the commit.
                file: Some(bae_core::import::AudioFile::Standalone {
                    file_id: "01.flac".to_string(),
                }),
            }],
        };
        assert_eq!(
            core,
            BridgeRawReleaseEdit::from_core(core.clone()).into_core()
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn release_edit_seed_keeps_cores_reset_eligibility() {
        let edit = bae_core::import::RawReleaseEdit {
            album_title: "Album Title".to_string(),
            album_artist_assignments: vec![bae_core::import::ArtistAssignment::new("Artist Name")],
            pressing: bae_core::import::RawPressingEdit {
                year: String::new(),
                format: String::new(),
                label: String::new(),
                catalog_number: String::new(),
                country: String::new(),
                barcode: String::new(),
            },
            tracks: Vec::new(),
        };

        for expected in [false, true] {
            let bridge = BridgeReleaseEditSeed::from_core(bae_core::import::ReleaseEditSeed {
                edit: edit.clone(),
                can_reset_to_source: expected,
            });
            assert_eq!(bridge.can_reset_to_source, expected);
            assert_eq!(bridge.edit.album_title, edit.album_title);
        }
    }

    /// The detail crosses the bridge outbound only — it is the picker's display
    /// shape, never a seed — so this pins the derived fields and the carried ones,
    /// not a round trip.
    #[cfg(feature = "desktop")]
    #[test]
    fn release_detail_derives_default_cover() {
        let core = bae_core::import::search::ImportSearchReleaseDetail {
            release_id: "rel-123".to_string(),
            source: bae_core::import::MetadataSource::MusicBrainz,
            source_group_id: Some("rg-1".to_string()),
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1990),
            format: Some("CD".to_string()),
            label: Some("Label Name".to_string()),
            catalog_number: Some("CAT-1".to_string()),
            country: Some("US".to_string()),
            barcode: Some("012345678905".to_string()),
            track_count: 10,
            tracks: vec![bae_core::import::search::ReleaseTrack {
                title: "Track Title".to_string(),
                artist: Some("Track Artist".to_string()),
                duration_ms: Some(210_000),
                position: "A1".to_string(),
                side: 1,
            }],
            cover_art: vec![bae_core::import::cover_art::RemoteCover {
                url: "https://example.test/cover.jpg".to_string(),
                thumbnail_url: "https://example.test/thumb.jpg".to_string(),
                label: "Front".to_string(),
                source: bae_core::import::MetadataSource::MusicBrainz,
            }],
        };
        // `default_cover` is derived from the first cover.
        let bridge = BridgeReleaseDetail::from_core(core.clone());
        assert!(bridge.default_cover.is_some());
        assert_eq!(bridge.release_id, core.release_id);
        assert_eq!(bridge.track_count, core.track_count);
        assert_eq!(bridge.tracks.len(), core.tracks.len());
        assert_eq!(bridge.tracks[0].title, core.tracks[0].title);
        assert_eq!(bridge.tracks[0].position, core.tracks[0].position);
        assert_eq!(bridge.cover_art.len(), core.cover_art.len());
        assert_eq!(bridge.barcode, core.barcode);
    }
}

/// A database failure crosses two wrappers that each prefix themselves onto it,
/// and the `DbError` inside prefixes itself too, so what a person was shown read
/// "database error: database error: …". The category beside the detail is what
/// names the kind of failure; the detail names the fault, once.
#[test]
fn a_database_fault_is_not_prefixed_twice() {
    let inner =
        coven::DbError::Message("folder scan column verdict_kind holds \"conflict\"".to_string());
    // Both wrappers, rendered the way the UI receives them.
    for detail in [
        match BridgeError::database_query(coven::CovenError::Database(Box::new(
            coven::DbError::Message(
                "folder scan column verdict_kind holds \"conflict\"".to_string(),
            ),
        ))) {
            BridgeError::Diagnostic { detail, .. } => detail,
            other => panic!("expected a diagnostic, got {other:?}"),
        },
        match BridgeError::database_query(bae_core::library::LibraryError::Database(inner)) {
            BridgeError::Diagnostic { detail, .. } => detail,
            other => panic!("expected a diagnostic, got {other:?}"),
        },
    ] {
        assert_eq!(
            detail, "folder scan column verdict_kind holds \"conflict\"",
            "the fault crosses without either wrapper's prefix"
        );
    }
}

/// A failure that is not a wrapped database error still renders whole.
#[test]
fn a_non_database_coven_error_keeps_its_own_text() {
    let detail = match BridgeError::database_query(coven::CovenError::Sqlite(
        coven::rusqlite::Error::QueryReturnedNoRows,
    )) {
        BridgeError::Diagnostic { detail, .. } => detail,
        other => panic!("expected a diagnostic, got {other:?}"),
    };
    assert!(
        detail.contains("sqlite error"),
        "an unwrapped variant keeps its own rendering, got {detail:?}"
    );
}

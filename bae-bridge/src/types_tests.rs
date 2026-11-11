use super::*;

#[test]
fn pregap_clock_crosses_the_bridge_in_both_time_modes() {
    let elapsed_mode = bridge_seek_bar(-1_001, Some(100_000), false);
    let remaining_mode = bridge_seek_bar(-1_001, Some(100_000), true);

    assert_eq!(remaining_mode.leading, elapsed_mode.leading);
    assert!(elapsed_mode.leading.negative);
    assert_eq!(elapsed_mode.leading.minutes, 0);
    assert_eq!(elapsed_mode.leading.seconds, 2);
}

#[cfg(all(test, desktop))]
mod triage_tests {
    use super::*;

    #[test]
    fn an_idle_candidate_crosses_with_its_running_import() {
        let idle =
            BridgeCandidateRuntimeSnapshot::from_core(bae_import::CandidateRuntimeSnapshot {
                identification: None,
                import: Some(bae_import::ImportInFlight {
                    step: bae_import::ImportStep::Running(bae_import::ImportPhase::ReadingFiles),
                }),
                search: None,
            });
        assert!(idle.identification.is_none());
        assert!(matches!(
            idle.import.map(|import| import.step),
            Some(crate::types::BridgeImportStep::Running { .. })
        ));
    }

    /// A candidate waiting on the identification queue crosses as queued, so
    /// its pane says so rather than offering to start.
    #[test]
    fn a_queued_candidate_crosses_as_queued() {
        let queued =
            BridgeCandidateRuntimeSnapshot::from_core(bae_import::CandidateRuntimeSnapshot {
                identification: Some(bae_import::IdentificationRuntime::Queued(
                    bae_import::Admission::Automatic,
                )),
                import: None,
                search: None,
            });
        assert!(matches!(queued.identification, Some(BridgeIdentificationInFlight::Queued)));
    }
}

/// Conversions across the bridge. The round trips catch two same-typed fields
/// swapped, which the conversions' exhaustive destructures cannot. Placeholder
/// names only.
#[cfg(test)]
mod conversion_roundtrip {
    use super::*;

    /// The UI tells a person's answer from a proposed one by this value, so
    /// each variant must cross as itself.
    #[cfg(desktop)]
    #[test]
    fn the_draft_s_author_crosses_the_bridge_as_itself() {
        use bae_import::MetadataAuthor;
        let crossed: Vec<_> = [
            MetadataAuthor::Nobody,
            MetadataAuthor::Prefill,
            MetadataAuthor::Identification,
            MetadataAuthor::Person,
        ]
        .into_iter()
        .map(BridgeMetadataAuthor::from_core)
        .collect();
        assert_eq!(
            crossed,
            vec![
                BridgeMetadataAuthor::Nobody,
                BridgeMetadataAuthor::Prefill,
                BridgeMetadataAuthor::Identification,
                BridgeMetadataAuthor::Person,
            ]
        );
    }

    /// A restore row shows which account a Dropbox or OneDrive library is in,
    /// so the label crosses to the app and back unchanged.
    #[test]
    fn a_provider_account_crosses_the_bridge_both_ways() {
        let locations = [
            coven::StorageConfig::Dropbox {
                location: coven::DropboxLocation::SharedFolder {
                    namespace_id: "namespace-1".into(),
                },
                account_label: Some("person@example.com".into()),
                folder_label: None,
            },
            coven::StorageConfig::OneDrive {
                drive_id: "drive-1".into(),
                folder_id: "folder-1".into(),
                account_label: Some("person@example.org".into()),
                folder_label: None,
            },
            coven::StorageConfig::Dropbox {
                location: coven::DropboxLocation::AppFolder,
                account_label: None,
                folder_label: None,
            },
        ];
        for location in locations {
            let crossed = BridgeStorageLocation::from_core(location.clone()).into_core().unwrap();
            assert!(crossed.same_location(&location));
            assert_eq!(
                BridgeStorageLocation::from_core(crossed),
                BridgeStorageLocation::from_core(location),
            );
        }
    }

    /// The Sync page shows the folder's name and opens its web address, so
    /// both cross to the app as coven recorded them. Going back, the label is
    /// left to coven, which reads it at every connection.
    #[test]
    fn a_folder_label_crosses_to_the_app() {
        let labelled = [
            coven::StorageConfig::Dropbox {
                location: coven::DropboxLocation::SharedFolder {
                    namespace_id: "namespace-1".into(),
                },
                account_label: Some("person@example.com".into()),
                folder_label: Some(coven::FolderLabel {
                    name: "Music".into(),
                    web_url: "https://www.dropbox.com/home/Music".parse().unwrap(),
                }),
            },
            coven::StorageConfig::OneDrive {
                drive_id: "drive-1".into(),
                folder_id: "folder-1".into(),
                account_label: Some("person@example.org".into()),
                folder_label: Some(coven::FolderLabel {
                    name: "Music Library".into(),
                    web_url: "https://onedrive.live.com/?id=folder-1".parse().unwrap(),
                }),
            },
        ];
        let crossed: Vec<_> = labelled
            .iter()
            .map(|location| match BridgeStorageLocation::from_core(location.clone()) {
                BridgeStorageLocation::Dropbox { folder_label, .. }
                | BridgeStorageLocation::OneDrive { folder_label, .. } => folder_label,
                other => panic!("a Dropbox or OneDrive location crossed as {other:?}"),
            })
            .collect();
        assert_eq!(
            crossed,
            vec![
                Some(BridgeFolderLabel {
                    name: "Music".into(),
                    web_url: "https://www.dropbox.com/home/Music".into(),
                }),
                Some(BridgeFolderLabel {
                    name: "Music Library".into(),
                    web_url: "https://onedrive.live.com/?id=folder-1".into(),
                }),
            ]
        );
        for location in labelled {
            let back = BridgeStorageLocation::from_core(location.clone()).into_core().unwrap();
            assert!(back.same_location(&location));
            assert!(matches!(
                back,
                coven::StorageConfig::Dropbox { folder_label: None, .. }
                    | coven::StorageConfig::OneDrive { folder_label: None, .. }
            ));
        }
    }

    #[test]
    fn config_exposes_identify_automatically() {
        use bae_preferences::Config;

        for identify_automatically in [true, false] {
            let mut config = Config::with_defaults(
                coven::StoreId("00000000-0000-4000-8000-000000000001".parse().unwrap()),
                "/library",
            );
            config.prefs.identification.automatic = identify_automatically;

            let bridge = BridgeConfig::from_core(&config);
            assert_eq!(bridge.identify_automatically, identify_automatically);
        }
    }

    #[test]
    fn config_exposes_the_import_settings() {
        use bae_preferences::Config;

        let mut config = Config::with_defaults(
            coven::StoreId("00000000-0000-4000-8000-000000000001".parse().unwrap()),
            "/library",
        );
        config.prefs.identification.import_when_identified = true;
        config.prefs.import_destination = bae_preferences::ImportDestination::Cloud;

        let bridge = BridgeConfig::from_core(&config);
        assert!(bridge.import_when_identified);
        assert_eq!(bridge.import_destination, BridgeImportDestination::Cloud);
    }

    #[test]
    fn config_carries_every_side_pause_countdown_choice_both_ways() {
        use bae_preferences::{Config, SidePauseCountdown as Core};

        for (core, bridge) in [
            (Core::Off, BridgeSidePauseCountdown::Off),
            (Core::Seconds5, BridgeSidePauseCountdown::Seconds5),
            (Core::Seconds15, BridgeSidePauseCountdown::Seconds15),
            (Core::Seconds30, BridgeSidePauseCountdown::Seconds30),
            (Core::Seconds45, BridgeSidePauseCountdown::Seconds45),
            (Core::Seconds60, BridgeSidePauseCountdown::Seconds60),
        ] {
            let mut config = Config::with_defaults(
                coven::StoreId("00000000-0000-4000-8000-000000000001".parse().unwrap()),
                "/library",
            );
            config.prefs.side_pause_countdown = core;

            assert_eq!(BridgeConfig::from_core(&config).side_pause_countdown, bridge);
            assert_eq!(bridge.into_core(), core);
        }
    }

    #[test]
    fn side_pause_prompt_carries_its_kind_and_countdown_deadline_in_epoch_millis() {
        use bae_playback::{PlaybackPauseBoundary, PlaybackSideCountdown, PlaybackSidePausePrompt};

        let resumes_at = "2026-01-01T00:00:05Z".parse().unwrap();
        let prompt = BridgeSidePausePrompt::from_core(PlaybackSidePausePrompt {
            id: "next:1:Cd".to_string(),
            boundary: PlaybackPauseBoundary::Disc,
            side_label: "1".to_string(),
            countdown: Some(PlaybackSideCountdown { resumes_at }),
        });

        assert_eq!(
            prompt,
            BridgeSidePausePrompt {
                id: "next:1:Cd".to_string(),
                boundary: BridgePauseBoundary::Disc,
                side_label: "1".to_string(),
                countdown: Some(BridgeSideCountdown { resumes_at_ms: 1_767_225_605_000 }),
            }
        );
    }

    #[test]
    fn storage_setup_failure_reason_crosses_the_bridge_unchanged() {
        use bae_diagnostics::UiErrorCategory;
        use coven::StorageFailure as Core;

        for (core, bridge) in [
            (Core::Authentication, BridgeStorageFailure::Authentication),
            (Core::PermissionDenied, BridgeStorageFailure::PermissionDenied),
            (Core::ContainerNotFound, BridgeStorageFailure::ContainerNotFound),
            (Core::RegionMismatch, BridgeStorageFailure::RegionMismatch),
            (Core::QuotaExceeded, BridgeStorageFailure::QuotaExceeded),
            (Core::InvalidConfiguration, BridgeStorageFailure::InvalidConfiguration),
            (Core::Network, BridgeStorageFailure::Network),
        ] {
            assert_eq!(
                BridgeErrorCategory::from_core(UiErrorCategory::Storage(core)),
                BridgeErrorCategory::Storage { failure: bridge },
            );
        }
    }

    /// A grouping block crosses as its own translated line from either path,
    /// with its names only in the untranslated detail; `Unbuildable`, which no
    /// one can act on, crosses as the generic import failure.
    #[cfg(desktop)]
    #[test]
    fn grouping_blocks_carry_their_own_lines() {
        use bae_import::{GroupingBlock, ImportError};
        let taken =
            GroupingBlock::FolderFilesTaken { folder: "Album".into(), release: "Disc 1".into() };
        let refused = BridgeError::from(ImportError::GroupingBlocked { reason: taken.clone() });
        let BridgeError::Diagnostic { category, detail } = &refused else {
            panic!("a block is a diagnostic with its own line: {refused:?}");
        };
        assert_eq!(
            *category,
            BridgeErrorCategory::GroupingBlocked { reason: BridgeGroupingBlock::FolderFilesTaken }
        );
        assert_eq!(
            bridge_error_category_key(category.clone()),
            "core.import.grouping.folder_files_taken"
        );
        assert!(detail.contains("Album") && detail.contains("Disc 1"), "{detail}");
        assert_eq!(BridgeError::from(&taken), refused);

        let unbuildable =
            BridgeError::from(&GroupingBlock::Unbuildable { detail: "no playable tracks".into() });
        assert!(matches!(
            unbuildable,
            BridgeError::Diagnostic { category: BridgeErrorCategory::Import, .. }
        ));
    }

    #[test]
    fn image_ref_round_trips() {
        let core = bae_records::ImageRef {
            id: "rel-123".to_string(),
            version: "v1".to_string(),
            image_type: bae_records::LibraryImageType::Artist,
        };
        assert_eq!(core, BridgeImageRef::from_core(core.clone()).into_core());
    }

    #[test]
    fn artist_search_result_keeps_identity_fields() {
        let core = bae_views::album_detail::ArtistSearchResult {
            credit_names: vec![bae_records::credits::ArtistCreditName {
                artist_id: "artist-1".to_string(),
                credit: bae_records::credits::CreditName {
                    key: "printed name".to_string(),
                    name: "Printed Name".to_string(),
                },
            }],
            artist: bae_records::DbArtist {
                id: "artist-1".to_string(),
                name: "Artist Name".to_string(),
                sort_name: Some("Name, Artist".to_string()),
                discogs_artist_id: Some("discogs-1".to_string()),
                musicbrainz_artist_id: Some("musicbrainz-1".to_string()),
                created_at: "2026-01-01T00:00:00Z".parse().unwrap(),
            },
            image: Some(bae_records::ImageRef {
                id: "artist-1".to_string(),
                version: "image-1".to_string(),
                image_type: bae_records::LibraryImageType::Artist,
            }),
        };

        let bridge = BridgeArtistSearchResult::from_core(core);
        assert_eq!(bridge.artist.artist_id, "artist-1");
        assert_eq!(bridge.artist.name, "Artist Name");
        assert_eq!(bridge.artist.sort_name.as_deref(), Some("Name, Artist"));
        assert_eq!(bridge.artist.discogs_artist_id.as_deref(), Some("discogs-1"));
        assert_eq!(bridge.artist.musicbrainz_artist_id.as_deref(), Some("musicbrainz-1"));
        assert_eq!(bridge.image.map(|image| image.id), Some("artist-1".to_string()));
        assert_eq!(
            bridge.credit_names,
            [BridgeArtistCreditName {
                artist_id: "artist-1".to_string(),
                credit: BridgeCreditName {
                    key: "printed name".to_string(),
                    name: "Printed Name".to_string(),
                },
            }]
        );
    }

    #[test]
    fn export_preset_round_trips() {
        let core = bae_preferences::SavePreset {
            id: "preset-1".to_string(),
            name: "Preset One".to_string(),
            codec: bae_preferences::SaveCodec::Flac {
                bit_depth: bae_preferences::SaveBitDepth::SameAsSource,
            },
        };
        let bridge = BridgeSavePreset::from_core(&core);
        assert_eq!(core, bridge.into_core());
    }

    #[test]
    fn the_config_carries_the_save_extras() {
        let mut config = bae_preferences::Config::with_defaults(
            coven::StoreId("00000000-0000-4000-8000-000000000001".parse().unwrap()),
            "/library",
        );
        assert_eq!(
            BridgeConfig::from_core(&config).save_extras,
            BridgeSaveExtras { embed_cover: true, cover_file: true, loudness_tags: true }
        );
        config.prefs.save_extras.cover_file = false;
        assert!(!BridgeConfig::from_core(&config).save_extras.cover_file);
    }

    fn defaults() -> bae_preferences::Config {
        bae_preferences::Config::with_defaults(
            coven::StoreId("00000000-0000-4000-8000-000000000001".parse().unwrap()),
            "/library",
        )
    }

    #[test]
    fn the_config_carries_the_folder_name_pattern_as_typed() {
        let mut config = defaults();
        let pattern = bae_preferences::NamePattern::new(vec![
            bae_preferences::NamePiece::Value(bae_preferences::SaveFolderValue::Label),
            bae_preferences::NamePiece::Text(" / ".to_string()),
            bae_preferences::NamePiece::Value(bae_preferences::SaveFolderValue::AlbumYear),
        ]);
        config.prefs.save_folder_pattern = pattern;
        let bridge = BridgeConfig::from_core(&config);
        assert_eq!(
            bridge.folder_pieces,
            vec![
                BridgeSaveFolderPiece::Value { value: BridgeSaveFolderValue::Label },
                BridgeSaveFolderPiece::Text { text: " / ".to_string() },
                BridgeSaveFolderPiece::Value { value: BridgeSaveFolderValue::AlbumYear },
            ]
        );
        assert_eq!(
            bridge.folder_pattern_error,
            Some(BridgeNamePatternError::ForbiddenCharacter { character: "/".to_string() })
        );
        assert_eq!(bridge.filename_pattern_error, None);
    }

    #[test]
    fn the_config_carries_the_one_file_name_pattern_and_why_it_cant_make_a_name() {
        let mut config = defaults();
        let bridge = BridgeConfig::from_core(&config);
        assert_eq!(
            bridge.filename_pieces,
            vec![
                BridgeSaveFilenamePiece::Value { value: BridgeSaveFilenameValue::TrackNumber },
                BridgeSaveFilenamePiece::Text { text: " ".to_string() },
                BridgeSaveFilenamePiece::Value { value: BridgeSaveFilenameValue::Title },
            ]
        );
        assert_eq!(bridge.filename_pattern_error, None);
        assert_eq!(bridge.folder_pattern_error, None);

        config.prefs.save_filename_pattern = bae_preferences::NamePattern::new(vec![
            bae_preferences::NamePiece::Text(".x".to_string()),
            bae_preferences::NamePiece::Value(bae_preferences::SaveFilenameValue::Title),
        ]);
        config.prefs.save_folder_pattern = bae_preferences::NamePattern::new(vec![]);
        let bridge = BridgeConfig::from_core(&config);
        assert_eq!(bridge.filename_pattern_error, Some(BridgeNamePatternError::StartsWithDot));
        assert_eq!(bridge.folder_pattern_error, Some(BridgeNamePatternError::Empty));
    }

    #[test]
    fn a_piece_round_trips_through_the_bridge() {
        let piece = bae_preferences::NamePiece::Text("  - ".to_string());
        assert_eq!(BridgeSaveFilenamePiece::from_core(&piece).into_core(), piece);
        let piece =
            bae_preferences::NamePiece::Value(bae_preferences::SaveFolderValue::ReleaseYear);
        assert_eq!(BridgeSaveFolderPiece::from_core(&piece).into_core(), piece);
    }

    #[cfg(desktop)]
    #[test]
    fn raw_release_edit_round_trips() {
        let core = bae_import::RawReleaseEdit {
            album_title: "Album Title".to_string(),
            album_artist_assignments: bae_import::Credited::plain_list([
                bae_import::ArtistAssignment::named("Artist Name"),
                bae_import::ArtistAssignment::named("Artist Beta"),
            ]),
            album_year: "1987".to_string(),
            pressing: bae_import::RawPressingEdit {
                year: "1990".to_string(),
                labels: vec![
                    bae_import::RawLabelEdit {
                        name: "Label A".to_string(),
                        catalog_number: "AB 100".to_string(),
                        ..Default::default()
                    },
                    bae_import::RawLabelEdit::default(),
                ],
                barcodes: vec!["012345678905".to_string(), String::new()],
                facts: every_kind_of_fact(),
            },
            tracks: vec![bae_import::RawTrackEdit {
                id: "row-1".to_string(),
                title: "Track Title".to_string(),
                artist_assignments: bae_import::TrackArtistAssignments::Explicit(vec![
                    bae_import::Credited::plain(bae_import::ArtistAssignment::named(
                        "Track Artist",
                    )),
                ]),
                // Not a form field, so the round trip must keep it or a
                // corrected file pairing is lost before the commit.
                file: Some(bae_import::AudioFile::Standalone { file_id: "01.flac".to_string() }),
            }],
        };
        assert_eq!(core, BridgeRawReleaseEdit::from_core(core.clone()).into_core());
    }

    #[cfg(desktop)]
    #[test]
    fn release_edit_draft_carries_the_persisted_editor_projection() {
        let edit = bae_import::RawReleaseEdit {
            album_title: "Album Title".to_string(),
            album_artist_assignments: bae_import::Credited::plain_list([
                bae_import::ArtistAssignment::named("Artist Name"),
            ]),
            album_year: String::new(),
            pressing: bae_import::RawPressingEdit {
                year: String::new(),
                labels: Vec::new(),
                barcodes: Vec::new(),
                facts: bae_types::pressing::PressingFacts::default(),
            },
            tracks: vec![bae_import::RawTrackEdit {
                id: "track-id".to_string(),
                title: "Track Title".to_string(),
                artist_assignments: bae_import::TrackArtistAssignments::AlbumArtists,
                file: None,
            }],
        };

        {
            let bridge = BridgeReleaseEditDraft::from_core(bae_import::ReleaseEditDraft {
                edit: edit.clone(),
                cover: None,
                display: bae_import::ReleaseEditDisplayContext {
                    source_audio: None,
                    sides: vec![bae_import::ReleaseEditTrackSide {
                        side: bae_types::mediums::TrackSide::Disc { disc: 2 },
                        runs: vec![bae_import::ReleaseEditTrackRun::Track(
                            bae_import::ReleaseEditTrackContext {
                                track_id: "track-id".to_string(),
                                source: bae_import::ReleaseEditTrackSource {
                                    file_id: "file-id".to_string(),
                                    name: "source.flac".to_string(),
                                    layout: bae_records::source_audio::SourceAudioLayout::File,
                                },
                                duration_ms: 123_000,
                                position_text: "3".to_string(),
                            },
                        )],
                    }],
                    files: Vec::new(),
                },
                album: bae_import::DraftAlbum::Named {
                    album_id: "album-id".to_string(),
                    title: "Album Title".to_string(),
                },
                artist_resolutions: Vec::new(),
                changed: true,
                invalid: Some(bae_import::EditValidationError::NoAlbumArtist),
            });
            assert_eq!(bridge.edit.album_title, edit.album_title);
            assert!(bridge.changed);
            assert!(matches!(bridge.invalid, Some(BridgeValidationReason::NoAlbumArtist)));
            assert_eq!(
                bridge.album,
                BridgeDraftAlbum::Named {
                    album_id: "album-id".to_string(),
                    title: "Album Title".to_string(),
                }
            );
            let side = &bridge.display.sides[0];
            assert_eq!(side.header_key.as_deref(), Some("core.track.disc"));
            let BridgeReleaseEditTrackRun::Track { track } = &side.runs[0] else {
                panic!("the side's one run is its track");
            };
            assert_eq!(track.track_id, "track-id");
            assert_eq!(track.source.name, "source.flac");
            let clock = track.duration_clock;
            assert_eq!((clock.minutes, clock.seconds), (2, 3));
        }
    }
}

/// Facts stating one of every kind: a region, two media, a status, a
/// packaging and details of both wordings.
#[cfg(desktop)]
fn every_kind_of_fact() -> bae_types::pressing::PressingFacts {
    use bae_types::pressing::*;
    PressingFacts {
        area: Some(ReleaseArea::Region(Region::UkAndEurope)),
        media: vec![
            MediaCount { medium: Medium::Cd, count: 2 },
            MediaCount { medium: Medium::Dvd, count: 1 },
        ],
        status: Some(ReleaseStatus::Promotion),
        packaging: Some(Packaging::Digipak),
        discogs_details: vec![DiscogsDetail::Reissue, DiscogsDetail::Size12In],
    }
}

/// Each wrapper around a database failure adds its own prefix; the category
/// already names the kind of failure, so the detail carries only the fault.
#[test]
fn a_database_fault_is_not_prefixed_twice() {
    let fault = || bae_store::DbError::unreadable("a folder scan column", "invalid");
    let expected = fault().to_string();
    let error = bae_library::LibraryError::Database(fault());
    let BridgeError::Diagnostic { detail, .. } = BridgeError::from(error) else {
        panic!("diagnostic")
    };
    assert_eq!(detail, expected);
    let BridgeError::Diagnostic { detail, .. } = BridgeError::from(fault()) else {
        panic!("diagnostic")
    };
    assert_eq!(detail, expected, "a bare database failure reads the same");
    let error = coven::CovenError::Database(coven::DbError::DamagedDatabase);
    let expected = coven::DbError::DamagedDatabase.to_string();
    let BridgeError::Diagnostic { detail, .. } = BridgeError::database_query(error) else {
        panic!("diagnostic")
    };
    assert_eq!(detail, expected);
}

/// A database failure holding a bug, which its owner reported as it
/// arrived, or an earlier one the store failed with, shows as a bug.
#[test]
fn a_database_failure_holding_a_bug_shows_as_one() {
    let bug = || coven::task::Bug::new("live query", "boom");
    for error in [coven::DbError::Bug(bug()), coven::DbError::StoreFailed(bug())] {
        let BridgeError::Diagnostic { category, .. } =
            BridgeError::database_query(coven::CovenError::Database(error))
        else {
            panic!("diagnostic")
        };
        assert_eq!(category, BridgeErrorCategory::Internal);
    }
}

#[test]
fn a_non_database_coven_error_keeps_its_own_text() {
    let error = coven::CovenError::Key(coven::KeyError::Unsupported);
    let expected = error.to_string();
    let BridgeError::Diagnostic { detail, .. } = BridgeError::database_query(error) else {
        panic!("diagnostic")
    };
    assert_eq!(detail, expected);
}

/// A release read from several folders crosses as its folders, in play order,
/// each with the name the source list shows.
#[cfg(all(test, desktop))]
#[test]
fn a_release_read_from_several_folders_crosses_as_its_folders() {
    let parts = [
        bae_import::ReleasePart {
            folder: std::path::PathBuf::from("/music/Album/Disc 1"),
            prefix: "Disc 1/".into(),
        },
        bae_import::ReleasePart {
            folder: std::path::PathBuf::from("/music/Album/Disc 2"),
            prefix: "Disc 2/".into(),
        },
    ];
    let crossed: Vec<BridgeReleasePart> = parts.iter().map(BridgeReleasePart::from_core).collect();
    assert_eq!(
        crossed,
        vec![
            BridgeReleasePart { folder_path: "/music/Album/Disc 1".into(), name: "Disc 1".into() },
            BridgeReleasePart { folder_path: "/music/Album/Disc 2".into(), name: "Disc 2".into() },
        ]
    );
}

/// A record crosses with the error behind it as its details, since that names
/// the cause the record's class leaves out; a record with no error has no
/// details, never the reason's debug text.
#[test]
fn a_pending_record_with_its_error_crosses_with_that_error_as_its_details() {
    let mut item = bae_sync_status::PendingItem::before_open(coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::Storage {
            provider: coven::CloudProvider::S3,
            failure: coven::StorageFailure::Network,
            answered: false,
        },
    ));
    let without = BridgePendingRecord::from_core(item.clone());
    assert_eq!(without.reason.detail, None);

    item.wait.cause = Some(
        "S3: Network: S3 request failed: dispatch failure: io error: connection refused".into(),
    );
    let with = BridgePendingRecord::from_core(item);
    assert_eq!(with.subject.key, "core.pending.subject.connection");
    assert_eq!(with.reason.key, "core.pending.reason.storage");
    assert_eq!(
        with.reason.detail.as_deref(),
        Some("S3: Network: S3 request failed: dispatch failure: io error: connection refused")
    );
}

/// A record crosses with its labels as catalog keys: a release file's subject
/// with its file name and album title as the message's arguments, and the
/// action's key beside the action.
#[test]
fn a_pending_record_crosses_with_its_labels_as_catalog_messages() {
    let record = BridgePendingRecord::from_core(bae_sync_status::PendingItem {
        subject: bae_sync_status::PendingSubjectName::ReleaseFile {
            file_name: "01 Track Title.flac".into(),
            album_title: "Album Title".into(),
        },
        wait: bae_sync_status::PendingWait {
            reason: coven::PendingReason::Paused,
            cause: None,
            action: bae_sync_status::PendingAction::RestoreFile,
        },
        record_subject: coven::PendingSubject::Connection,
    });
    assert_eq!(
        record.subject,
        BridgeMessage {
            key: "core.pending.subject.release_file".into(),
            args: vec![
                BridgeMessageArg { name: "file".into(), value: "01 Track Title.flac".into() },
                BridgeMessageArg { name: "album".into(), value: "Album Title".into() },
            ],
        }
    );
    assert_eq!(record.reason.key, "core.pending.reason.paused");
    assert_eq!(record.action_key.as_deref(), Some("core.pending.action.restore_file"));

    let waiting = BridgePendingRecord::from_core(bae_sync_status::PendingItem {
        subject: bae_sync_status::PendingSubjectName::Snapshot,
        wait: bae_sync_status::PendingWait {
            reason: coven::PendingReason::Paused,
            cause: None,
            action: bae_sync_status::PendingAction::Wait,
        },
        record_subject: coven::PendingSubject::Connection,
    });
    assert_eq!(
        waiting.subject,
        BridgeMessage { key: "core.pending.subject.snapshot".into(), args: Vec::new() }
    );
    assert_eq!(waiting.action_key, None);
}

/// A preview target crosses with its file's name, which the preview shows.
#[test]
fn a_preview_target_crosses_with_its_files_name() {
    let target = bae_track_audio::PreviewTarget::whole_file("/music/Album/01 Track.flac".into());
    assert_eq!(BridgePreviewTarget::from_core(target).name, "01 Track.flac");
}

/// A file whose name isn't UTF-8 crosses to the app and back as its own
/// bytes, so auditioning it opens that file, and shows as its escape.
#[cfg(target_os = "linux")]
#[test]
fn a_preview_target_whose_name_is_not_utf8_comes_back_as_its_path() {
    use std::os::unix::ffi::OsStrExt;

    let path =
        std::path::Path::new("/music/Album").join(std::ffi::OsStr::from_bytes(b"Caf\xE9.flac"));
    let target = bae_track_audio::PreviewTarget::sample_range(path.clone(), 0, Some(44_100));
    let crossed = BridgePreviewTarget::from_core(target);
    assert_eq!(crossed.path, b"/music/Album/Caf\xE9.flac");
    assert_eq!(crossed.name, "Caf\\xE9.flac");
    let back = crossed.into_core().expect("a path core gave the app comes back");
    assert_eq!(back.into_sample_range(), (path, 0, Some(44_100)));
}

/// A watched folder and a candidate key whose names aren't UTF-8 show as
/// their escapes, while the bytes that name them back to core stay the
/// folder's own.
#[cfg(all(unix, desktop))]
#[test]
fn a_folder_whose_name_is_not_utf8_shows_as_its_escape() {
    use std::os::unix::ffi::OsStrExt;

    let path = std::path::Path::new("/music").join(std::ffi::OsStr::from_bytes(b"Caf\xE9"));
    let folder = BridgeWatchedFolder::from_core(bae_import::WatchedFolder {
        path: path.clone(),
        name: "Caf\\xE9".into(),
    });
    assert_eq!(folder.path, b"/music/Caf\xE9");
    assert_eq!(folder.path_text, "/music/Caf\\xE9");

    let key = bae_import::CandidateKey::folder(&path);
    assert_eq!(bridge_candidate_key_text(Vec::<u8>::from(key)), "/music/Caf\\xE9");
    let grouping = bae_import::CandidateKey::grouping("g1");
    assert_eq!(bridge_candidate_key_text(Vec::<u8>::from(grouping)), "grouping:g1");
}

/// A new installation's open crosses as the opening state with its counted
/// step, and loading the store as the loading state with each step, a
/// snapshot's step naming whom it holds data for.
#[test]
fn a_bootstrap_open_and_each_load_step_cross_with_their_progress() {
    use crate::types::{
        BridgeBootstrapStatus, BridgeBootstrapStep, BridgeLoadStep, BridgeOpenStep,
    };
    let (opening, step) =
        BridgeBootstrapStatus::from_core(coven::BootstrapStatus::Opening(coven::Progress {
            step: coven::OpenStep::Migrating,
            done: 1,
            total: 4,
        }));
    assert_eq!(opening, BridgeBootstrapStatus::Opening);
    assert!(matches!(
        step,
        Some(BridgeBootstrapStep::Opening { progress })
            if progress.step == BridgeOpenStep::Migrating && progress.done == 1 && progress.total == 4
    ));
    let load = |step| match BridgeBootstrapStatus::from_core(coven::BootstrapStatus::Loading(
        coven::Progress { step, done: 3, total: 9 },
    )) {
        (BridgeBootstrapStatus::Loading, Some(BridgeBootstrapStep::Loading { progress })) => {
            assert_eq!((progress.done, progress.total), (3, 9));
            progress.step
        }
        _ => panic!("loading crosses as loading, with its step"),
    };
    assert_eq!(load(coven::LoadStep::FindingKeys), BridgeLoadStep::FindingKeys);
    assert_eq!(
        load(coven::LoadStep::Streaming(coven::Audience::Store)),
        BridgeLoadStep::StreamingLibrary
    );
    assert_eq!(
        load(coven::LoadStep::Streaming(coven::Audience::Circle(coven::CircleId(
            coven::IdSource::new_id(&coven::UuidIds)
        )))),
        BridgeLoadStep::StreamingCircle
    );
    assert_eq!(load(coven::LoadStep::Installing), BridgeLoadStep::Installing);
    assert_eq!(load(coven::LoadStep::InstallingSlots), BridgeLoadStep::InstallingSlots);
    assert_eq!(load(coven::LoadStep::Reading), BridgeLoadStep::Reading);
    assert_eq!(load(coven::LoadStep::Applying), BridgeLoadStep::Applying);
}

/// An error shows its detail under its line only when core says the detail
/// adds to it: a category whose line names the whole fault shows the line
/// alone, one whose detail carries a cause keeps it, and an error with no
/// diagnostic has no detail to show.
#[test]
fn an_error_shows_its_detail_only_when_it_adds_to_the_line() {
    let repeated = BridgeError::diagnostic(
        BridgeErrorCategory::FilesAlreadyImported,
        "this device already imported these files, as release r1",
    );
    assert_eq!(bridge_error_shown_detail(&repeated), None);
    let caused = BridgeError::diagnostic(BridgeErrorCategory::Network, "dns error: no such host");
    assert_eq!(bridge_error_shown_detail(&caused).as_deref(), Some("dns error: no such host"));

    // AirPlay's categories follow core's rule too.
    let receiver = BridgeError::diagnostic(
        BridgeErrorCategory::AirPlayUnreadableSettings,
        "features value `0xZZ` is not hex",
    );
    assert_eq!(
        bridge_error_shown_detail(&receiver).as_deref(),
        Some("features value `0xZZ` is not hex")
    );

    assert_eq!(bridge_error_shown_detail(&BridgeError::Cancelled), None);
    let missing =
        BridgeError::NotFound { entity: BridgeEntityKind::Library, id: "library-1".into() };
    assert_eq!(bridge_error_shown_detail(&missing), None);
}

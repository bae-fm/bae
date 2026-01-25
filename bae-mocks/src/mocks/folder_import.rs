//! FolderImportView mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::stores::import::{
    CandidateState, ConfirmPhase, ConfirmingState, IdentifyingState, ImportState, ManualSearchState,
};
use bae_ui::{
    AudioContentInfo, CategorizedFileInfo, CueFlacPairInfo, DetectedCandidate,
    DetectedCandidateStatus, FileInfo, FolderImportView, FolderMetadata, IdentifyMode,
    ImportSource, ImportStep, ImportView, MatchCandidate, MatchSourceType, ReleaseSidebarView,
    SearchSource, SearchTab, SelectedCover, StorageLocation, StorageProfile,
};
use dioxus::prelude::*;
use std::collections::HashMap;

/// Available cover images for mock artwork
const MOCK_COVERS: &[&str] = &[
    "/covers/the-midnight-signal_neon-frequencies.png",
    "/covers/glass-harbor_pacific-standard.png",
    "/covers/glass-harbor_landlocked.png",
    "/covers/velvet-mathematics_set-theory.png",
    "/covers/velvet-mathematics_proof-by-induction.png",
    "/covers/apartment-garden_grow-light.png",
];

/// Helper to create mock FileInfo with a path derived from name
fn mock_file(name: &str, size: u64, format: &str) -> FileInfo {
    FileInfo {
        name: name.to_string(),
        path: format!("/mock/{}", name),
        size,
        format: format.to_string(),
        display_url: String::new(),
    }
}

/// Helper to create mock artwork FileInfo with a display URL
fn mock_artwork(name: &str, size: u64, format: &str, cover_index: usize) -> FileInfo {
    FileInfo {
        name: name.to_string(),
        path: format!("/mock/{}", name),
        size,
        format: format.to_string(),
        display_url: MOCK_COVERS[cover_index % MOCK_COVERS.len()].to_string(),
    }
}

/// Helper to create mock CueFlacPairInfo
fn mock_cue_flac(
    cue_name: &str,
    flac_name: &str,
    track_count: usize,
    total_size: u64,
) -> CueFlacPairInfo {
    CueFlacPairInfo {
        cue_name: cue_name.to_string(),
        cue_path: format!("/mock/{}", cue_name),
        flac_name: flac_name.to_string(),
        total_size,
        track_count,
    }
}

#[component]
pub fn FolderImportMock(initial_state: Option<String>) -> Element {
    // Build control registry with URL sync
    let registry = ControlRegistryBuilder::new()
        .bool_control("has_releases", "Has Releases", true)
        .doc("When false, shows folder picker. When true, shows two-pane layout.")
        .enum_control(
            "step",
            "Step",
            "Identify",
            vec![("Identify", "Identify"), ("Confirm", "Confirm")],
        )
        .visible_when("has_releases", "true")
        .enum_control(
            "identify_mode",
            "Identify Mode",
            "ManualSearch",
            vec![
                ("Created", "Created"),
                ("DiscIdLookup", "DiscID Lookup"),
                ("MultipleExactMatches", "Multiple Exact Matches"),
                ("ManualSearch", "Manual Search"),
            ],
        )
        .visible_when("has_releases", "true")
        .visible_when("step", "Identify")
        .bool_control("searching", "Searching", false)
        .doc("Shows spinner during manual search")
        .visible_when("step", "Identify")
        .visible_when("identify_mode", "ManualSearch")
        .bool_control("results", "Has Results", false)
        .doc("Shows search results in manual search")
        .visible_when("step", "Identify")
        .visible_when("identify_mode", "ManualSearch")
        .bool_control("discid_error", "DiscID Error", false)
        .doc("Shows DiscID lookup error banner")
        .visible_when("step", "Identify")
        .visible_when("identify_mode", "ManualSearch")
        .bool_control("retrying", "Retrying DiscID", false)
        .doc("Shows retry spinner on error banner")
        .visible_when("step", "Identify")
        .visible_when("identify_mode", "ManualSearch")
        .visible_when("discid_error", "true")
        .bool_control("importing", "Importing", false)
        .doc("Shows progress during import")
        .visible_when("step", "Confirm")
        .bool_control("error", "Error", false)
        .doc("Shows error banner")
        .visible_when("step", "Confirm")
        .with_presets(vec![
            Preset::new("Select Folder").set_bool("has_releases", false),
            Preset::new("DiscID Lookup")
                .set_bool("has_releases", true)
                .set_string("step", "Identify")
                .set_string("identify_mode", "DiscIdLookup"),
            Preset::new("Multiple Exact Matches")
                .set_bool("has_releases", true)
                .set_string("step", "Identify")
                .set_string("identify_mode", "MultipleExactMatches"),
            Preset::new("Manual Search")
                .set_bool("has_releases", true)
                .set_string("step", "Identify")
                .set_string("identify_mode", "ManualSearch"),
            Preset::new("Searching")
                .set_bool("has_releases", true)
                .set_string("step", "Identify")
                .set_string("identify_mode", "ManualSearch")
                .set_bool("searching", true),
            Preset::new("With Results")
                .set_bool("has_releases", true)
                .set_string("step", "Identify")
                .set_string("identify_mode", "ManualSearch")
                .set_bool("results", true),
            Preset::new("Confirm")
                .set_bool("has_releases", true)
                .set_string("step", "Confirm"),
            Preset::new("Importing")
                .set_bool("has_releases", true)
                .set_string("step", "Confirm")
                .set_bool("importing", true),
            Preset::new("Error")
                .set_bool("has_releases", true)
                .set_string("step", "Confirm")
                .set_bool("error", true),
        ])
        .build(initial_state);

    // Set up URL sync
    registry.use_url_sync_folder_import();

    // Local state (not persisted to URL)
    let mut selected_candidate_index = use_signal(|| Some(0usize)); // Default to first release
    let mut selected_match_index = use_signal(|| None::<usize>);
    let mut search_source = use_signal(|| SearchSource::MusicBrainz);
    let mut search_tab = use_signal(|| SearchTab::General);
    let mut search_artist = use_signal(|| "The Midnight Signal".to_string());
    let mut search_album = use_signal(|| "Neon Frequencies".to_string());
    let mut search_year = use_signal(String::new);
    let mut search_label = use_signal(String::new);
    let mut search_catalog_number = use_signal(String::new);
    let mut search_barcode = use_signal(String::new);
    let mut selected_cover = use_signal(|| None::<SelectedCover>);
    let mut selected_profile_id = use_signal(|| Some("profile-1".to_string()));

    // Parse step from registry
    let step = match registry.get_string("step").as_str() {
        "Identify" => ImportStep::Identify,
        "Confirm" => ImportStep::Confirm,
        _ => ImportStep::Identify,
    };

    // Parse identify mode
    let identify_mode = match registry.get_string("identify_mode").as_str() {
        "Created" => IdentifyMode::Created,
        "DiscIdLookup" => IdentifyMode::DiscIdLookup,
        "MultipleExactMatches" => IdentifyMode::MultipleExactMatches,
        "ManualSearch" => IdentifyMode::ManualSearch,
        _ => IdentifyMode::DiscIdLookup,
    };

    let is_retrying_discid_lookup = registry.get_bool("retrying");
    let is_searching = registry.get_bool("searching");
    let has_searched = registry.get_bool("results");
    let is_importing = registry.get_bool("importing");
    let show_error = registry.get_bool("error");
    let show_discid_error = registry.get_bool("discid_error");

    let has_releases = registry.get_bool("has_releases");

    // Define folder data - each folder has different file compositions
    let folder_data: Vec<(DetectedCandidate, CategorizedFileInfo)> = vec![
        // Folder 1: Track files with full artwork and docs
        (
            DetectedCandidate {
                name: "The Midnight Signal - Neon Frequencies (2023) [FLAC 24-96]".to_string(),
                path: "/Users/demo/Music/Imports/The Midnight Signal - Neon Frequencies (2023) [FLAC 24-96]"
                    .to_string(),
                status: DetectedCandidateStatus::Pending,
            },
            CategorizedFileInfo {
                audio: AudioContentInfo::TrackFiles(vec![
                    mock_file("01 - Broadcast.flac", 32_000_000, "FLAC"),
                    mock_file("02 - Static Dreams.flac", 28_500_000, "FLAC"),
                    mock_file("03 - Frequency Drift.flac", 31_200_000, "FLAC"),
                    mock_file("04 - Night Transmission.flac", 29_800_000, "FLAC"),
                    mock_file("05 - Signal Lost.flac", 27_600_000, "FLAC"),
                    mock_file("06 - Wavelength.flac", 33_100_000, "FLAC"),
                    mock_file("07 - Clear Channel.flac", 30_400_000, "FLAC"),
                    mock_file("08 - Transmission End.flac", 35_200_000, "FLAC"),
                ]),
                artwork: vec![
                    mock_artwork("cover.jpg", 2_500_000, "JPEG", 0),
                    mock_artwork("back.jpg", 2_100_000, "JPEG", 1),
                    mock_artwork("cd.jpg", 1_800_000, "JPEG", 2),
                ],
                documents: vec![
                    mock_file("rip.log", 4_500, "LOG"),
                    mock_file("info.txt", 1_200, "TXT"),
                ],
                other: vec![mock_file(".DS_Store", 6_148, "")],
            },
        ),
        // Folder 2: CUE/FLAC pair, minimal extras
        (
            DetectedCandidate {
                name: "Glass Harbor - 2022 - Pacific Standard".to_string(),
                path: "/Users/demo/Music/Imports/Glass Harbor - 2022 - Pacific Standard"
                    .to_string(),
                status: DetectedCandidateStatus::Pending,
            },
            CategorizedFileInfo {
                audio: AudioContentInfo::CueFlacPairs(vec![mock_cue_flac(
                    "Glass Harbor - Pacific Standard.cue",
                    "Glass Harbor - Pacific Standard.flac",
                    10,
                    380_000_000,
                )]),
                artwork: vec![mock_artwork("folder.jpg", 850_000, "JPEG", 1)],
                documents: vec![],
                other: vec![],
            },
        ),
        // Folder 3: Track files, lots of scans, torrent style
        (
            DetectedCandidate {
                name: "Velvet_Mathematics-Proof_by_Induction-2021-FLAC".to_string(),
                path: "/Users/demo/Music/Imports/Velvet_Mathematics-Proof_by_Induction-2021-FLAC"
                    .to_string(),
                status: DetectedCandidateStatus::Pending,
            },
            CategorizedFileInfo {
                audio: AudioContentInfo::TrackFiles(vec![
                    mock_file("01-velvet_mathematics-axiom.flac", 25_000_000, "FLAC"),
                    mock_file("02-velvet_mathematics-lemma.flac", 27_500_000, "FLAC"),
                    mock_file("03-velvet_mathematics-theorem.flac", 29_200_000, "FLAC"),
                    mock_file("04-velvet_mathematics-corollary.flac", 24_800_000, "FLAC"),
                    mock_file("05-velvet_mathematics-qed.flac", 31_600_000, "FLAC"),
                ]),
                artwork: vec![
                    mock_artwork("cover.jpg", 3_200_000, "JPEG", 3),
                    mock_artwork("back.jpg", 2_800_000, "JPEG", 4),
                    mock_artwork("booklet-01.png", 4_500_000, "PNG", 0),
                    mock_artwork("booklet-02.png", 4_200_000, "PNG", 1),
                    mock_artwork("booklet-03.png", 4_100_000, "PNG", 2),
                    mock_artwork("booklet-04.png", 3_900_000, "PNG", 3),
                    mock_artwork("cd.jpg", 1_900_000, "JPEG", 4),
                    mock_artwork("matrix.jpg", 1_200_000, "JPEG", 5),
                ],
                documents: vec![
                    mock_file("Velvet_Mathematics-Proof_by_Induction-2021-FLAC.nfo", 8_500, "NFO"),
                    mock_file("Velvet_Mathematics-Proof_by_Induction-2021-FLAC.m3u", 450, "M3U"),
                ],
                other: vec![],
            },
        ),
        // Folder 4: Simple rip, no docs, junk files
        (
            DetectedCandidate {
                name: "Apartment Garden - Grow Light".to_string(),
                path: "/Users/demo/Downloads/Apartment Garden - Grow Light".to_string(),
                status: DetectedCandidateStatus::Pending,
            },
            CategorizedFileInfo {
                audio: AudioContentInfo::TrackFiles(vec![
                    mock_file("01 Seedling.flac", 28_200_000, "FLAC"),
                    mock_file("02 Photosynthesis.flac", 27_800_000, "FLAC"),
                    mock_file("03 Root System.flac", 29_100_000, "FLAC"),
                    mock_file("04 Bloom.flac", 28_500_000, "FLAC"),
                ]),
                artwork: vec![mock_artwork("AlbumArt.jpg", 450_000, "JPEG", 5)],
                documents: vec![],
                other: vec![
                    mock_file("desktop.ini", 282, ""),
                    mock_file("Thumbs.db", 12_288, ""),
                ],
            },
        ),
        // Folder 5: Vinyl rip with extensive documentation
        (
            DetectedCandidate {
                name: "The Cold Equations - Fuel Weight (Vinyl Rip) [24-96]".to_string(),
                path: "/Users/demo/Music/Vinyl Rips/The Cold Equations - Fuel Weight (Vinyl Rip) [24-96]"
                    .to_string(),
                status: DetectedCandidateStatus::Pending,
            },
            CategorizedFileInfo {
                audio: AudioContentInfo::TrackFiles(vec![
                    mock_file("A1 - Launch Window.flac", 85_000_000, "FLAC"),
                    mock_file("A2 - Orbital Mechanics.flac", 92_000_000, "FLAC"),
                    mock_file("B1 - Escape Velocity.flac", 78_000_000, "FLAC"),
                    mock_file("B2 - Gravity Well.flac", 88_000_000, "FLAC"),
                ]),
                artwork: vec![
                    mock_artwork("front.png", 15_000_000, "PNG", 0),
                    mock_artwork("back.png", 14_200_000, "PNG", 1),
                    mock_artwork("label-a.png", 8_500_000, "PNG", 2),
                    mock_artwork("label-b.png", 8_300_000, "PNG", 3),
                    mock_artwork("inner-sleeve.png", 12_000_000, "PNG", 4),
                ],
                documents: vec![
                    mock_file("ripping-notes.txt", 2_800, "TXT"),
                    mock_file("vinyl-condition.txt", 1_500, "TXT"),
                    mock_file("dr-analysis.txt", 3_200, "TXT"),
                ],
                other: vec![],
            },
        ),
    ];

    let detected_candidates: Vec<DetectedCandidate> = if has_releases {
        folder_data.iter().map(|(r, _)| r.clone()).collect()
    } else {
        vec![]
    };

    // Get folder files for selected release
    let selected_idx = selected_candidate_index.read().unwrap_or(0);
    let folder_files = folder_data
        .get(selected_idx)
        .map(|(_, files)| files.clone())
        .unwrap_or_else(|| folder_data[0].1.clone());

    let folder_path = detected_candidates
        .get(selected_idx)
        .map(|r| r.path.clone())
        .unwrap_or_else(|| "/Users/demo/Music/Imports".to_string());

    let exact_match_candidates = vec![
        MatchCandidate {
            title: "Neon Frequencies".to_string(),
            artist: "The Midnight Signal".to_string(),
            year: Some("2023".to_string()),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
            format: Some("CD".to_string()),
            country: Some("US".to_string()),
            label: Some("Synthwave Records".to_string()),
            catalog_number: Some("SWR-001".to_string()),
            source_type: MatchSourceType::MusicBrainz,
            original_year: Some("2023".to_string()),
            musicbrainz_release_id: Some("mock-mb-release-001".to_string()),
            musicbrainz_release_group_id: Some("mock-mb-rg-001".to_string()),
            discogs_release_id: None,
            discogs_master_id: None,
        },
        MatchCandidate {
            title: "Neon Frequencies (Deluxe)".to_string(),
            artist: "The Midnight Signal".to_string(),
            year: Some("2023".to_string()),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
            format: Some("Digital".to_string()),
            country: Some("XW".to_string()),
            label: Some("Synthwave Records".to_string()),
            catalog_number: Some("SWR-001D".to_string()),
            source_type: MatchSourceType::MusicBrainz,
            original_year: Some("2023".to_string()),
            musicbrainz_release_id: Some("mock-mb-release-002".to_string()),
            musicbrainz_release_group_id: Some("mock-mb-rg-001".to_string()),
            discogs_release_id: None,
            discogs_master_id: None,
        },
    ];

    let manual_match_candidates = if has_searched {
        exact_match_candidates.clone()
    } else {
        vec![]
    };
    let confirmed_candidate = exact_match_candidates.first().cloned();

    // Get track count from current folder's audio
    let track_count = match &folder_files.audio {
        AudioContentInfo::TrackFiles(tracks) => tracks.len() as u32,
        AudioContentInfo::CueFlacPairs(pairs) => {
            pairs.first().map(|p| p.track_count as u32).unwrap_or(0)
        }
    };

    let detected_metadata = Some(FolderMetadata {
        artist: Some("The Midnight Signal".to_string()),
        album: Some("Neon Frequencies".to_string()),
        year: Some(2023),
        track_count: Some(track_count),
        discid: None,
        mb_discid: None,
        confidence: 0.85,
        folder_tokens: vec![
            "midnight".to_string(),
            "signal".to_string(),
            "neon".to_string(),
            "frequencies".to_string(),
        ],
    });

    let storage_profiles = use_signal(|| {
        vec![
            StorageProfile {
                id: "profile-1".to_string(),
                name: "Cloud Storage".to_string(),
                location: StorageLocation::Cloud,
                is_default: true,
                ..Default::default()
            },
            StorageProfile {
                id: "profile-2".to_string(),
                name: "Local Backup".to_string(),
                location: StorageLocation::Local,
                is_default: false,
                ..Default::default()
            },
        ]
    });

    let import_error = if show_error {
        Some("Failed to import: Network timeout".to_string())
    } else {
        None
    };
    let discid_lookup_error = if show_discid_error {
        Some("DiscID lookup failed: No matching release found".to_string())
    } else {
        None
    };

    // Build the ImportState
    let current_key = if has_releases {
        Some(folder_path.clone())
    } else {
        None
    };

    // Build search state
    let mock_search_state = ManualSearchState {
        search_source: search_source(),
        search_artist: search_artist(),
        search_album: search_album(),
        search_year: search_year(),
        search_label: search_label(),
        search_catalog_number: search_catalog_number(),
        search_barcode: search_barcode(),
        search_tab: search_tab(),
        has_searched,
        is_searching,
        search_results: manual_match_candidates.clone(),
        selected_result_index: selected_match_index(),
        error_message: None,
    };

    // Build candidate state based on step
    let candidate_state = match step {
        bae_ui::ImportStep::Identify => CandidateState::Identifying(IdentifyingState {
            files: folder_files.clone(),
            metadata: detected_metadata.clone().unwrap_or_default(),
            mode: identify_mode,
            auto_matches: exact_match_candidates.clone(),
            selected_match_index: selected_match_index(),
            search_state: mock_search_state,
            discid_lookup_error,
        }),
        bae_ui::ImportStep::Confirm => {
            let phase = if is_importing {
                ConfirmPhase::Importing
            } else if let Some(ref err) = import_error {
                ConfirmPhase::Failed(err.clone())
            } else {
                ConfirmPhase::Ready
            };
            CandidateState::Confirming(Box::new(ConfirmingState {
                files: folder_files.clone(),
                metadata: detected_metadata.clone().unwrap_or_default(),
                confirmed_candidate: confirmed_candidate
                    .clone()
                    .or_else(|| exact_match_candidates.first().cloned())
                    .unwrap_or_else(|| MatchCandidate {
                        artist: "Unknown Artist".to_string(),
                        title: "Unknown Album".to_string(),
                        year: None,
                        format: None,
                        label: None,
                        catalog_number: None,
                        country: None,
                        cover_url: None,
                        source_type: MatchSourceType::MusicBrainz,
                        original_year: None,
                        musicbrainz_release_id: None,
                        musicbrainz_release_group_id: None,
                        discogs_release_id: None,
                        discogs_master_id: None,
                    }),
                selected_cover: selected_cover(),
                selected_profile_id: selected_profile_id(),
                phase,
                auto_matches: exact_match_candidates.clone(),
                search_state: mock_search_state,
            }))
        }
    };

    // Build candidate_states HashMap
    let mut candidate_states = HashMap::new();
    if has_releases {
        candidate_states.insert(folder_path.clone(), candidate_state);
    }

    // Create store once, then update it when computed values change
    let mut import_state = use_store(ImportState::default);

    // Update store whenever registry-derived values change
    // (reading registry values in component body creates subscriptions, so this runs on each re-render)
    import_state.set(ImportState {
        detected_candidates: detected_candidates.clone(),
        current_candidate_key: current_key,
        candidate_states,
        loading_candidates: HashMap::new(),
        is_looking_up: is_retrying_discid_lookup,
        duplicate_album_id: None,
        import_error_message: import_error,
        folder_files: folder_files.clone(),
        is_scanning_candidates: false,
        discid_lookup_attempted: std::collections::HashSet::new(),
        selected_release_indices: Vec::new(),
        current_release_index: 0,
        selected_import_source: ImportSource::Folder,
        cd_toc_info: None,
    });

    let registry_for_search = registry.clone();

    let sidebar = rsx! {
        ReleaseSidebarView {
            state: import_state,
            on_select: move |idx| selected_candidate_index.set(Some(idx)),
            on_add_folder: |_| {},
            on_remove: |_| {},
            on_clear_all: |_| {},
        }
    };

    rsx! {
        MockPanel {
            current_mock: MockPage::FolderImport,
            registry,
            max_width: "full",
            ImportView {
                selected_source: ImportSource::Folder,
                on_source_select: |_| {},
                sidebar,
                FolderImportView {
                    state: import_state,
                    selected_text_file: None,
                    text_file_content: None,
                    storage_profiles,
                    on_folder_select_click: |_| {},
                    on_text_file_select: |_| {},
                    on_text_file_close: |_| {},
                    on_skip_detection: |_| {},
                    on_exact_match_select: move |idx| selected_match_index.set(Some(idx)),
                    on_search_source_change: move |src| search_source.set(src),
                    on_search_tab_change: move |tab| search_tab.set(tab),
                    on_artist_change: move |v| search_artist.set(v),
                    on_album_change: move |v| search_album.set(v),
                    on_year_change: move |v| search_year.set(v),
                    on_label_change: move |v| search_label.set(v),
                    on_catalog_number_change: move |v| search_catalog_number.set(v),
                    on_barcode_change: move |v| search_barcode.set(v),
                    on_manual_match_select: move |idx| selected_match_index.set(Some(idx)),
                    on_search: move |_| registry_for_search.set_bool("searching", true),
                    on_manual_confirm: |_| {},
                    on_retry_discid_lookup: |_| {},
                    on_select_remote_cover: move |url| {
                        selected_cover
                            .set(
                                Some(SelectedCover::Remote {
                                    url,
                                    source: "MusicBrainz".to_string(),
                                }),
                            )
                    },
                    on_select_local_cover: move |filename| { selected_cover.set(Some(SelectedCover::Local { filename })) },
                    on_storage_profile_change: move |id| selected_profile_id.set(id),
                    on_edit: |_| {},
                    on_confirm: |_| {},
                    on_configure_storage: |_| {},
                    on_view_duplicate: |_| {},
                }
            }
        }
    }
}

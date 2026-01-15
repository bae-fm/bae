//! FolderImportView mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{
    ArtworkFile, AudioContentInfo, CategorizedFileInfo, CueFlacPairInfo, DetectedRelease, FileInfo,
    FolderImportView, FolderMetadata, IdentifyMode, MatchCandidate, MatchSourceType, SearchSource,
    SearchTab, SelectSourceMode, SelectedCover, StorageProfileInfo, WizardStep,
};
use dioxus::prelude::*;
use std::collections::HashMap;

#[component]
pub fn FolderImportMock(initial_state: Option<String>) -> Element {
    // Build control registry with URL sync
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "step",
            "Step",
            "SelectSource",
            vec![
                ("SelectSource", "Select Source"),
                ("Identify", "Identify"),
                ("Confirm", "Confirm"),
            ],
        )
        .enum_control(
            "select_mode",
            "Select Mode",
            "FolderSelection",
            vec![
                ("FolderSelection", "Picking Folder"),
                ("ReleaseSelection", "Multi-Release Picker"),
            ],
        )
        .visible_when("step", "SelectSource")
        .enum_control(
            "identify_mode",
            "Identify Mode",
            "Detecting",
            vec![
                ("Detecting", "Detecting"),
                ("ExactLookup", "Exact Lookup"),
                ("ManualSearch", "Manual Search"),
            ],
        )
        .visible_when("step", "Identify")
        .bool_control("dragging", "Dragging", false)
        .visible_when("step", "SelectSource")
        .bool_control("loading", "Loading Exact Matches", false)
        .doc("Shows spinner during exact match lookup")
        .visible_when("step", "Identify")
        .visible_when("identify_mode", "ExactLookup")
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
        .enum_control(
            "audio_type",
            "Audio Type",
            "TrackFiles",
            vec![("TrackFiles", "Track Files"), ("CueFlac", "CUE/FLAC")],
        )
        .inline()
        .hidden_when("step", "SelectSource")
        .enum_control(
            "track_count",
            "Tracks",
            "8",
            vec![("3", "3"), ("8", "8"), ("15", "15")],
        )
        .inline()
        .hidden_when("step", "SelectSource")
        .enum_control(
            "image_count",
            "Images",
            "3",
            vec![("0", "0"), ("3", "3"), ("12", "12")],
        )
        .inline()
        .hidden_when("step", "SelectSource")
        .enum_control(
            "doc_count",
            "Docs",
            "2",
            vec![("0", "0"), ("2", "2"), ("4", "4")],
        )
        .inline()
        .hidden_when("step", "SelectSource")
        .enum_control(
            "other_count",
            "Other",
            "2",
            vec![("0", "0"), ("2", "2"), ("4", "4")],
        )
        .inline()
        .hidden_when("step", "SelectSource")
        .with_presets(vec![
            Preset::new("Select Folder"),
            Preset::new("Multi-Release")
                .set_string("step", "SelectSource")
                .set_string("select_mode", "ReleaseSelection"),
            Preset::new("Detecting")
                .set_string("step", "Identify")
                .set_string("identify_mode", "Detecting"),
            Preset::new("Exact Matches")
                .set_string("step", "Identify")
                .set_string("identify_mode", "ExactLookup"),
            Preset::new("Loading Matches")
                .set_string("step", "Identify")
                .set_string("identify_mode", "ExactLookup")
                .set_bool("loading", true),
            Preset::new("Manual Search")
                .set_string("step", "Identify")
                .set_string("identify_mode", "ManualSearch"),
            Preset::new("Searching")
                .set_string("step", "Identify")
                .set_string("identify_mode", "ManualSearch")
                .set_bool("searching", true),
            Preset::new("With Results")
                .set_string("step", "Identify")
                .set_string("identify_mode", "ManualSearch")
                .set_bool("results", true),
            Preset::new("Confirm").set_string("step", "Confirm"),
            Preset::new("Importing")
                .set_string("step", "Confirm")
                .set_bool("importing", true),
            Preset::new("Error")
                .set_string("step", "Confirm")
                .set_bool("error", true),
        ])
        .build(initial_state);

    // Set up URL sync
    registry.use_url_sync_folder_import();

    // Local state (not persisted to URL)
    let mut selected_release_indices = use_signal(Vec::<usize>::new);
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
        "SelectSource" => WizardStep::SelectSource,
        "Identify" => WizardStep::Identify,
        "Confirm" => WizardStep::Confirm,
        _ => WizardStep::SelectSource,
    };

    // Parse select source mode
    let select_source_mode = match registry.get_string("select_mode").as_str() {
        "FolderSelection" => SelectSourceMode::FolderSelection,
        "ReleaseSelection" => SelectSourceMode::ReleaseSelection,
        _ => SelectSourceMode::FolderSelection,
    };

    // Parse identify mode
    let identify_mode = match registry.get_string("identify_mode").as_str() {
        "Detecting" => IdentifyMode::Detecting,
        "ExactLookup" => IdentifyMode::ExactLookup,
        "ManualSearch" => IdentifyMode::ManualSearch,
        _ => IdentifyMode::Detecting,
    };

    let is_dragging = registry.get_bool("dragging");
    let is_loading_exact_matches = registry.get_bool("loading");
    let is_retrying_discid_lookup = registry.get_bool("retrying");
    let is_searching = registry.get_bool("searching");
    let has_searched = registry.get_bool("results");
    let is_importing = registry.get_bool("importing");
    let show_error = registry.get_bool("error");
    let show_discid_error = registry.get_bool("discid_error");

    // File content controls
    let audio_type = registry.get_string("audio_type");
    let track_count = registry.get_string("track_count").parse().unwrap_or(8);
    let image_count = registry.get_string("image_count").parse().unwrap_or(3);
    let doc_count = registry.get_string("doc_count").parse().unwrap_or(2);
    let other_count = registry.get_string("other_count").parse().unwrap_or(2);

    // Mock data
    let folder_path = "/Users/demo/Music/The Midnight Signal - Neon Frequencies (2023)".to_string();

    // Generate audio content based on type
    let track_names = [
        "Broadcast",
        "Static Dreams",
        "Frequency Drift",
        "Night Transmission",
        "Signal Lost",
        "Wavelength",
        "Interference",
        "Clear Channel",
        "Dead Air",
        "Transmission End",
        "Resonance",
        "Amplitude",
        "Oscillation",
        "Harmonic",
        "Feedback",
        "White Noise",
        "Pink Noise",
        "Brown Noise",
        "Silence",
        "Outro",
    ];

    let audio = if audio_type == "CueFlac" {
        AudioContentInfo::CueFlacPairs(vec![CueFlacPairInfo {
            cue_name: "album.cue".to_string(),
            flac_name: "album.flac".to_string(),
            track_count,
            total_size: 450_000_000,
        }])
    } else {
        AudioContentInfo::TrackFiles(
            (0..track_count)
                .map(|i| FileInfo {
                    name: format!(
                        "{:02} - {}.flac",
                        i + 1,
                        track_names.get(i).unwrap_or(&"Track")
                    ),
                    size: 28_000_000 + (i as u64 * 2_000_000),
                    format: "FLAC".to_string(),
                })
                .collect(),
        )
    };

    // Generate artwork files
    let image_names = [
        "cover.jpg",
        "back.jpg",
        "cd.jpg",
        "inlay.jpg",
        "booklet-01.jpg",
        "booklet-02.jpg",
        "booklet-03.jpg",
        "booklet-04.jpg",
        "obi.jpg",
        "matrix.jpg",
        "spine.jpg",
        "tray.jpg",
        "sticker.jpg",
    ];
    let artwork: Vec<FileInfo> = (0..image_count)
        .map(|i| FileInfo {
            name: image_names.get(i).unwrap_or(&"image.jpg").to_string(),
            size: 2_500_000 - (i as u64 * 200_000),
            format: "JPEG".to_string(),
        })
        .collect();

    // Generate document files
    let doc_names = [
        "rip.log",
        "info.txt",
        "accurip.txt",
        "cue.txt",
        "readme.nfo",
    ];
    let documents: Vec<FileInfo> = (0..doc_count)
        .map(|i| FileInfo {
            name: doc_names.get(i).unwrap_or(&"file.txt").to_string(),
            size: 4_500 - (i as u64 * 500),
            format: if i == 0 { "LOG" } else { "TXT" }.to_string(),
        })
        .collect();

    // Generate other files
    let other_names = [".DS_Store", "Thumbs.db", "desktop.ini", "playlist.m3u"];
    let other: Vec<FileInfo> = (0..other_count)
        .map(|i| FileInfo {
            name: other_names.get(i).unwrap_or(&"file.dat").to_string(),
            size: 1_000 + (i as u64 * 500),
            format: "".to_string(),
        })
        .collect();

    let folder_files = CategorizedFileInfo {
        audio,
        artwork,
        documents,
        other,
    };

    let detected_releases = vec![
        DetectedRelease {
            name: "CD1".to_string(),
            path: "/Users/demo/Music/Album/CD1".to_string(),
        },
        DetectedRelease {
            name: "CD2".to_string(),
            path: "/Users/demo/Music/Album/CD2".to_string(),
        },
    ];

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
        },
    ];

    let manual_match_candidates = if has_searched {
        exact_match_candidates.clone()
    } else {
        vec![]
    };
    let confirmed_candidate = exact_match_candidates.first().cloned();

    let detected_metadata = Some(FolderMetadata {
        artist: Some("The Midnight Signal".to_string()),
        album: Some("Neon Frequencies".to_string()),
        year: Some(2023),
        track_count: Some(track_count as u32),
        discid: None,
        confidence: 0.85,
        folder_tokens: vec![
            "midnight".to_string(),
            "signal".to_string(),
            "neon".to_string(),
            "frequencies".to_string(),
        ],
    });

    // Generate artwork files for selection (with display URLs)
    let cover_urls = [
        "/covers/the-midnight-signal_neon-frequencies.png",
        "/covers/velvet-mathematics_proof-by-induction.png",
        "/covers/glass-harbor_pacific-standard.png",
        "/covers/the-borrowed-time_interest.png",
        "/covers/stairwell-echo_floors-1-12.png",
        "/covers/newspaper-weather_tomorrows-forecast.png",
        "/covers/parking-structure_level-4.png",
    ];
    let artwork_files: Vec<ArtworkFile> = (0..image_count)
        .map(|i| ArtworkFile {
            name: image_names.get(i).unwrap_or(&"image.jpg").to_string(),
            display_url: cover_urls
                .get(i % cover_urls.len())
                .unwrap_or(&cover_urls[0])
                .to_string(),
        })
        .collect();

    let storage_profiles = vec![
        StorageProfileInfo {
            id: "profile-1".to_string(),
            name: "Cloud Storage".to_string(),
            is_default: true,
        },
        StorageProfileInfo {
            id: "profile-2".to_string(),
            name: "Local Backup".to_string(),
            is_default: false,
        },
    ];

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

    let registry_for_clear = registry.clone();

    rsx! {
        MockPanel {
            current_mock: MockPage::FolderImport,
            registry,
            max_width: "4xl",
            FolderImportView {
                step,
                select_source_mode,
                identify_mode,
                folder_path: folder_path.clone(),
                folder_files: folder_files.clone(),
                image_data: artwork_files.iter().map(|f| (f.name.clone(), f.display_url.clone())).collect(),
                text_file_contents: HashMap::new(),
                is_dragging,
                on_folder_select_click: |_| {},
                detected_releases: detected_releases.clone(),
                selected_release_indices: selected_release_indices(),
                on_release_selection_change: move |indices| selected_release_indices.set(indices),
                on_releases_import: |_| {},
                on_skip_detection: |_| {},
                is_loading_exact_matches,
                exact_match_candidates: exact_match_candidates.clone(),
                selected_match_index: selected_match_index(),
                on_exact_match_select: move |idx| selected_match_index.set(Some(idx)),
                detected_metadata: detected_metadata.clone(),
                search_source: search_source(),
                on_search_source_change: move |src| search_source.set(src),
                search_tab: search_tab(),
                on_search_tab_change: move |tab| search_tab.set(tab),
                search_artist: search_artist(),
                on_artist_change: move |v| search_artist.set(v),
                search_album: search_album(),
                on_album_change: move |v| search_album.set(v),
                search_year: search_year(),
                on_year_change: move |v| search_year.set(v),
                search_label: search_label(),
                on_label_change: move |v| search_label.set(v),
                search_catalog_number: search_catalog_number(),
                on_catalog_number_change: move |v| search_catalog_number.set(v),
                search_barcode: search_barcode(),
                on_barcode_change: move |v| search_barcode.set(v),
                is_searching,
                search_error: None,
                has_searched,
                manual_match_candidates,
                on_manual_match_select: move |idx| selected_match_index.set(Some(idx)),
                on_search: {
                    let registry = registry_for_clear.clone();
                    move |_| registry.set_bool("searching", true)
                },
                on_manual_confirm: |_| {},
                discid_lookup_error,
                is_retrying_discid_lookup,
                on_retry_discid_lookup: |_| {},
                confirmed_candidate,
                selected_cover: selected_cover(),
                display_cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
                artwork_files,
                storage_profiles,
                selected_profile_id: selected_profile_id(),
                is_importing,
                preparing_step_text: if is_importing { Some("Encoding tracks...".to_string()) } else { None },
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
                on_clear: move |_| registry_for_clear.set_string("step", "SelectSource".to_string()),
                import_error,
                duplicate_album_id: None,
                on_view_duplicate: |_| {},
            }
        }
    }
}

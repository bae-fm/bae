//! AlbumDetailView mock component

use super::framework::{ControlRegistryBuilder, MockPanel, Preset};
use bae_ui::{Album, AlbumDetailView, Artist, PlaybackDisplay, Release, Track, TrackImportState};
use dioxus::prelude::*;

#[component]
pub fn AlbumDetailMock(initial_state: Option<String>) -> Element {
    // Build control registry with URL sync
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "playback",
            "Playback",
            "Stopped",
            vec![
                ("Stopped", "Stopped"),
                ("Playing", "Playing"),
                ("Paused", "Paused"),
                ("Loading", "Loading"),
            ],
        )
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("Playing").set_string("playback", "Playing"),
            Preset::new("Paused").set_string("playback", "Paused"),
            Preset::new("Loading").set_string("playback", "Loading"),
        ])
        .build(initial_state);

    // Set up URL sync
    registry.use_url_sync_album_detail();

    // Local state
    let position_ms = use_signal(|| 45_000u64);
    let import_progress = use_signal(|| None::<u8>);
    let import_error = use_signal(|| None::<String>);
    let mut selected_release_id = use_signal(|| Some("release-1".to_string()));

    // Parse playback state from registry
    let playback_state = registry.get_string("playback");

    // Mock data
    let album = Album {
        id: "album-1".to_string(),
        title: "Neon Frequencies".to_string(),
        year: Some(2023),
        cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
        is_compilation: false,
    };

    let artists = vec![Artist {
        id: "artist-1".to_string(),
        name: "The Midnight Signal".to_string(),
    }];

    let releases = vec![
        Release {
            id: "release-1".to_string(),
            album_id: "album-1".to_string(),
            release_name: Some("CD Edition".to_string()),
            year: Some(2023),
            format: Some("CD".to_string()),
            label: Some("Synthwave Records".to_string()),
            catalog_number: Some("SWR-001".to_string()),
            country: Some("US".to_string()),
            barcode: Some("123456789012".to_string()),
            discogs_release_id: Some("12345678".to_string()),
            musicbrainz_release_id: Some("abc-123".to_string()),
        },
        Release {
            id: "release-2".to_string(),
            album_id: "album-1".to_string(),
            release_name: Some("Digital Deluxe".to_string()),
            year: Some(2023),
            format: Some("Digital".to_string()),
            label: Some("Synthwave Records".to_string()),
            catalog_number: Some("SWR-001D".to_string()),
            country: Some("XW".to_string()),
            barcode: None,
            discogs_release_id: None,
            musicbrainz_release_id: Some("def-456".to_string()),
        },
    ];

    let tracks: Vec<Signal<Track>> = [
        ("track-1", "Broadcast", 1, 198_000i64),
        ("track-2", "Static Dreams", 2, 245_000),
        ("track-3", "Frequency Drift", 3, 312_000),
        ("track-4", "Night Transmission", 4, 267_000),
        ("track-5", "Signal Lost", 5, 289_000),
        ("track-6", "Airwave", 6, 234_000),
        ("track-7", "Carrier Wave", 7, 301_000),
        ("track-8", "Sign Off", 8, 356_000),
    ]
    .iter()
    .map(|(id, title, num, duration)| {
        Signal::new(Track {
            id: id.to_string(),
            title: title.to_string(),
            track_number: Some(*num),
            disc_number: Some(1),
            duration_ms: Some(*duration),
            is_available: true,
            import_state: TrackImportState::Complete,
        })
    })
    .collect();

    let playback = match playback_state.as_str() {
        "Stopped" => PlaybackDisplay::Stopped,
        "Playing" => PlaybackDisplay::Playing {
            track_id: "track-1".to_string(),
            position_ms: position_ms(),
            duration_ms: 198_000,
        },
        "Paused" => PlaybackDisplay::Paused {
            track_id: "track-1".to_string(),
            position_ms: position_ms(),
            duration_ms: 198_000,
        },
        "Loading" => PlaybackDisplay::Loading {
            track_id: "track-1".to_string(),
        },
        _ => PlaybackDisplay::Stopped,
    };

    rsx! {
        MockPanel {
            title: "AlbumDetailView".to_string(),
            registry,
            max_width: "6xl",
            viewport_enabled: true,
            AlbumDetailView {
                album,
                releases,
                artists,
                tracks,
                selected_release_id: selected_release_id(),
                import_progress,
                import_error,
                playback,
                on_release_select: move |id| selected_release_id.set(Some(id)),
                on_album_deleted: |_| {},
                on_export_release: |_| {},
                on_delete_album: |_| {},
                on_delete_release: |_| {},
                on_track_play: |_| {},
                on_track_pause: |_| {},
                on_track_resume: |_| {},
                on_track_add_next: |_| {},
                on_track_add_to_queue: |_| {},
                on_track_export: |_| {},
                on_play_album: |_| {},
                on_add_album_to_queue: |_| {},
            }
        }
    }
}

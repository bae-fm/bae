//! LibraryView mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::stores::LibraryState;
use bae_ui::{Album, Artist, LibraryView};
use dioxus::prelude::*;
use std::collections::HashMap;

#[component]
pub fn LibraryMock(initial_state: Option<String>) -> Element {
    let mut cycle = use_signal(|| 0u32);

    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "state",
            "State",
            "Populated",
            vec![
                ("Loading", "Loading"),
                ("Error", "Error"),
                ("Empty", "Empty"),
                ("Populated", "Populated"),
            ],
        )
        .int_control("albums", "Albums count", 12, 0, None)
        .action("Remount", Callback::new(move |_| cycle += 1))
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("Loading").set_string("state", "Loading"),
            Preset::new("Error").set_string("state", "Error"),
            Preset::new("Empty").set_string("state", "Empty"),
        ])
        .build(initial_state);

    registry.use_url_sync_library();

    let ui_state = registry.get_string("state");
    let album_count = registry.get_int("albums") as usize;

    let (albums, artists_by_album) = if ui_state == "Populated" {
        mock_albums_with_artists(album_count)
    } else {
        (vec![], HashMap::new())
    };

    let loading = ui_state == "Loading";
    let error = if ui_state == "Error" {
        Some("Failed to load library: Database connection error".to_string())
    } else {
        None
    };

    // Create store once, then update when registry values change
    let mut state = use_store(LibraryState::default);

    state.set(LibraryState {
        albums,
        artists_by_album,
        loading,
        error,
    });

    let cycle_val = cycle();

    rsx! {
        MockPanel { current_mock: MockPage::Library, registry, max_width: "6xl",
            LibraryView {
                key: "{cycle_val}", // Change cycle to force complete remount
                state,
                on_album_click: |_| {},
                on_play_album: |_| {},
                on_add_album_to_queue: |_| {},
                on_empty_action: |_| {},
            }
        }
    }
}

/// Base album data that cycles for any count (title, artist, year, cover)
const ALBUM_DATA: &[(&str, &str, i32, &str)] = &[
    (
        "Neon Frequencies",
        "The Midnight Signal",
        2023,
        "/covers/the-midnight-signal_neon-frequencies.png",
    ),
    (
        "Pacific Standard",
        "Glass Harbor",
        2022,
        "/covers/glass-harbor_pacific-standard.png",
    ),
    (
        "Landlocked",
        "Glass Harbor",
        2021,
        "/covers/glass-harbor_landlocked.png",
    ),
    (
        "Set Theory",
        "Velvet Mathematics",
        2023,
        "/covers/velvet-mathematics_set-theory.png",
    ),
    (
        "Proof by Induction",
        "Velvet Mathematics",
        2022,
        "/covers/velvet-mathematics_proof-by-induction.png",
    ),
    (
        "Floors 1-12",
        "Stairwell Echo",
        2020,
        "/covers/stairwell-echo_floors-1-12.png",
    ),
    (
        "Level 4",
        "Parking Structure",
        2021,
        "/covers/parking-structure_level-4.png",
    ),
    (
        "Dial Tone",
        "The Last Payphone",
        2019,
        "/covers/the-last-payphone_dial-tone.png",
    ),
    (
        "Express",
        "The Checkout Lane",
        2023,
        "/covers/the-checkout-lane_express.png",
    ),
    (
        "Your Number",
        "The Waiting Room",
        2022,
        "/covers/the-waiting-room_your-number.png",
    ),
    (
        "Grow Light",
        "Apartment Garden",
        2021,
        "/covers/apartment-garden_grow-light.png",
    ),
    (
        "Window Sill",
        "Apartment Garden",
        2020,
        "/covers/apartment-garden_window-sill.png",
    ),
    (
        "Collated",
        "Copy Machine",
        2023,
        "/covers/copy-machine_collated.png",
    ),
    (
        "Back Page",
        "Newspaper Weather",
        2022,
        "/covers/newspaper-weather_back-page.png",
    ),
    (
        "Tomorrow's Forecast",
        "Newspaper Weather",
        2021,
        "/covers/newspaper-weather_tomorrows-forecast.png",
    ),
    (
        "Interest",
        "The Borrowed Time",
        2020,
        "/covers/the-borrowed-time_interest.png",
    ),
    (
        "Seconds",
        "The Borrowed Time",
        2019,
        "/covers/the-borrowed-time_seconds.png",
    ),
    (
        "Fuel Weight",
        "The Cold Equations",
        2023,
        "/covers/the-cold-equations_fuel-weight.png",
    ),
    (
        "Mission Control",
        "The Cold Equations",
        2022,
        "/covers/the-cold-equations_mission-control.png",
    ),
    (
        "Alphabetical",
        "The Filing Cabinets",
        2021,
        "/covers/the-filing-cabinets_alphabetical.png",
    ),
];

fn mock_albums_with_artists(count: usize) -> (Vec<Album>, HashMap<String, Vec<Artist>>) {
    let mut albums = Vec::with_capacity(count);
    let mut artists_by_album = HashMap::new();

    for i in 0..count {
        let idx = i % ALBUM_DATA.len();
        let (title, artist_name, year, cover) = ALBUM_DATA[idx];
        let id = (i + 1).to_string();

        albums.push(Album {
            id: id.clone(),
            title: title.to_string(),
            year: Some(year),
            cover_url: Some(cover.to_string()),
            is_compilation: false,
        });

        artists_by_album.insert(
            id,
            vec![Artist {
                id: format!("a{}", idx + 1),
                name: artist_name.to_string(),
            }],
        );
    }

    (albums, artists_by_album)
}

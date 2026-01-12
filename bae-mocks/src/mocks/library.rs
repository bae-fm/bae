//! LibraryView mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{Album, Artist, LibraryView};
use dioxus::prelude::*;
use std::collections::HashMap;

#[component]
pub fn LibraryMock(initial_state: Option<String>) -> Element {
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
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("Loading").set_string("state", "Loading"),
            Preset::new("Error").set_string("state", "Error"),
            Preset::new("Empty").set_string("state", "Empty"),
        ])
        .build(initial_state);

    registry.use_url_sync_library();

    let state = registry.get_string("state");

    let loading = state == "Loading";
    let error = if state == "Error" {
        Some("Failed to load library: Database connection error".to_string())
    } else {
        None
    };

    let albums = if state == "Populated" {
        mock_albums()
    } else {
        vec![]
    };

    let artists_by_album = if state == "Populated" {
        mock_artists_by_album()
    } else {
        HashMap::new()
    };

    rsx! {
        MockPanel {
            current_mock: MockPage::Library,
            registry,
            max_width: "6xl",
            LibraryView {
                albums,
                artists_by_album,
                loading,
                error,
                on_album_click: |_| {},
                on_play_album: |_| {},
                on_add_album_to_queue: |_| {},
                on_empty_action: Some(EventHandler::new(|_| {})),
            }
        }
    }
}

fn mock_albums() -> Vec<Album> {
    vec![
        Album {
            id: "1".to_string(),
            title: "Neon Frequencies".to_string(),
            year: Some(2023),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "2".to_string(),
            title: "Pacific Standard".to_string(),
            year: Some(2022),
            cover_url: Some("/covers/glass-harbor_pacific-standard.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "3".to_string(),
            title: "Landlocked".to_string(),
            year: Some(2021),
            cover_url: Some("/covers/glass-harbor_landlocked.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "4".to_string(),
            title: "Set Theory".to_string(),
            year: Some(2023),
            cover_url: Some("/covers/velvet-mathematics_set-theory.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "5".to_string(),
            title: "Proof by Induction".to_string(),
            year: Some(2022),
            cover_url: Some("/covers/velvet-mathematics_proof-by-induction.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "6".to_string(),
            title: "Floors 1-12".to_string(),
            year: Some(2020),
            cover_url: Some("/covers/stairwell-echo_floors-1-12.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "7".to_string(),
            title: "Level 4".to_string(),
            year: Some(2021),
            cover_url: Some("/covers/parking-structure_level-4.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "8".to_string(),
            title: "Dial Tone".to_string(),
            year: Some(2019),
            cover_url: Some("/covers/the-last-payphone_dial-tone.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "9".to_string(),
            title: "Express".to_string(),
            year: Some(2023),
            cover_url: Some("/covers/the-checkout-lane_express.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "10".to_string(),
            title: "Your Number".to_string(),
            year: Some(2022),
            cover_url: Some("/covers/the-waiting-room_your-number.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "11".to_string(),
            title: "Grow Light".to_string(),
            year: Some(2021),
            cover_url: Some("/covers/apartment-garden_grow-light.png".to_string()),
            is_compilation: false,
        },
        Album {
            id: "12".to_string(),
            title: "Window Sill".to_string(),
            year: Some(2020),
            cover_url: Some("/covers/apartment-garden_window-sill.png".to_string()),
            is_compilation: false,
        },
    ]
}

fn mock_artists_by_album() -> HashMap<String, Vec<Artist>> {
    let mut map = HashMap::new();
    map.insert(
        "1".to_string(),
        vec![Artist {
            id: "a1".to_string(),
            name: "The Midnight Signal".to_string(),
        }],
    );
    map.insert(
        "2".to_string(),
        vec![Artist {
            id: "a2".to_string(),
            name: "Glass Harbor".to_string(),
        }],
    );
    map.insert(
        "3".to_string(),
        vec![Artist {
            id: "a2".to_string(),
            name: "Glass Harbor".to_string(),
        }],
    );
    map.insert(
        "4".to_string(),
        vec![Artist {
            id: "a3".to_string(),
            name: "Velvet Mathematics".to_string(),
        }],
    );
    map.insert(
        "5".to_string(),
        vec![Artist {
            id: "a3".to_string(),
            name: "Velvet Mathematics".to_string(),
        }],
    );
    map.insert(
        "6".to_string(),
        vec![Artist {
            id: "a4".to_string(),
            name: "Stairwell Echo".to_string(),
        }],
    );
    map.insert(
        "7".to_string(),
        vec![Artist {
            id: "a5".to_string(),
            name: "Parking Structure".to_string(),
        }],
    );
    map.insert(
        "8".to_string(),
        vec![Artist {
            id: "a6".to_string(),
            name: "The Last Payphone".to_string(),
        }],
    );
    map.insert(
        "9".to_string(),
        vec![Artist {
            id: "a7".to_string(),
            name: "The Checkout Lane".to_string(),
        }],
    );
    map.insert(
        "10".to_string(),
        vec![Artist {
            id: "a8".to_string(),
            name: "The Waiting Room".to_string(),
        }],
    );
    map.insert(
        "11".to_string(),
        vec![Artist {
            id: "a9".to_string(),
            name: "Apartment Garden".to_string(),
        }],
    );
    map.insert(
        "12".to_string(),
        vec![Artist {
            id: "a9".to_string(),
            name: "Apartment Garden".to_string(),
        }],
    );
    map
}

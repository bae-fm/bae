//! TitleBarView mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{NavItem, SearchResult, TitleBarView, UpdateState};
use dioxus::prelude::*;

#[component]
pub fn TitleBarMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "active_nav",
            "Active Nav",
            "library",
            vec![
                ("library", "Library"),
                ("import", "Import"),
                ("settings", "Settings"),
                ("none", "None"),
            ],
        )
        .inline()
        .enum_control(
            "update_state",
            "Update State",
            "idle",
            vec![
                ("idle", "Idle"),
                ("downloading", "Downloading"),
                ("ready", "Ready"),
            ],
        )
        .inline()
        .bool_control("show_search_results", "Show Search Results", false)
        .int_control("search_results_count", "Search Results", 3, 0, Some(10))
        .visible_when("show_search_results", "true")
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("Update Ready").set_string("update_state", "ready"),
            Preset::new("Downloading").set_string("update_state", "downloading"),
            Preset::new("With Search")
                .set_bool("show_search_results", true)
                .set_int("search_results_count", 5),
        ])
        .build(initial_state);

    registry.use_url_sync_title_bar();

    let active_nav = registry.get_string("active_nav");
    let update_state_str = registry.get_string("update_state");
    let show_search_results = registry.get_bool("show_search_results");
    let search_results_count = registry.get_int("search_results_count") as usize;

    let nav_items = vec![
        NavItem {
            id: "library".to_string(),
            label: "Library".to_string(),
            is_active: active_nav == "library",
        },
        NavItem {
            id: "import".to_string(),
            label: "Import".to_string(),
            is_active: active_nav == "import",
        },
    ];

    let settings_active = active_nav == "settings";

    let search_results: Vec<SearchResult> = if show_search_results {
        mock_search_results()
            .into_iter()
            .take(search_results_count)
            .collect()
    } else {
        vec![]
    };

    let update_state = match update_state_str.as_str() {
        "downloading" => UpdateState::Downloading,
        "ready" => UpdateState::Ready,
        _ => UpdateState::Idle,
    };

    rsx! {
        MockPanel {
            current_mock: MockPage::TitleBar,
            registry,
            max_width: "6xl",
            TitleBarView {
                nav_items,
                on_nav_click: |_| {},
                search_value: if show_search_results { "glass".to_string() } else { String::new() },
                on_search_change: |_| {},
                search_results,
                on_search_result_click: |_| {},
                show_search_results,
                on_search_dismiss: |_| {},
                on_search_focus: |_| {},
                on_settings_click: |_| {},
                settings_active,
                update_state,
                on_update_click: Some(EventHandler::new(|_| {})),
                left_padding: 16,
                relative: true,
            }
        }
    }
}

fn mock_search_results() -> Vec<SearchResult> {
    vec![
        SearchResult {
            id: "1".to_string(),
            title: "Pacific Standard".to_string(),
            subtitle: "Glass Harbor • 2022".to_string(),
            cover_url: Some("/covers/glass-harbor_pacific-standard.png".to_string()),
        },
        SearchResult {
            id: "2".to_string(),
            title: "Landlocked".to_string(),
            subtitle: "Glass Harbor • 2021".to_string(),
            cover_url: Some("/covers/glass-harbor_landlocked.png".to_string()),
        },
        SearchResult {
            id: "3".to_string(),
            title: "Grow Light".to_string(),
            subtitle: "Apartment Garden • 2021".to_string(),
            cover_url: Some("/covers/apartment-garden_grow-light.png".to_string()),
        },
        SearchResult {
            id: "4".to_string(),
            title: "Set Theory".to_string(),
            subtitle: "Velvet Mathematics • 2023".to_string(),
            cover_url: Some("/covers/velvet-mathematics_set-theory.png".to_string()),
        },
        SearchResult {
            id: "5".to_string(),
            title: "Neon Frequencies".to_string(),
            subtitle: "The Midnight Signal • 2023".to_string(),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
        },
    ]
}

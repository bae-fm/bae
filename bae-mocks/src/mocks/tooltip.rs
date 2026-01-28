//! Tooltip mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::floating_ui::Placement;
use bae_ui::Tooltip;
use dioxus::prelude::*;

#[component]
pub fn TooltipMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "placement",
            "Placement",
            "top",
            vec![
                ("top", "Top"),
                ("bottom", "Bottom"),
                ("left", "Left"),
                ("right", "Right"),
            ],
        )
        .bool_control("nowrap", "No Wrap", true)
        .string_control("text", "Text", "This is a tooltip")
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("Bottom")
                .set_string("placement", "bottom"),
            Preset::new("Long Text")
                .set_bool("nowrap", false)
                .set_string("text", "This is a longer tooltip with wrapping enabled to show how multi-line tooltips look"),
        ])
        .build(initial_state);

    registry.use_url_sync_button();

    let placement_str = registry.get_string("placement");
    let nowrap = registry.get_bool("nowrap");
    let text = registry.get_string("text");

    let placement = match placement_str.as_str() {
        "bottom" => Placement::Bottom,
        "left" => Placement::Left,
        "right" => Placement::Right,
        _ => Placement::Top,
    };

    rsx! {
        MockPanel { current_mock: MockPage::Tooltip, registry,
            div { class: "p-8 bg-gray-900 min-h-full",
                h2 { class: "text-lg font-semibold text-white mb-6", "Tooltip Component" }

                // Interactive demo
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Interactive Demo" }
                    div { class: "flex items-center justify-center h-32",
                        Tooltip { text, placement, nowrap,
                            button { class: "px-4 py-2 bg-gray-700 text-white rounded hover:bg-gray-600",
                                "Hover me"
                            }
                        }
                    }
                }

                // All placements
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "All Placements" }
                    div { class: "flex flex-wrap items-center justify-center gap-8 py-12",
                        Tooltip {
                            text: "Top tooltip",
                            placement: Placement::Top,
                            nowrap: true,
                            button { class: "px-3 py-1.5 bg-gray-700 text-sm text-white rounded hover:bg-gray-600",
                                "Top"
                            }
                        }
                        Tooltip {
                            text: "Bottom tooltip",
                            placement: Placement::Bottom,
                            nowrap: true,
                            button { class: "px-3 py-1.5 bg-gray-700 text-sm text-white rounded hover:bg-gray-600",
                                "Bottom"
                            }
                        }
                        Tooltip {
                            text: "Left tooltip",
                            placement: Placement::Left,
                            nowrap: true,
                            button { class: "px-3 py-1.5 bg-gray-700 text-sm text-white rounded hover:bg-gray-600",
                                "Left"
                            }
                        }
                        Tooltip {
                            text: "Right tooltip",
                            placement: Placement::Right,
                            nowrap: true,
                            button { class: "px-3 py-1.5 bg-gray-700 text-sm text-white rounded hover:bg-gray-600",
                                "Right"
                            }
                        }
                    }
                }

                // Wrapping behavior
                div {
                    h3 { class: "text-sm text-gray-400 mb-3", "Wrapping Behavior" }
                    div { class: "flex flex-wrap items-center gap-8 py-4",
                        Tooltip {
                            text: "Short nowrap tooltip",
                            placement: Placement::Top,
                            nowrap: true,
                            button { class: "px-3 py-1.5 bg-gray-700 text-sm text-white rounded hover:bg-gray-600",
                                "No Wrap"
                            }
                        }
                        Tooltip {
                            text: "This is a longer tooltip that will wrap to multiple lines when the content exceeds the max width",
                            placement: Placement::Top,
                            nowrap: false,
                            button { class: "px-3 py-1.5 bg-gray-700 text-sm text-white rounded hover:bg-gray-600",
                                "Wrapping"
                            }
                        }
                    }
                }
            }
        }
    }
}

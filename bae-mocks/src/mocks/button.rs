//! Button mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;

#[component]
pub fn ButtonMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .enum_control(
            "variant",
            "Variant",
            "primary",
            vec![
                ("primary", "Primary"),
                ("secondary", "Secondary"),
                ("danger", "Danger"),
                ("ghost", "Ghost"),
            ],
        )
        .enum_control(
            "size",
            "Size",
            "medium",
            vec![("small", "Small"), ("medium", "Medium")],
        )
        .bool_control("disabled", "Disabled", false)
        .bool_control("loading", "Loading", false)
        .with_presets(vec![
            Preset::new("Default"),
            Preset::new("Primary Small")
                .set_string("variant", "primary")
                .set_string("size", "small"),
            Preset::new("Danger Disabled")
                .set_string("variant", "danger")
                .set_bool("disabled", true),
            Preset::new("Loading")
                .set_string("variant", "primary")
                .set_bool("loading", true),
        ])
        .build(initial_state);

    registry.use_url_sync_button();

    let variant_str = registry.get_string("variant");
    let size_str = registry.get_string("size");
    let disabled = registry.get_bool("disabled");
    let loading = registry.get_bool("loading");

    let variant = match variant_str.as_str() {
        "secondary" => ButtonVariant::Secondary,
        "danger" => ButtonVariant::Danger,
        "ghost" => ButtonVariant::Ghost,
        _ => ButtonVariant::Primary,
    };

    let size = match size_str.as_str() {
        "small" => ButtonSize::Small,
        _ => ButtonSize::Medium,
    };

    let label = if loading {
        "Loading..."
    } else {
        match variant {
            ButtonVariant::Primary => "Save Changes",
            ButtonVariant::Secondary => "Cancel",
            ButtonVariant::Danger => "Delete",
            ButtonVariant::Ghost => "Learn More",
        }
    };

    rsx! {
        MockPanel { current_mock: MockPage::Button, registry,
            div { class: "p-8 bg-gray-900 min-h-full",
                h2 { class: "text-lg font-semibold text-white mb-6", "Button Component" }

                // Single button demo
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Interactive Demo" }
                    div { class: "flex items-center gap-4",
                        Button {
                            variant,
                            size,
                            disabled,
                            loading,
                            onclick: |_| {},
                            "{label}"
                        }
                    }
                }

                // All variants showcase
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "All Variants" }
                    div { class: "flex flex-wrap items-center gap-3",
                        Button { variant: ButtonVariant::Primary, onclick: |_| {}, "Primary" }
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: |_| {},
                            "Secondary"
                        }
                        Button { variant: ButtonVariant::Danger, onclick: |_| {}, "Danger" }
                        Button { variant: ButtonVariant::Ghost, onclick: |_| {}, "Ghost" }
                    }
                }

                // Size comparison
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Sizes" }
                    div { class: "flex flex-wrap items-center gap-3",
                        Button { size: ButtonSize::Small, onclick: |_| {}, "Small" }
                        Button { size: ButtonSize::Medium, onclick: |_| {}, "Medium" }
                    }
                }

                // Disabled states
                div { class: "mb-8",
                    h3 { class: "text-sm text-gray-400 mb-3", "Disabled States" }
                    div { class: "flex flex-wrap items-center gap-3",
                        Button {
                            variant: ButtonVariant::Primary,
                            disabled: true,
                            onclick: |_| {},
                            "Primary"
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            disabled: true,
                            onclick: |_| {},
                            "Secondary"
                        }
                        Button {
                            variant: ButtonVariant::Danger,
                            disabled: true,
                            onclick: |_| {},
                            "Danger"
                        }
                    }
                }

                // Common button patterns
                div {
                    h3 { class: "text-sm text-gray-400 mb-3", "Common Patterns" }
                    div { class: "space-y-4",
                        // Save/Cancel pair
                        div { class: "flex gap-3",
                            Button {
                                variant: ButtonVariant::Primary,
                                onclick: |_| {},
                                "Save"
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: |_| {},
                                "Cancel"
                            }
                        }
                        // Confirm delete dialog
                        div { class: "flex gap-3",
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: |_| {},
                                "Cancel"
                            }
                            Button {
                                variant: ButtonVariant::Danger,
                                onclick: |_| {},
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}

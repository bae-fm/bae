//! Auto-generated control panel UI

use super::registry::ControlRegistry;
use super::viewport::{MockViewport, DEFAULT_BREAKPOINTS};
use crate::Route;
use dioxus::prelude::*;

const COLLAPSED_KEY: &str = "mock_panel_collapsed";
const VIEWPORT_KEY: &str = "mock_panel_viewport";

fn get_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

fn get_stored_collapsed() -> bool {
    get_storage()
        .and_then(|s| s.get_item(COLLAPSED_KEY).ok().flatten())
        .is_some_and(|v| v == "true")
}

fn set_stored_collapsed(collapsed: bool) {
    if let Some(storage) = get_storage() {
        let _ = storage.set_item(COLLAPSED_KEY, if collapsed { "true" } else { "false" });
    }
}

fn get_stored_viewport() -> u32 {
    get_storage()
        .and_then(|s| s.get_item(VIEWPORT_KEY).ok().flatten())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn set_stored_viewport(width: u32) {
    if let Some(storage) = get_storage() {
        let _ = storage.set_item(VIEWPORT_KEY, &width.to_string());
    }
}

/// Main mock panel component that renders controls, presets, and viewport
#[component]
pub fn MockPanel(
    title: String,
    registry: ControlRegistry,
    #[props(default = "4xl")] max_width: &'static str,
    children: Element,
) -> Element {
    let viewport_width = use_signal(get_stored_viewport);
    let mut collapsed = use_signal(get_stored_collapsed);

    let max_w_class = match max_width {
        "4xl" => "max-w-4xl",
        "6xl" => "max-w-6xl",
        _ => max_width,
    };

    let header_mb = if collapsed() { "" } else { "mb-3" };

    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white",
            // Controls panel
            div { class: "sticky top-0 z-50 bg-gray-800 border-b border-gray-700 p-4",
                div { class: "{max_w_class} mx-auto",
                    // Header row with title and viewport
                    div { class: "flex items-center gap-3 {header_mb}",
                        Link {
                            to: Route::MockIndex {},
                            class: "text-gray-400 hover:text-white",
                            "←"
                        }
                        h1 { class: "text-lg font-semibold text-white", "{title}" }
                        div { class: "ml-auto flex items-center gap-3",
                            ViewportDropdown { viewport_width }
                            button {
                                class: "text-gray-400 hover:text-white px-2",
                                onclick: move |_| {
                                    let new_val = !collapsed();
                                    set_stored_collapsed(new_val);
                                    collapsed.set(new_val);
                                },
                                if collapsed() { "▼" } else { "▲" }
                            }
                        }
                    }

                    if !collapsed() {
                        // Presets row
                        if !registry.presets.is_empty() {
                            PresetBar { registry: registry.clone() }
                        }

                        // Controls row
                        ControlsRow { registry: registry.clone() }
                    }
                }
            }

            // Content area
            div { class: "{max_w_class} mx-auto p-6",
                MockViewport { width: viewport_width(), {children} }
            }
        }
    }
}

/// Preset buttons bar
#[component]
fn PresetBar(registry: ControlRegistry) -> Element {
    rsx! {
        div { class: "flex flex-wrap gap-2 mb-3",
            span { class: "text-xs text-gray-500 self-center mr-2", "Presets:" }
            for preset in &registry.presets {
                button {
                    class: "px-2 py-1 text-xs rounded bg-gray-700 text-gray-300 hover:bg-gray-600",
                    onclick: {
                        let preset = preset.clone();
                        let registry = registry.clone();
                        move |_| registry.apply_preset(&preset)
                    },
                    "{preset.name}"
                }
            }
        }
    }
}

/// Auto-generated controls row
#[component]
fn ControlsRow(registry: ControlRegistry) -> Element {
    // Separate enum controls (buttons) from bool controls (checkboxes)
    let enum_controls: Vec<_> = registry
        .controls
        .iter()
        .filter(|c| c.enum_options.is_some())
        .collect();
    let bool_controls: Vec<_> = registry
        .controls
        .iter()
        .filter(|c| c.enum_options.is_none())
        .collect();

    rsx! {
        // Enum controls as button groups
        for control in enum_controls {
            div { class: "flex flex-wrap gap-2 mb-3",
                if let Some(options) = &control.enum_options {
                    for (value , label) in options {
                        EnumButton {
                            registry: registry.clone(),
                            control_key: control.key,
                            value,
                            label,
                            doc: control.doc,
                        }
                    }
                }
            }
        }

        // Bool controls
        if !bool_controls.is_empty() {
            div { class: "flex flex-wrap gap-4 text-sm",
                for control in bool_controls {
                    BoolCheckbox {
                        registry: registry.clone(),
                        control_key: control.key,
                        label: control.label,
                        doc: control.doc,
                    }
                }
            }
        }
    }
}

/// Viewport dropdown selector
#[component]
fn ViewportDropdown(mut viewport_width: Signal<u32>) -> Element {
    let current = viewport_width();

    rsx! {
        select {
            class: "bg-gray-700 text-gray-300 text-sm rounded px-2 py-1 border border-gray-600",
            value: current.to_string(),
            onchange: move |e| {
                if let Ok(w) = e.value().parse::<u32>() {
                    set_stored_viewport(w);
                    viewport_width.set(w);
                }
            },
            for bp in DEFAULT_BREAKPOINTS {
                option {
                    value: bp.width.to_string(),
                    selected: current == bp.width,
                    if bp.width > 0 { "{bp.name} ({bp.width}px)" } else { "{bp.name}" }
                }
            }
        }
    }
}

/// Individual enum button - reads signal reactively
#[component]
fn EnumButton(
    registry: ControlRegistry,
    control_key: &'static str,
    value: &'static str,
    label: &'static str,
    doc: Option<&'static str>,
) -> Element {
    // Reading inside component body creates reactive subscription
    let is_selected = registry.get_string(control_key) == value;

    rsx! {
        button {
            class: if is_selected { "px-3 py-1.5 text-sm rounded bg-blue-600 text-white" } else { "px-3 py-1.5 text-sm rounded bg-gray-700 text-gray-300 hover:bg-gray-600" },
            onclick: move |_| registry.set_string(control_key, value.to_string()),
            title: doc.unwrap_or(""),
            "{label}"
        }
    }
}

/// Individual bool checkbox - reads signal reactively
#[component]
fn BoolCheckbox(
    registry: ControlRegistry,
    control_key: &'static str,
    label: &'static str,
    doc: Option<&'static str>,
) -> Element {
    // Reading inside component body creates reactive subscription
    let current = registry.get_bool(control_key);

    rsx! {
        label {
            class: "flex items-center gap-2 text-gray-400",
            title: doc.unwrap_or(""),
            input {
                r#type: "checkbox",
                checked: current,
                onchange: move |e| registry.set_bool(control_key, e.checked()),
            }
            "{label}"
            if doc.is_some() {
                span { class: "text-gray-600", "ⓘ" }
            }
        }
    }
}

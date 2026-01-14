//! Auto-generated control panel UI

use super::registry::ControlRegistry;
use super::viewport::{MockViewport, DEFAULT_BREAKPOINTS};
use crate::storage;
use crate::ui::{
    Checkbox, Chevron, ChevronDirection, Dropdown, DropdownStyle, IconButton, ToggleButton,
};
use crate::Route;
use dioxus::prelude::*;

const COLLAPSED_KEY: &str = "mock_panel_collapsed";
const VIEWPORT_KEY: &str = "mock_panel_viewport";

/// All available mock pages - add new mocks here
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockPage {
    Library,
    AlbumDetail,
    FolderImport,
}

impl MockPage {
    /// All variants - update when adding new mocks
    pub const ALL: &[MockPage] = &[
        MockPage::Library,
        MockPage::AlbumDetail,
        MockPage::FolderImport,
    ];

    /// Display name shown in UI
    pub fn label(self) -> &'static str {
        match self {
            MockPage::Library => "LibraryView",
            MockPage::AlbumDetail => "AlbumDetailView",
            MockPage::FolderImport => "FolderImportView",
        }
    }

    /// URL key for serialization
    pub fn key(self) -> &'static str {
        match self {
            MockPage::Library => "library",
            MockPage::AlbumDetail => "album-detail",
            MockPage::FolderImport => "folder-import",
        }
    }

    /// Description shown in mock index
    pub fn description(self) -> &'static str {
        match self {
            MockPage::Library => "Album grid with loading/error/empty states",
            MockPage::AlbumDetail => "Album detail page with tracks and controls",
            MockPage::FolderImport => "Folder import workflow with all phases",
        }
    }

    /// Convert to Route
    pub fn to_route(self, state: Option<String>) -> Route {
        match self {
            MockPage::Library => Route::MockLibrary { state },
            MockPage::AlbumDetail => Route::MockAlbumDetail { state },
            MockPage::FolderImport => Route::MockFolderImport { state },
        }
    }

    /// Parse from key string
    pub fn from_key(key: &str) -> Option<MockPage> {
        MockPage::ALL.iter().find(|p| p.key() == key).copied()
    }
}

/// Main mock panel component that renders controls, presets, and viewport
#[component]
pub fn MockPanel(
    current_mock: MockPage,
    registry: ControlRegistry,
    #[props(default = "4xl")] max_width: &'static str,
    children: Element,
) -> Element {
    let viewport_width = use_signal(|| storage::get_parsed(VIEWPORT_KEY).unwrap_or(0));
    let mut collapsed = use_signal(|| storage::get_bool(COLLAPSED_KEY).unwrap_or(false));

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
                    // Header row with breadcrumb, presets, and viewport
                    div { class: "flex items-center {header_mb}",
                        nav { class: "flex items-center gap-2 text-sm",
                            Link {
                                to: Route::MockIndex {},
                                class: "text-gray-400 hover:text-white",
                                "Component mocks"
                            }
                            span { class: "text-gray-600", "/" }
                            MockDropdown { current_mock }
                        }
                        div { class: "ml-auto flex items-center gap-3",
                            if !registry.presets.is_empty() {
                                PresetDropdown { registry: registry.clone() }
                            }
                            ViewportDropdown { viewport_width }
                            IconButton {
                                onclick: move |_| {
                                    let new_val = !collapsed();
                                    storage::set_bool(COLLAPSED_KEY, new_val);
                                    collapsed.set(new_val);
                                },
                                Chevron { direction: if collapsed() { ChevronDirection::Down } else { ChevronDirection::Up } }
                            }
                        }
                    }

                    if !collapsed() {
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

/// Dropdown for switching between mocks
#[component]
fn MockDropdown(current_mock: MockPage) -> Element {
    let nav = use_navigator();

    rsx! {
        Dropdown {
            value: current_mock.key().to_string(),
            style: DropdownStyle::Transparent,
            onchange: move |value: String| {
                if let Some(page) = MockPage::from_key(&value) {
                    nav.push(page.to_route(None));
                }
            },
            for page in MockPage::ALL {
                option { value: page.key(), selected: *page == current_mock, "{page.label()}" }
            }
        }
    }
}

/// Preset dropdown - shows current preset name or "Custom"
#[component]
fn PresetDropdown(registry: ControlRegistry) -> Element {
    // Find which preset matches current state (if any)
    let active_preset = registry
        .presets
        .iter()
        .find(|p| p.matches(&registry))
        .map(|p| p.name)
        .unwrap_or("Custom");

    rsx! {
        Dropdown {
            value: active_preset.to_string(),
            onchange: {
                let registry = registry.clone();
                move |name: String| {
                    if let Some(preset) = registry.presets.iter().find(|p| p.name == name) {
                        registry.apply_preset(preset);
                    }
                }
            },
            for preset in &registry.presets {
                option {
                    value: preset.name,
                    selected: preset.name == active_preset,
                    "{preset.name}"
                }
            }
            if active_preset == "Custom" {
                option { value: "Custom", selected: true, disabled: true, "Custom" }
            }
        }
    }
}

/// Auto-generated controls row
#[component]
fn ControlsRow(registry: ControlRegistry) -> Element {
    use super::registry::ControlValue;

    // Separate controls by type, filtering by visibility
    let enum_controls: Vec<_> = registry
        .controls
        .iter()
        .filter(|c| c.enum_options.is_some() && c.is_visible(&registry))
        .collect();
    let int_controls: Vec<_> = registry
        .controls
        .iter()
        .filter(|c| c.int_range.is_some() && c.is_visible(&registry))
        .collect();
    let bool_controls: Vec<_> = registry
        .controls
        .iter()
        .filter(|c| matches!(c.default, ControlValue::Bool(_)) && c.is_visible(&registry))
        .collect();
    let string_controls: Vec<_> = registry
        .controls
        .iter()
        .filter(|c| {
            c.enum_options.is_none()
                && c.int_range.is_none()
                && matches!(c.default, ControlValue::String(_))
                && c.is_visible(&registry)
        })
        .collect();

    rsx! {
        // Enum controls as button groups
        for control in enum_controls {
            div { class: "flex flex-wrap gap-2 mb-3",
                span { class: "text-xs text-gray-500 self-center mr-2", "{control.label}:" }
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

        // Int controls
        if !int_controls.is_empty() {
            div { class: "flex flex-wrap gap-4 text-sm mb-3",
                for control in int_controls {
                    IntInput {
                        registry: registry.clone(),
                        control_key: control.key,
                        label: control.label,
                        min: control.int_range.map(|(min, _)| min).unwrap_or(0),
                        max: control.int_range.and_then(|(_, max)| max),
                    }
                }
            }
        }

        // String controls
        if !string_controls.is_empty() {
            div { class: "flex flex-wrap gap-4 text-sm mb-3",
                for control in string_controls {
                    StringInput {
                        registry: registry.clone(),
                        control_key: control.key,
                        label: control.label,
                    }
                }
            }
        }

        // Bool controls
        if !bool_controls.is_empty() {
            div { class: "flex flex-wrap gap-4 text-sm mb-3",
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

        // Action buttons
        if !registry.actions.is_empty() {
            div { class: "flex flex-wrap gap-2 text-sm",
                for action in &registry.actions {
                    ActionButton { label: action.label, callback: action.callback }
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
        Dropdown {
            value: current.to_string(),
            onchange: move |value: String| {
                if let Ok(w) = value.parse::<u32>() {
                    storage::set_display(VIEWPORT_KEY, w);
                    viewport_width.set(w);
                }
            },
            for bp in DEFAULT_BREAKPOINTS {
                option {
                    value: bp.width.to_string(),
                    selected: current == bp.width,
                    if bp.width > 0 {
                        "{bp.name} ({bp.width}px)"
                    } else {
                        "{bp.name}"
                    }
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
        ToggleButton {
            selected: is_selected,
            onclick: move |_| registry.set_string(control_key, value.to_string()),
            label,
            tooltip: doc,
        }
    }
}

/// Integer input control - reads signal reactively
#[component]
fn IntInput(
    registry: ControlRegistry,
    control_key: &'static str,
    label: &'static str,
    min: i32,
    max: Option<i32>,
) -> Element {
    let current = registry.get_int(control_key);

    rsx! {
        label { class: "flex items-center gap-2 text-gray-400",
            "{label}:"
            input {
                r#type: "number",
                class: "w-16 bg-gray-700 text-white text-sm rounded px-2 py-1 border border-gray-600",
                min: min.to_string(),
                max: max.map(|m| m.to_string()),
                value: current.to_string(),
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<i32>() {
                        let clamped = if let Some(m) = max { v.clamp(min, m) } else { v.max(min) };
                        registry.set_int(control_key, clamped);
                    }
                },
            }
        }
    }
}

/// String input control - reads signal reactively
#[component]
fn StringInput(
    registry: ControlRegistry,
    control_key: &'static str,
    label: &'static str,
) -> Element {
    let current = registry.get_string(control_key);

    rsx! {
        label { class: "flex items-center gap-2 text-gray-400",
            "{label}:"
            input {
                r#type: "text",
                class: "w-24 bg-gray-700 text-white text-sm rounded px-2 py-1 border border-gray-600",
                value: current,
                oninput: move |e| {
                    registry.set_string(control_key, e.value());
                },
            }
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
        Checkbox {
            checked: current,
            onchange: move |checked| registry.set_bool(control_key, checked),
            label,
            tooltip: doc,
        }
    }
}

/// Action button component
#[component]
fn ActionButton(label: &'static str, callback: Callback<()>) -> Element {
    rsx! {
        button {
            class: "px-2 py-1 text-xs rounded bg-gray-700 text-gray-300 hover:bg-gray-600",
            onclick: move |_| callback.call(()),
            "{label}"
        }
    }
}

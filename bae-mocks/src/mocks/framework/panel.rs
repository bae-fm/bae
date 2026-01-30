//! Auto-generated control panel UI

use super::registry::ControlRegistry;
use super::viewport::{MockViewport, DEFAULT_BREAKPOINTS};
use crate::storage;
use crate::ui::{
    Checkbox, Chevron, ChevronDirection, Dropdown, DropdownStyle, IconButton, ToggleButton,
};
use crate::Route;
use bae_ui::{LayersIcon, MonitorIcon};
use dioxus::prelude::*;

const COLLAPSED_KEY: &str = "mock_panel_collapsed";
const VIEWPORT_KEY: &str = "mock_panel_viewport";

/// Mock page section for grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockSection {
    DesignSystem,
    Components,
}

impl MockSection {
    pub fn label(self) -> &'static str {
        match self {
            MockSection::DesignSystem => "Design System",
            MockSection::Components => "Components",
        }
    }
}

/// All available mock pages - add new mocks here
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockPage {
    // Design System
    Button,
    Menu,
    Pill,
    TextInput,
    Tooltip,
    // Components
    Library,
    AlbumDetail,
    FolderImport,
    TitleBar,
}

impl MockPage {
    /// All variants - update when adding new mocks
    pub const ALL: &[MockPage] = &[
        // Design System
        MockPage::Button,
        MockPage::Menu,
        MockPage::Pill,
        MockPage::TextInput,
        MockPage::Tooltip,
        // Components
        MockPage::Library,
        MockPage::AlbumDetail,
        MockPage::FolderImport,
        MockPage::TitleBar,
    ];

    /// Section this mock belongs to
    pub fn section(self) -> MockSection {
        match self {
            MockPage::Button
            | MockPage::Menu
            | MockPage::Pill
            | MockPage::TextInput
            | MockPage::Tooltip => MockSection::DesignSystem,
            _ => MockSection::Components,
        }
    }

    /// Display name shown in UI
    pub fn label(self) -> &'static str {
        match self {
            MockPage::Button => "Button",
            MockPage::Menu => "Menu",
            MockPage::Pill => "Pill",
            MockPage::TextInput => "TextInput",
            MockPage::Tooltip => "Tooltip",
            MockPage::Library => "LibraryView",
            MockPage::AlbumDetail => "AlbumDetailView",
            MockPage::FolderImport => "FolderImportView",
            MockPage::TitleBar => "TitleBarView",
        }
    }

    /// URL key for serialization
    pub fn key(self) -> &'static str {
        match self {
            MockPage::Button => "button",
            MockPage::Menu => "menu",
            MockPage::Pill => "pill",
            MockPage::TextInput => "text-input",
            MockPage::Tooltip => "tooltip",
            MockPage::Library => "library",
            MockPage::AlbumDetail => "album-detail",
            MockPage::FolderImport => "folder-import",
            MockPage::TitleBar => "title-bar",
        }
    }

    /// Description shown in mock index
    pub fn description(self) -> &'static str {
        match self {
            MockPage::Button => "Button component with variants, sizes, and states",
            MockPage::Menu => "Dropdown menu with items, dividers, and danger states",
            MockPage::Pill => "Pill component for tokens, tags, and inline labels",
            MockPage::TextInput => "Text input component with sizes and states",
            MockPage::Tooltip => "Hover tooltip with placement and wrapping options",
            MockPage::Library => "Album grid with loading/error/empty states",
            MockPage::AlbumDetail => "Album detail page with tracks and controls",
            MockPage::FolderImport => "Folder import workflow with all phases",
            MockPage::TitleBar => "Title bar with nav, search, and update indicator",
        }
    }

    /// Convert to Route
    pub fn to_route(self, state: Option<String>) -> Route {
        match self {
            MockPage::Button => Route::MockButton { state },
            MockPage::Menu => Route::MockMenu { state },
            MockPage::Pill => Route::MockPill { state },
            MockPage::TextInput => Route::MockTextInput { state },
            MockPage::Tooltip => Route::MockTooltip { state },
            MockPage::Library => Route::MockLibrary { state },
            MockPage::AlbumDetail => Route::MockAlbumDetail { state },
            MockPage::FolderImport => Route::MockFolderImport { state },
            MockPage::TitleBar => Route::MockTitleBar { state },
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
        "full" => "w-full",
        _ => max_width,
    };

    let header_mb = if collapsed() { "" } else { "mb-3" };

    rsx! {
        div { class: "h-screen flex flex-col bg-surface-base text-white",
            // Controls panel
            div { class: "bg-gray-800 border-b border-gray-700 p-4 flex-shrink-0",
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
            MockViewport { width: viewport_width(), {children} }
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
        label { class: "flex items-center gap-1.5 text-gray-400 text-sm",
            LayersIcon { class: "w-3.5 h-3.5" }
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
}

/// Auto-generated controls row
#[component]
fn ControlsRow(registry: ControlRegistry) -> Element {
    use super::registry::ControlValue;

    // Separate controls by type, filtering by visibility
    // Non-inline enum controls get their own button group rows
    let enum_controls: Vec<_> = registry
        .controls
        .iter()
        .filter(|c| c.enum_options.is_some() && !c.inline && c.is_visible(&registry))
        .collect();
    // Inline enum controls render as dropdowns in the flags row
    let inline_enum_controls: Vec<_> = registry
        .controls
        .iter()
        .filter(|c| c.enum_options.is_some() && c.inline && c.is_visible(&registry))
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

        // Bool controls + inline enum dropdowns
        if !bool_controls.is_empty() || !inline_enum_controls.is_empty() {
            div { class: "flex flex-wrap gap-4 text-sm mb-3",
                // Inline enum dropdowns first
                for control in inline_enum_controls {
                    InlineEnumDropdown {
                        registry: registry.clone(),
                        control_key: control.key,
                        label: control.label,
                        options: control.enum_options.clone().unwrap_or_default(),
                    }
                }
                // Then bool checkboxes
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
        label { class: "flex items-center gap-1.5 text-gray-400 text-sm",
            MonitorIcon { class: "w-3.5 h-3.5" }
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

/// Inline enum dropdown - renders in the same row as bool controls
#[component]
fn InlineEnumDropdown(
    registry: ControlRegistry,
    control_key: &'static str,
    label: &'static str,
    options: Vec<(&'static str, &'static str)>,
) -> Element {
    let current = registry.get_string(control_key);

    rsx! {
        label { class: "flex items-center gap-2 text-gray-400",
            "{label}:"
            Dropdown {
                value: current.clone(),
                onchange: move |value: String| registry.set_string(control_key, value),
                for (value , display) in &options {
                    option { value: *value, selected: current == *value, "{display}" }
                }
            }
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

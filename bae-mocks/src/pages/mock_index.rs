//! Mock pages with URL state persistence

use crate::mocks::framework::{MockPage, MockSection};
use crate::mocks::{
    AlbumDetailMock, ButtonMock, FolderImportMock, LibraryMock, MenuMock, PillMock, TextInputMock,
    TitleBarMock, TooltipMock,
};
use crate::ui::LinkCard;
use crate::Route;
use bae_ui::{
    Button, ButtonSize, ButtonVariant, MenuItem, Pill, PillVariant, TextInput, TextInputSize,
    TooltipBubble,
};
use dioxus::prelude::*;

#[component]
pub fn MockIndex() -> Element {
    let components: Vec<_> = MockPage::ALL
        .iter()
        .filter(|p| p.section() == MockSection::Components)
        .collect();

    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white p-8",
            h1 { class: "text-2xl font-bold mb-6", "bae mocks" }

            h2 { class: "text-lg font-semibold text-gray-400 mb-3", "Demo App" }
            div { class: "space-y-2 mb-8",
                LinkCard {
                    to: Route::Library {},
                    title: "Full App",
                    description: "Complete app with nav, playback bar, and all pages",
                }
            }

            h2 { class: "text-lg font-semibold text-gray-400 mb-3", "Design System" }
            div { class: "grid grid-cols-3 gap-4 mb-8",
                // Button specimens
                SpecimenCard { title: "Button", to: Route::MockButton { state: None },
                    div { class: "flex flex-wrap gap-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Small,
                            onclick: |_| {},
                            "Primary"
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: |_| {},
                            "Secondary"
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Small,
                            onclick: |_| {},
                            "Outline"
                        }
                    }
                }

                // Menu specimens
                SpecimenCard { title: "Menu", to: Route::MockMenu { state: None },
                    div { class: "flex flex-col gap-0.5 bg-gray-900 rounded-lg border border-white/5 p-1 w-[120px]",
                        MenuItem { onclick: |_| {}, "Add" }
                        MenuItem { onclick: |_| {}, "Clear" }
                    }
                }

                // Pill specimens
                SpecimenCard { title: "Pill", to: Route::MockPill { state: None },
                    div { class: "flex flex-wrap gap-2",
                        Pill { variant: PillVariant::Muted, "Muted" }
                        Pill { variant: PillVariant::Link, "Link" }
                        Pill { variant: PillVariant::Link, monospace: true, "mono" }
                    }
                }

                // TextInput specimens
                SpecimenCard {
                    title: "TextInput",
                    to: Route::MockTextInput {
                        state: None,
                    },
                    div { class: "space-y-2",
                        TextInput {
                            value: "Sample text".to_string(),
                            on_input: |_| {},
                            size: TextInputSize::Medium,
                        }
                        TextInput {
                            value: "".to_string(),
                            on_input: |_| {},
                            size: TextInputSize::Small,
                            placeholder: "Placeholder...",
                        }
                    }
                }

                // Tooltip specimens (static previews since pointer-events are disabled)
                SpecimenCard { title: "Tooltip", to: Route::MockTooltip { state: None },
                    div { class: "flex flex-col items-start gap-2",
                        TooltipBubble { text: "Save changes", nowrap: true }
                        TooltipBubble {
                            text: "Based on CD layout. Calculated using rip logs or CUE/FLAC files.",
                            nowrap: false,
                        }
                    }
                }
            }

            h2 { class: "text-lg font-semibold text-gray-400 mb-3", "Components" }
            div { class: "space-y-2",
                for page in components {
                    LinkCard {
                        to: page.to_route(None),
                        title: page.label(),
                        description: page.description(),
                    }
                }
            }
        }
    }
}

/// A card showing specimen samples with a link to the full page
#[component]
fn SpecimenCard(title: &'static str, to: Route, children: Element) -> Element {
    rsx! {
        Link {
            to,
            class: "block bg-gray-950 rounded-lg p-4 hover:bg-gray-900 transition-colors border border-gray-800",
            h3 { class: "text-sm font-medium text-gray-300 mb-3", "{title}" }
            div { class: "pointer-events-none", {children} }
        }
    }
}

// ============================================================================
// Button page wrapper
// ============================================================================

#[component]
pub fn MockButton(state: Option<String>) -> Element {
    rsx! {
        ButtonMock { initial_state: state }
    }
}

// ============================================================================
// FolderImport page wrapper
// ============================================================================

#[component]
pub fn MockFolderImport(state: Option<String>) -> Element {
    rsx! {
        FolderImportMock { initial_state: state }
    }
}

// ============================================================================
// AlbumDetail page wrapper
// ============================================================================

#[component]
pub fn MockAlbumDetail(state: Option<String>) -> Element {
    rsx! {
        AlbumDetailMock { initial_state: state }
    }
}

// ============================================================================
// Library page wrapper
// ============================================================================

#[component]
pub fn MockLibrary(state: Option<String>) -> Element {
    rsx! {
        LibraryMock { initial_state: state }
    }
}

// ============================================================================
// Menu page wrapper
// ============================================================================

#[component]
pub fn MockMenu(state: Option<String>) -> Element {
    rsx! {
        MenuMock { initial_state: state }
    }
}

// ============================================================================
// Pill page wrapper
// ============================================================================

#[component]
pub fn MockPill(state: Option<String>) -> Element {
    rsx! {
        PillMock { initial_state: state }
    }
}

// ============================================================================
// TextInput page wrapper
// ============================================================================

#[component]
pub fn MockTextInput(state: Option<String>) -> Element {
    rsx! {
        TextInputMock { initial_state: state }
    }
}

// ============================================================================
// Tooltip page wrapper
// ============================================================================

#[component]
pub fn MockTooltip(state: Option<String>) -> Element {
    rsx! {
        TooltipMock { initial_state: state }
    }
}

// ============================================================================
// TitleBar page wrapper
// ============================================================================

#[component]
pub fn MockTitleBar(state: Option<String>) -> Element {
    rsx! {
        TitleBarMock { initial_state: state }
    }
}

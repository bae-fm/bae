//! Menu components for dropdown menus
//!
//! Provides a consistent menu styling across the app:
//! - `MenuDropdown` - positioned dropdown with menu styling
//! - `MenuItem` - individual menu item with hover states

use crate::components::{ChromelessButton, Dropdown, Placement};
use dioxus::prelude::*;

/// Dropdown menu with standard styling
///
/// Combines `Dropdown` positioning with consistent menu appearance.
/// Use with `MenuItem` children for consistent item styling.
///
/// ```ignore
/// MenuDropdown {
///     anchor_id: "my-menu",
///     is_open,
///     on_close: move |_| show_menu.set(false),
///     MenuItem { onclick: move |_| { ... }, "Edit" }
///     MenuItem { onclick: move |_| { ... }, "Export" }
///     MenuItem { danger: true, onclick: move |_| { ... }, "Delete" }
/// }
/// ```
#[component]
pub fn MenuDropdown(
    /// ID of the anchor element (button that opens the menu)
    anchor_id: String,
    /// Whether the menu is open
    is_open: ReadSignal<bool>,
    /// Called when menu should close (click outside, escape, etc.)
    on_close: EventHandler<()>,
    /// Menu placement relative to anchor
    #[props(default = Placement::BottomEnd)]
    placement: Placement,
    /// Menu contents (typically MenuItem components)
    children: Element,
) -> Element {
    rsx! {
        Dropdown {
            anchor_id,
            is_open,
            on_close,
            placement,
            class: "bg-gray-900 rounded-lg shadow-xl border border-white/5 p-1 min-w-[120px]",
            {children}
        }
    }
}

/// Individual menu item
///
/// Use inside `MenuDropdown` for consistent styling.
#[component]
pub fn MenuItem(
    /// Whether the item is disabled
    #[props(default)]
    disabled: bool,
    /// Whether this is a destructive action (red text)
    #[props(default)]
    danger: bool,
    /// Click handler
    onclick: EventHandler<MouseEvent>,
    /// Item content (text and optional icon)
    children: Element,
) -> Element {
    let base =
        "w-full text-left px-2.5 py-1.5 text-xs rounded transition-colors flex items-center gap-2";
    let variant = if danger {
        "text-red-400 hover:bg-red-500/10"
    } else {
        "text-gray-200 hover:bg-gray-700 hover:text-white"
    };
    let disabled_class = if disabled {
        "opacity-50 cursor-not-allowed"
    } else {
        ""
    };

    rsx! {
        ChromelessButton {
            disabled,
            class: Some(format!("{base} {variant} {disabled_class}")),
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                if !disabled {
                    onclick.call(e);
                }
            },
            {children}
        }
    }
}

/// Menu divider line
#[component]
pub fn MenuDivider() -> Element {
    rsx! {
        div { class: "my-1 border-t border-border-subtle" }
    }
}

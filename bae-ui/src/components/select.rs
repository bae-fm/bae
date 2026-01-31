//! Custom select component built on the Dropdown popover infrastructure
//!
//! Replaces native `<select>` elements with styled dropdowns that match
//! the dark theme. Uses `Dropdown` + floating-ui for positioning and
//! light dismiss behavior.
//!
//! ```ignore
//! Select {
//!     value: "s3",
//!     onchange: move |val: String| { ... },
//!     SelectOption { value: "__none__", label: "No Storage" }
//!     SelectOption { value: "s3", label: "S3 Archive" }
//! }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;

use crate::components::icons::{CheckIcon, ChevronDownIcon};
use crate::components::{Dropdown, Placement};

/// Counter for generating unique select anchor IDs
static SELECT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Context shared between Select and its SelectOption children
#[derive(Clone)]
struct SelectContext {
    current_value: String,
    onchange: EventHandler<String>,
    close: Callback<()>,
    /// Registered options: (value, label)
    options: Signal<Vec<(String, String)>>,
}

/// Custom styled select dropdown
#[component]
pub fn Select(
    /// Currently selected value
    value: String,
    /// Called when selection changes
    onchange: EventHandler<String>,
    /// Whether the select is disabled
    #[props(default)]
    disabled: bool,
    /// Options (SelectOption children)
    children: Element,
) -> Element {
    let mut is_open = use_signal(|| false);
    let is_open_read: ReadSignal<bool> = is_open.into();
    let options: Signal<Vec<(String, String)>> = use_signal(Vec::new);

    let anchor_id = use_hook(|| {
        let id = SELECT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("select-anchor-{}", id)
    });

    let close_cb = use_callback(move |()| {
        is_open.set(false);
    });

    let ctx = SelectContext {
        current_value: value.clone(),
        onchange,
        close: close_cb,
        options,
    };

    // Provide context so SelectOption children can register and interact
    use_context_provider(|| ctx);

    // Find the display label for the current value
    let display_label = options
        .read()
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, label)| label.clone())
        .unwrap_or_default();

    rsx! {
        div { class: "relative inline-block",
            // Trigger button
            button {
                id: "{anchor_id}",
                class: "inline-flex items-center gap-2 text-sm rounded-lg px-3 py-1.5 border border-gray-600 text-gray-300 hover:border-gray-500 hover:text-white hover:bg-gray-700/30 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                disabled,
                onclick: move |_| {
                    if !disabled {
                        is_open.set(!is_open());
                    }
                },
                span { class: "truncate", "{display_label}" }
                ChevronDownIcon { class: "w-3.5 h-3.5 text-gray-400 flex-shrink-0" }
            }

            // Dropdown panel
            Dropdown {
                anchor_id: anchor_id.clone(),
                is_open: is_open_read,
                on_close: move |_| is_open.set(false),
                placement: Placement::BottomStart,
                class: "bg-gray-900 rounded-lg shadow-xl border border-white/5 p-1 min-w-[120px]",
                {children}
            }
        }
    }
}

/// An option within a Select dropdown
#[component]
pub fn SelectOption(
    /// Value for this option
    value: String,
    /// Display label text
    label: String,
) -> Element {
    let ctx = use_context::<SelectContext>();
    let is_selected = ctx.current_value == value;

    // Register this option's value and label
    {
        let value = value.clone();
        let label = label.clone();
        let mut options = ctx.options;
        use_hook(move || {
            let mut opts = options.write();
            if !opts.iter().any(|(v, _)| *v == value) {
                opts.push((value, label));
            }
        });
    }

    let value_for_click = value.clone();

    rsx! {
        button {
            class: "w-full text-left px-2.5 py-1.5 text-xs rounded transition-colors flex items-center gap-2 {selected_class(is_selected)}",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                ctx.onchange.call(value_for_click.clone());
                ctx.close.call(());
            },
            if is_selected {
                CheckIcon { class: "w-3.5 h-3.5 text-indigo-400 flex-shrink-0" }
            } else {
                span { class: "w-3.5 h-3.5 flex-shrink-0" }
            }
            "{label}"
        }
    }
}

fn selected_class(is_selected: bool) -> &'static str {
    if is_selected {
        "text-white bg-gray-700/50"
    } else {
        "text-gray-200 hover:bg-gray-700 hover:text-white"
    }
}

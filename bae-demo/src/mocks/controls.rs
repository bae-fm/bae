//! Reusable control components for mock pages

use super::mock_header::MockHeader;
use dioxus::prelude::*;

/// Layout wrapper for mock pages with sticky controls panel
#[component]
pub fn MockLayout(
    title: String,
    max_width: &'static str,
    controls: Element,
    children: Element,
) -> Element {
    let max_w_class = match max_width {
        "4xl" => "max-w-4xl",
        "6xl" => "max-w-6xl",
        _ => max_width,
    };

    rsx! {
        div { class: "min-h-screen bg-gray-900 text-white",
            div { class: "sticky top-0 z-50 bg-gray-800 border-b border-gray-700 p-4",
                div { class: "{max_w_class} mx-auto",
                    MockHeader { title }
                    {controls}
                }
            }
            div { class: "{max_w_class} mx-auto p-6", {children} }
        }
    }
}

/// Button group for selecting enum values
#[component]
pub fn MockEnumButtons<T: PartialEq + Clone + 'static>(
    options: Vec<(T, &'static str)>,
    value: Signal<T>,
) -> Element {
    rsx! {
        div { class: "flex flex-wrap gap-2 mb-3",
            for (option_value , label) in options {
                button {
                    class: if value() == option_value { "px-3 py-1.5 text-sm rounded bg-blue-600 text-white" } else { "px-3 py-1.5 text-sm rounded bg-gray-700 text-gray-300 hover:bg-gray-600" },
                    onclick: move |_| value.set(option_value.clone()),
                    "{label}"
                }
            }
        }
    }
}

/// Checkbox control for boolean values
#[component]
pub fn MockCheckbox(label: &'static str, value: Signal<bool>) -> Element {
    rsx! {
        label { class: "flex items-center gap-2 text-gray-400",
            input {
                r#type: "checkbox",
                checked: value(),
                onchange: move |e| value.set(e.checked()),
            }
            "{label}"
        }
    }
}

/// Dropdown selector for optional values
#[component]
pub fn MockSelect(
    label: &'static str,
    options: Vec<(String, &'static str)>,
    value: Signal<Option<String>>,
) -> Element {
    rsx! {
        label { class: "flex items-center gap-2 text-gray-400",
            "{label}:"
            select {
                class: "bg-gray-700 rounded px-2 py-1 text-white",
                onchange: move |e| value.set(Some(e.value())),
                for (option_value , option_label) in options {
                    option {
                        value: "{option_value}",
                        selected: value() == Some(option_value.clone()),
                        "{option_label}"
                    }
                }
            }
        }
    }
}

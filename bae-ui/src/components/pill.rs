//! Pill component for tags, tokens, and inline labels

use dioxus::prelude::*;

/// Visual style for pills
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillVariant {
    /// Subtle gray background - for tokens/tags
    Muted,
    /// Blue text for links/interactive
    Link,
}

/// Pill component - a rounded label for tags, tokens, or inline elements
#[component]
pub fn Pill(
    variant: PillVariant,
    #[props(default)] href: Option<String>,
    #[props(default)] monospace: bool,
    children: Element,
) -> Element {
    let base = "inline-flex items-center px-2 py-0.5 text-xs rounded-full transition-colors";

    let font = if monospace { "font-mono" } else { "" };

    let variant_class = match variant {
        PillVariant::Muted => "bg-surface-raised text-gray-300",
        PillVariant::Link => "bg-gray-700 text-blue-400 hover:text-blue-300",
    };

    if let Some(url) = href {
        rsx! {
            a {
                href: "{url}",
                target: "_blank",
                class: "{base} {font} {variant_class}",
                {children}
            }
        }
    } else {
        rsx! {
            span { class: "{base} {font} {variant_class}", {children} }
        }
    }
}

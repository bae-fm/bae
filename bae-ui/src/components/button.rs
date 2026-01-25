//! Reusable button component

use dioxus::prelude::*;

/// Chromeless button component - provides accessibility and base functionality
/// without visual styling. Used internally by Button and for special cases.
#[component]
pub fn ChromelessButton(
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] class: Option<String>,
    #[props(default)] r#type: Option<&'static str>,
    #[props(default)] title: Option<String>,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] onmousedown: Option<EventHandler<MouseEvent>>,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let is_disabled = disabled || loading;

    rsx! {
        button {
            class: class.as_deref(),
            id: id.as_deref(),
            r#type,
            disabled: is_disabled,
            title: title.as_deref(),
            aria_label: aria_label.as_deref(),
            aria_disabled: if is_disabled { Some("true") } else { None },
            onmousedown: move |e| {
                if let Some(ref handler) = onmousedown {
                    handler.call(e);
                }
            },
            onclick: move |e| {
                if !is_disabled {
                    onclick.call(e);
                }
            },
            {children}
        }
    }
}

/// Button visual variant
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ButtonVariant {
    /// Indigo background - for primary actions
    #[default]
    Primary,
    /// Gray background - for secondary/cancel actions
    Secondary,
    /// Red background - for destructive actions
    Danger,
    /// No background - text only with hover
    Ghost,
}

/// Button size
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ButtonSize {
    /// Smaller padding, text-sm
    Small,
    /// Standard padding
    #[default]
    Medium,
}

/// Reusable button component with consistent styling
#[component]
pub fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] class: Option<String>,
    #[props(default)] id: Option<String>,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let base = match size {
        ButtonSize::Small => "inline-flex items-center gap-2 text-sm rounded-lg transition-colors",
        ButtonSize::Medium => "inline-flex items-center gap-2 rounded-lg transition-colors",
    };

    let padding = match size {
        ButtonSize::Small => "px-3 py-1.5",
        ButtonSize::Medium => "px-4 py-2",
    };

    let variant_class = match variant {
        ButtonVariant::Primary => {
            "bg-indigo-600 hover:bg-indigo-500 text-white disabled:opacity-50 disabled:cursor-not-allowed"
        }
        ButtonVariant::Secondary => {
            "bg-gray-700 hover:bg-gray-600 text-gray-300 disabled:opacity-50 disabled:cursor-not-allowed"
        }
        ButtonVariant::Danger => {
            "bg-red-600 hover:bg-red-500 text-white disabled:opacity-50 disabled:cursor-not-allowed"
        }
        ButtonVariant::Ghost => "text-gray-400 hover:text-white hover:bg-gray-700/50",
    };

    let computed_class = match &class {
        Some(extra) => format!("{base} {padding} {variant_class} {extra}"),
        None => format!("{base} {padding} {variant_class}"),
    };

    rsx! {
        ChromelessButton {
            id,
            disabled,
            loading,
            class: Some(computed_class),
            onclick,
            {children}
        }
    }
}

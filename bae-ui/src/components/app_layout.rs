//! App layout view component
//!
//! Provides the overall app structure with slots for title bar, main content,
//! playback bar, queue sidebar, and extra elements.

use dioxus::prelude::*;

/// Internal content wrapper with proper padding for fixed elements
#[component]
fn ContentArea(children: Element, has_title_bar: bool, has_playback_bar: bool) -> Element {
    let class = match (has_title_bar, has_playback_bar) {
        (true, true) => "pt-10 pb-24",
        (true, false) => "pt-10",
        (false, true) => "pb-24",
        (false, false) => "",
    };

    rsx! {
        div { class: "{class}", {children} }
    }
}

/// App layout view (pure, props-based)
/// Provides the structural layout with optional slots for each section.
/// Automatically adds appropriate padding to content based on present elements.
#[component]
pub fn AppLayoutView(
    /// Main content (typically the router outlet)
    children: Element,
    /// Optional title bar at the top
    #[props(default)]
    title_bar: Option<Element>,
    /// Optional playback bar at the bottom
    #[props(default)]
    playback_bar: Option<Element>,
    /// Optional queue sidebar
    #[props(default)]
    queue_sidebar: Option<Element>,
    /// Optional extra elements (dialogs, modals, etc.)
    #[props(default)]
    extra: Option<Element>,
) -> Element {
    let has_title_bar = title_bar.is_some();
    let has_playback_bar = playback_bar.is_some();

    rsx! {
        if let Some(tb) = title_bar {
            {tb}
        }
        ContentArea { has_title_bar, has_playback_bar, {children} }
        if let Some(pb) = playback_bar {
            {pb}
        }
        if let Some(qs) = queue_sidebar {
            {qs}
        }
        if let Some(ex) = extra {
            {ex}
        }
    }
}

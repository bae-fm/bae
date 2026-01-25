//! App layout view component
//!
//! Provides the overall app structure with slots for title bar, main content,
//! playback bar, queue sidebar, and extra elements.

use dioxus::prelude::*;

/// App layout view (pure, props-based)
/// Provides the structural layout with optional slots for each section.
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
    rsx! {
        LayoutContainer {
            if let Some(tb) = title_bar {
                {tb}
            }
            {children}
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
}

#[component]
fn LayoutContainer(children: Element) -> Element {
    rsx! {
        div { class: "h-screen flex flex-col", {children} }
    }
}

//! Dropdown component using popover API + floating-ui for positioning
//!
//! Uses native popover API for:
//! - Top-layer rendering (no z-index needed)
//! - Light dismiss (click outside closes)
//!
//! Uses floating-ui for:
//! - Anchor positioning relative to trigger element
//! - Viewport collision handling (flip, shift)

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use wasm_bindgen_x::JsCast;

pub use crate::floating_ui::Placement;
use crate::floating_ui::{self, ComputePositionOptions};

/// Counter for generating unique dropdown IDs
static DROPDOWN_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Dropdown component that positions content relative to an anchor element
#[component]
pub fn Dropdown(
    /// ID of the anchor element to position relative to
    anchor_id: String,
    /// Controls whether the dropdown is visible
    is_open: ReadSignal<bool>,
    /// Called when the dropdown should close (light dismiss)
    on_close: EventHandler<()>,
    /// Placement relative to anchor (default: Bottom)
    #[props(default)]
    placement: Placement,
    /// Offset from anchor in pixels (default: 4)
    #[props(default = 4.0)]
    offset: f64,
    /// Dropdown content
    children: Element,
    /// Optional CSS class for the dropdown container
    #[props(default)]
    class: Option<String>,
) -> Element {
    // Flag set before we call showPopover/hidePopover, cleared after ontoggle processes it.
    // This distinguishes our programmatic toggles from browser-initiated light dismiss.
    let mut programmatic_toggle = use_signal(|| false);

    // Generate unique ID for this dropdown instance using atomic counter
    let popover_id = use_hook(|| {
        let id = DROPDOWN_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("dropdown-{}", id)
    });
    let popover_id_clone = popover_id.clone();

    // Effect to handle open/close
    use_effect(move || {
        let is_open_val = is_open();
        let anchor_id = anchor_id.clone();
        let popover_id = popover_id_clone.clone();

        let Some(window) = web_sys_x::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(floating) = document.get_element_by_id(&popover_id) else {
            return;
        };

        // Check current popover state to avoid redundant toggles (effect may run multiple times)
        let is_popover_open = js_sys_x::Reflect::get(&floating, &"matches".into())
            .ok()
            .and_then(|f| {
                f.dyn_ref::<js_sys_x::Function>()
                    .map(|f| f.call1(&floating, &":popover-open".into()))
            })
            .and_then(|r| r.ok())
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_open_val {
            if is_popover_open {
                return;
            }

            // Reset position to invisible first (prevents flash at old position)
            let _ = floating.set_attribute(
                "style",
                "position: absolute; top: 0; left: 0; width: max-content; margin: 0; opacity: 0;",
            );

            // Mark as programmatic so ontoggle knows to ignore
            programmatic_toggle.set(true);

            // Show the popover (invisible due to opacity: 0)
            if let Ok(show_popover) = js_sys_x::Reflect::get(&floating, &"showPopover".into()) {
                if let Some(func) = show_popover.dyn_ref::<js_sys_x::Function>() {
                    let _ = func.call0(&floating);
                }
            }

            // Calculate position and make visible
            if let Some(anchor) = document.get_element_by_id(&anchor_id) {
                let options = ComputePositionOptions {
                    placement,
                    offset: Some(offset),
                    flip: true,
                    shift: true,
                };

                spawn(async move {
                    if let Ok(result) =
                        floating_ui::compute_position(&anchor, &floating, options).await
                    {
                        let style = format!(
                            "position: absolute; top: 0; left: 0; width: max-content; margin: 0; transform: translate({}px, {}px); opacity: 1;",
                            result.x, result.y
                        );
                        let _ = floating.set_attribute("style", &style);
                    }
                });
            }
        } else {
            if !is_popover_open {
                return;
            }

            // Mark as programmatic so ontoggle knows to ignore
            programmatic_toggle.set(true);

            // Hide the popover
            if let Ok(hide_popover) = js_sys_x::Reflect::get(&floating, &"hidePopover".into()) {
                if let Some(func) = hide_popover.dyn_ref::<js_sys_x::Function>() {
                    let _ = func.call0(&floating);
                }
            }
        }
    });

    let dropdown_class = class.unwrap_or_default();

    rsx! {
        div {
            id: "{popover_id}",
            popover: "auto",
            class: "{dropdown_class}",
            style: "position: absolute; top: 0; left: 0; width: max-content; margin: 0; opacity: 0;",
            ontoggle: move |_| {
                // If we triggered this toggle programmatically, ignore it
                if programmatic_toggle() {
                    programmatic_toggle.set(false);
                    return;
                }

                // This is a light dismiss (browser closed it, not us)
                on_close.call(());
            },
            {children}
        }
    }
}

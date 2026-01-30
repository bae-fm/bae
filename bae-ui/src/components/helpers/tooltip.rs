//! Tooltip component using popover API + floating-ui for positioning
//!
//! Uses `popover="manual"` for top-layer rendering (escapes overflow-clip),
//! and floating-ui for anchor positioning.
//!
//! Three pieces:
//! - **`use_tooltip_handle()`**: Hook returning a `TooltipHandle` with `onmounted()`,
//!   `show()`, `hide()` methods. The caller wires these onto their elements.
//! - **`TooltipPopover`**: Renders just the tooltip bubble. Takes a `TooltipHandle`.
//! - **`Tooltip`**: Sugar that wraps children in a span, creates its own handle internally.

use std::rc::Rc;

use dioxus::prelude::*;
use dioxus_core::Task;
use wasm_bindgen_x::JsCast;

use crate::floating_ui::{self, ComputePositionOptions, Placement};

/// Delay before showing tooltip (in milliseconds)
const TOOLTIP_DELAY_MS: u64 = 700;

/// Horizontal padding of the tooltip bubble in pixels (Tailwind `px-2.5` = 10px).
/// Exposed so callers can use it for cross-axis offset alignment.
pub const TOOLTIP_PADDING_X: i32 = 10;

/// Handle for controlling tooltip visibility and anchor positioning.
///
/// Created by `use_tooltip_handle()`. Wire `onmounted()` onto the anchor element,
/// and `show()`/`hide()` onto the trigger element's mouse events.
#[derive(Clone, Copy, PartialEq)]
pub struct TooltipHandle {
    anchor: Signal<Option<Rc<MountedData>>>,
    is_visible: Signal<bool>,
    hover_task: Signal<Option<Task>>,
}

impl TooltipHandle {
    /// Callback for the anchor element's `onmounted`.
    pub fn onmounted(&self) -> impl FnMut(MountedEvent) {
        let mut anchor = self.anchor;
        move |evt: MountedEvent| {
            anchor.set(Some(evt.data()));
        }
    }

    /// Call on mouseenter of the trigger element (starts delayed show).
    pub fn show(&self) {
        let mut hover_task = self.hover_task;
        let mut is_visible = self.is_visible;
        if let Some(task) = hover_task.take() {
            task.cancel();
        }
        let task = spawn(async move {
            sleep_ms(TOOLTIP_DELAY_MS).await;
            is_visible.set(true);
        });
        hover_task.set(Some(task));
    }

    /// Call on mouseleave of the trigger element (hides immediately).
    pub fn hide(&self) {
        let mut hover_task = self.hover_task;
        let mut is_visible = self.is_visible;
        if let Some(task) = hover_task.take() {
            task.cancel();
        }
        is_visible.set(false);
    }
}

/// Hook that creates a `TooltipHandle` for manual tooltip control.
///
/// ## Example: separate anchor and trigger
/// ```ignore
/// let tip = use_tooltip_handle();
/// rsx! {
///     div {
///         onmounted: tip.onmounted(),                  // anchor = row
///         button {
///             onmouseenter: move |_| tip.show(),       // trigger = icon
///             onmouseleave: move |_| tip.hide(),
///             FolderIcon { class: "w-4 h-4" }
///         }
///         span { "Album Name" }
///     }
///     TooltipPopover { handle: tip, text: "path/to/folder",
///         placement: Placement::TopStart, nowrap: true,
///     }
/// }
/// ```
pub fn use_tooltip_handle() -> TooltipHandle {
    let anchor = use_signal(|| None::<Rc<MountedData>>);
    let is_visible = use_signal(|| false);
    let hover_task = use_signal(|| None::<Task>);

    use_drop(move || {
        if let Some(task) = hover_task.peek().as_ref() {
            task.cancel();
        }
    });

    TooltipHandle {
        anchor,
        is_visible,
        hover_task,
    }
}

/// Tooltip bubble positioned via popover API + floating-ui.
///
/// Takes a `TooltipHandle` for anchor positioning and visibility control.
/// The caller is responsible for wiring `onmounted`/`show`/`hide` on their elements.
#[component]
pub fn TooltipPopover(
    /// Handle from `use_tooltip_handle()`
    handle: TooltipHandle,
    /// The tooltip text to display
    text: String,
    /// Placement relative to anchor
    placement: Placement,
    /// Prevent text wrapping
    nowrap: bool,
    /// Cross-axis offset in pixels (shifts perpendicular to placement)
    #[props(default)]
    cross_axis_offset: Option<i32>,
) -> Element {
    let is_visible = handle.is_visible;
    let anchor = handle.anchor;
    let mut floating_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    use_effect(move || {
        let visible = is_visible();

        let Some(floating_mounted) = floating_ref() else {
            return;
        };
        let Some(floating) = floating_mounted.downcast::<web_sys_x::Element>().cloned() else {
            return;
        };

        let is_popover_open = js_sys_x::Reflect::get(&floating, &"matches".into())
            .ok()
            .and_then(|f| {
                f.dyn_ref::<js_sys_x::Function>()
                    .map(|f| f.call1(&floating, &":popover-open".into()))
            })
            .and_then(|r| r.ok())
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if visible {
            if is_popover_open {
                return;
            }

            let _ = floating.set_attribute(
                "style",
                "position: absolute; top: 0; left: 0; width: max-content; margin: 0; opacity: 0;",
            );

            if let Ok(show) = js_sys_x::Reflect::get(&floating, &"showPopover".into()) {
                if let Some(func) = show.dyn_ref::<js_sys_x::Function>() {
                    let _ = func.call0(&floating);
                }
            }

            let Some(anchor_mounted) = anchor() else {
                return;
            };
            let Some(anchor_el) = anchor_mounted.downcast::<web_sys_x::Element>().cloned() else {
                return;
            };

            let options = ComputePositionOptions {
                placement,
                offset: Some(4.0),
                cross_axis_offset: cross_axis_offset.map(|v| v as f64),
                flip: true,
                shift: true,
            };

            spawn(async move {
                if let Ok(result) =
                    floating_ui::compute_position(&anchor_el, &floating, options).await
                {
                    let style = format!(
                        "position: absolute; top: 0; left: 0; width: max-content; margin: 0; transform: translate({}px, {}px); opacity: 1;",
                        result.x, result.y
                    );
                    let _ = floating.set_attribute("style", &style);
                }
            });
        } else {
            if !is_popover_open {
                return;
            }
            if let Ok(hide) = js_sys_x::Reflect::get(&floating, &"hidePopover".into()) {
                if let Some(func) = hide.dyn_ref::<js_sys_x::Function>() {
                    let _ = func.call0(&floating);
                }
            }
        }
    });

    let bubble_class = tooltip_bubble_class(nowrap);

    rsx! {
        div {
            popover: "manual",
            class: "{bubble_class}",
            style: "position: absolute; top: 0; left: 0; width: max-content; margin: 0; opacity: 0;",
            onmounted: move |evt: MountedEvent| floating_ref.set(Some(evt.data())),
            "{text}"
        }
    }
}

/// A hover-triggered tooltip that wraps children.
///
/// Sugar for the common case where anchor = trigger. Wraps children in a span,
/// creates its own `TooltipHandle` internally.
///
/// ```ignore
/// Tooltip { text: "Hello", placement: Placement::Top, nowrap: true,
///     button { "Hover me" }
/// }
/// ```
#[component]
pub fn Tooltip(
    /// The tooltip text to display
    text: String,
    /// Placement relative to anchor
    placement: Placement,
    /// Prevent text wrapping
    nowrap: bool,
    /// Cross-axis offset in pixels (shifts perpendicular to placement)
    #[props(default)]
    cross_axis_offset: Option<i32>,
    children: Element,
) -> Element {
    let handle = use_tooltip_handle();

    rsx! {
        span {
            class: "inline-flex min-w-0",
            onmounted: handle.onmounted(),
            onmouseenter: move |_| handle.show(),
            onmouseleave: move |_| handle.hide(),
            {children}
        }
        TooltipPopover {
            handle,
            text,
            placement,
            nowrap,
            cross_axis_offset,
        }
    }
}

fn tooltip_bubble_class(nowrap: bool) -> &'static str {
    if nowrap {
        "px-2.5 py-1.5 text-xs leading-relaxed text-gray-200 bg-gray-900 rounded-lg shadow-xl border border-white/5 whitespace-nowrap"
    } else {
        "px-2.5 py-1.5 text-xs leading-relaxed text-gray-200 bg-gray-900 rounded-lg shadow-xl border border-white/5 max-w-xs"
    }
}

/// Static tooltip bubble for use in previews/specimens where hover behavior isn't needed.
#[component]
pub fn TooltipBubble(text: String, nowrap: bool) -> Element {
    rsx! {
        div { class: tooltip_bubble_class(nowrap), role: "tooltip", "{text}" }
    }
}

#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

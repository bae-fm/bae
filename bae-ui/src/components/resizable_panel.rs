//! Resizable panel component with drag handle

use crate::wasm_utils::DocumentEventListener;
use dioxus::prelude::*;

/// Direction for resize operations.
///
/// The direction refers to which dimension you're changing, not the orientation of the bar.
/// - `Horizontal` = resizing in the horizontal direction (changing width), so the grab bar
///   is vertical, cursor is `col-resize`, reads `clientX`
/// - `Vertical` = resizing in the vertical direction (changing height), so the grab bar
///   is horizontal, cursor is `row-resize`, reads `clientY`
#[derive(Clone, Copy, PartialEq)]
pub enum ResizeDirection {
    Horizontal,
    Vertical,
}

/// CSS position mode for the panel container
#[derive(Clone, Copy, PartialEq)]
pub enum PanelPosition {
    Relative,
    Absolute,
}

impl PanelPosition {
    fn as_class(&self) -> &'static str {
        match self {
            PanelPosition::Relative => "relative",
            PanelPosition::Absolute => "absolute",
        }
    }
}

/// A draggable grab bar for resizing panels
#[component]
pub fn GrabBar(
    /// Resize direction (horizontal = col-resize, vertical = row-resize)
    direction: ResizeDirection,
    /// Whether resize drag is currently active
    is_active: bool,
    /// Called when user starts dragging
    on_drag_start: EventHandler<()>,
) -> Element {
    let is_horizontal = direction == ResizeDirection::Horizontal;

    rsx! {
        div {
            class: format!(
                "group flex items-center justify-center transition-opacity opacity-0 hover:opacity-100 {} {}",
                if is_horizontal { "w-2 h-full" } else { "h-2 w-full" },
                if is_active { "opacity-100" } else { "" },
            ),
            style: if is_horizontal { "cursor: col-resize;" } else { "cursor: row-resize;" },
            onmousedown: move |e: MouseEvent| {
                e.prevent_default();
                on_drag_start.call(());
            },
            div {
                class: format!(
                    "rounded-full transition-colors {} {}",
                    if is_horizontal { "w-1 h-full" } else { "h-1 w-full" },
                    if is_active { "bg-gray-700" } else { "bg-gray-800 hover:bg-gray-700" },
                ),
            }
            // Visual indicator line
            div {
                class: format!(
                    "rounded-full transition-all opacity-0 group-hover:opacity-100 {} {} {}",
                    if is_horizontal { "w-0.5 h-12" } else { "h-0.5 w-12" },
                    if is_active { "bg-gray-200" } else { "bg-gray-300 hover:bg-gray-200" },
                    if is_active { "opacity-100" } else { "" },
                ),
            }
        }
    }
}

/// Cleanup handle for drag operation listeners
struct DragListeners {
    _mousemove: DocumentEventListener,
    _mouseup: DocumentEventListener,
}

/// A panel that can be resized by dragging its edge
#[component]
pub fn ResizablePanel(
    /// Key for persisting size to localStorage
    storage_key: &'static str,
    /// Minimum size in pixels
    min_size: f64,
    /// Maximum size in pixels
    max_size: f64,
    /// Initial/default size in pixels
    default_size: f64,
    /// Grab bar span as a ratio of the panel edge (0.0 - 1.0)
    grabber_span_ratio: f64,
    /// Resize direction
    direction: ResizeDirection,
    /// CSS position mode (relative or absolute)
    position: PanelPosition,
    /// Additional CSS classes
    #[props(default = "")]
    class: &'static str,
    /// Optional snap points - size snaps to nearest point on drag end
    #[props(default)]
    snap_points: Option<Vec<f64>>,
    /// Panel contents
    children: Element,
) -> Element {
    let mut size = use_signal(|| default_size);
    let mut loaded_from_storage = use_signal(|| false);

    // Load initial size from localStorage after mount (can't do this in use_hook
    // because the wry-bindgen JS bridge may not be ready during component init)
    use_effect(move || {
        if loaded_from_storage() {
            return;
        }
        loaded_from_storage.set(true);

        if let Some(stored_size) = web_sys_x::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(storage_key).ok().flatten())
            .and_then(|v| v.parse::<f64>().ok())
        {
            size.set(stored_size.clamp(min_size, max_size));
        }
    });
    let mut is_resizing = use_signal(|| false);
    let mut drag_listeners: Signal<Option<DragListeners>> = use_signal(|| None);
    let is_horizontal = direction == ResizeDirection::Horizontal;
    let grabber_span_ratio = grabber_span_ratio.clamp(0.1, 1.0);
    let grabber_span_percent = grabber_span_ratio * 100.0;
    let grabber_offset_percent = (100.0 - grabber_span_percent) / 2.0;

    // Save size to localStorage when resize ends
    use_effect(move || {
        if is_resizing() {
            return;
        }

        if let Some(storage) = web_sys_x::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = storage.set_item(storage_key, &size().to_string());
        }
    });

    // Document-level mouse listeners for resize dragging
    use_effect(move || {
        use web_sys_x::js_sys;

        if !is_resizing() {
            // Drop listeners to remove them
            drag_listeners.set(None);
            return;
        }

        let Some(window) = web_sys_x::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };

        // For horizontal (width), we use clientX directly since panel is on the left.
        // For vertical (height), panel is at bottom so we need: viewport_height - clientY
        let coord_key = if is_horizontal { "clientX" } else { "clientY" };
        let viewport_height = window.inner_height().ok().and_then(|v| v.as_f64());

        let mousemove = DocumentEventListener::new(
            document.clone(),
            "mousemove",
            move |e: wasm_bindgen_x::JsValue| {
                if let Ok(coord) = js_sys::Reflect::get(&e, &coord_key.into()) {
                    if let Some(val) = coord.as_f64() {
                        let new_size = if is_horizontal {
                            val
                        } else {
                            // For bottom panel: height = viewport_height - mouse_y
                            viewport_height.unwrap_or(800.0) - val
                        };
                        let clamped = new_size.clamp(min_size, max_size);
                        size.set(clamped);
                    }
                }
            },
        );

        let snap_points_clone = snap_points.clone();
        let mouseup =
            DocumentEventListener::new(document, "mouseup", move |_: wasm_bindgen_x::JsValue| {
                // Snap to nearest point if snap_points provided
                if let Some(ref points) = snap_points_clone {
                    if !points.is_empty() {
                        let current = size();
                        let snapped = points
                            .iter()
                            .min_by(|a, b| {
                                (current - **a)
                                    .abs()
                                    .partial_cmp(&(current - **b).abs())
                                    .unwrap()
                            })
                            .copied()
                            .unwrap_or(current);
                        size.set(snapped);
                    }
                }

                is_resizing.set(false);
            });

        drag_listeners.set(Some(DragListeners {
            _mousemove: mousemove,
            _mouseup: mouseup,
        }));
    });

    let size_style = if is_horizontal {
        format!("width: {}px;", size())
    } else {
        format!("height: {}px;", size())
    };

    rsx! {
        div {
            class: format!(
                "{} flex flex-shrink-0 overflow-clip {} {} {}",
                position.as_class(),
                if is_horizontal { "self-stretch" } else { "self-stretch flex-col" },
                if is_resizing() { "select-none" } else { "" },
                class,
            ),
            style: "{size_style}",

            // For vertical resize, grab bar overlays top edge
            if !is_horizontal {
                div { class: "absolute inset-0 z-10 pointer-events-none",
                    div {
                        class: "absolute top-0 pointer-events-auto -translate-y-1/2",
                        style: "left: {grabber_offset_percent}%; width: {grabber_span_percent}%;",
                        GrabBar {
                            direction,
                            is_active: is_resizing(),
                            on_drag_start: move |_| is_resizing.set(true),
                        }
                    }
                }
            }

            // Content area
            div { class: "flex-1 min-w-0 min-h-0 overflow-clip", {children} }

            // For horizontal resize, grab bar overlays right edge
            if is_horizontal {
                div { class: "absolute inset-0 z-10 pointer-events-none",
                    div {
                        class: "absolute right-0 top-0 bottom-0 pointer-events-auto -translate-x-1/2",
                        style: "top: {grabber_offset_percent}%; height: {grabber_span_percent}%;",
                        GrabBar {
                            direction,
                            is_active: is_resizing(),
                            on_drag_start: move |_| is_resizing.set(true),
                        }
                    }
                }
            }
        }
    }
}

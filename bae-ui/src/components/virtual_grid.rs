//! Virtual scrolling grid component
//!
//! Renders only visible items plus a buffer, using spacer elements to maintain scroll height.
//!
//! ## Scroll Target
//!
//! By default, the grid creates its own scrollable container. Set `scroll_target` to `"body"`
//! to use window scrolling instead (useful when the grid is in a page that scrolls).

use dioxus::prelude::*;
use std::rc::Rc;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast};

/// Cleanup handle for ResizeObserver - disconnects on drop
#[cfg(target_arch = "wasm32")]
struct ResizeObserverCleanup {
    observer: web_sys::ResizeObserver,
    _callback: Closure<dyn FnMut(js_sys::Array)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for ResizeObserverCleanup {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

/// Cleanup handle for window event listeners - removes listeners on drop
#[cfg(target_arch = "wasm32")]
struct WindowListenersCleanup {
    scroll_callback: Closure<dyn FnMut()>,
    resize_callback: Closure<dyn FnMut()>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for WindowListenersCleanup {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            let _ = window.remove_event_listener_with_callback(
                "scroll",
                self.scroll_callback.as_ref().unchecked_ref(),
            );
            let _ = window.remove_event_listener_with_callback(
                "resize",
                self.resize_callback.as_ref().unchecked_ref(),
            );
        }
    }
}

/// Scroll target for the virtual grid
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ScrollTarget {
    /// Grid has its own scrollable container (default)
    #[default]
    Container,
    /// Use window/body scrolling
    Window,
}

/// Wrapper for render functions that allows capturing state.
/// PartialEq returns false to ensure re-renders when the closure might have changed.
pub struct RenderFn<T>(pub Rc<dyn Fn(T, usize) -> Element>);

impl<T> Clone for RenderFn<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T> PartialEq for RenderFn<T> {
    fn eq(&self, _other: &Self) -> bool {
        false // Conservative: assume render function may have changed
    }
}

/// Configuration for the virtual grid
#[derive(Clone, PartialEq)]
pub struct VirtualGridConfig {
    /// Minimum width of each item (used to calculate column count)
    pub item_width: f64,
    /// Height of each item (not including gap)
    pub item_height: f64,
    /// Number of extra rows to render above/below viewport
    pub buffer_rows: usize,
    /// Gap between items in pixels
    pub gap: f64,
}

/// Computed grid layout for virtual scrolling
#[derive(Debug, Clone, PartialEq)]
pub struct GridLayout {
    /// Number of columns that fit in the container
    pub columns: usize,
    /// First row to render (including buffer)
    pub start_row: usize,
    /// Last row to render (exclusive, including buffer)
    pub end_row: usize,
    /// First item index to render
    pub start_idx: usize,
    /// Last item index to render (exclusive)
    pub end_idx: usize,
    /// Height of top spacer in pixels
    pub top_padding: f64,
    /// Height of bottom spacer in pixels
    pub bottom_padding: f64,
}

impl GridLayout {
    /// Calculate grid layout based on container dimensions and scroll position
    pub fn calculate(
        item_count: usize,
        config: &VirtualGridConfig,
        container_width: f64,
        container_height: f64,
        scroll_top: f64,
    ) -> Self {
        let columns = ((container_width + config.gap) / (config.item_width + config.gap))
            .floor()
            .max(1.0) as usize;

        let total_rows = if item_count == 0 {
            0
        } else {
            item_count.div_ceil(columns)
        };

        let row_height = config.item_height + config.gap;
        let first_visible_row = (scroll_top / row_height).floor() as usize;
        let visible_row_count = ((container_height / row_height).ceil() as usize).max(1) + 1;

        let start_row = first_visible_row.saturating_sub(config.buffer_rows);
        let end_row = (first_visible_row + visible_row_count + config.buffer_rows).min(total_rows);

        Self {
            columns,
            start_row,
            end_row,
            start_idx: start_row * columns,
            end_idx: (end_row * columns).min(item_count),
            top_padding: (start_row as f64) * row_height,
            bottom_padding: ((total_rows.saturating_sub(end_row)) as f64) * row_height,
        }
    }
}

/// Virtual scrolling grid that only renders visible items
#[component]
pub fn VirtualGrid<T: Clone + PartialEq + 'static>(
    items: Vec<T>,
    config: VirtualGridConfig,
    render_item: RenderFn<T>,
    #[props(default = "grid-item".to_string())] item_class: String,
    /// Container class - must include height constraint for virtual scrolling to work
    /// (ignored when scroll_target is Window)
    #[props(default = "h-[calc(100vh-12rem)]".to_string())]
    container_class: String,
    /// Scroll target: Container (default) for own scrollable area, Window for body scrolling
    #[props(default)]
    scroll_target: ScrollTarget,
) -> Element {
    let mut scroll_top = use_signal(|| 0.0_f64);
    let mut container_width = use_signal(|| 1000.0_f64); // Default until measured
    let mut container_height = use_signal(|| 800.0_f64); // Default until measured
    let mut mounted_element: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut element_offset_top = use_signal(|| 0.0_f64);

    // Measured item dimensions (override config when available)
    let mut measured_item_height = use_signal(|| None::<f64>);
    let mut first_item_element: Signal<Option<Rc<MountedData>>> = use_signal(|| None);

    // Track if we've warned about virtualization not working (to avoid spam)
    #[cfg(target_arch = "wasm32")]
    let warned_for_item_count = use_hook(|| std::cell::Cell::new(0_usize));

    // Cleanup handles - stored in hooks so they're dropped on unmount
    // Using Rc<RefCell<...>> because use_hook requires Clone, but Closure isn't Clone
    #[cfg(target_arch = "wasm32")]
    let window_listeners_handle =
        use_hook(|| Rc::new(std::cell::RefCell::new(None::<WindowListenersCleanup>)));
    #[cfg(target_arch = "wasm32")]
    let resize_observer_handle =
        use_hook(|| Rc::new(std::cell::RefCell::new(None::<ResizeObserverCleanup>)));

    // Set up window scroll listener when using window scrolling
    #[cfg(target_arch = "wasm32")]
    {
        if scroll_target == ScrollTarget::Window && window_listeners_handle.borrow().is_none() {
            if let Some(window) = web_sys::window() {
                // Initial viewport height measurement
                if let Ok(inner_height) = window.inner_height() {
                    if let Some(h) = inner_height.as_f64() {
                        container_height.set(h);
                    }
                }

                // Scroll handler (passive for performance)
                let scroll_closure: Closure<dyn FnMut()> = Closure::wrap(Box::new(move || {
                    if let Some(window) = web_sys::window() {
                        let window_y = window.scroll_y().unwrap_or(0.0);
                        let offset = element_offset_top();
                        let new_scroll_top = (window_y - offset).max(0.0);
                        let current = scroll_top();

                        if (new_scroll_top - current).abs() > 0.5 {
                            scroll_top.set(new_scroll_top);
                        }
                    }
                })
                    as Box<dyn FnMut()>);

                // Window resize only updates viewport height (container_width is handled by ResizeObserver)
                // Only update if height changed significantly to avoid excessive re-renders
                let resize_closure: Closure<dyn FnMut()> = Closure::wrap(Box::new(move || {
                    if let Some(window) = web_sys::window() {
                        if let Ok(h) = window.inner_height() {
                            if let Some(h) = h.as_f64() {
                                if (container_height() - h).abs() > 1.0 {
                                    container_height.set(h);
                                }
                            }
                        }
                    }
                })
                    as Box<dyn FnMut()>);

                let scroll_options = web_sys::AddEventListenerOptions::new();
                scroll_options.set_passive(true);
                window
                    .add_event_listener_with_callback_and_add_event_listener_options(
                        "scroll",
                        scroll_closure.as_ref().unchecked_ref(),
                        &scroll_options,
                    )
                    .ok();

                let resize_options = web_sys::AddEventListenerOptions::new();
                resize_options.set_passive(true);
                window
                    .add_event_listener_with_callback_and_add_event_listener_options(
                        "resize",
                        resize_closure.as_ref().unchecked_ref(),
                        &resize_options,
                    )
                    .ok();

                // Store cleanup handle instead of forgetting
                *window_listeners_handle.borrow_mut() = Some(WindowListenersCleanup {
                    scroll_callback: scroll_closure,
                    resize_callback: resize_closure,
                });
            }
        }
    }

    // Use measured item height if available, otherwise config default
    let effective_config = {
        let mut cfg = config.clone();
        if let Some(h) = measured_item_height() {
            cfg.item_height = h;
        }
        cfg
    };

    // Calculate grid layout
    let layout = GridLayout::calculate(
        items.len(),
        &effective_config,
        container_width(),
        container_height(),
        scroll_top(),
    );

    // Slice visible items
    let visible_items: Vec<(usize, T)> = if layout.start_idx < items.len() {
        items[layout.start_idx..layout.end_idx]
            .iter()
            .enumerate()
            .map(|(i, item)| (layout.start_idx + i, item.clone()))
            .collect()
    } else {
        vec![]
    };

    // Warn if virtualization isn't working (showing all items when we should be virtualizing)
    // Only warn once per item count to avoid console spam
    #[cfg(target_arch = "wasm32")]
    {
        let should_virtualize = items.len() > 50;
        let is_showing_all = visible_items.len() == items.len();
        let already_warned = warned_for_item_count.get() == items.len();

        if should_virtualize && is_showing_all && !already_warned {
            warned_for_item_count.set(items.len());
            let hint = match scroll_target {
                ScrollTarget::Container => "ensure container has a height constraint",
                ScrollTarget::Window => "check element_offset_top measurement",
            };
            web_sys::console::warn_1(
                &format!(
                    "VirtualGrid: rendering all {} items (no virtualization). Hint: {}.",
                    items.len(),
                    hint
                )
                .into(),
            );
        }
    }

    let grid_style = format!(
        "display: grid; grid-template-columns: repeat(auto-fill, minmax({}px, 1fr)); gap: {}px;",
        config.item_width, config.gap
    );

    // Container class depends on scroll target
    let container_classes = match scroll_target {
        ScrollTarget::Container => {
            format!("virtual-grid-container overflow-y-auto flex flex-col {container_class}")
        }
        ScrollTarget::Window => "virtual-grid-container flex flex-col".to_string(),
    };

    // Disable browser scroll anchoring - it fights with virtual scrolling
    let container_style = "overflow-anchor: none;";

    let scroll_target_for_scroll = scroll_target;
    let scroll_target_for_mount = scroll_target;

    rsx! {
        div {
            key: "{scroll_target:?}-container",
            class: "{container_classes}",
            style: "{container_style}",
            onscroll: move |_evt| {
                // Only handle scroll for container mode
                if scroll_target_for_scroll != ScrollTarget::Container {
                    return;
                }
                // Query scroll position from the mounted element
                if let Some(element) = mounted_element.read().clone() {
                    spawn(async move {
                        if let Ok(scroll) = element.get_scroll_offset().await {
                            // Only update if scroll changed meaningfully to avoid re-render storms
                            if (scroll_top() - scroll.y).abs() > 0.5 {
                                scroll_top.set(scroll.y);
                            }
                        }
                    });
                }
            },
            onmounted: move |evt| {
                let data = evt.data();
                mounted_element.set(Some(data.clone()));

                // Set up ResizeObserver for container width changes
                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::JsCast;
                    if let Some(element) = data.downcast::<web_sys::Element>() {
                        let resize_callback: Closure<dyn FnMut(js_sys::Array)> =
                            Closure::wrap(Box::new(move |entries: js_sys::Array| {
                                if let Some(entry) = entries.get(0).dyn_ref::<web_sys::ResizeObserverEntry>() {
                                    let size_list = entry.content_box_size();
                                    if let Some(size) = size_list.get(0).dyn_ref::<web_sys::ResizeObserverSize>() {
                                        let w = size.inline_size();
                                        if (container_width() - w).abs() > 1.0 {
                                            container_width.set(w);
                                        }
                                    }
                                }
                            }) as Box<dyn FnMut(js_sys::Array)>);

                        if let Ok(observer) = web_sys::ResizeObserver::new(resize_callback.as_ref().unchecked_ref()) {
                            observer.observe(element);
                            // Store cleanup handle instead of forgetting
                            *resize_observer_handle.borrow_mut() = Some(ResizeObserverCleanup {
                                observer,
                                _callback: resize_callback,
                            });
                        }
                    }
                }

                // Initial measurement
                spawn(async move {
                    if let Ok(rect) = data.get_client_rect().await {
                        if scroll_target_for_mount == ScrollTarget::Window {
                            #[cfg(target_arch = "wasm32")]
                            {
                                let window_y = web_sys::window()
                                    .and_then(|w| w.scroll_y().ok())
                                    .unwrap_or(0.0);
                                let page_offset = window_y + rect.origin.y;
                                element_offset_top.set(page_offset);
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            element_offset_top.set(rect.origin.y);

                            container_width.set(rect.width());
                        } else {
                            container_width.set(rect.width());
                            container_height.set(rect.height());
                        }
                    }
                });
            },
            // Top spacer - always render to maintain stable DOM structure
            div {
                key: "spacer-top",
                class: "virtual-grid-spacer-top",
                style: "height: {layout.top_padding}px;",
            }

            // Grid content
            div {
                key: "grid-content",
                class: "virtual-grid-content min-h-0",
                style: "{grid_style}",
                for (i, (idx, item)) in visible_items.into_iter().enumerate() {
                    if i == 0 {
                        // First item: measure it to get actual dimensions
                        div {
                            key: "{idx}",
                            class: "{item_class}",
                            "data-index": "{idx}",
                            onmounted: move |evt| {
                                first_item_element.set(Some(evt.data()));
                                spawn(async move {
                                    if let Ok(rect) = evt.get_client_rect().await {
                                        let h = rect.height();
                                        // Only update if significantly different to avoid loops
                                        if measured_item_height().is_none_or(|current| (current - h).abs() > 1.0) {
                                            measured_item_height.set(Some(h));
                                        }
                                    }
                                });
                            },
                            {(render_item.0)(item, idx)}
                        }
                    } else {
                        div {
                            key: "{idx}",
                            class: "{item_class}",
                            "data-index": "{idx}",
                            {(render_item.0)(item, idx)}
                        }
                    }
                }
            }

            // Bottom spacer - always render to maintain stable DOM structure
            div {
                key: "spacer-bottom",
                class: "virtual-grid-spacer-bottom",
                style: "height: {layout.bottom_padding}px;",
            }
        }
    }
}

//! Dropdown component using popover API + manual positioning via web-sys-x
//!
//! Uses native popover API for:
//! - Top-layer rendering (no z-index needed)
//! - Light dismiss (click outside closes)
//!
//! Uses web-sys-x getBoundingClientRect for:
//! - Anchor positioning relative to trigger element
//! - Viewport collision handling (flip, shift)

use dioxus::prelude::*;
use wasm_bindgen_x::JsCast;
use web_sys_x::js_sys;

/// Placement options for dropdown relative to anchor
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Placement {
    /// Below anchor, aligned to start (left in LTR)
    #[default]
    BottomStart,
    /// Below anchor, centered
    Bottom,
    /// Below anchor, aligned to end (right in LTR)
    BottomEnd,
    /// Above anchor, aligned to start
    TopStart,
    /// Above anchor, centered
    Top,
    /// Above anchor, aligned to end
    TopEnd,
    /// Left of anchor, aligned to start (top)
    LeftStart,
    /// Left of anchor, centered
    Left,
    /// Left of anchor, aligned to end (bottom)
    LeftEnd,
    /// Right of anchor, aligned to start (top)
    RightStart,
    /// Right of anchor, centered
    Right,
    /// Right of anchor, aligned to end (bottom)
    RightEnd,
}

impl Placement {
    /// Get the opposite placement for flipping
    fn flip(self) -> Self {
        match self {
            Self::BottomStart => Self::TopStart,
            Self::Bottom => Self::Top,
            Self::BottomEnd => Self::TopEnd,
            Self::TopStart => Self::BottomStart,
            Self::Top => Self::Bottom,
            Self::TopEnd => Self::BottomEnd,
            Self::LeftStart => Self::RightStart,
            Self::Left => Self::Right,
            Self::LeftEnd => Self::RightEnd,
            Self::RightStart => Self::LeftStart,
            Self::Right => Self::Left,
            Self::RightEnd => Self::LeftEnd,
        }
    }

    /// Check if this is a vertical placement (top/bottom)
    fn is_vertical(self) -> bool {
        matches!(
            self,
            Self::BottomStart
                | Self::Bottom
                | Self::BottomEnd
                | Self::TopStart
                | Self::Top
                | Self::TopEnd
        )
    }
}

/// Position result from calculation
#[derive(Clone, Copy, Debug, Default)]
struct Position {
    top: f64,
    left: f64,
}

/// Calculate dropdown position relative to anchor
fn calculate_position(
    anchor_rect: &web_sys_x::DomRect,
    floating_width: f64,
    floating_height: f64,
    placement: Placement,
    offset: f64,
) -> Position {
    let (top, left) = match placement {
        // Bottom placements
        Placement::BottomStart => (anchor_rect.bottom() + offset, anchor_rect.left()),
        Placement::Bottom => (
            anchor_rect.bottom() + offset,
            anchor_rect.left() + (anchor_rect.width() - floating_width) / 2.0,
        ),
        Placement::BottomEnd => (
            anchor_rect.bottom() + offset,
            anchor_rect.right() - floating_width,
        ),
        // Top placements
        Placement::TopStart => (
            anchor_rect.top() - floating_height - offset,
            anchor_rect.left(),
        ),
        Placement::Top => (
            anchor_rect.top() - floating_height - offset,
            anchor_rect.left() + (anchor_rect.width() - floating_width) / 2.0,
        ),
        Placement::TopEnd => (
            anchor_rect.top() - floating_height - offset,
            anchor_rect.right() - floating_width,
        ),
        // Left placements
        Placement::LeftStart => (
            anchor_rect.top(),
            anchor_rect.left() - floating_width - offset,
        ),
        Placement::Left => (
            anchor_rect.top() + (anchor_rect.height() - floating_height) / 2.0,
            anchor_rect.left() - floating_width - offset,
        ),
        Placement::LeftEnd => (
            anchor_rect.bottom() - floating_height,
            anchor_rect.left() - floating_width - offset,
        ),
        // Right placements
        Placement::RightStart => (anchor_rect.top(), anchor_rect.right() + offset),
        Placement::Right => (
            anchor_rect.top() + (anchor_rect.height() - floating_height) / 2.0,
            anchor_rect.right() + offset,
        ),
        Placement::RightEnd => (
            anchor_rect.bottom() - floating_height,
            anchor_rect.right() + offset,
        ),
    };

    Position { top, left }
}

/// Check if position overflows viewport and needs flipping
fn should_flip(
    pos: &Position,
    floating_width: f64,
    floating_height: f64,
    viewport_width: f64,
    viewport_height: f64,
    placement: Placement,
) -> bool {
    if placement.is_vertical() {
        // Check vertical overflow
        match placement {
            Placement::BottomStart | Placement::Bottom | Placement::BottomEnd => {
                pos.top + floating_height > viewport_height
            }
            Placement::TopStart | Placement::Top | Placement::TopEnd => pos.top < 0.0,
            _ => false,
        }
    } else {
        // Check horizontal overflow
        match placement {
            Placement::RightStart | Placement::Right | Placement::RightEnd => {
                pos.left + floating_width > viewport_width
            }
            Placement::LeftStart | Placement::Left | Placement::LeftEnd => pos.left < 0.0,
            _ => false,
        }
    }
}

/// Shift position to stay within viewport bounds
fn shift_position(
    pos: Position,
    floating_width: f64,
    floating_height: f64,
    viewport_width: f64,
    viewport_height: f64,
) -> Position {
    let left = pos.left.max(0.0).min(viewport_width - floating_width);
    let top = pos.top.max(0.0).min(viewport_height - floating_height);
    Position { top, left }
}

/// Dropdown component that positions content relative to an anchor element
#[component]
pub fn Dropdown(
    /// ID of the anchor element to position relative to
    anchor_id: String,
    /// Controls whether the dropdown is visible
    is_open: ReadSignal<bool>,
    /// Called when the dropdown should close (light dismiss)
    on_close: EventHandler<()>,
    /// Placement relative to anchor (default: BottomStart)
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
    let mut position = use_signal(Position::default);

    // Generate unique ID for popover
    let popover_id = use_hook(|| format!("dropdown-{}", js_sys::Math::random() as u64));
    let popover_id_for_position = popover_id.clone();
    let popover_id_for_visibility = popover_id.clone();
    let popover_id_for_rsx = popover_id.clone();

    // Calculate position when opening
    use_effect(move || {
        if !is_open() {
            return;
        }

        let Some(window) = web_sys_x::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(anchor) = document.get_element_by_id(&anchor_id) else {
            return;
        };
        let Some(popover) = document.get_element_by_id(&popover_id_for_position) else {
            return;
        };

        let anchor_rect = anchor.get_bounding_client_rect();
        let popover_rect = popover.get_bounding_client_rect();

        let viewport_width = window
            .inner_width()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(1920.0);
        let viewport_height = window
            .inner_height()
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(1080.0);

        let floating_width = popover_rect.width();
        let floating_height = popover_rect.height();

        // Calculate initial position
        let mut pos = calculate_position(
            &anchor_rect,
            floating_width,
            floating_height,
            placement,
            offset,
        );

        // Check if we need to flip
        if should_flip(
            &pos,
            floating_width,
            floating_height,
            viewport_width,
            viewport_height,
            placement,
        ) {
            let flipped = placement.flip();
            let flipped_pos = calculate_position(
                &anchor_rect,
                floating_width,
                floating_height,
                flipped,
                offset,
            );

            // Only flip if it actually helps
            if !should_flip(
                &flipped_pos,
                floating_width,
                floating_height,
                viewport_width,
                viewport_height,
                flipped,
            ) {
                pos = flipped_pos;
            }
        }

        // Shift to stay in viewport
        pos = shift_position(
            pos,
            floating_width,
            floating_height,
            viewport_width,
            viewport_height,
        );

        position.set(pos);
    });

    // Control popover visibility
    use_effect(move || {
        let is_open = is_open();

        let Some(window) = web_sys_x::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(element) = document.get_element_by_id(&popover_id_for_visibility) else {
            return;
        };

        if is_open {
            if let Ok(show_popover) = js_sys::Reflect::get(&element, &"showPopover".into()) {
                if let Some(func) = show_popover.dyn_ref::<js_sys::Function>() {
                    let _ = func.call0(&element);
                }
            }
        } else if let Ok(hide_popover) = js_sys::Reflect::get(&element, &"hidePopover".into()) {
            if let Some(func) = hide_popover.dyn_ref::<js_sys::Function>() {
                let _ = func.call0(&element);
            }
        }
    });

    let dropdown_class = class.unwrap_or_default();
    let pos = position();

    rsx! {
        div {
            id: "{popover_id_for_rsx}",
            popover: "auto",
            class: "{dropdown_class}",
            style: "position: fixed; top: {pos.top}px; left: {pos.left}px; margin: 0;",
            ontoggle: move |_| {
                if is_open() {
                    on_close.call(());
                }
            },
            {children}
        }
    }
}

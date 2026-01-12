//! Viewport switcher for responsive testing

use dioxus::prelude::*;

/// Breakpoint definition
#[derive(Clone, Copy, PartialEq)]
pub struct Breakpoint {
    pub name: &'static str,
    pub width: u32, // 0 = full width
}

impl Breakpoint {
    pub const fn new(name: &'static str, width: u32) -> Self {
        Self { name, width }
    }
}

/// Default breakpoints
pub const DEFAULT_BREAKPOINTS: &[Breakpoint] = &[
    Breakpoint::new("Mobile", 375),
    Breakpoint::new("Tablet", 768),
    Breakpoint::new("Desktop", 1280),
    Breakpoint::new("Full", 0),
];

/// Viewport container - just applies width constraint
#[component]
pub fn MockViewport(width: u32, children: Element) -> Element {
    rsx! {
        div {
            class: "bg-gray-950 rounded-lg overflow-hidden",
            style: if width > 0 { format!("width: {}px; margin: 0 auto;", width) } else { String::new() },
            {children}
        }
    }
}

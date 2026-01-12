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

/// Viewport wrapper with breakpoint switching
#[component]
pub fn MockViewport(
    #[props(default = DEFAULT_BREAKPOINTS.to_vec())] breakpoints: Vec<Breakpoint>,
    children: Element,
) -> Element {
    let mut current_width = use_signal(|| 0u32); // 0 = full width

    rsx! {
        div { class: "flex flex-col",
            // Breakpoint buttons
            div { class: "flex gap-1 mb-3",
                for bp in breakpoints {
                    button {
                        class: if current_width() == bp.width { "px-2 py-1 text-xs rounded bg-purple-600 text-white" } else { "px-2 py-1 text-xs rounded bg-gray-700 text-gray-300 hover:bg-gray-600" },
                        onclick: move |_| current_width.set(bp.width),
                        "{bp.name}"
                        if bp.width > 0 {
                            span { class: "ml-1 text-gray-400", "({bp.width}px)" }
                        }
                    }
                }
            }

            // Viewport container
            div {
                class: "bg-gray-950 rounded-lg overflow-hidden",
                style: if current_width() > 0 { format!("width: {}px; margin: 0 auto;", current_width()) } else { String::new() },
                {children}
            }
        }
    }
}

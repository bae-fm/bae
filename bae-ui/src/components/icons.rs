//! Icon components using Lucide icon set (https://lucide.dev)
//!
//! All icons use stroke="currentColor" so they inherit text color from Tailwind classes.
//! Default size is w-4 h-4, override with the `class` prop.

use dioxus::prelude::*;

/// Play icon (triangle pointing right)
#[component]
pub fn PlayIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z" }
        }
    }
}

/// Pause icon (two vertical bars)
#[component]
pub fn PauseIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            rect { x: "14", y: "3", width: "5", height: "18", rx: "1" }
            rect { x: "5", y: "3", width: "5", height: "18", rx: "1" }
        }
    }
}

/// Skip back icon (previous track)
#[component]
pub fn SkipBackIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M17.971 4.285A2 2 0 0 1 21 6v12a2 2 0 0 1-3.029 1.715l-9.997-5.998a2 2 0 0 1-.003-3.432z" }
            path { d: "M3 20V4" }
        }
    }
}

/// Skip forward icon (next track)
#[component]
pub fn SkipForwardIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M21 4v16" }
            path { d: "M6.029 4.285A2 2 0 0 0 3 6v12a2 2 0 0 0 3.029 1.715l9.997-5.998a2 2 0 0 0 .003-3.432z" }
        }
    }
}

/// Menu icon (hamburger - three horizontal lines)
#[component]
pub fn MenuIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M4 5h16" }
            path { d: "M4 12h16" }
            path { d: "M4 19h16" }
        }
    }
}

/// Ellipsis icon (three dots - more options)
#[component]
pub fn EllipsisIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            circle { cx: "12", cy: "12", r: "1" }
            circle { cx: "19", cy: "12", r: "1" }
            circle { cx: "5", cy: "12", r: "1" }
        }
    }
}

/// Music icon (music notes)
#[component]
pub fn MusicIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M9 18V5l12-2v13" }
            circle { cx: "6", cy: "18", r: "3" }
            circle { cx: "18", cy: "16", r: "3" }
        }
    }
}

/// Plus icon (add)
#[component]
pub fn PlusIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M5 12h14" }
            path { d: "M12 5v14" }
        }
    }
}

/// Chevron down icon (dropdown indicator)
#[component]
pub fn ChevronDownIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m6 9 6 6 6-6" }
        }
    }
}

/// Chevron right icon (collapsed indicator)
#[component]
pub fn ChevronRightIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m9 18 6-6-6-6" }
        }
    }
}

/// X icon (close/dismiss)
#[component]
pub fn XIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M18 6 6 18" }
            path { d: "m6 6 12 12" }
        }
    }
}

/// Folder icon
#[component]
pub fn FolderIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" }
        }
    }
}

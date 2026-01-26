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
            rect {
                x: "14",
                y: "3",
                width: "5",
                height: "18",
                rx: "1",
            }
            rect {
                x: "5",
                y: "3",
                width: "5",
                height: "18",
                rx: "1",
            }
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
pub fn ChevronRightIcon(
    #[props(default = "w-4 h-4")] class: &'static str,
    #[props(default = "2")] stroke_width: &'static str,
) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "{stroke_width}",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m9 18 6-6-6-6" }
        }
    }
}

/// Chevron left icon (back navigation)
#[component]
pub fn ChevronLeftIcon(
    #[props(default = "w-4 h-4")] class: &'static str,
    #[props(default = "2")] stroke_width: &'static str,
) -> Element {
    rsx! {
        svg {
            class: "{class}",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "{stroke_width}",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            path { d: "m15 18-6-6 6-6" }
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

/// Arrow left icon (back navigation)
#[component]
pub fn ArrowLeftIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "m12 19-7-7 7-7" }
            path { d: "M19 12H5" }
        }
    }
}

/// Check icon (success/complete)
#[component]
pub fn CheckIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M20 6 9 17l-5-5" }
        }
    }
}

/// Download icon
#[component]
pub fn DownloadIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M12 15V3" }
            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
            path { d: "m7 10 5 5 5-5" }
        }
    }
}

/// File text icon (document with lines)
#[component]
pub fn FileTextIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" }
            path { d: "M14 2v5a1 1 0 0 0 1 1h5" }
            path { d: "M10 9H8" }
            path { d: "M16 13H8" }
            path { d: "M16 17H8" }
        }
    }
}

/// File icon (generic file)
#[component]
pub fn FileIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" }
            path { d: "M14 2v5a1 1 0 0 0 1 1h5" }
        }
    }
}

/// Alert triangle icon (warning/error)
#[component]
pub fn AlertTriangleIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" }
            path { d: "M12 9v4" }
            path { d: "M12 17h.01" }
        }
    }
}

/// Cloud off icon (connection failed)
#[component]
pub fn CloudOffIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "m2 2 20 20" }
            path { d: "M5.782 5.782A7 7 0 0 0 9 19h8.5a4.5 4.5 0 0 0 1.307-.193" }
            path { d: "M21.532 16.5A4.5 4.5 0 0 0 17.5 10h-1.79A7 7 0 0 0 8.689 5.042" }
        }
    }
}

/// Trash icon (delete)
#[component]
pub fn TrashIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M10 11v6" }
            path { d: "M14 11v6" }
            path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" }
            path { d: "M3 6h18" }
            path { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
        }
    }
}

/// Pencil icon (edit)
#[component]
pub fn PencilIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" }
            path { d: "m15 5 4 4" }
        }
    }
}

/// Star icon (favorite/default)
#[component]
pub fn StarIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z" }
        }
    }
}

/// Lock icon (security/encryption)
#[component]
pub fn LockIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            rect {
                x: "3",
                y: "11",
                width: "18",
                height: "11",
                rx: "2",
                ry: "2",
            }
            path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
        }
    }
}

/// Key icon (API keys/credentials)
#[component]
pub fn KeyIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z" }
            circle {
                cx: "16.5",
                cy: "7.5",
                r: ".5",
                fill: "currentColor",
            }
        }
    }
}

/// Upload icon
#[component]
pub fn UploadIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M12 3v12" }
            path { d: "m17 8-5-5-5 5" }
            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
        }
    }
}

/// Disc icon (CD/media)
#[component]
pub fn DiscIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            circle { cx: "12", cy: "12", r: "10" }
            circle { cx: "12", cy: "12", r: "2" }
        }
    }
}

/// Loader icon (spinner - use with animate-spin)
#[component]
pub fn LoaderIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M12 2v4" }
            path { d: "m16.2 7.8 2.9-2.9" }
            path { d: "M18 12h4" }
            path { d: "m16.2 16.2 2.9 2.9" }
            path { d: "M12 18v4" }
            path { d: "m4.9 19.1 2.9-2.9" }
            path { d: "M2 12h4" }
            path { d: "m4.9 4.9 2.9 2.9" }
        }
    }
}

/// External link icon
#[component]
pub fn ExternalLinkIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M15 3h6v6" }
            path { d: "M10 14 21 3" }
            path { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" }
        }
    }
}

/// Refresh icon (retry/reload)
#[component]
pub fn RefreshIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" }
            path { d: "M21 3v5h-5" }
            path { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" }
            path { d: "M8 16H3v5" }
        }
    }
}

/// Rows icon (stacked horizontal lines - for track lists)
#[component]
pub fn RowsIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            rect {
                x: "3",
                y: "3",
                width: "18",
                height: "18",
                rx: "2",
            }
            path { d: "M21 9H3" }
            path { d: "M21 15H3" }
        }
    }
}

/// Info icon (information circle)
#[component]
pub fn InfoIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            circle { cx: "12", cy: "12", r: "10" }
            path { d: "M12 16v-4" }
            path { d: "M12 8h.01" }
        }
    }
}

/// Image icon (picture placeholder - for missing album art)
#[component]
pub fn ImageIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            rect {
                x: "3",
                y: "3",
                width: "18",
                height: "18",
                rx: "2",
                ry: "2",
            }
            circle { cx: "9", cy: "9", r: "2" }
            path { d: "m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" }
        }
    }
}

/// Monitor icon (screen/viewport)
#[component]
pub fn MonitorIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            rect {
                x: "2",
                y: "3",
                width: "20",
                height: "14",
                rx: "2",
            }
            path { d: "M8 21h8" }
            path { d: "M12 17v4" }
        }
    }
}

/// Layers icon (stacked layers - for presets)
#[component]
pub fn LayersIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z" }
            path { d: "m22 12.5-8.58 3.91a2 2 0 0 1-1.66 0L2 12.5" }
            path { d: "m22 17.5-8.58 3.91a2 2 0 0 1-1.66 0L2 17.5" }
        }
    }
}

/// Settings gear icon
#[component]
pub fn SettingsIcon(#[props(default = "w-4 h-4")] class: &'static str) -> Element {
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
            path { d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" }
            circle { cx: "12", cy: "12", r: "3" }
        }
    }
}

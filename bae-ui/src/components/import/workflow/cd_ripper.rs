//! CD ripper view component

use crate::components::{Select, SelectOption};
use crate::display_types::CdDriveInfo;
use dioxus::prelude::*;

/// CD ripper view for selecting a CD drive
#[component]
pub fn CdRipperView(
    /// Whether drives are being scanned
    is_scanning: bool,
    /// List of detected drives
    drives: Vec<CdDriveInfo>,
    /// Currently selected drive path (if any)
    selected_drive: Option<String>,
    /// Called when a drive is selected
    on_drive_select: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "space-y-4",
            if is_scanning {
                div { class: "text-center py-4 text-gray-400", "Scanning for CD drives..." }
            } else {
                div { class: "space-y-4",
                    if drives.is_empty() {
                        div { class: "text-center py-8 text-gray-400", "No CD drives detected" }
                    } else {
                        div { class: "space-y-2",
                            label { class: "block text-sm font-medium text-gray-300",
                                "Select CD Drive"
                            }
                            Select {
                                value: selected_drive.clone().unwrap_or_default(),
                                onchange: move |val: String| {
                                    if !val.is_empty() {
                                        on_drive_select.call(val);
                                    }
                                },
                                for drive in drives.iter() {
                                    SelectOption {
                                        value: "{drive.device_path}",
                                        label: drive.name.clone(),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

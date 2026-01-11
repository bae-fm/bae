//! CD ripper view component

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
                            select {
                                class: "w-full px-3 py-2 border border-gray-600 bg-gray-700 text-white rounded-md shadow-sm focus:outline-none focus:ring-blue-500 focus:border-blue-500",
                                onchange: move |evt| {
                                    let value = evt.value();
                                    if !value.is_empty() {
                                        on_drive_select.call(value);
                                    }
                                },
                                for drive in drives.iter() {
                                    option {
                                        value: "{drive.device_path}",
                                        selected: selected_drive.as_ref().map(|p| p == &drive.device_path).unwrap_or(false),
                                        "{drive.name}"
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

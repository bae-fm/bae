//! CD selector view component

use crate::components::icons::DiscIcon;
use crate::components::{Button, ButtonVariant};
use dioxus::prelude::*;

/// CD drive status
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CdDriveStatus {
    #[default]
    NoDrive,
    NoDisc,
    Reading,
    Ready {
        disc_id: String,
        track_count: u32,
    },
    Ripping {
        progress: u8,
    },
}

/// CD selector view - drive status and rip button
#[component]
pub fn CdSelectorView(
    /// Current drive status
    status: CdDriveStatus,
    /// Called when rip button is clicked
    on_rip_click: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "space-y-4",
            div { class: "bg-gray-800 rounded-lg p-6",
                h3 { class: "text-lg font-medium text-white mb-4", "CD Drive" }

                match &status {
                    CdDriveStatus::NoDrive => rsx! {
                        div { class: "flex items-center justify-between p-4 bg-gray-700 rounded-lg",
                            div { class: "flex items-center gap-3",
                                CdIcon {}
                                div {
                                    p { class: "text-white font-medium", "No CD drive detected" }
                                    p { class: "text-sm text-gray-400", "Connect a CD/DVD drive to import CDs" }
                                }
                            }
                        }
                    },
                    CdDriveStatus::NoDisc => rsx! {
                        div { class: "flex items-center justify-between p-4 bg-gray-700 rounded-lg",
                            div { class: "flex items-center gap-3",
                                CdIcon {}
                                div {
                                    p { class: "text-white font-medium", "No CD detected" }
                                    p { class: "text-sm text-gray-400", "Insert an audio CD to begin" }
                                }
                            }
                            Button { variant: ButtonVariant::Secondary, disabled: true, onclick: |_| {}, "Rip CD" }
                        }
                    },
                    CdDriveStatus::Reading => rsx! {
                        div { class: "flex items-center justify-between p-4 bg-gray-700 rounded-lg",
                            div { class: "flex items-center gap-3",
                                CdIcon {}
                                div {
                                    p { class: "text-white font-medium", "Reading CD..." }
                                    p { class: "text-sm text-gray-400", "Please wait while the disc is being read" }
                                }
                            }
                            div { class: "animate-spin w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full" }
                        }
                    },
                    CdDriveStatus::Ready { disc_id, track_count } => rsx! {
                        div { class: "flex items-center justify-between p-4 bg-gray-700 rounded-lg",
                            div { class: "flex items-center gap-3",
                                CdIcon {}
                                div {
                                    p { class: "text-white font-medium", "Audio CD detected" }
                                    p { class: "text-sm text-gray-400", "{track_count} tracks · DiscID: {disc_id}" }
                                }
                            }
                            Button { onclick: move |_| on_rip_click.call(()), "Rip CD" }
                        }
                    },
                    CdDriveStatus::Ripping { progress } => rsx! {
                        div { class: "p-4 bg-gray-700 rounded-lg",
                            div { class: "flex items-center justify-between mb-3",
                                div { class: "flex items-center gap-3",
                                    CdIcon {}
                                    div {
                                        p { class: "text-white font-medium", "Ripping CD..." }
                                        p { class: "text-sm text-gray-400", "{progress}% complete" }
                                    }
                                }
                            }
                            div { class: "w-full bg-gray-600 rounded-full h-2",
                                div {
                                    class: "bg-blue-500 h-2 rounded-full transition-all duration-300",
                                    style: "width: {progress}%",
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn CdIcon() -> Element {
    rsx! {
        div { class: "w-10 h-10 bg-gray-600 rounded-full flex items-center justify-center",
            DiscIcon { class: "w-6 h-6 text-gray-300" }
        }
    }
}

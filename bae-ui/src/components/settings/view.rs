//! Settings view - tabbed layout shell

use dioxus::prelude::*;

/// Available settings tabs
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SettingsTab {
    #[default]
    StorageProfiles,
    ApiKeys,
    Encryption,
    BitTorrent,
    Subsonic,
    About,
}

impl SettingsTab {
    pub fn label(&self) -> &'static str {
        match self {
            SettingsTab::StorageProfiles => "Storage Profiles",
            SettingsTab::ApiKeys => "API Keys",
            SettingsTab::Encryption => "Encryption",
            SettingsTab::BitTorrent => "BitTorrent",
            SettingsTab::Subsonic => "Subsonic",
            SettingsTab::About => "About",
        }
    }

    pub fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::StorageProfiles,
            SettingsTab::ApiKeys,
            SettingsTab::Encryption,
            SettingsTab::BitTorrent,
            SettingsTab::Subsonic,
            SettingsTab::About,
        ]
    }
}

/// Settings page view with tabbed navigation
#[component]
pub fn SettingsView(
    active_tab: SettingsTab,
    on_tab_change: EventHandler<SettingsTab>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "flex flex-col h-full bg-gray-900",
            div { class: "p-6 border-b border-gray-700",
                h1 { class: "text-2xl font-bold text-white", "Settings" }
            }
            div { class: "flex flex-1 overflow-hidden",
                nav { class: "w-56 bg-gray-800 border-r border-gray-700 p-4 flex-shrink-0",
                    ul { class: "space-y-1",
                        for tab in SettingsTab::all() {
                            li {
                                button {
                                    class: {
                                        let base = "w-full text-left px-4 py-2 rounded-lg text-sm font-medium transition-colors";
                                        if active_tab == *tab {
                                            format!("{} bg-indigo-600 text-white", base)
                                        } else {
                                            format!("{} text-gray-300 hover:bg-gray-700 hover:text-white", base)
                                        }
                                    },
                                    onclick: {
                                        let tab = *tab;
                                        move |_| on_tab_change.call(tab)
                                    },
                                    "{tab.label()}"
                                }
                            }
                        }
                    }
                }
                div { class: "flex-1 overflow-y-auto p-6", {children} }
            }
        }
    }
}

mod about;
mod api_keys;
mod encryption;
mod import_settings;
mod network;
mod storage_profiles;

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SettingsTab {
    StorageProfiles,
    ApiKeys,
    Encryption,
    ImportSettings,
    Network,
    About,
}

impl SettingsTab {
    fn label(&self) -> &'static str {
        match self {
            SettingsTab::StorageProfiles => "Storage Profiles",
            SettingsTab::ApiKeys => "API Keys",
            SettingsTab::Encryption => "Encryption",
            SettingsTab::ImportSettings => "Import",
            SettingsTab::Network => "Network",
            SettingsTab::About => "About",
        }
    }

    fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::StorageProfiles,
            SettingsTab::ApiKeys,
            SettingsTab::Encryption,
            SettingsTab::ImportSettings,
            SettingsTab::Network,
            SettingsTab::About,
        ]
    }
}

/// Settings page with tabbed navigation
#[component]
pub fn Settings() -> Element {
    let mut active_tab = use_signal(|| SettingsTab::StorageProfiles);

    rsx! {
        div { class: "flex flex-col h-full bg-gray-900",
            // Header
            div { class: "p-6 border-b border-gray-700",
                h1 { class: "text-2xl font-bold text-white", "Settings" }
            }

            div { class: "flex flex-1 overflow-hidden",
                // Sidebar navigation
                nav { class: "w-56 bg-gray-800 border-r border-gray-700 p-4 flex-shrink-0",
                    ul { class: "space-y-1",
                        for tab in SettingsTab::all() {
                            li {
                                button {
                                    class: {
                                        let base = "w-full text-left px-4 py-2 rounded-lg text-sm font-medium transition-colors";
                                        if *active_tab.read() == *tab {
                                            format!("{} bg-indigo-600 text-white", base)
                                        } else {
                                            format!("{} text-gray-300 hover:bg-gray-700 hover:text-white", base)
                                        }
                                    },
                                    onclick: move |_| active_tab.set(*tab),
                                    "{tab.label()}"
                                }
                            }
                        }
                    }
                }

                // Content area
                div { class: "flex-1 overflow-y-auto p-6",
                    match *active_tab.read() {
                        SettingsTab::StorageProfiles => rsx! {
                            storage_profiles::StorageProfilesSection {}
                        },
                        SettingsTab::ApiKeys => rsx! {
                            api_keys::ApiKeysSection {}
                        },
                        SettingsTab::Encryption => rsx! {
                            encryption::EncryptionSection {}
                        },
                        SettingsTab::ImportSettings => rsx! {
                            import_settings::ImportSettingsSection {}
                        },
                        SettingsTab::Network => rsx! {
                            network::NetworkSection {}
                        },
                        SettingsTab::About => rsx! {
                            about::AboutSection {}
                        },
                    }
                }
            }
        }
    }
}

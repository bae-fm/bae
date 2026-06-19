import SwiftUI

/// Minimal per-device settings: the library's sync status, and a destructive
/// action to remove the library from this device. Read-only otherwise — v1
/// mobile doesn't edit library config. Presented as a sheet from `LibraryView`.
struct SettingsView: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(AppSessionHolder.self)
    private var holder
    @Environment(Sync.self)
    private var sync
    @Environment(\.dismiss)
    private var dismiss

    @State
    private var confirmLeave = false

    var body: some View {
        NavigationStack {
            List {
                if holder.hasMultipleLibraries {
                    Section("Library") {
                        ForEach(holder.libraries, id: \.id) { library in
                            Button {
                                holder.openLibrary(library)
                            } label: {
                                HStack {
                                    Text(library.name)
                                    Spacer()
                                    // Always in the tree; toggle visibility so an
                                    // active-state change doesn't re-measure rows.
                                    Image(systemName: "checkmark")
                                        .foregroundStyle(Theme.accent)
                                        .opacity(holder.isActive(library) ? 1 : 0)
                                        .allowsHitTesting(false)
                                }
                                .contentShape(Rectangle())
                            }
                            .foregroundStyle(.primary)
                            .disabled(holder.isActive(library))
                        }
                    }
                }

                Section("Sync") {
                    LabeledContent(
                        "Cloud sync",
                        value: configStore.config.sync != nil
                            ? String(localized: "On")
                            : String(localized: "Local only")
                    )
                    if configStore.config.sync != nil {
                        if let syncError = configStore.syncError {
                            LabeledContent(
                                "Status",
                                value: String(localized: "Disconnected")
                            )
                            Text(syncError.line)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Button("Reconnect") { sync.triggerSync() }
                        }
                        else {
                            LabeledContent(
                                "Status",
                                value: configStore.syncReady
                                    ? String(localized: "Synced")
                                    : String(localized: "Syncing\u{2026}")
                            )
                        }
                    }
                }

                Section {
                    Button(role: .destructive) {
                        confirmLeave = true
                    } label: {
                        Text("Remove this library from this device")
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                } footer: {
                    Text(
                        "Removes this library and its downloaded files from this device. Your library in the cloud is untouched — you can re-pair this device later."
                    )
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .confirmationDialog(
                "Remove this library from this device?",
                isPresented: $confirmLeave,
                titleVisibility: .visible
            ) {
                Button("Remove", role: .destructive) {
                    dismiss()
                    holder.forgetActiveLibrary()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Your library in the cloud is untouched.")
            }
        }
    }
}

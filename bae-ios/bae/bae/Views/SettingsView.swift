import SwiftUI

/// Minimal per-device settings: the library's sync status, and a destructive
/// action to remove the library from this device. Read-only otherwise — v1
/// mobile doesn't edit library config. Presented as a sheet from `LibraryView`.
struct SettingsView: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(AppService.self)
    private var appService
    @Environment(AppSessionHolder.self)
    private var holder
    @Environment(Sync.self)
    private var sync
    @Environment(\.dismiss)
    private var dismiss

    @State
    private var confirmLeave = false
    @State
    private var showRecoveryCode = false

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

                Section("Playback") {
                    PauseBetweenSidesToggle(
                        configStore: configStore,
                        appHandle: appService.appHandle,
                        showError: { @MainActor error in
                            configStore.showError(error)
                        }
                    )
                }

                // Managing members and revealing the recovery code both need a
                // live sync session this run (the membership chain lives in the
                // library's cloud storage), so gate on syncReady — runtime status
                // — not merely a configured provider.
                if configStore.syncReady {
                    Section {
                        NavigationLink {
                            MembersView()
                        } label: {
                            Text("Members")
                        }
                    } footer: {
                        Text(
                            "Devices that share this library. Approve a new device, or remove one."
                        )
                    }

                    Section {
                        Button("Show recovery code\u{2026}") {
                            showRecoveryCode = true
                        }
                    } footer: {
                        Text(
                            "Your recovery code restores this library on a new device when you have no other device available to approve it. Anyone with it has full access — keep it secret."
                        )
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
            .sheet(isPresented: $showRecoveryCode) {
                RecoveryCodeView(
                    generate: sync.generateRestoreCode,
                    onDismiss: { showRecoveryCode = false }
                )
            }
            .alert(
                "Error",
                isPresented: Binding(
                    get: { configStore.lastError != nil },
                    set: { if !$0 { configStore.clearError() } },
                )
            ) {
                Button("Close") { configStore.clearError() }
            } message: {
                if let error = configStore.lastError {
                    Text(error.line)
                }
            }
        }
    }
}

import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("Settings")

/// Device appearance, playback, casting, and library settings.
/// Presented as a sheet from LibraryView.
struct SettingsView: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(SyncStatusStore.self)
    private var syncStatusStore
    @Environment(AppSessionHolder.self)
    private var holder
    @Environment(Sync.self)
    private var sync
    @Environment(Playback.self)
    private var playback
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(\.dismiss)
    private var dismiss

    // Mobile defaults to restoring: the app resumes where playback left off
    // unless the user turns this off. Read at the next initApp (app launch).
    @AppStorage("persistPlayback")
    private var persistPlayback = true

    @State
    private var confirmLeave = false
    @State
    private var showRecoveryCode = false
    /// The device an unconfirmed "turn casting off" would disconnect from.
    @State
    private var pendingCastDisconnect: String?

    var body: some View {
        NavigationStack {
            List {
                Section("Appearance") {
                    AppearanceControls()
                }
                if holder.hasMultipleLibraries {
                    Section("Library") {
                        ForEach(holder.libraries, id: \.id) { library in
                            Button {
                                holder.openLibrary(library)
                            } label: {
                                HStack {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(library.name)
                                        // A library whose config won't load stays
                                        // listed — it must not silently vanish.
                                        if let error = library.error {
                                            Text(error)
                                                .font(.caption)
                                                .foregroundStyle(.red)
                                                .lineLimit(2)
                                        }
                                    }
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
                            .disabled(
                                holder.isActive(library) || library.error != nil
                            )
                        }
                    }
                }

                Section {
                    LabeledContent(
                        "Cloud sync",
                        value: configStore.config.sync != nil
                            ? String(localized: "On")
                            : String(localized: "Local only")
                    )
                    if let syncConfig = configStore.config.sync {
                        SyncConnectedControls(
                            config: syncConfig,
                            sync: sync,
                            libraryId: configStore.config.libraryId
                        )
                    }
                } header: {
                    Text("Sync")
                } footer: {
                    if configStore.config.sync != nil {
                        Text(
                            "While paused, changes wait on this device and upload when you resume."
                        )
                    }
                }

                Section {
                    PauseBetweenSidesToggle(
                        configStore: configStore,
                        setEnabled: playback.setPauseBetweenSides,
                        showError: { @MainActor error in
                            configStore.showError(error)
                        }
                    )
                    Toggle("Restore on launch", isOn: $persistPlayback)
                } header: {
                    Text("Playback")
                } footer: {
                    Text(
                        "Restores the last session's track, position, queue, and volume when the app opens."
                    )
                }

                Section {
                    Toggle("Enable casting", isOn: castEnabledBinding)
                } header: {
                    Text("Casting")
                } footer: {
                    Text(
                        "Plays to Cast and AirPlay receivers on your network. While off, bae does not look for devices."
                    )
                }

                // Managing members and revealing the recovery code both need a
                // live sync session this run (the membership chain lives in the
                // library's cloud storage), so gate on syncReady — runtime status
                // — not merely a configured provider.
                if syncStatusStore.syncReady {
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
                            "Your recovery code restores this library on a new device when you have no other device available to approve it. Anyone with it has full access. Keep it secret."
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
                        "Removes this library and its downloaded files from this device. Your library in the cloud is untouched. You can re-pair this device later."
                    )
                }

                Section("About") {
                    LabeledContent("Version", value: Self.appVersion)
                }
            }
            .scrollContentBackground(.hidden)
            .windowBackground()
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
            .alert(
                "Turn off casting?",
                isPresented: Binding(
                    get: { pendingCastDisconnect != nil },
                    set: { presented in
                        if !presented { pendingCastDisconnect = nil }
                    }
                ),
                presenting: pendingCastDisconnect
            ) { _ in
                Button("Turn Off", role: .destructive) { setCastEnabled(false) }
                Button("Cancel", role: .cancel) {}
            } message: { device in
                Text("This will stop casting to \(device).")
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
            .onAppear { holder.reportScreen(.settings) }
        }
    }

    /// Reads the persisted setting and writes through the bridge — the config
    /// subscription is what moves the switch, so a refused or cancelled flip
    /// leaves it where it was with nothing to undo.
    private var castEnabledBinding: Binding<Bool> {
        Binding(
            get: { configStore.config.castEnabled },
            set: { enabled in
                switch Cast.toggleAction(
                    enabled: enabled,
                    castingDeviceName: castStore.castingDeviceName
                ) {
                case .apply(let enabled): setCastEnabled(enabled)
                case .confirmDisconnect(let device):
                    pendingCastDisconnect = device
                }
            }
        )
    }

    private func setCastEnabled(_ enabled: Bool) {
        do {
            try cast.setEnabled(enabled)
        }
        catch {
            configStore.showError(error)
        }
    }
}

/// The connected-provider controls in the Sync section: provider details, the
/// disconnect flow, the live sync status rows, and the upload-pause toggle.
/// Split into its own view so it can seed the `DisconnectSyncFlow` model as
/// `@State` from the sync service and library id — values a parent can't read at
/// `@State` init time because they come from the environment.
private struct SyncConnectedControls: View {
    let config: BridgeSyncConfig

    @Environment(SyncStatusStore.self)
    private var syncStatusStore
    @Environment(OutboxStore.self)
    private var outboxStore

    @State
    private var flow: DisconnectSyncFlow
    @State
    private var reconnecting = false

    private let sync: Sync

    init(config: BridgeSyncConfig, sync: Sync, libraryId: String) {
        self.config = config
        self.sync = sync
        _flow = State(
            initialValue: DisconnectSyncFlow(
                cloudOnlyReleaseCount: sync.cloudOnlyReleaseCount,
                atRiskMessage: { count in
                    String.localizedStringWithFormat(
                        NSLocalizedString(
                            "core.sync.cloud_only_releases",
                            tableName: "Core",
                            bundle: .main,
                            comment: ""
                        ),
                        count
                    )
                },
                disconnect: sync.disconnectCloudProvider,
                deleteRestoreCode: {
                    try KeychainService.deleteRestoreCode(libraryId: libraryId)
                },
                baseMessage: {
                    String(
                        localized:
                            "This will stop syncing and remove the cloud provider configuration."
                    )
                        + " "
                        + String(
                            localized:
                                "To sync this library again, pair this device from another device."
                        )
                },
                warningCheckFailedMessage: {
                    String(
                        localized:
                            "Couldn't check for cloud-only releases: \($0)"
                    )
                },
                disconnectFailedMessage: {
                    String(localized: "Failed to disconnect: \($0)")
                },
                restoreCodeDeleteFailedMessage: {
                    String(
                        localized:
                            "Disconnected, but couldn't remove the restore code: \($0)"
                    )
                }
            )
        )
    }

    var body: some View {
        @Bindable
        var flow = flow
        Group {
            CloudProviderConnectedSection(
                config: config,
                onDisconnect: { flow.promptDisconnect() }
            )

            if let syncError = syncStatusStore.error {
                if syncStatusStore.canReconnect {
                    LabeledContent(
                        "Status",
                        value: String(localized: "Disconnected")
                    )
                }
                Text(syncError.line)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                // The line above names a category; this names the fault. Without
                // it a failing cycle reads as "Something went wrong." and the
                // reason lives only in the device log.
                if let fault = syncError.detailSummary {
                    Text(fault)
                        .font(.caption2.monospaced())
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                if syncStatusStore.canReconnect {
                    if reconnecting {
                        ProgressView()
                    }
                    else {
                        Button("Reconnect") { Task { await reconnect() } }
                    }
                }
            }
            else {
                LabeledContent(
                    "Status",
                    value: SyncIndicatorLabel.text(syncStatusStore.indicator)
                )
            }

            BlockedSyncOperationRows(retry: sync.retryBlockedSyncOperation)

            Toggle(
                "Pause uploads",
                isOn: Binding(
                    get: { outboxStore.snapshot.pauseRequested },
                    set: { paused in
                        Task { try await sync.setSyncPaused(paused) }
                    }
                )
            )

            if let error = flow.error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
        }
        .confirmationDialog(
            "Disconnect sync?",
            isPresented: $flow.showConfirm,
            titleVisibility: .visible
        ) {
            Button("Disconnect", role: .destructive) {
                Task { await flow.confirm() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(flow.message)
        }
        .onDisappear { flow.cancelWarningTask() }
    }

    /// Retry the provider this library is already configured for. The provider
    /// config and its keyring credentials survive a failure, so a failed sync is
    /// retried rather than set up again — and unlike a bare sync wake, this also
    /// connects when a failed launch left no connection to wake.
    ///
    /// A failed retry is recorded as the sync-status error, which is the
    /// `syncStatusStore.error` line right above this button: it re-appears
    /// naming the new reason, so the failure's display path is the row the user
    /// tapped in.
    private func reconnect() async {
        reconnecting = true
        do {
            try await sync.reconnectSync()
        }
        catch {
            logger.error(
                "Sync reconnect failed: \(error.localizedDescription)"
            )
        }
        reconnecting = false
    }
}

extension SettingsView {
    /// The marketing version and build number from the app bundle, e.g.
    /// "1.2 (345)". Both Info.plist keys are stamped by every build; a missing
    /// one is a packaging bug and fails loud.
    fileprivate static var appVersion: String {
        let info = Bundle.main
        guard
            let short = info.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
            let build = info.object(forInfoDictionaryKey: "CFBundleVersion") as? String
        else {
            preconditionFailure("the app bundle always stamps its version keys")
        }
        return "\(short) (\(build))"
    }
}

#if DEBUG
#Preview {
    SettingsView()
        .previewStores()
}
#endif

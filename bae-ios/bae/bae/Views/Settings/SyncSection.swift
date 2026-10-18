import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("SyncSection")

/// The Sync section for a library with a cloud home: where it is stored and
/// the disconnect, the one status line — "Synced", or "Waiting: 3 items" —
/// that opens what coven is waiting on, and the upload pause.
struct SyncSection: View {
    let location: BridgeStorageLocation

    @Environment(SyncStatusStore.self)
    private var status
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(Sync.self)
    private var sync
    @Environment(ConfigStore.self)
    private var configStore

    @State
    private var reconnectTask: Task<Void, Never>?

    var body: some View {
        Section {
            ConnectedStorageControls(
                location: location,
                sync: sync,
                configStore: configStore
            )
            NavigationLink {
                PendingRecordsScreen(location: location)
            } label: {
                LabeledContent("Status", value: status.statusText)
            }
            if !status.syncReady {
                Button("Reconnect") { reconnect() }
                    .disabled(reconnectTask != nil)
            }
            if let error = status.error {
                ErrorDetailDisclosure(error: error)
            }
            Toggle(
                "Pause uploads",
                isOn: Binding(
                    get: { outboxStore.snapshot.pauseRequested },
                    set: { paused in setPaused(paused) }
                )
            )
        } header: {
            Text("Sync")
        } footer: {
            Text(
                "While paused, changes wait on this device and upload when you resume."
            )
        }
        .onDisappear { reconnectTask?.cancel() }
    }

    private func reconnect() {
        reconnectTask = Task {
            defer { reconnectTask = nil }
            do { try await sync.reconnectSync() }
            catch {
                // The failure reaches the status line as its error.
                logger.error("Reconnect failed: \(error.localizedDescription)")
            }
        }
    }

    private func setPaused(_ paused: Bool) {
        Task {
            do { try await sync.setSyncPaused(paused) }
            catch {
                logger.error(
                    "Failed to set upload pause: \(error.localizedDescription)"
                )
                configStore.showError(error)
            }
        }
    }
}

/// Where the library is stored and the disconnect flow; its own view so its
/// init can seed `DisconnectSyncFlow`.
private struct ConnectedStorageControls: View {
    let location: BridgeStorageLocation

    @State
    private var flow: DisconnectSyncFlow

    init(
        location: BridgeStorageLocation,
        sync: Sync,
        configStore: ConfigStore
    ) {
        self.location = location
        _flow = State(
            initialValue: DisconnectSyncFlow(
                cloudOnlyReleaseCount: sync.cloudOnlyReleaseCount,
                atRiskMessage: { count in
                    String.localizedStringWithFormat(
                        localizedCoreString("core.sync.cloud_only_releases"),
                        count
                    )
                },
                disconnect: { [sync, configStore] in
                    try await sync.disconnectCloudProvider()
                    configStore.applyStorage(try await sync.storageLocation())
                },
                baseMessage: {
                    String(
                        localized:
                            "This device stops syncing and removes its storage credentials. Sign in again to resume syncing."
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
                }
            )
        )
    }

    var body: some View {
        @Bindable
        var flow = flow
        Group {
            CloudProviderConnectedSection(
                location: location,
                onDisconnect: { flow.promptDisconnect() }
            )
            if let error = flow.error {
                ErrorText(error)
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
}

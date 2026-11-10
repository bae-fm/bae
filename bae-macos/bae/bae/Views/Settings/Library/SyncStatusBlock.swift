import AppKit
import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("SyncStatusBlock")

/// What the buttons of a sync state's fix hand back to the settings window.
struct SyncFixActions {
    /// Shows the device-reset warning for the active library.
    let onResetDevice: () -> Void
    /// Asks Sparkle for a newer version of the app.
    let onCheckForUpdates: () -> Void
    /// Asks whether to remove the active library from this Mac.
    let onRemoveLibrary: () -> Void
}

/// The top of the Sync page: the state in words and the color core gives it,
/// then its cause and its fix, each only when core sends one. Until the status is first read it shows nothing.
struct SyncStatusBlock: View {
    /// The library's storage, which a fix connects again or opens.
    let location: BridgeStorageLocation?
    let actions: SyncFixActions

    @Environment(SyncStatusStore.self)
    private var status

    var body: some View {
        if status.unreadable {
            LabeledContent("Status") {
                Text("Couldn't read sync status")
                    .foregroundStyle(StatusTone.danger.color)
            }
        }
        else if let shown = status.shown {
            LabeledContent("Status") {
                HStack(spacing: ThemeSpace.inline) {
                    if shown.status == .synced {
                        Circle()
                            .fill(StatusTone.success.color)
                            .frame(width: 8, height: 8)
                    }
                    Text(shown.statusLine)
                        .foregroundStyle(shown.statusTone.color)
                }
            }
            if let cause = shown.causeLine {
                Text(cause)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            if let fix = shown.fix {
                SyncFixButton(fix: fix, location: location, actions: actions)
            }
        }
    }
}

/// When sync last finished, in a box of its own under the status block, only
/// when core sends it.
struct SyncLastSyncedSection: View {
    @Environment(SyncStatusStore.self)
    private var status

    var body: some View {
        if let lastSynced = status.shown?.lastSyncedLine {
            Section {
                LabeledContent("Last synced", value: lastSynced)
            }
        }
    }
}

/// The button of a sync state's fix, and the progress or failure of what it
/// runs. The words of a button are the Mac's own; core says only which fix.
struct SyncFixButton: View {
    let fix: BridgeSyncFix
    let location: BridgeStorageLocation?
    let actions: SyncFixActions

    @Environment(Sync.self)
    private var sync
    @Environment(\.openURL)
    private var openURL
    #if BAE_OAUTH_PROVIDERS || BAE_CLOUDKIT
        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup
    #endif

    @State
    private var reconnection = StorageReconnection()
    @State
    private var askingForKey = false
    @State
    private var error: DisplayError?

    var body: some View {
        Button(title, action: run)
            .buttonStyle(.borderedProminent)
            .disabled(reconnection.isRunning)
            .sheet(isPresented: $askingForKey) {
                if let location {
                    S3KeySheet(
                        location: location,
                        onDismiss: { askingForKey = false }
                    )
                }
            }
            .onDisappear { reconnection.cancel() }
        if let steps = reconnection.steps {
            StepProgressLine(steps: steps.steps) { EmptyView() }
        }
        if let error = error ?? reconnection.error {
            Notice(error: error)
        }
    }

    private var title: String {
        switch fix {
        case .resumeUploads:
            localizedCoreString("core.pending.action.resume_uploads")
        case .retryNow:
            localizedCoreString("core.activity.retry_now")
        case .reconnect:
            localizedCoreString("core.pending.action.reconnect")
        case .checkForUpdates:
            String(localized: "Check for updates…")
        case .signIn(let provider):
            String(localized: "Sign in to \(provider.displayName)…")
        case .enterAccessKey:
            String(localized: "Enter access key…")
        case .openSystemSettings:
            String(localized: "Open System Settings…")
        case .resetDevice:
            localizedCoreString("core.sync.reset_device.confirm")
        case .removeFromThisDevice:
            String(localized: "Remove from this Mac…")
        case .openInBrowser:
            String(localized: "Open in browser")
        }
    }

    private func run() {
        error = nil
        switch fix {
        case .resumeUploads:
            attempt { try await sync.setSyncPaused(false) }
        case .retryNow:
            attempt {
                error = try await sync.nudges.retryNow().failure
            }
        case .reconnect:
            attempt { try await sync.reconnectSync() }
        case .checkForUpdates:
            actions.onCheckForUpdates()
        case .signIn:
            #if BAE_OAUTH_PROVIDERS
                reconnection.run(sync: sync) {
                    try await cloudSyncSetup.signInAgain($0)
                }
            #else
                preconditionFailure("sign-in needs the OAuth build")
            #endif
        case .enterAccessKey:
            askingForKey = true
        case .openSystemSettings:
            openSystemSettings()
        case .resetDevice:
            actions.onResetDevice()
        case .removeFromThisDevice:
            actions.onRemoveLibrary()
        case .openInBrowser:
            openFolder()
        }
    }

    /// Run `operation`, showing its failure after.
    private func attempt(_ operation: @escaping () async throws -> Void) {
        Task {
            do { try await operation() }
            catch {
                logger.error("\(String(reflecting: error))")
                self.error = DisplayError(error)
            }
        }
    }

    /// Open the system's sign-in for iCloud, and connect again once the
    /// person comes back to bae.
    private func openSystemSettings() {
        let settings = URL(
            string:
                "x-apple.systempreferences:com.apple.systempreferences.AppleIDSettings"
        )!
        NSWorkspace.shared.open(settings)
        #if BAE_CLOUDKIT
            reconnection.run(
                sync: sync,
                after: NSApplication.didBecomeActiveNotification
            ) {
                try await cloudSyncSetup.useCloudKit($0)
            }
        #endif
    }

    private func openFolder() {
        // coven records only an https address, so one Foundation can't read
        // is a broken guarantee.
        guard let folder = location?.folderLabel else { return }
        guard let url = URL(string: folder.webUrl) else {
            let detail =
                "the folder's web address isn't a URL: \(folder.webUrl)"
            logger.error("\(detail)")
            CoreBugReporter()
                .report(
                    operation: "open the folder in the browser",
                    detail: detail
                )
            error = DisplayError(
                line: localizedCoreString("core.error.category.internal")
            )
            return
        }
        openURL(url) { opened in
            guard !opened else { return }
            logger.warning("no app opened the folder's web address \(url)")
            error = DisplayError(
                line: String(
                    localized: "No app on this device opened the folder's page."
                )
            )
        }
    }
}

/// A reconnect of the library's recorded storage: its steps while it runs and
/// its failure after. The Sync page comes back on its own once the location
/// is connected.
@MainActor
@Observable
final class StorageReconnection {
    private(set) var steps: StorageSetupFollowing?
    private(set) var error: DisplayError?
    private var task: Task<Void, Never>?

    var isRunning: Bool { task != nil }

    /// Run `operation`, once `notification` has been posted when one is
    /// given.
    func run(
        sync: Sync,
        after notification: Notification.Name? = nil,
        _ operation:
            @escaping (BridgeLibrarySetup) async throws -> BridgeStorageSetup
    ) {
        error = nil
        let running = sync.followStorageSetup()
        steps = running
        task = Task {
            defer {
                task = nil
                steps = nil
            }
            do {
                if let notification {
                    for await _ in NotificationCenter.default.notifications(
                        named: notification
                    ) {
                        break
                    }
                }
                _ = try await operation(running.setup)
            }
            catch is CancellationError {
                logger.debug("Signing in to storage again cancelled")
            }
            catch {
                logger.error(
                    "Signing in to storage again failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }

    func cancel() {
        task?.cancel()
    }
}

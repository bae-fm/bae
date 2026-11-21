import AppKit
import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("MacSyncStatus")

/// What the buttons of a sync state's fix hand back to the settings window.
struct SyncFixActions {
    /// Shows the screen that offers resetting this device for the active
    /// library.
    let onResetDevice: () -> Void
    /// Closes the active library and asks for the person's new restore code,
    /// after another of their devices made it.
    let onRestoreFromCode: () -> Void
    /// Asks Sparkle for a newer version of the app.
    let onCheckForUpdates: () -> Void
    /// Asks whether to remove the active library from this Mac.
    let onRemoveLibrary: () -> Void
    /// Opens the S3 key sheet, for a new key for the library's S3 storage.
    let onEnterAccessKey: () -> Void
}

/// The top of the Sync page: the state in words and the color core gives it,
/// then its cause with any countdown under it, its fix and when sync last
/// finished, each only when core sends one. Until the status is first read
/// it shows nothing.
struct MacSyncStatus: View {
    /// The library's storage, whose folder a fix opens.
    let location: BridgeStorageLocation?
    let actions: SyncFixActions

    @Environment(SyncStatusStore.self)
    private var status

    var body: some View {
        if status.unreadable {
            MacRow("Status") {
                ToneDotValue(
                    text: String(localized: "Could not read sync status"),
                    tone: .danger
                )
            }
        }
        else if let shown = status.shown {
            // One row: the state, its cause and its fix are a single box
            // with no line between them, and only Last synced sits under a
            // divider.
            VStack(
                alignment: .leading,
                spacing: ThemeSpace.related + ThemeSpace.line
            ) {
                MacRow("Status") {
                    ToneDotValue(
                        text: shown.statusLine,
                        tone: shown.statusTone
                    )
                }
                if let cause = shown.causeLine {
                    VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                        SyncCauseText(text: cause)
                        if let countdown = shown.causeCountdown {
                            CountdownTimeline(countdown) { line in
                                SyncCauseText(text: line)
                                    .monospacedDigit()
                                    .accessibilityAddTraits(.updatesFrequently)
                            }
                        }
                    }
                }
                if let fix = shown.fix {
                    SyncFixButton(
                        fix: fix,
                        location: location,
                        actions: actions
                    )
                }
            }
            if let lastSynced = shown.lastSyncedLine {
                MacRow("Last synced", value: lastSynced)
            }
        }
    }
}

/// A line of a sync state's cause, in the block's quieter text.
private struct SyncCauseText: View {
    let text: String

    var body: some View {
        Text(text)
            .themeText(.detail)
            .lineSpacing(ThemeSpace.line)
            .foregroundStyle(Theme.macTextSecondary)
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
    private var error: DisplayError?

    var body: some View {
        MacButton(verbatim: title, kind: .primary, action: run)
            .disabled(reconnection.isRunning)
            .onDisappear { reconnection.cancel() }
        if let steps = reconnection.steps {
            MacProgressContainer(steps: steps.steps) { EmptyView() }
        }
        if let error = error ?? reconnection.error {
            MacNotice(error: error)
        }
    }

    private var title: String {
        switch fix {
        case .retryNow:
            localizedCoreString("core.activity.retry_now")
        case .reconnect:
            localizedCoreString("core.pending.action.reconnect")
        case .checkForUpdates:
            String(localized: "Check for updates…")
        case .signIn(let provider):
            String(localized: "Sign in to \(provider.displayName)…")
        case .enterAccessKey:
            String(localized: "Sign in to S3…")
        case .openSystemSettings:
            String(localized: "Open System Settings…")
        case .resetDevice:
            localizedCoreString("core.sync.reset_from_cloud")
        case .enterRestoreCode:
            localizedCoreString("core.sync.enter_restore_code")
        case .removeFromThisDevice:
            String(localized: "Remove from this Mac…")
        case .openInBrowser:
            String(localized: "Open in browser")
        }
    }

    private func run() {
        error = nil
        switch fix {
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
            actions.onEnterAccessKey()
        case .openSystemSettings:
            openSystemSettings()
        case .resetDevice:
            actions.onResetDevice()
        case .enterRestoreCode:
            actions.onRestoreFromCode()
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
                    localized: "No app on this device opened the folder’s page."
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

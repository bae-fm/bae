import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("SyncStatusLine")

/// The one line Settings shows for sync — what it is doing, or "Waiting: 3 items" —
/// and, opened, each thing coven is waiting on with its action where the
/// person can act; under it the last failure (`SyncFailureLines`).
struct SyncStatusLine: View {
    /// The library's cloud home, whose S3 access key the key sheet replaces.
    let location: BridgeStorageLocation
    /// Offered when sync found another copy of this device.
    let onResetDevice: () -> Void
    let onRestoreFromCode: () -> Void
    let onCheckForUpdates: () -> Void

    @Environment(SyncStatusStore.self)
    private var status
    @Environment(Sync.self)
    private var sync
    #if BAE_OAUTH_PROVIDERS
        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup
    #endif

    @State
    private var expanded = false
    /// The pending record whose action needs more than one step.
    @State
    private var sheet: PendingActionSheet?
    /// The reconnect a sign-in runs; nil while none runs.
    @State
    private var signInSteps: StorageSetupFollowing?

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            // What is waiting is a local read; nothing shows until it lands.
            if let pending = status.pending {
                PendingRecordsView(records: pending, perform: perform)
            }
        } label: {
            if let statusText = status.statusText {
                Text(statusText)
            }
        }
        .sheet(item: $sheet) { sheet in
            PendingActionSheetView(
                sheet: sheet,
                location: location,
                onDismiss: { self.sheet = nil }
            )
        }
        if let signInSteps {
            StepProgressLine(steps: signInSteps.steps) { EmptyView() }
        }
        SyncFailureLines(onResetDevice: onResetDevice)
    }

    private func perform(_ record: BridgePendingRecord) async throws {
        if let sheet = PendingActionSheet(record) {
            self.sheet = sheet
            return
        }
        switch record.action {
        case .updateApp:
            onCheckForUpdates()
        case .signIn(let provider):
            try await signIn(provider)
        case .reconnect:
            try await sync.reconnectSync()
        case .resumeUploads:
            try await sync.setSyncPaused(false)
        case .retryOperation(let id):
            try await sync.pendingActions.retryOperation(id)
        case .retryDownload(let releaseId):
            try await sync.pendingActions.retryDownload(releaseId)
        case .cancelUpload(let id):
            try await sync.pendingActions.cancelFailedMove(id)
        case .restoreFromCode:
            onRestoreFromCode()
        case .wait, .restoreFile, .review:
            preconditionFailure(
                "a record without a button, or one its sheet handles"
            )
        }
    }

    /// Sign in to the provider again and reconnect this library's location with
    /// the new sign-in. An S3 library takes a new access key instead.
    private func signIn(_ provider: BridgeCloudProvider) async throws {
        switch StorageReauthorization(provider: provider) {
        case .s3Key:
            sheet = .s3Key
        case .accountSignIn:
            #if BAE_OAUTH_PROVIDERS
                let steps = sync.followStorageSetup()
                signInSteps = steps
                defer { signInSteps = nil }
                _ = try await cloudSyncSetup.signInAgain(steps.setup)
            #else
                preconditionFailure("\(provider) sign-in needs the OAuth build")
            #endif
        case .systemAccount:
            try await sync.reconnectSync()
        }
    }
}

/// The last sync failure, under the status line or under Sign In where this
/// device has no storage set up: Reconnect where a start failed with storage
/// set up, and Reset device where sync found another copy of this device.
struct SyncFailureLines: View {
    /// Offered when sync found another copy of this device.
    let onResetDevice: () -> Void

    @Environment(SyncStatusStore.self)
    private var status
    @Environment(Sync.self)
    private var sync

    /// The Reconnect running after a failed start; nil while none runs.
    @State
    private var reconnectTask: Task<Void, Never>?

    var body: some View {
        if let error = status.error {
            ErrorDetailDisclosure(error: error)
        }
        if status.offersReconnect {
            Button(localizedCoreString("core.pending.action.reconnect")) {
                reconnect()
            }
            .disabled(reconnectTask != nil)
        }
        if status.offersDeviceReset {
            Button(
                localizedCoreString("core.sync.reset_device.confirm"),
                role: .destructive,
                action: onResetDevice
            )
        }
    }

    /// Start sync again after a failed start. A start that fails again is
    /// recorded as the status's failure, which shows in place of the last
    /// one.
    private func reconnect() {
        reconnectTask = Task {
            defer { reconnectTask = nil }
            do { try await sync.reconnectSync() }
            catch {
                logger.error("Reconnect failed: \(String(reflecting: error))")
            }
        }
    }
}

/// A storage setup that ended undone with no setup screen there to hear it,
/// such as one bae was stopped partway through: why, and Dismiss.
struct UndoneSetupLine: View {
    @Environment(SyncStatusStore.self)
    private var status
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    /// The Dismiss running; nil while none runs.
    @State
    private var dismissTask: Task<Void, Never>?

    var body: some View {
        if let undone = status.snapshot?.undoneSetup,
            let error = DisplayError(undone.error)
        {
            ErrorDetailDisclosure(error: error)
            Button("Dismiss") { dismiss(undone.operation) }
                .disabled(dismissTask != nil)
        }
    }

    private func dismiss(_ operation: Int64) {
        dismissTask = Task {
            defer { dismissTask = nil }
            do { try await sync.dismissUndoneStorageSetup(operation) }
            catch {
                logger.error(
                    "Dismissing the undone setup failed: \(String(reflecting: error))"
                )
                uiStore.showError(error)
            }
        }
    }
}

private struct PendingActionSheetView: View {
    let sheet: PendingActionSheet
    let location: BridgeStorageLocation
    let onDismiss: () -> Void

    var body: some View {
        switch sheet {
        case .s3Key:
            S3KeySheet(location: location, onDismiss: onDismiss)
        case .restoreFile(let record):
            RestoreFileSheet(record: record, onDismiss: onDismiss)
        case .review(let record):
            PendingReviewSheet(record: record, onDismiss: onDismiss)
        }
    }
}

/// A source file that moved before it uploaded: put it back, then retry.
private struct RestoreFileSheet: View {
    let record: BridgePendingRecord
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text(localizedCoreString(record.reason.key))
                .themeText(.heading)
            Text(
                "Put the file back where it was when it was imported, then retry."
            )
            .themeText(.body)
            .foregroundStyle(.secondary)
            if let detail = record.reason.detail {
                Text(detail)
                    .themeText(.mono)
                    .textSelection(.enabled)
            }
            if let error {
                ErrorDetailDisclosure(error: error)
            }
            MacSheetFooter {
                Button("Cancel", action: onDismiss)
                    .keyboardShortcut(.cancelAction)
                Button("Retry") { retry() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(task != nil)
            }
        }
        .padding()
        .frame(width: 440)
        .onDisappear { task?.cancel() }
    }

    private func retry() {
        error = nil
        task = Task {
            defer { task = nil }
            do {
                try await sync.retryOutbox()
                onDismiss()
            }
            catch is CancellationError {
                logger.debug("upload retry cancelled")
            }
            catch {
                self.error = DisplayError(error)
            }
        }
    }
}

/// Everything coven recorded about a pending record the person can only
/// read: the reason, the error behind it while one is held, and any causes
/// behind it.
private struct PendingReviewSheet: View {
    let record: BridgePendingRecord
    let onDismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            reason(record.reason)
            ForEach(Array(record.reason.causes.enumerated()), id: \.offset) {
                _,
                cause in
                reason(cause)
            }
            MacSheetFooter {
                Button("Done", action: onDismiss)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 480)
    }

    private func reason(_ reason: BridgePendingReason) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text(localizedCoreString(reason.key))
                .themeText(.heading)
            if let detail = reason.detail {
                Text(detail)
                    .themeText(.mono)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
    }
}

/// A new S3 access key for this library's bucket, after the old one stopped
/// working or this Mac lost it. Setup checks it against the recorded location
/// before keeping it.
struct S3KeySheet: View {
    let location: BridgeStorageLocation
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var accessKey = ""
    @State
    private var secretKey = ""
    /// The reconnect; a cancelled one closes the sheet once its call ends.
    @State
    private var run: StorageSetupRun
    @State
    private var error: DisplayError?

    init(location: BridgeStorageLocation, onDismiss: @escaping () -> Void) {
        self.location = location
        self.onDismiss = onDismiss
        _run = State(initialValue: StorageSetupRun(onCancelled: onDismiss))
    }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    TextField("Access key", text: $accessKey)
                    SecureField("Secret key", text: $secretKey)
                } footer: {
                    Text("Enter an access key for this library's bucket.")
                }
            }
            .formStyle(.grouped)
            if run.isRunning {
                StorageSetupRunLine(run: run)
                    .padding(.horizontal)
            }
            if let error {
                ErrorDetailDisclosure(error: error)
                    .padding(.horizontal)
            }
            MacSheetFooter {
                if !run.isRunning || run.cancelOffered {
                    Button("Cancel", action: cancel)
                        .keyboardShortcut(.cancelAction)
                }
                Button("Connect") { connect() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(
                        run.isRunning || accessKey.isEmpty
                            || secretKey.isEmpty
                    )
            }
            .padding([.horizontal, .bottom])
        }
        .frame(width: 440)
        .onDisappear { run.leave() }
    }

    /// Close the sheet. A running reconnect is cancelled first, and the
    /// sheet closes once coven has undone it.
    private func cancel() {
        if run.isRunning {
            run.cancel(sync)
        }
        else {
            onDismiss()
        }
    }

    private func connect() {
        let config = location.replacingS3Key(
            accessKey: accessKey,
            secretKey: secretKey
        )
        error = nil
        run.start(
            sync.followStorageSetup(),
            operation: { [sync] setup in
                try await sync.saveSyncConfig(config, setup)
            },
            onConnected: { _ in onDismiss() },
            onFailed: { error = $0 }
        )
    }
}

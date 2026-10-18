import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("SyncStatusLine")

/// The one line Settings shows for sync — "Synced", or "Waiting: 3 items" —
/// and, opened, each thing coven is waiting on with its action where the
/// person can act.
struct SyncStatusLine: View {
    /// The library's cloud home, which a sign-in reconnects to.
    let location: BridgeStorageLocation
    let onResetDevice: () -> Void
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
    @State
    private var reconnectTask: Task<Void, Never>?

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            PendingRecordsView(records: status.pending, perform: perform)
        } label: {
            HStack {
                Text(statusText)
                Spacer()
                if !status.syncReady {
                    Button("Reconnect") { reconnect() }
                        .disabled(reconnectTask != nil)
                }
            }
        }
        .sheet(item: $sheet) { sheet in
            PendingActionSheetView(
                sheet: sheet,
                location: location,
                onDismiss: { self.sheet = nil }
            )
        }
        if let error = status.error {
            ErrorDetailDisclosure(error: error)
        }
    }

    private var statusText: String {
        guard let key = bridgeSyncIndicatorKey(indicator: status.indicator)
        else {
            return String(localized: "Not connected")
        }
        if case .waiting(let items) = status.indicator {
            return String.localizedStringWithFormat(
                localizedCoreString(key),
                Int(items)
            )
        }
        return localizedCoreString(key)
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
            try await sync.retryPendingOperation(id)
        case .resetDevice:
            onResetDevice()
        case .wait, .restoreFile, .provideAdminAccess, .deleteAccessKey,
            .review:
            preconditionFailure(
                "a record without a button, or one its sheet handles"
            )
        }
    }

    /// Sign in to the provider again and reconnect this library's location with
    /// the new sign-in. An S3 library takes a new access key instead.
    private func signIn(_ provider: BridgeCloudProvider) async throws {
        switch location {
        case .s3:
            sheet = .s3Key
        case .dropbox(let folder):
            #if BAE_OAUTH_PROVIDERS
                let access: BridgeDropboxAccess =
                    switch folder {
                    case .appFolder: .appFolder
                    case .sharedFolder: .fullDropbox
                    }
                try await cloudSyncSetup.authenticate(.dropbox(access: access))
                _ = try await cloudSyncSetup.connect(location)
            #else
                preconditionFailure("\(provider) sign-in needs the OAuth build")
            #endif
        case .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                try await cloudSyncSetup.authenticate(.oneDrive)
                _ = try await cloudSyncSetup.connect(location)
            #else
                preconditionFailure("\(provider) sign-in needs the OAuth build")
            #endif
        case .cloudKit:
            // iCloud signs in through System Settings; reconnecting picks the
            // account up.
            try await sync.reconnectSync()
        }
    }
}

/// A pending record's action that asks for more before it runs.
enum PendingActionSheet: Identifiable {
    case adminAccess(AdminAccessRequest)
    case s3Key
    case restoreFile(BridgePendingRecord)
    case review(BridgePendingRecord)

    /// The sheet a record's action asks for, or nil when it runs at once.
    init?(_ record: BridgePendingRecord) {
        switch record.action {
        case .restoreFile:
            self = .restoreFile(record)
        case .provideAdminAccess(let id):
            self = .adminAccess(.retry(operation: id))
        case .deleteAccessKey(let operation, let accessKeyId):
            self = .adminAccess(
                .deleteAccessKey(operation: operation, accessKeyId: accessKeyId)
            )
        case .review:
            self = .review(record)
        case .wait, .updateApp, .signIn, .reconnect, .resumeUploads,
            .retryOperation, .resetDevice:
            return nil
        }
    }

    var id: String {
        switch self {
        case .adminAccess(.retry(let operation)): "admin-\(operation)"
        case .adminAccess(.deleteAccessKey(let operation, _)):
            "delete-key-\(operation)"
        case .s3Key: "s3-key"
        case .restoreFile(let record): "restore-file-\(record.hashValue)"
        case .review(let record): "review-\(record.hashValue)"
        }
    }
}

private struct PendingActionSheetView: View {
    let sheet: PendingActionSheet
    let location: BridgeStorageLocation
    let onDismiss: () -> Void

    var body: some View {
        switch sheet {
        case .adminAccess(let request):
            AdminAccessSheet(request: request, onDismiss: onDismiss)
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
            Text(
                localizedCoreString(
                    bridgePendingReasonKey(reason: record.reason.kind)
                )
            )
            .themeText(.heading)
            Text(
                "Put the file back where it was when it was imported, then retry."
            )
            .themeText(.body)
            .foregroundStyle(.secondary)
            Text(record.reason.detail)
                .themeText(.mono)
                .textSelection(.enabled)
            if let error {
                ErrorDetailDisclosure(error: error)
            }
            HStack {
                Spacer()
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
/// read: the reason, its diagnostic, and any causes behind it.
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
            HStack {
                Spacer()
                Button("Done", action: onDismiss)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 480)
    }

    private func reason(_ reason: BridgePendingReason) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text(
                localizedCoreString(bridgePendingReasonKey(reason: reason.kind))
            )
            .themeText(.heading)
            Text(reason.detail)
                .themeText(.mono)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
    }
}

/// A new S3 access key for this library's bucket, after the old one stopped
/// working. Setup checks it against the recorded location before keeping it.
private struct S3KeySheet: View {
    let location: BridgeStorageLocation
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var accessKey = ""
    @State
    private var secretKey = ""
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    TextField("Access Key", text: $accessKey)
                    SecureField("Secret Key", text: $secretKey)
                } footer: {
                    Text("Enter an access key for this library's bucket.")
                }
            }
            .formStyle(.grouped)
            if let error {
                ErrorDetailDisclosure(error: error)
                    .padding(.horizontal)
            }
            HStack {
                Spacer()
                Button("Cancel", action: onDismiss)
                    .keyboardShortcut(.cancelAction)
                Button("Connect") { connect() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(
                        task != nil || accessKey.isEmpty || secretKey.isEmpty
                    )
            }
            .padding()
        }
        .frame(width: 440)
        .onDisappear { task?.cancel() }
    }

    private func connect() {
        guard
            case .s3(
                let bucket,
                let region,
                let endpoint,
                let profile,
                let prefix
            ) =
                location
        else {
            preconditionFailure("an S3 key is asked only for an S3 library")
        }
        let config = BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint,
            keyPrefix: prefix.isEmpty ? nil : prefix,
            accessKey: accessKey,
            secretKey: secretKey,
            profile: profile
        )
        error = nil
        task = Task {
            defer { task = nil }
            do {
                _ = try await sync.saveSyncConfig(config)
                onDismiss()
            }
            catch is CancellationError {
                logger.debug("S3 key replacement cancelled")
            }
            catch {
                self.error = DisplayError(error)
            }
        }
    }
}

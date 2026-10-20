import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("PendingRecordsScreen")

/// Each thing coven is waiting on, in plain words with its diagnostic behind
/// a disclosure, and its action wherever the person can act; above them, why
/// sync is failing, with Reconnect where reconnecting helps and Reset Device
/// where sync found another copy of this device.
struct PendingRecordsScreen: View {
    /// The library's cloud home, whose S3 access key the key sheet replaces.
    let location: BridgeStorageLocation

    @Environment(SyncStatusStore.self)
    private var status
    @Environment(Sync.self)
    private var sync
    @Environment(AppSessionHolder.self)
    private var holder
    #if BAE_OAUTH_PROVIDERS
    @Environment(CloudSyncSetup.self)
    private var cloudSyncSetup
    #endif

    /// The pending record whose action needs more than one step.
    @State
    private var sheet: PendingActionSheet?

    var body: some View {
        List {
            if let error = status.error {
                Section {
                    SyncFailureNotice(
                        error: error,
                        canReconnect: status.canReconnect,
                        onReconnect: reconnect
                    )
                    if status.offersDeviceReset {
                        Button(
                            localizedCoreString("core.sync.reset_device.confirm"),
                            role: .destructive
                        ) {
                            holder.presentDeviceReset()
                        }
                    }
                }
            }
            Section {
                PendingRecordsView(records: status.pending, perform: perform)
                    // Each action takes only its own taps, not the row's.
                    .buttonStyle(.borderless)
            } header: {
                Text(status.statusText)
            }
        }
        .navigationTitle("Sync")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $sheet) { sheet in
            PendingActionSheetView(
                sheet: sheet,
                location: location,
                onDismiss: { self.sheet = nil }
            )
        }
    }

    /// Connect again after a failed launch, or run the next cycle now; a
    /// failed retry keeps the failure up with the new reason.
    private func reconnect() async {
        do {
            try await sync.reconnectSync()
        }
        catch {
            logger.error(
                "Sync reconnect failed: \(error.localizedDescription)"
            )
        }
    }

    private func perform(_ record: BridgePendingRecord) async throws {
        if let sheet = PendingActionSheet(record) {
            self.sheet = sheet
            return
        }
        switch record.action {
        case .updateApp:
            // This device installs updates itself; the record says which.
            sheet = .review(record)
        case .signIn(let provider):
            try await signIn(provider)
        case .reconnect:
            try await sync.reconnectSync()
        case .resumeUploads:
            try await sync.setSyncPaused(false)
        case .retryOperation(let id):
            try await sync.retryPendingOperation(id)
        case .restoreFromCode:
            holder.presentDeviceRestore()
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
        switch StorageReauthorization(provider: provider) {
        case .s3Key:
            sheet = .s3Key
        case .accountSignIn:
            #if BAE_OAUTH_PROVIDERS
            _ = try await cloudSyncSetup.signInAgain()
            #else
            preconditionFailure("\(provider) sign-in needs the OAuth build")
            #endif
        case .systemAccount:
            try await sync.reconnectSync()
        }
    }
}

private struct PendingActionSheetView: View {
    let sheet: PendingActionSheet
    let location: BridgeStorageLocation
    let onDismiss: () -> Void

    var body: some View {
        NavigationStack {
            switch sheet {
            case .adminAccess(let request):
                AdminAccessView(request: request, onDismiss: onDismiss)
            case .s3Key:
                S3KeyView(location: location, onDismiss: onDismiss)
            case .restoreFile(let record):
                RestoreFileView(record: record, onDismiss: onDismiss)
            case .review(let record):
                PendingReviewView(record: record, onDismiss: onDismiss)
            }
        }
    }
}

/// A source file that moved before it uploaded: put it back, then retry.
private struct RestoreFileView: View {
    let record: BridgePendingRecord
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        List {
            Section {
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
            } header: {
                Text(
                    localizedCoreString(
                        bridgePendingReasonKey(reason: record.reason.kind)
                    )
                )
            }
            if let error {
                Section {
                    ErrorDetailDisclosure(error: error)
                }
            }
        }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel", action: onDismiss)
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Retry") { retry() }
                    .disabled(task != nil)
            }
        }
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

/// Everything coven recorded about a pending record the person can only read:
/// the reason, the error behind it while one is held, and any causes behind
/// it.
private struct PendingReviewView: View {
    let record: BridgePendingRecord
    let onDismiss: () -> Void

    var body: some View {
        List {
            reason(record.reason)
            ForEach(Array(record.reason.causes.enumerated()), id: \.offset) {
                _,
                cause in
                reason(cause)
            }
        }
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Done", action: onDismiss)
            }
        }
    }

    private func reason(_ reason: BridgePendingReason) -> some View {
        Section {
            if let detail = reason.detail {
                Text(detail)
                    .themeText(.mono)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        } header: {
            Text(
                localizedCoreString(bridgePendingReasonKey(reason: reason.kind))
            )
        }
    }
}

/// A new S3 access key for this library's bucket, after the old one stopped
/// working. Setup checks it against the recorded location before keeping it.
private struct S3KeyView: View {
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
        List {
            Section {
                TextField("Access Key", text: $accessKey)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                SecureField("Secret Key", text: $secretKey)
            } footer: {
                Text("Enter an access key for this library's bucket.")
            }
            if let error {
                Section {
                    ErrorDetailDisclosure(error: error)
                }
            }
        }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel", action: onDismiss)
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Connect") { connect() }
                    .disabled(
                        task != nil || accessKey.isEmpty || secretKey.isEmpty
                    )
            }
        }
        .onDisappear { task?.cancel() }
    }

    private func connect() {
        let config = location.replacingS3Key(
            accessKey: accessKey,
            secretKey: secretKey
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

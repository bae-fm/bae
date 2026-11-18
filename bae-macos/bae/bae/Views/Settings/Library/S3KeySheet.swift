import BaeKit
import SwiftUI

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
                    Text("Enter an access key for this library’s bucket.")
                }
            }
            .formStyle(.grouped)
            if run.isRunning {
                StorageSetupRunLine(run: run)
                    .padding(.horizontal)
            }
            if let error {
                Notice(error: error)
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
            onFailed: { error = DisplayError($0) }
        )
    }
}

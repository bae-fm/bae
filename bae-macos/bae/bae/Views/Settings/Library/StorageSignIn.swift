import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("StorageSignIn")

/// Sign In to the library's recorded storage, for a Mac that no longer holds
/// its credentials for it: a new access key for its S3 bucket, the provider's
/// sign-in for Dropbox and OneDrive, or iCloud's setup for iCloud. It connects
/// that location again and never sets up other storage.
struct StorageSignIn: View {
    let location: BridgeStorageLocation

    @Environment(Sync.self)
    private var sync
    #if BAE_OAUTH_PROVIDERS
        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup
    #endif

    @State
    private var askingForKey = false
    @State
    private var task: Task<Void, Never>?
    /// The running reconnect; nil while none runs.
    @State
    private var steps: StorageSetupFollowing?
    @State
    private var error: DisplayError?

    var body: some View {
        Button(localizedCoreString("core.pending.action.sign_in")) {
            signIn()
        }
        .disabled(task != nil)
        .sheet(isPresented: $askingForKey) {
            S3KeySheet(location: location, onDismiss: { askingForKey = false })
        }
        .onDisappear { task?.cancel() }
        if let steps {
            StepProgressLine(steps: steps.steps) { EmptyView() }
        }
        if let error {
            ErrorDetailDisclosure(error: error)
        }
    }

    private func signIn() {
        switch location {
        case .s3:
            askingForKey = true
        case .dropbox, .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                reconnect { try await cloudSyncSetup.signInAgain($0) }
            #else
                preconditionFailure("\(location) sign-in needs the OAuth build")
            #endif
        case .cloudKit:
            #if BAE_CLOUDKIT
                reconnect { try await cloudSyncSetup.useCloudKit($0) }
            #else
                preconditionFailure(
                    "\(location) sign-in needs the iCloud build"
                )
            #endif
        }
    }

    /// Run `operation`, showing its steps while it runs and its failure after.
    /// The status line comes back on its own once the location is connected.
    private func reconnect(
        _ operation:
            @escaping (BridgeLibrarySetup) async throws ->
            BridgeStorageSetup
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
}

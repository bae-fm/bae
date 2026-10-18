import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("AdminAccessView")

/// Credentials of a storage admin, held for this one operation and never
/// stored: a pasted access key or token. The sign-ins a desktop's tools
/// provide aren't available on this device.
struct AdminAccessView: View {
    let request: AdminAccessRequest
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync

    private enum Source: Hashable {
        case accessKey
        case bearerToken
    }

    @State
    private var source: Source = .accessKey
    @State
    private var accessKeyId = ""
    @State
    private var secretAccessKey = ""
    @State
    private var sessionToken = ""
    @State
    private var iamEndpoint = ""
    @State
    private var bearerToken = ""
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        List {
            Section {
                Text(explanation)
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                Picker("Credentials", selection: $source) {
                    Text("Access Key").tag(Source.accessKey)
                    Text("Bearer token").tag(Source.bearerToken)
                }
            }
            Section {
                switch source {
                case .accessKey:
                    TextField("Access Key", text: $accessKeyId)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    SecureField("Secret Key", text: $secretAccessKey)
                    SecureField("Session Token (optional)", text: $sessionToken)
                    TextField("IAM Endpoint (optional)", text: $iamEndpoint)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                case .bearerToken:
                    SecureField("Token", text: $bearerToken)
                }
            }
            if case .deleteAccessKey(_, let accessKeyId) = request {
                Section {
                    Text(
                        "Already deleted key \(accessKeyId) in your provider's console?"
                    )
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    Button("I Deleted It") { confirmDeleted(accessKeyId) }
                        .disabled(task != nil)
                }
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
                Button("Continue") { submit() }
                    .disabled(task != nil || !ready)
            }
        }
        .onDisappear { task?.cancel() }
    }

    private var explanation: LocalizedStringKey {
        switch request {
        case .retry:
            "This change needs a storage admin's credentials. They're used once and never saved."
        case .deleteAccessKey:
            "A removed member's access key still needs deleting. A storage admin's credentials delete it; they're used once and never saved."
        }
    }

    private var ready: Bool {
        switch source {
        case .accessKey: !accessKeyId.isEmpty && !secretAccessKey.isEmpty
        case .bearerToken: !bearerToken.isEmpty
        }
    }

    /// The chosen source; an empty optional field means the service's
    /// default.
    private var access: BridgeAdminAccess {
        switch source {
        case .accessKey:
            .accessKey(
                accessKeyId: accessKeyId,
                secretAccessKey: secretAccessKey,
                sessionToken: sessionToken.isEmpty ? nil : sessionToken,
                iamEndpoint: iamEndpoint.isEmpty ? nil : iamEndpoint
            )
        case .bearerToken: .bearerToken(token: bearerToken)
        }
    }

    private func submit() {
        let access = access
        let request = request
        run {
            switch request {
            case .retry(let operation):
                try await sync.retryWithAdminAccess(operation, access)
            case .deleteAccessKey(let operation, _):
                try await sync.deleteAccessKey(operation, access)
            }
        }
    }

    private func confirmDeleted(_ accessKeyId: String) {
        run { try await sync.confirmAccessKeyDeleted(accessKeyId) }
    }

    private func run(_ operation: @escaping () async throws -> Void) {
        error = nil
        task = Task {
            defer { task = nil }
            do {
                try await operation()
                onDismiss()
            }
            catch is CancellationError {
                logger.debug("admin access operation cancelled")
            }
            catch {
                logger.error(
                    "admin access operation failed: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

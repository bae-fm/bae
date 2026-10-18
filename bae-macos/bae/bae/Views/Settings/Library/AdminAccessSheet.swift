import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("AdminAccessSheet")

/// What a storage admin's credentials are for, from a pending record.
enum AdminAccessRequest {
    /// Retry an operation that needs storage admin rights.
    case retry(operation: Int64)
    /// Delete a removed member's storage access key.
    case deleteAccessKey(operation: Int64, accessKeyId: String)
}

/// Credentials of a storage admin, held for this one operation and never
/// stored: pasted keys or a token, or a sign-in this Mac's tools provide.
struct AdminAccessSheet: View {
    let request: AdminAccessRequest
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync

    private enum Source: Hashable, CaseIterable {
        case accessKey
        case bearerToken
        case awsCli
        case gcloudCli
        case googleSignIn
        case awsIdentityCenter
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
    private var awsProfile = ""
    @State
    private var gcloudAccount = ""
    @State
    private var startUrl = ""
    @State
    private var ssoRegion = ""
    @State
    private var accountId = ""
    @State
    private var roleName = ""
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    Text(explanation)
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                    Picker("Credentials", selection: $source) {
                        Text("Access Key").tag(Source.accessKey)
                        Text("Bearer token").tag(Source.bearerToken)
                        Text("AWS CLI profile").tag(Source.awsCli)
                        Text("gcloud account").tag(Source.gcloudCli)
                        Text("Sign in with Google").tag(Source.googleSignIn)
                        Text(verbatim: "AWS IAM Identity Center")
                            .tag(Source.awsIdentityCenter)
                    }
                }
                Section {
                    sourceFields
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
                Button("Continue") { submit() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(task != nil || !ready)
            }
            .padding()
        }
        .frame(width: 480, height: 520)
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

    @ViewBuilder
    private var sourceFields: some View {
        switch source {
        case .accessKey:
            TextField("Access Key", text: $accessKeyId)
            SecureField("Secret Key", text: $secretAccessKey)
            SecureField("Session Token (optional)", text: $sessionToken)
            TextField("IAM Endpoint (optional)", text: $iamEndpoint)
        case .bearerToken:
            SecureField("Token", text: $bearerToken)
        case .awsCli:
            TextField("Profile (optional)", text: $awsProfile)
        case .gcloudCli:
            TextField("Account (optional)", text: $gcloudAccount)
        case .googleSignIn:
            Text("Opens your browser to sign in to Google.")
                .themeText(.detail)
                .foregroundStyle(.secondary)
        case .awsIdentityCenter:
            TextField("Start URL", text: $startUrl)
            TextField("Region", text: $ssoRegion)
            TextField("Account ID", text: $accountId)
            TextField("Role", text: $roleName)
        }
    }

    private var ready: Bool {
        switch source {
        case .accessKey: !accessKeyId.isEmpty && !secretAccessKey.isEmpty
        case .bearerToken: !bearerToken.isEmpty
        case .awsCli, .gcloudCli, .googleSignIn: true
        case .awsIdentityCenter:
            !startUrl.isEmpty && !ssoRegion.isEmpty && !accountId.isEmpty
                && !roleName.isEmpty
        }
    }

    /// The chosen source; an empty optional field means the tool's default.
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
        case .awsCli: .awsCli(profile: awsProfile.isEmpty ? nil : awsProfile)
        case .gcloudCli:
            .gcloudCli(account: gcloudAccount.isEmpty ? nil : gcloudAccount)
        case .googleSignIn: .googleSignIn
        case .awsIdentityCenter:
            .awsIdentityCenter(
                startUrl: startUrl,
                region: ssoRegion,
                accountId: accountId,
                roleName: roleName
            )
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

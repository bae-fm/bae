// Sync setup is a modal wizard so the operation is atomic: the user either
// completes the full provider configuration or cancels. No half-configured
// sync state is ever visible in the settings view.

import SwiftUI
import os.log

private let logger = Logger.bae("SyncSetupWizard")

// MARK: - Wizard Steps

private enum WizardStep: Equatable {
    case selectProvider
    case configure(BridgeCloudProvider)
}

// MARK: - Provider Row Data

private struct ProviderOption: Identifiable {
    let id: BridgeCloudProvider
    let name: String
    let description: String
    let icon: String
}

private let providerOptions: [ProviderOption] = [
    ProviderOption(
        id: .cloudKit,
        name: "iCloud",
        description: "Sync via your iCloud account",
        icon: "icloud"
    ),
    ProviderOption(
        id: .googleDrive,
        name: "Google Drive",
        description: "Sync via Google Drive",
        icon: "externaldrive"
    ),
    ProviderOption(
        id: .dropbox,
        name: "Dropbox",
        description: "Sync via Dropbox",
        icon: "externaldrive"
    ),
    ProviderOption(
        id: .oneDrive,
        name: "OneDrive",
        description: "Sync via Microsoft OneDrive",
        icon: "externaldrive"
    ),
    ProviderOption(
        id: .s3,
        name: "S3-compatible",
        description: "Any S3-compatible storage (AWS, Backblaze, Minio, ...)",
        icon: "externaldrive.connected.to.line.below"
    ),
]

// MARK: - SyncSetupWizard (pure leaf)

struct SyncSetupWizard: View {
    let onConnectS3: (BridgeSaveSyncConfig) async throws -> Void
    /// Awaits the OAuth browser round-trip; cancellation aborts the listener.
    let onConnectOAuth: (_ provider: BridgeCloudProvider) async throws -> Void
    let onConnectCloudKit: () async throws -> Void
    let onDone: () -> Void

    @State
    private var step: WizardStep = .selectProvider
    @State
    private var error: String?
    @State
    private var isWorking = false
    @State
    private var connectTask: Task<Void, Never>?

    // S3 fields
    @State
    private var bucket = ""
    @State
    private var region = ""
    @State
    private var endpoint = ""
    @State
    private var keyPrefix = ""
    @State
    private var accessKey = ""
    @State
    private var secretKey = ""

    var body: some View {
        VStack(spacing: 0) {
            header

            Divider()

            Group {
                switch step {
                case .selectProvider:
                    providerList
                case .configure(let provider):
                    configureStep(for: provider)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(width: 460, height: 400)
        .onDisappear { connectTask?.cancel() }
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            if case .configure = step {
                Button {
                    withAnimation(.easeInOut(duration: 0.15)) {
                        step = .selectProvider
                        error = nil
                    }
                } label: {
                    Image(systemName: "chevron.left")
                }
                .buttonStyle(.borderless)
            }

            Text(headerTitle)
                .font(.headline)

            Spacer()

            Button("Cancel") {
                onDone()
            }
            .buttonStyle(.borderless)
        }
        .padding()
    }

    private var headerTitle: String {
        switch step {
        case .selectProvider:
            "Set Up Sync"
        case .configure(let provider):
            cloudProviderLabel(provider: provider)
        }
    }

    // MARK: - Provider List

    private var providerList: some View {
        ScrollView {
            VStack(spacing: 1) {
                ForEach(providerOptions) { option in
                    Button {
                        withAnimation(.easeInOut(duration: 0.15)) {
                            step = .configure(option.id)
                            error = nil
                        }
                    } label: {
                        HStack(spacing: 12) {
                            Image(systemName: option.icon)
                                .frame(width: 24)
                                .foregroundStyle(.secondary)

                            VStack(alignment: .leading, spacing: 2) {
                                Text(option.name)
                                    .fontWeight(.medium)
                                Text(option.description)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }

                            Spacer()

                            Image(systemName: "chevron.right")
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                        }
                        .contentShape(Rectangle())
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.vertical, 8)
        }
    }

    // MARK: - Configure Step

    private func configureStep(for provider: BridgeCloudProvider) -> some View {
        Form {
            switch provider {
            case .s3:
                s3Fields
            case .googleDrive, .dropbox, .oneDrive:
                oauthFields(provider: provider)
            case .cloudKit:
                cloudKitFields
            }

            if let error {
                Section {
                    Text(error)
                        .foregroundStyle(.red)
                        .font(.callout)
                }
            }
        }
        .formStyle(.grouped)
    }

    // MARK: - S3

    private var s3Fields: some View {
        Group {
            Section {
                TextField("Bucket", text: $bucket)
                TextField("Region", text: $region)
                TextField("Endpoint", text: $endpoint)
                    .textContentType(.URL)
                TextField("Key Prefix", text: $keyPrefix)
            }

            Section {
                SecureField("Access Key", text: $accessKey)
                SecureField("Secret Key", text: $secretKey)
            }

            Section {
                HStack {
                    Spacer()
                    Button("Connect") {
                        connectS3()
                    }
                    .disabled(
                        bucket.isEmpty || region.isEmpty || accessKey.isEmpty
                            || secretKey.isEmpty
                            || isWorking
                    )
                }
            }
        }
    }

    // MARK: - OAuth

    private func oauthFields(provider: BridgeCloudProvider) -> some View {
        Section {
            VStack(spacing: 12) {
                Text(
                    "Opens your browser to authorize bae with \(cloudProviderLabel(provider: provider))."
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)

                HStack {
                    Spacer()
                    Button(
                        isWorking
                            ? "Connecting..."
                            : "Connect \(cloudProviderLabel(provider: provider))"
                    ) {
                        connectOAuth(provider: provider)
                    }
                    .disabled(isWorking)
                }
            }
        }
    }

    // MARK: - iCloud

    private var cloudKitFields: some View {
        Section {
            VStack(spacing: 12) {
                Text(
                    "Uses iCloud for sync. Requires iCloud to be enabled in System Settings."
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)

                HStack {
                    Spacer()
                    Button("Use iCloud") {
                        connectCloudKit()
                    }
                    .disabled(isWorking)
                }
            }
        }
    }

    // MARK: - Actions

    private func connectS3() {
        let data = BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint.isEmpty ? nil : endpoint,
            keyPrefix: keyPrefix.isEmpty ? nil : keyPrefix,
            accessKey: accessKey,
            secretKey: secretKey,
        )
        runConnect { try await onConnectS3(data) }
    }

    private func connectOAuth(provider: BridgeCloudProvider) {
        runConnect { try await onConnectOAuth(provider) }
    }

    private func connectCloudKit() {
        runConnect { try await onConnectCloudKit() }
    }

    /// Shared task lifecycle for the async connect actions (OAuth, iCloud):
    /// cancel any in-flight attempt, run `operation`, finish the wizard on
    /// success, reset on cancellation (sheet dismissed or retried), surface
    /// the error otherwise.
    private func runConnect(_ operation: @escaping () async throws -> Void) {
        connectTask?.cancel()
        isWorking = true
        error = nil

        connectTask = Task {
            do {
                try await operation()
                onDone()
            }
            catch is CancellationError {
                logger.debug("Connect attempt cancelled")
                isWorking = false
            }
            catch {
                logger.error("Connect failed: \(error.localizedDescription)")
                isWorking = false
                self.error = connectErrorMessage(error)
            }
        }
    }

    /// Map a connect error to user-facing text. OAuth surfaces a denied
    /// authorization specially; `CloudKitError` already carries a ready
    /// sentence in `msg`, but its `localizedDescription` is the reflected
    /// enum, so unwrap the case instead.
    private func connectErrorMessage(_ error: Error) -> String {
        if case BridgeError.Config(let msg) = error {
            return msg.contains("denied") ? "Access denied" : msg
        }
        if case CloudKitError.Storage(let msg) = error {
            return msg
        }
        return error.localizedDescription
    }
}

// MARK: - Previews

#Preview("Provider Selection") {
    SyncSetupWizard(
        onConnectS3: { _ in },
        onConnectOAuth: { _ in },
        onConnectCloudKit: {},
        onDone: {},
    )
}

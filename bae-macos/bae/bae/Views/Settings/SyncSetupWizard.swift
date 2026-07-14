// Sync setup is a modal wizard so the operation is atomic: the user either
// completes the full provider configuration or cancels. No half-configured
// sync state is ever visible in the settings view.

import BaeKit
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
    // Resolved through the catalog at the access site (not stored as a
    // `LocalizedStringKey`, which isn't `Sendable` and would break the
    // concurrency-safe global `providerDisplay`/`providerOptions` tables).
    let name: String
    let description: String
    let icon: String
}

/// Display data (name, blurb, icon) for every provider bae can sync to. The
/// bridge decides which are actually compiled in via `availableCloudProviders()`;
/// `providerOptions` filters this table to that set, so a baeium (S3-only) build
/// shows just S3.
private let providerDisplay: [BridgeCloudProvider: ProviderOption] = [
    .cloudKit: ProviderOption(
        id: .cloudKit,
        name: String(localized: "iCloud"),
        description: String(localized: "Sync via your iCloud account"),
        icon: "icloud"
    ),
    .googleDrive: ProviderOption(
        id: .googleDrive,
        name: String(localized: "Google Drive"),
        description: String(localized: "Sync via Google Drive"),
        icon: "externaldrive"
    ),
    .dropbox: ProviderOption(
        id: .dropbox,
        name: String(localized: "Dropbox"),
        description: String(localized: "Sync via Dropbox"),
        icon: "externaldrive"
    ),
    .oneDrive: ProviderOption(
        id: .oneDrive,
        name: String(localized: "OneDrive"),
        description: String(localized: "Sync via Microsoft OneDrive"),
        icon: "externaldrive"
    ),
    .s3: ProviderOption(
        id: .s3,
        name: String(localized: "S3-compatible"),
        description: String(
            localized: "Any S3-compatible storage (AWS, Backblaze, Minio, ...)"
        ),
        icon: "externaldrive.connected.to.line.below"
    ),
]

/// The providers this build offers, in the bridge's display order. Drives the
/// wizard's provider list so the picker matches the compiled-in feature set.
private let providerOptions: [ProviderOption] = availableCloudProviders()
    .compactMap { providerDisplay[$0] }

// MARK: - SyncSetupWizard (pure leaf)

struct SyncSetupWizard: View {
    let onConnectS3: (BridgeSaveSyncConfig) async throws -> Void
    /// Awaits the OAuth browser round-trip; cancellation aborts the listener.
    /// The OAuth provider UI that invokes this is compiled out of baeium builds,
    /// where the closure is an unused stub.
    let onConnectOAuth:
        (_ provider: BridgeCloudProvider, _ storage: BridgeHomeStorage)
            async throws -> Void
    /// The iCloud UI that invokes this is compiled out of baeium builds, where
    /// the closure is an unused stub.
    let onConnectCloudKit: (_ storage: BridgeHomeStorage) async throws -> Void
    let onDone: () -> Void

    @State
    private var step: WizardStep = .selectProvider
    @State
    private var error: String?
    @State
    private var isWorking = false
    @State
    private var connectTask: Task<Void, Never>?

    // How the home stores its objects: opaque (encrypted) or browsable (stored in
    // the clear). Applies to every provider; defaults to the secure choice.
    @State
    private var storage: BridgeHomeStorage = .opaque

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
            String(localized: "Set Up Sync")
        case .configure(let provider):
            provider.displayName
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

    @ViewBuilder
    private func configureStep(for provider: BridgeCloudProvider) -> some View {
        Form {
            storageSection

            // The switch stays exhaustive over all five providers in every
            // build; only the arms whose bridge calls are feature-gated are
            // hollowed out. A gated-out provider never reaches here — it isn't
            // in `availableCloudProviders()`, so it can't be selected.
            switch provider {
            case .s3:
                s3Fields
            case .googleDrive, .dropbox, .oneDrive:
                #if BAE_OAUTH_PROVIDERS
                    oauthFields(provider: provider)
                #else
                    EmptyView()
                #endif
            case .cloudKit:
                #if BAE_CLOUDKIT
                    cloudKitFields
                #else
                    EmptyView()
                #endif
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

    // MARK: - Storage mode

    /// Opaque vs. browsable, shown for every provider. Opaque is the secure
    /// default; browsable trades encryption for a bucket you can read by name.
    /// This is not access control — the provider's own credentials gate the
    /// bucket either way.
    private var storageSection: some View {
        Section("Storage") {
            Picker("Storage", selection: $storage) {
                Text("Opaque — end-to-end encrypted")
                    .tag(BridgeHomeStorage.opaque)
                Text("Browsable — stored unencrypted")
                    .tag(BridgeHomeStorage.browsable)
            }
            .pickerStyle(.inline)
            .labelsHidden()

            Text(
                storage == .opaque
                    ? "Every object is encrypted before upload. Anyone with access to the storage sees only ciphertext under opaque keys."
                    : "Objects are stored in the clear at readable paths, so anyone with access to the storage can read your files by name. Sharing is unavailable for a browsable library."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
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

    #if BAE_OAUTH_PROVIDERS
        private func oauthFields(provider: BridgeCloudProvider) -> some View {
            let connectTitle: LocalizedStringKey =
                isWorking
                ? "Connecting..."
                : "Connect \(provider.displayName)"
            return Section {
                VStack(spacing: 12) {
                    Text(
                        "Opens your browser to authorize bae with \(provider.displayName)."
                    )
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)

                    HStack {
                        Spacer()
                        Button(connectTitle) {
                            connectOAuth(provider: provider)
                        }
                        .disabled(isWorking)
                    }
                }
            }
        }
    #endif

    // MARK: - iCloud

    #if BAE_CLOUDKIT
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
    #endif

}

// MARK: - Actions

extension SyncSetupWizard {
    fileprivate func connectS3() {
        let data = BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint.isEmpty ? nil : endpoint,
            keyPrefix: keyPrefix.isEmpty ? nil : keyPrefix,
            accessKey: accessKey,
            secretKey: secretKey,
            storage: storage,
        )
        runConnect { try await onConnectS3(data) }
    }

    #if BAE_OAUTH_PROVIDERS
        fileprivate func connectOAuth(provider: BridgeCloudProvider) {
            runConnect { try await onConnectOAuth(provider, storage) }
        }
    #endif

    #if BAE_CLOUDKIT
        fileprivate func connectCloudKit() {
            runConnect { try await onConnectCloudKit(storage) }
        }
    #endif

    /// Shared task lifecycle for the async connect actions (OAuth, iCloud):
    /// cancel any in-flight attempt, run `operation`, finish the wizard on
    /// success, reset on cancellation (sheet dismissed or retried), surface
    /// the error otherwise.
    fileprivate func runConnect(_ operation: @escaping () async throws -> Void)
    {
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

    /// Map a connect error to user-facing text. `CloudKitError` already carries a
    /// ready sentence in `msg`, but its `localizedDescription` is the reflected
    /// enum, so unwrap the case instead. Everything else — a rejected credential,
    /// an unreachable backend, a keyring failure, a config-write failure — reads
    /// as its own line through the shared path.
    fileprivate func connectErrorMessage(_ error: Error) -> String {
        #if BAE_CLOUDKIT
            if case CloudKitError.Storage(let msg) = error {
                return msg
            }
        #endif
        return error.displayLine
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Provider Selection") {
        SyncSetupWizard(
            onConnectS3: { _ in },
            onConnectOAuth: { _, _ in },
            onConnectCloudKit: { _ in },
            onDone: {},
        )
    }
#endif

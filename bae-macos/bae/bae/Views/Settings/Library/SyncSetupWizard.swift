// A modal wizard, so sync is either fully configured or not at all.

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
    // A `String`, since `LocalizedStringKey` isn't `Sendable`.
    let name: String
    let description: String
    let icon: String
}

/// Name, blurb and icon for every provider bae can sync to.
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

/// The providers this build offers, in the bridge's display order.
private let providerOptions: [ProviderOption] = availableCloudProviders()
    .compactMap { providerDisplay[$0] }

/// A provider in the wizard's list: its icon, name and blurb.
private struct ProviderRow: View {
    let option: ProviderOption

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            Image(systemName: option.icon)
                .frame(width: 24)
                .foregroundStyle(.secondary)

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(option.name)
                    .themeText(.rowTitle)
                Text(option.description)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Image(systemName: "chevron.right")
                .themeIcon(.small)
                .foregroundStyle(.tertiary)
        }
        .contentShape(Rectangle())
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
    }
}

// MARK: - SyncSetupWizard (pure leaf)

struct SyncSetupWizard: View {
    let onConnectS3: (BridgeSaveSyncConfig) async throws -> Void
    #if BAE_OAUTH_PROVIDERS
        /// Awaits the OAuth browser sign-in; cancelling stops the listener.
        let onConnectOAuth:
            (_ provider: BridgeCloudProvider, _ storage: BridgeHomeStorage)
                async throws -> Void
        let onConnectCloudKit:
            (_ storage: BridgeHomeStorage) async throws -> Void
    #endif
    let onDone: () -> Void

    @State
    private var step: WizardStep = .selectProvider
    @State
    private var error: DisplayError?
    @State
    private var isWorking = false
    @State
    private var connectTask: Task<Void, Never>?

    // How the home stores its objects; defaults to encrypted.
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
        .frame(width: 460, height: 520)
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
                .themeText(.heading)

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
            VStack(spacing: ThemeSpace.hairline) {
                ForEach(providerOptions) { option in
                    Button {
                        withAnimation(.easeInOut(duration: 0.15)) {
                            step = .configure(option.id)
                            error = nil
                        }
                    } label: {
                        ProviderRow(option: option)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.vertical, ThemeSpace.related)
        }
    }

    // MARK: - Configure Step

    /// The fields scroll; the error and the connect button stay below them.
    @ViewBuilder
    private func configureStep(for provider: BridgeCloudProvider) -> some View {
        VStack(spacing: 0) {
            Form {
                storageSection

                // A provider compiled out of this build can't be selected.
                switch provider {
                case .s3:
                    s3Fields
                case .googleDrive, .dropbox, .oneDrive:
                    #if BAE_OAUTH_PROVIDERS
                        oauthExplanation(provider: provider)
                    #else
                        EmptyView()
                    #endif
                case .cloudKit:
                    #if BAE_CLOUDKIT
                        cloudKitExplanation
                    #else
                        EmptyView()
                    #endif
                }
            }
            .formStyle(.grouped)

            if let error {
                ErrorDetailDisclosure(error: error)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }

            connectRow(for: provider)
                .padding(.horizontal)
                .padding(.bottom, ThemeSpace.section)
        }
    }

    // MARK: - Connect Row

    /// The configure step's one action.
    private func connectRow(for provider: BridgeCloudProvider) -> some View {
        HStack(spacing: ThemeSpace.related) {
            if isWorking {
                ProgressView()
                    .controlSize(.small)
            }
            Spacer()
            Button(connectTitle(for: provider)) {
                connect(provider: provider)
            }
            .buttonStyle(PrimaryButtonStyle())
            .keyboardShortcut(.defaultAction)
            .disabled(isWorking || !connectReady(for: provider))
        }
    }

    private func connectTitle(
        for provider: BridgeCloudProvider
    ) -> LocalizedStringKey {
        if isWorking {
            return "Connecting..."
        }
        switch provider {
        case .s3:
            return "Connect"
        case .googleDrive, .dropbox, .oneDrive:
            return "Connect \(provider.displayName)"
        case .cloudKit:
            return "Use iCloud"
        }
    }

    /// Only S3 takes credentials here; the other flows collect theirs later.
    private func connectReady(for provider: BridgeCloudProvider) -> Bool {
        switch provider {
        case .s3:
            !bucket.isEmpty && !region.isEmpty && !accessKey.isEmpty
                && !secretKey.isEmpty
        case .googleDrive, .dropbox, .oneDrive, .cloudKit:
            true
        }
    }

    // MARK: - Storage mode

    /// Opaque or browsable storage, shown for every provider.
    private var storageSection: some View {
        Section("Storage") {
            Picker("Storage", selection: $storage) {
                Text("Opaque: end-to-end encrypted")
                    .tag(BridgeHomeStorage.opaque)
                Text("Browsable: stored unencrypted")
                    .tag(BridgeHomeStorage.browsable)
            }
            .pickerStyle(.inline)
            .labelsHidden()

            Text(
                storage == .opaque
                    ? "Every object is encrypted before upload. Anyone with access to the storage sees only ciphertext under opaque keys."
                    : "Objects are stored in the clear at readable paths, so anyone with access to the storage can read your files by name. Sharing is unavailable for a browsable library."
            )
            .themeText(.detail)
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
        }
    }

    // MARK: - OAuth

    #if BAE_OAUTH_PROVIDERS
        private func oauthExplanation(
            provider: BridgeCloudProvider
        ) -> some View {
            Section {
                Text(
                    "Opens your browser to authorize bae with \(provider.displayName)."
                )
                .themeText(.body)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    #endif

    // MARK: - iCloud

    #if BAE_CLOUDKIT
        private var cloudKitExplanation: some View {
            Section {
                Text(
                    "Uses iCloud for sync. Requires iCloud to be enabled in System Settings."
                )
                .themeText(.body)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    #endif

}

// MARK: - Actions

extension SyncSetupWizard {
    /// Route the connect row's press to the flow the selected provider uses.
    fileprivate func connect(provider: BridgeCloudProvider) {
        switch provider {
        case .s3:
            connectS3()
        case .googleDrive, .dropbox, .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                connectOAuth(provider: provider)
            #else
                break
            #endif
        case .cloudKit:
            #if BAE_CLOUDKIT
                connectCloudKit()
            #else
                break
            #endif
        }
    }

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

    /// Runs one connect attempt, replacing any in flight: success closes the
    /// wizard and a failure shows its error.
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
                self.error = connectFailure(error)
            }
        }
    }

    /// The failure to show for a connect error, or nil for a cancellation;
    /// a `CloudKitError` carries its sentence in `msg`.
    fileprivate func connectFailure(_ error: Error) -> DisplayError? {
        #if BAE_CLOUDKIT
            if case CloudKitError.Storage(let msg) = error {
                return DisplayError(line: msg)
            }
        #endif
        return DisplayError(error)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Provider Selection") {
        #if BAE_OAUTH_PROVIDERS
            SyncSetupWizard(
                onConnectS3: { _ in },
                onConnectOAuth: { _, _ in },
                onConnectCloudKit: { _ in },
                onDone: {}
            )
        #else
            SyncSetupWizard(
                onConnectS3: { _ in },
                onDone: {}
            )
        #endif
    }
#endif

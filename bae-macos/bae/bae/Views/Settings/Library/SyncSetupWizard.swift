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

// MARK: - Provider Row

/// The providers this build offers, in the bridge's display order.
private let providerOptions: [BridgeCloudProvider] = availableCloudProviders()

/// A provider in the wizard's list: its icon, name and blurb.
private struct ProviderRow: View {
    let provider: BridgeCloudProvider

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            Image(systemName: provider.setupSymbol)
                .frame(width: 24)
                .foregroundStyle(.secondary)

            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(provider.displayName)
                    .themeText(.rowTitle)
                Text(provider.setupSummary)
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

// MARK: - SyncSetupWizard

/// Sets up the library's cloud home: pick a provider, then its short form or
/// sign-in and folder choice. Success records the location coven connected.
struct SyncSetupWizard: View {
    let onDone: () -> Void

    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Sync.self)
    private var sync
    #if BAE_OAUTH_PROVIDERS
        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup
    #endif

    @State
    private var step: WizardStep = .selectProvider

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
        .frame(width: 460, height: 560)
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            if case .configure = step {
                Button {
                    withAnimation(.easeInOut(duration: 0.15)) {
                        step = .selectProvider
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
                ForEach(providerOptions, id: \.self) { provider in
                    Button {
                        withAnimation(.easeInOut(duration: 0.15)) {
                            step = .configure(provider)
                        }
                    } label: {
                        ProviderRow(provider: provider)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.vertical, ThemeSpace.related)
        }
    }

    // MARK: - Configure Step

    /// A provider compiled out of this build can't be selected.
    @ViewBuilder
    private func configureStep(for provider: BridgeCloudProvider) -> some View {
        switch provider {
        case .s3:
            S3SetupForm(
                detectSettings: sync.detectS3Settings,
                onConnected: connected
            )
        case .dropbox:
            #if BAE_OAUTH_PROVIDERS
                OAuthFolderSetup(
                    browser: cloudSyncSetup.folderBrowser(.dropbox),
                    onConnected: connected
                )
            #else
                EmptyView()
            #endif
        case .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                OAuthFolderSetup(
                    browser: cloudSyncSetup.folderBrowser(.oneDrive),
                    onConnected: connected
                )
            #else
                EmptyView()
            #endif
        case .cloudKit:
            #if BAE_CLOUDKIT
                CloudKitSetup(onConnected: connected)
            #else
                EmptyView()
            #endif
        }
    }

    private func connected(_ setup: BridgeStorageSetup) {
        configStore.applyStorage(setup.location)
        onDone()
    }
}

#if BAE_CLOUDKIT
    /// Uses the Apple Account this Mac is signed in to.
    private struct CloudKitSetup: View {
        let onConnected: (BridgeStorageSetup) -> Void

        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup

        var body: some View {
            SetupStep(
                actionTitle: "Use iCloud",
                ready: true,
                operation: { try await cloudSyncSetup.useCloudKit() },
                onConnected: onConnected,
                fields: {
                    Section {
                        Text(
                            "Uses iCloud for sync. Requires iCloud to be enabled in System Settings."
                        )
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            )
        }
    }
#endif

/// A setup step's fields above its one action: the error and the button stay
/// below the fields while they scroll.
struct SetupStep<Fields: View>: View {
    let actionTitle: LocalizedStringKey
    let ready: Bool
    let operation: () async throws -> BridgeStorageSetup
    let onConnected: (BridgeStorageSetup) -> Void
    @ViewBuilder
    let fields: Fields

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(spacing: 0) {
            Form {
                fields
            }
            .formStyle(.grouped)
            .disabled(task != nil)

            if let error {
                ErrorDetailDisclosure(error: error)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }

            HStack(spacing: ThemeSpace.related) {
                if task != nil {
                    ProgressView()
                        .controlSize(.small)
                }
                Spacer()
                Button(task != nil ? "Connecting..." : actionTitle) {
                    connect()
                }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .disabled(task != nil || !ready)
            }
            .padding(.horizontal)
            .padding(.bottom, ThemeSpace.section)
        }
        .onDisappear { task?.cancel() }
    }

    private func connect() {
        error = nil
        task = Task {
            defer { task = nil }
            do {
                onConnected(try await operation())
            }
            catch is CancellationError {
                logger.debug("Connect attempt cancelled")
            }
            catch {
                logger.error("Connect failed: \(String(reflecting: error))")
                self.error = DisplayError(error)
            }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Provider Selection") {
        SyncSetupWizard(onDone: {})
            .environment(PreviewData.configStore())
            .environment(PreviewData.previewSync())
            #if BAE_OAUTH_PROVIDERS
                .environment(CloudSyncSetup())
            #endif
    }
#endif

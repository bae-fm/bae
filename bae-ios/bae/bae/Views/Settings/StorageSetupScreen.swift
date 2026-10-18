import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("StorageSetupScreen")

/// Sets up the library's cloud home: pick a provider, then its short form or
/// sign-in and folder choice. Success records the location coven connected
/// and returns to Settings.
struct StorageSetupScreen: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Sync.self)
    private var sync
    #if BAE_OAUTH_PROVIDERS
    @Environment(CloudSyncSetup.self)
    private var cloudSyncSetup
    #endif
    @Environment(\.dismiss)
    private var dismiss

    /// The providers this build offers, in the bridge's display order.
    private let providers = availableCloudProviders()

    var body: some View {
        List {
            ForEach(providers, id: \.self) { provider in
                NavigationLink {
                    configure(provider)
                        .navigationTitle(provider.displayName)
                        .navigationBarTitleDisplayMode(.inline)
                } label: {
                    Label {
                        VStack(alignment: .leading, spacing: ThemeSpace.line) {
                            Text(provider.displayName)
                            Text(provider.setupSummary)
                                .themeText(.detail)
                                .foregroundStyle(.secondary)
                        }
                    } icon: {
                        Image(systemName: provider.setupSymbol)
                    }
                }
            }
        }
        .navigationTitle("Set Up Sync")
        .navigationBarTitleDisplayMode(.inline)
    }

    /// A provider compiled out of this build isn't listed.
    @ViewBuilder
    private func configure(_ provider: BridgeCloudProvider) -> some View {
        switch provider {
        case .s3:
            S3SetupView(
                fields: S3SetupFields(detectSettings: sync.detectS3Settings),
                onConnected: connected
            )
        case .dropbox:
            #if BAE_OAUTH_PROVIDERS
            FolderSetupView(
                browser: cloudSyncSetup.folderBrowser(.dropbox),
                onConnected: connected
            )
            #else
            EmptyView()
            #endif
        case .oneDrive:
            #if BAE_OAUTH_PROVIDERS
            FolderSetupView(
                browser: cloudSyncSetup.folderBrowser(.oneDrive),
                onConnected: connected
            )
            #else
            EmptyView()
            #endif
        case .cloudKit:
            #if BAE_CLOUDKIT && BAE_OAUTH_PROVIDERS
            CloudKitSetupView(onConnected: connected)
            #else
            EmptyView()
            #endif
        }
    }

    private func connected(_ setup: BridgeStorageSetup) {
        configStore.applyStorage(setup.location)
        dismiss()
    }
}

/// A setup step's fields above its error, with its one action in the
/// toolbar.
private struct SetupStep<Fields: View>: View {
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
        List {
            fields
            if let error {
                Section {
                    ErrorDetailDisclosure(error: error)
                }
            }
        }
        .disabled(task != nil)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                if task != nil {
                    ProgressView()
                }
                else {
                    Button(actionTitle) { connect() }
                        .disabled(!ready)
                }
            }
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
                logger.error("Connect failed: \(error.localizedDescription)")
                self.error = DisplayError(error)
            }
        }
    }
}

/// S3 setup: the bucket, region, endpoint and access key, and nothing else.
/// coven knows a service by its endpoint and fills in how to talk to it;
/// Advanced holds the key prefix, the profile, and a detection of the
/// settings for a service coven doesn't know.
private struct S3SetupView: View {
    let onConnected: (BridgeStorageSetup) -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var fields: S3SetupFields
    @State
    private var showAdvanced = false

    init(
        fields: S3SetupFields,
        onConnected: @escaping (BridgeStorageSetup) -> Void
    ) {
        _fields = State(initialValue: fields)
        self.onConnected = onConnected
    }

    var body: some View {
        @Bindable
        var fields = fields
        SetupStep(
            actionTitle: "Connect",
            ready: fields.ready,
            operation: { [sync, fields] in
                try await sync.saveSyncConfig(fields.config)
            },
            onConnected: onConnected
        ) {
            Section {
                plainField("Bucket", text: $fields.bucket)
                plainField("Region", text: $fields.region)
                plainField("Endpoint", text: $fields.endpoint)
                    .keyboardType(.URL)
                    .textContentType(.URL)
            } footer: {
                Text("Leave the endpoint empty for AWS.")
            }
            Section {
                plainField("Access Key", text: $fields.accessKey)
                SecureField("Secret Key", text: $fields.secretKey)
            }
            Section {
                DisclosureGroup("Advanced", isExpanded: $showAdvanced) {
                    advanced
                }
            }
        }
        .onDisappear { fields.cancelDetection() }
    }

    private func plainField(
        _ title: LocalizedStringKey,
        text: Binding<String>
    ) -> some View {
        TextField(title, text: text)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
    }

    @ViewBuilder
    private var advanced: some View {
        @Bindable
        var fields = fields
        plainField("Key Prefix", text: $fields.keyPrefix)
        Picker("Profile", selection: $fields.profile) {
            Text("Automatic").tag(S3SetupFields.ProfileChoice.automatic)
            ForEach(S3SetupFields.profileNames, id: \.self) { name in
                Text(verbatim: name.displayName)
                    .tag(S3SetupFields.ProfileChoice.named(name))
            }
            Text("Detected settings")
                .tag(S3SetupFields.ProfileChoice.detected)
                .selectionDisabled(fields.detection == nil)
        }
        HStack {
            Button("Detect Settings") { fields.detect() }
                .buttonStyle(.borderless)
                .disabled(fields.isDetecting || !fields.ready)
            if fields.isDetecting {
                ProgressView()
            }
        }
        if let detectError = fields.detectError {
            ErrorDetailDisclosure(error: detectError)
        }
        if let detection = fields.detection {
            S3DetectedSettings(detection: detection)
        }
    }
}

#if BAE_OAUTH_PROVIDERS
/// Dropbox or OneDrive setup: sign in, then choose the folder the library
/// lives in — a Dropbox shared folder, so the household can be invited, or
/// any OneDrive folder, which can be made here.
private struct FolderSetupView: View {
    let onConnected: (BridgeStorageSetup) -> Void

    @Environment(CloudSyncSetup.self)
    private var cloudSyncSetup

    @State
    private var browser: StorageFolderBrowser

    init(
        browser: StorageFolderBrowser,
        onConnected: @escaping (BridgeStorageSetup) -> Void
    ) {
        _browser = State(initialValue: browser)
        self.onConnected = onConnected
    }

    var body: some View {
        SetupStep(
            actionTitle: "Connect",
            ready: browser.target != nil,
            operation: { [cloudSyncSetup, browser] in
                guard let target = browser.target else {
                    preconditionFailure("Connect needs a chosen folder")
                }
                return try await cloudSyncSetup.connect(target.location)
            },
            onConnected: onConnected
        ) {
            if let listing = browser.listing {
                folders(listing)
            }
            else {
                Section {
                    Text(
                        "Sign in to \(browser.provider.cloudProvider.displayName) to choose the folder for your library."
                    )
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    Button("Sign In\u{2026}") { browser.signIn() }
                        .disabled(browser.isWorking)
                }
            }
            if let error = browser.error {
                Section {
                    ErrorDetailDisclosure(error: error)
                }
            }
        }
        .onDisappear { browser.cancel() }
    }

    @ViewBuilder
    private func folders(_ listing: StorageFolderBrowser.Listing) -> some View {
        @Bindable
        var browser = browser
        switch browser.provider {
        case .dropbox:
            Section {
                if listing.folders.isEmpty {
                    Text(
                        "No shared folders. Share a folder in Dropbox, then look again."
                    )
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                }
                Picker("Shared folder", selection: $browser.selected) {
                    ForEach(listing.folders, id: \.location) { folder in
                        Text(folder.name).tag(Optional(folder))
                    }
                }
                .pickerStyle(.inline)
                Button("Look Again") { browser.lookAgain() }
                    .disabled(browser.isWorking)
            }
        case .oneDrive:
            Section {
                LabeledContent("Folder", value: listing.path.last?.name ?? "")
                if browser.canOpenParent {
                    Button("Up") { browser.openParent() }
                        .disabled(browser.isWorking)
                }
            }
            Section("Folders") {
                ForEach(listing.folders, id: \.location) { folder in
                    Button(folder.name) { browser.open(folder) }
                        .disabled(browser.isWorking)
                }
                HStack {
                    TextField("New folder", text: $browser.newFolderName)
                    Button("Create") { browser.createFolder() }
                        .buttonStyle(.borderless)
                        .disabled(!browser.canCreateFolder)
                }
            }
        }
    }
}
#endif

#if BAE_CLOUDKIT && BAE_OAUTH_PROVIDERS
/// Uses the Apple Account this device is signed in to.
private struct CloudKitSetupView: View {
    let onConnected: (BridgeStorageSetup) -> Void

    @Environment(CloudSyncSetup.self)
    private var cloudSyncSetup

    var body: some View {
        SetupStep(
            actionTitle: "Use iCloud",
            ready: true,
            operation: { [cloudSyncSetup] in
                try await cloudSyncSetup.useCloudKit()
            },
            onConnected: onConnected
        ) {
            Section {
                Text(
                    "Uses iCloud for sync. Requires iCloud to be turned on in Settings."
                )
                .themeText(.body)
                .foregroundStyle(.secondary)
            }
        }
    }
}
#endif

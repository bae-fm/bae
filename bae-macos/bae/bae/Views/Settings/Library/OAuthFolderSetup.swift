#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import SwiftUI

    /// Dropbox or OneDrive setup: sign in through the browser, then choose the
    /// folder the library lives in — a Dropbox shared folder, so the household
    /// can be invited, or any OneDrive folder, which can be made here.
    struct OAuthFolderSetup: View {
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
                            "Opens your browser to authorize bae with \(browser.provider.cloudProvider.displayName)."
                        )
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        Button("Sign In...") { browser.signIn() }
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
        private func folders(
            _ listing: StorageFolderBrowser.Listing
        ) -> some View {
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
                    LabeledContent(
                        "Folder",
                        value: listing.path.last?.name ?? ""
                    )
                    if browser.canOpenParent {
                        Button("Up") { browser.openParent() }
                            .disabled(browser.isWorking)
                    }
                }
                Section("Folders") {
                    ForEach(listing.folders, id: \.location) { folder in
                        Button(folder.name) { browser.open(folder) }
                            .buttonStyle(.borderless)
                            .disabled(browser.isWorking)
                    }
                    HStack {
                        TextField("New folder", text: $browser.newFolderName)
                        Button("Create") { browser.createFolder() }
                            .disabled(!browser.canCreateFolder)
                    }
                }
            }
        }
    }
#endif

#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import SwiftUI

    /// Dropbox or OneDrive setup: sign in through the browser, then choose the
    /// folder the library lives in — a Dropbox shared folder, so the household
    /// can be invited, or any OneDrive folder; either can be made here.
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
                operation: { [cloudSyncSetup, browser] setup in
                    guard let target = browser.target else {
                        preconditionFailure("Connect needs a chosen folder")
                    }
                    return try await cloudSyncSetup.connect(
                        target.location,
                        setup
                    )
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
                        Button("Sign in…") { browser.signIn() }
                            .disabled(browser.isWorking)
                    }
                }
                if let work = browser.working {
                    Section {
                        FolderBrowserWorkLine(
                            work: work,
                            provider: browser.provider
                        )
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
                    Button("Look again") { browser.lookAgain() }
                        .disabled(browser.isWorking)
                }
                Section {
                    HStack {
                        TextField("New folder", text: $browser.newFolderName)
                        Button("Create") { browser.createDropboxFolder() }
                            .disabled(!browser.canCreateFolder)
                    }
                    if let taken = browser.takenName {
                        Text(
                            "A folder named “\(taken)” is already in your Dropbox. Choose another name."
                        )
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                    }
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

    /// The provider call folder choice is running, named under its spinner: the
    /// person signing in, the provider listing folders, or OneDrive making one.
    struct FolderBrowserWorkLine: View {
        let work: StorageFolderBrowser.Work
        let provider: StorageFolderBrowser.Provider

        var body: some View {
            ProgressView(String(localized: title))
                .controlSize(.small)
                .frame(maxWidth: .infinity, alignment: .leading)
        }

        /// The words under the spinner, by the call running.
        var title: LocalizedStringResource {
            switch (work, provider) {
            case (.signingIn, _):
                LocalizedStringResource(
                    "Signing in…",
                    comment:
                        "Cloud setup's folder step, while the person signs in to Dropbox or OneDrive in their browser."
                )
            case (.listing, .dropbox):
                LocalizedStringResource(
                    "Reading your shared Dropbox folders…",
                    comment:
                        "Dropbox setup, while bae lists the person's shared Dropbox folders."
                )
            case (.listing, .oneDrive):
                LocalizedStringResource(
                    "Reading the folders on your OneDrive…",
                    comment:
                        "OneDrive setup, while bae lists the folders inside the one shown."
                )
            case (.makingFolder, _):
                LocalizedStringResource(
                    "Making the folder…",
                    comment:
                        "Dropbox or OneDrive setup, while bae makes the new folder the person named."
                )
            }
        }
    }
#endif

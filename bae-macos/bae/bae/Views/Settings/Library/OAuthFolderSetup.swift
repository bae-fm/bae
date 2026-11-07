#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import SwiftUI

    /// Dropbox or OneDrive setup: sign in through the browser, opened as the
    /// screen opens, then choose the
    /// folder the library lives in — a Dropbox shared folder, so the household
    /// can be invited, or any OneDrive folder; either can be made here.
    struct OAuthFolderSetup: View {
        let leave: () -> Void
        let onConnected: (BridgeStorageSetup) -> Void

        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup

        @State
        private var browser: StorageFolderBrowser

        init(
            browser: StorageFolderBrowser,
            leave: @escaping () -> Void,
            onConnected: @escaping (BridgeStorageSetup) -> Void
        ) {
            _browser = State(initialValue: browser)
            self.leave = leave
            self.onConnected = onConnected
        }

        var body: some View {
            Group {
                if let listing = browser.listing {
                    folderChoice(listing)
                }
                else {
                    signIn
                }
            }
            .onDisappear { browser.cancel() }
        }

        /// The browser opens as the screen does, on the Continue that chose
        /// the provider; the screen waits for the sign-in, and a failed one
        /// is tried again from here.
        private var signIn: some View {
            VStack(spacing: 0) {
                VStack(alignment: .leading, spacing: ThemeSpace.related) {
                    if let work = browser.working {
                        FolderBrowserWorkLine(
                            work: work,
                            provider: browser.provider
                        )
                        if work == .signingIn {
                            Text(
                                "Finish signing in in the browser window that opened, then come back here."
                            )
                            .themeText(.detail)
                            .foregroundStyle(.secondary)
                        }
                    }
                    if let error = browser.error {
                        ErrorDetailDisclosure(error: error)
                    }
                }
                .padding()
                .frame(maxWidth: .infinity, alignment: .leading)
                Spacer(minLength: 0)
                MacSheetFooter {
                    Button("Cancel") { leave() }
                    if !browser.isWorking {
                        Button("Sign in…") { browser.signIn() }
                            .buttonStyle(PrimaryButtonStyle())
                            .keyboardShortcut(.defaultAction)
                    }
                }
                .padding(.horizontal)
                .padding(.bottom, ThemeSpace.section)
            }
            .onAppear {
                if !browser.isWorking && browser.error == nil {
                    browser.signIn()
                }
            }
        }

        private func folderChoice(
            _ listing: StorageFolderBrowser.Listing
        ) -> some View {
            SetupStep(
                ready: browser.target != nil,
                operation: { [cloudSyncSetup, browser] setup in
                    guard let target = browser.target else {
                        preconditionFailure("Set up needs a chosen folder")
                    }
                    return try await cloudSyncSetup.connect(
                        target.location,
                        setup
                    )
                },
                leave: leave,
                onConnected: onConnected,
                fields: {
                    folders(listing)
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
            )
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

    /// The provider call folder choice is running, named beside its spinner: the
    /// person signing in, the provider listing folders, or OneDrive making one.
    struct FolderBrowserWorkLine: View {
        let work: StorageFolderBrowser.Work
        let provider: StorageFolderBrowser.Provider

        var body: some View {
            WaitingIndicator(Text(title))
                .frame(maxWidth: .infinity, alignment: .leading)
        }

        /// The words beside the spinner, by the call running.
        var title: LocalizedStringResource {
            switch (work, provider) {
            case (.signingIn, _):
                LocalizedStringResource(
                    "Signing in to \(provider.cloudProvider.displayName) in your browser…",
                    comment:
                        "Dropbox or OneDrive setup, while the person signs in in their browser; the argument is the provider's name."
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

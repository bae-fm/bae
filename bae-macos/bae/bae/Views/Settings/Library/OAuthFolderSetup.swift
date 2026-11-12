#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import SwiftUI

    /// Dropbox or OneDrive setup: sign in through the browser, opened as the
    /// screen opens, then choose the empty folder the library lives in, or make
    /// one, in the one folder picker both providers share.
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
                if browser.listing != nil {
                    folderChoice()
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
                        Notice(error: error)
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

        private func folderChoice() -> some View {
            SetupStep(
                ready: browser.target != nil && !browser.isNaming,
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
                claim: { browser.place($0) },
                checkedFolder: browser.target?.name,
                leading: {
                    HStack(spacing: ThemeSpace.related) {
                        Button("New folder…") { browser.startNaming() }
                            .disabled(browser.isNaming || browser.isWorking)
                        Button("Look again") { browser.lookAgain() }
                            .disabled(browser.isNaming || browser.isWorking)
                    }
                },
                fields: {
                    FolderPicker(browser: browser)
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
                            Notice(error: error)
                        }
                    }
                }
            )
        }
    }

    /// The provider call folder choice is running, named beside its spinner: the
    /// person signing in, the provider listing folders, or OneDrive making one.
    struct FolderBrowserWorkLine: View {
        let work: StorageFolderBrowser.Work
        let provider: StorageFolderBrowser.Provider

        var body: some View {
            MacSpinner(Text(title))
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
            case (.makingFolder(let name), .dropbox):
                LocalizedStringResource(
                    "Making and sharing the \(name) folder…",
                    comment:
                        "Dropbox setup, while bae makes the new shared folder the person named."
                )
            case (.makingFolder(let name), .oneDrive):
                LocalizedStringResource(
                    "Making the \(name) folder…",
                    comment:
                        "OneDrive setup, while bae makes the new folder the person named."
                )
            }
        }
    }
#endif

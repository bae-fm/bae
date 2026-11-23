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
                if let work = browser.working {
                    working(work)
                }
                else if let listing = browser.listing {
                    folderChoice(listing)
                }
                else {
                    signIn
                }
            }
            .onDisappear { browser.cancel() }
        }

        /// A provider call runs: its name over a bar, with only Cancel under
        /// it, as the screen of a setup step runs.
        private func working(_ work: StorageFolderBrowser.Work) -> some View {
            VStack(alignment: .leading, spacing: 0) {
                VStack(alignment: .leading, spacing: ThemeSpace.related) {
                    FolderBrowserWorkLine(
                        work: work,
                        provider: browser.provider
                    )
                    if work == .signingIn {
                        Text(
                            "Finish signing in in the browser window that opened, then come back here."
                        )
                        .themeText(.detail)
                        .foregroundStyle(Theme.textSecondary)
                    }
                }
                .padding(.bottom, ThemeSpace.related + ThemeSpace.line)
                MacSheetFooter {
                    MacButton("Cancel") { leave() }
                }
            }
        }

        /// The browser opens as the screen does, on the Continue that chose
        /// the provider; a failed sign-in is tried again from here.
        private var signIn: some View {
            VStack(alignment: .leading, spacing: 0) {
                if let error = browser.error {
                    MacNotice(error: error, look: .quiet)
                        .padding(.bottom, ThemeSpace.related + ThemeSpace.line)
                }
                MacSheetFooter {
                    MacButton("Cancel") { leave() }
                    MacButton("Sign in…", kind: .primary) {
                        browser.signIn()
                    }
                    .keyboardShortcut(.defaultAction)
                }
            }
            .onAppear {
                if browser.error == nil {
                    browser.signIn()
                }
            }
        }

        private func folderChoice(
            _ listing: StorageFolderBrowser.Listing
        ) -> some View {
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
                        MacButton("New folder…") { browser.startNaming() }
                            .disabled(browser.isNaming)
                        MacButton("Look again") { browser.lookAgain() }
                            .disabled(browser.isNaming)
                    }
                },
                fields: {
                    ScrollView {
                        VStack(alignment: .leading, spacing: ThemeSpace.group) {
                            MacFolderPicker(browser: browser, listing: listing)
                            if let error = browser.error {
                                MacNotice(error: error)
                            }
                        }
                    }
                    .fitsItsContent()
                    .padding(.bottom, ThemeSpace.related + ThemeSpace.line)
                }
            )
        }
    }

    /// The provider call folder choice is running, named over a bar: the
    /// person signing in, the provider listing folders, or a folder being made.
    struct FolderBrowserWorkLine: View {
        let work: StorageFolderBrowser.Work
        let provider: StorageFolderBrowser.Provider

        var body: some View {
            MacProgress(
                title: String(localized: title),
                bar: ProgressTrackBar(progress: nil)
            )
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

#if BAE_OAUTH_PROVIDERS
    import AppKit
    import BaeKit
    import Observation
    import SwiftUI
    import Testing

    @testable import bae

    /// A provider call waiting on its continuation until the test lets it
    /// answer.
    @MainActor
    @Observable
    private final class HeldCall<Value: Sendable> {
        private var waiting: CheckedContinuation<Value, Never>?

        var isWaiting: Bool { waiting != nil }

        func call() async -> Value {
            await withCheckedContinuation { waiting = $0 }
        }

        func answer(_ value: Value) {
            waiting?.resume(returning: value)
            waiting = nil
        }
    }

    /// Folder choice names the provider call it runs: signing in, then
    /// listing, and nothing once the folders show.
    @MainActor
    @Suite("Storage folder browser")
    struct StorageFolderBrowserTests {
        @Test("signing in, then listing, are each named while they run")
        func namesEachCall() async throws {
            let signIn = HeldCall<Void>()
            let folders = HeldCall<[BridgeStorageFolder]>()
            let browser = StorageFolderBrowser(
                provider: .dropbox,
                operations: StorageFolderBrowser.Operations(
                    signIn: { _ in await signIn.call() },
                    dropboxSharedFolders: { await folders.call() },
                    createDropboxSharedFolder: { _ in
                        preconditionFailure("this test makes no folder")
                    },
                    oneDriveRoot: {
                        preconditionFailure("Dropbox has no root")
                    },
                    oneDriveFolders: { _ in
                        preconditionFailure("Dropbox lists no folders inside")
                    },
                    createOneDriveFolder: { _, _ in
                        preconditionFailure("Dropbox makes no folders")
                    }
                )
            )

            browser.signIn()
            #expect(browser.working == .signingIn)
            try await Wait.until { signIn.isWaiting }
            signIn.answer(())

            try await Wait.until { folders.isWaiting }
            #expect(browser.working == .listing)
            folders.answer([])

            try await Wait.until { browser.listing != nil }
            #expect(browser.working == nil)
        }

        @Test("a Dropbox name already taken leaves the listing and names it")
        func takenNameIsNamed() async throws {
            let existing = BridgeStorageFolder(
                name: "Music",
                location: .dropbox(
                    location: .sharedFolder(namespaceId: "ns:1"),
                    accountLabel: nil,
                    folderLabel: nil
                )
            )
            let browser = StorageFolderBrowser(
                provider: .dropbox,
                operations: StorageFolderBrowser.Operations(
                    signIn: { _ in },
                    dropboxSharedFolders: { [existing] },
                    createDropboxSharedFolder: { _ in
                        throw BridgeError.Diagnostic(
                            category: .storage(failure: .alreadyExists),
                            detail: "taken"
                        )
                    },
                    oneDriveRoot: {
                        preconditionFailure("Dropbox has no root")
                    },
                    oneDriveFolders: { _ in
                        preconditionFailure("Dropbox lists no folders inside")
                    },
                    createOneDriveFolder: { _, _ in
                        preconditionFailure("Dropbox makes no OneDrive folders")
                    }
                )
            )
            browser.signIn()
            try await Wait.until { browser.listing != nil }

            browser.newFolderName = " Music "
            browser.createDropboxFolder()
            try await Wait.until { browser.takenName != nil }
            try await Wait.until { browser.working == nil }

            #expect(browser.takenName == "Music")
            #expect(browser.error == nil)
            #expect(browser.listing?.folders == [existing])
            #expect(browser.selected == nil)
        }

        @Test("the line names the call its provider runs")
        func lineNamesTheCall() {
            func title(
                _ work: StorageFolderBrowser.Work,
                _ provider: StorageFolderBrowser.Provider
            ) -> String {
                String(
                    localized: FolderBrowserWorkLine(
                        work: work,
                        provider: provider
                    )
                    .title
                )
            }
            #expect(
                title(.signingIn, .dropbox)
                    == "Signing in to Dropbox in your browser…"
            )
            #expect(
                title(.listing, .dropbox)
                    == "Reading your shared Dropbox folders…"
            )
            #expect(
                title(.listing, .oneDrive)
                    == "Reading the folders on your OneDrive…"
            )
            #expect(
                title(.makingFolder(name: "bae"), .oneDrive)
                    == "Making the folder “bae”…"
            )
            #expect(
                title(.makingFolder(name: "bae"), .dropbox)
                    == "Making and sharing the folder “bae”…"
            )
        }
    }
#endif

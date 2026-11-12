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

    private func dropboxFolder(_ name: String) -> BridgeStorageFolder {
        BridgeStorageFolder(
            name: name,
            location: .dropbox(
                location: .sharedFolder(namespaceId: name),
                accountLabel: nil,
                folderLabel: nil
            )
        )
    }

    /// A flat provider's calls: its shared folders, and what making one does.
    private func flatOperations(
        folders: @escaping @Sendable () async -> [BridgeStorageFolder],
        make: @escaping @Sendable (String) async throws -> BridgeStorageFolder
    ) -> StorageFolderBrowser.Operations {
        StorageFolderBrowser.Operations(
            signIn: { _ in },
            top: {
                StorageFolderBrowser.Listing(path: [], folders: await folders())
            },
            inside: nil,
            make: { _, name in try await make(name) }
        )
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
                    top: {
                        StorageFolderBrowser.Listing(
                            path: [],
                            folders: await folders.call()
                        )
                    },
                    inside: nil,
                    make: { _, _ in
                        preconditionFailure("this test makes no folder")
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

        @Test("a name already taken leaves the listing and names it")
        func takenNameIsNamed() async throws {
            let existing = dropboxFolder("Music")
            let browser = StorageFolderBrowser(
                provider: .dropbox,
                operations: flatOperations(
                    folders: { [existing] },
                    make: { _ in
                        throw BridgeError.Diagnostic(
                            category: .storage(failure: .alreadyExists),
                            detail: "taken"
                        )
                    }
                )
            )
            browser.signIn()
            try await Wait.until { browser.listing != nil }

            browser.newFolderName = " Music "
            browser.createFolder()
            try await Wait.until { browser.takenName != nil }
            try await Wait.until { browser.working == nil }

            #expect(browser.takenName == "Music")
            #expect(browser.error == nil)
            #expect(browser.listing?.folders == [existing])
            #expect(browser.selected == nil)
        }

        @Test("a made folder joins the list and is chosen")
        func madeFolderIsChosen() async throws {
            let made = dropboxFolder("bae")
            let browser = StorageFolderBrowser(
                provider: .dropbox,
                operations: flatOperations(folders: { [] }, make: { _ in made })
            )
            browser.signIn()
            try await Wait.until { browser.listing != nil }
            browser.startNaming()
            browser.newFolderName = "bae"
            browser.createFolder()
            try await Wait.until { browser.selected == made }
            try await Wait.until { browser.working == nil }

            #expect(browser.listing?.folders == [made])
            #expect(browser.target == made)
            #expect(!browser.isNaming)
        }

        @Test("a folder setup found holding other files is not set up again")
        func notEmptyFolderIsNamed() async throws {
            let music = dropboxFolder("Music")
            let other = dropboxFolder("Photos")
            let browser = StorageFolderBrowser(
                provider: .dropbox,
                operations: flatOperations(
                    folders: { [music, other] },
                    make: { _ in music }
                )
            )
            browser.signIn()
            try await Wait.until { browser.listing != nil }
            browser.select(music)

            let taken = BridgeError.Diagnostic(
                category: .storage(failure: .ownLocationRequired),
                detail: ""
            )
            #expect(browser.place(taken))
            #expect(browser.notEmpty == music)
            #expect(browser.target == nil)

            browser.select(other)
            #expect(browser.target == other)
            #expect(
                !browser.place(
                    BridgeError.Diagnostic(
                        category: .storage(failure: .authentication),
                        detail: ""
                    )
                )
            )
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
                title(.makingFolder, .oneDrive)
                    == "Making the folder…"
            )
            #expect(
                title(.makingFolder, .dropbox)
                    == "Making and sharing the folder…"
            )
        }
    }
#endif

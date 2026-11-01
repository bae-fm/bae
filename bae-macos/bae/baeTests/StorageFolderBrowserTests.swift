#if BAE_OAUTH_PROVIDERS
    import AppKit
    import BaeKit
    import SwiftUI
    import Testing

    @testable import bae

    /// A provider call waiting on its continuation until the test lets it
    /// answer.
    @MainActor
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

        @Test("the line names the listing its provider runs")
        func lineNamesTheListing() async throws {
            let size = NSSize(width: 420, height: 60)
            try await SnapshotTestSupport.withHostedWindow(
                FolderBrowserWorkLine(work: .listing, provider: .dropbox)
                    .padding()
                    .frame(width: size.width, height: size.height),
                size: size
            ) { _, host in
                try await SnapshotTestSupport.settle(host)
                let png = try await SnapshotTestSupport.capturePNG(
                    host,
                    size: size
                )
                let lines =
                    try await SnapshotTestSupport.recognizedText(
                        in: png
                    )
                    .map(\.text)
                #expect(
                    lines.carrying(
                        String(
                            localized: "Reading your shared Dropbox folders…"
                        )
                        .replacingOccurrences(of: "…", with: "")
                    ),
                    "drew \(lines)"
                )
            }
        }
    }
#endif

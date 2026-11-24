#if BAE_OAUTH_PROVIDERS
    import AppKit
    import BaeKit
    import SwiftUI
    import Testing

    @testable import bae

    /// The folder picker's two problem lines, the folder holding other files
    /// and the name already taken, are quiet notices: the danger glyph beside
    /// the words, and no fill. The folder picker's design draws the first one,
    /// and the app draws the second the same way.
    @MainActor
    @Suite("Folder picker notices")
    struct MacFolderPickerNoticeTests {
        private static let size = NSSize(width: 460, height: 420)

        private static func folder(_ name: String) -> BridgeStorageFolder {
            BridgeStorageFolder(
                name: name,
                location: .dropbox(
                    location: .sharedFolder(namespaceId: name),
                    accountLabel: nil,
                    folderLabel: nil
                )
            )
        }

        @Test("a folder holding other files is a quiet danger notice")
        func notEmptyIsANotice() async throws {
            let music = Self.folder("Music")
            let browser = StorageFolderBrowser(
                staged: .dropbox,
                listing: StorageFolderBrowser.Listing(
                    path: [],
                    folders: [music]
                ),
                nests: false,
                selected: music,
                notEmpty: music
            )
            try await Self.expectQuietNotice(browser)
        }

        @Test("a name already taken is a quiet danger notice")
        func nameTakenIsANotice() async throws {
            let existing = Self.folder("Music")
            let browser = StorageFolderBrowser(
                provider: .dropbox,
                operations: StorageFolderBrowser.Operations(
                    signIn: { _ in },
                    top: {
                        StorageFolderBrowser.Listing(
                            path: [],
                            folders: [existing]
                        )
                    },
                    inside: nil,
                    make: { _, _ in
                        throw BridgeError.Diagnostic(
                            category: .storage(failure: .alreadyExists),
                            detail: "taken"
                        )
                    }
                )
            )
            browser.signIn()
            try await Wait.until { browser.listing != nil }
            browser.newFolderName = "Music"
            browser.createFolder()
            try await Wait.until { browser.takenName != nil }
            try await Wait.until { browser.working == nil }
            try await Self.expectQuietNotice(browser)
        }

        /// The picker drawn in light shows the danger glyph's red and no box
        /// filled in the danger notice's pale pink.
        private static func expectQuietNotice(
            _ browser: StorageFolderBrowser
        ) async throws {
            let listing = try #require(browser.listing)
            let view = MacFolderPicker(browser: browser, listing: listing)
                .appearance(mode: .light, accent: .standard, tone: .standard)
            try await SnapshotTestSupport.withHostedWindow(
                view,
                size: size
            ) { window, host in
                window.appearance = NSAppearance(named: .aqua)
                try await SnapshotTestSupport.settle(host)
                let bitmap = try #require(
                    host.bitmapImageRepForCachingDisplay(in: host.bounds)
                )
                host.cacheDisplay(in: host.bounds, to: bitmap)
                var red = 0
                var palePink = 0
                for y in 0..<bitmap.pixelsHigh {
                    for x in 0..<bitmap.pixelsWide {
                        guard
                            let c = bitmap.colorAt(x: x, y: y)?
                                .usingColorSpace(.sRGB)
                        else { continue }
                        let overGreen = c.redComponent - c.greenComponent
                        let overBlue = c.redComponent - c.blueComponent
                        guard c.redComponent > 0.85 else { continue }
                        if overGreen > 0.4, overBlue > 0.4 {
                            red += 1
                        }
                        else if overGreen > 0.04, overBlue > 0.04 {
                            palePink += 1
                        }
                    }
                }
                #expect(red > 0, "the danger glyph is drawn")
                // The glyph's smoothed edges are pale too; a filled notice
                // is thousands of pale pixels.
                #expect(palePink < 1000, "the notice has no fill")
            }
        }
    }
#endif

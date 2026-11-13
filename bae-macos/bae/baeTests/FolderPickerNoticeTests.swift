#if BAE_OAUTH_PROVIDERS
    import AppKit
    import BaeKit
    import SwiftUI
    import Testing

    @testable import bae

    /// The folder picker's two problem lines, the folder holding other files
    /// and the name already taken, are notices as the folder picker board
    /// draws them: a box filled in the danger notice colour, not red text.
    @MainActor
    @Suite("Folder picker notices")
    struct FolderPickerNoticeTests {
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

        @Test("a folder holding other files is a filled danger notice")
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
            #expect(try await Self.noticeFillPixels(browser) > 5000)
        }

        @Test("a name already taken is a filled danger notice")
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
            #expect(try await Self.noticeFillPixels(browser) > 5000)
        }

        /// How many pixels of the picker drawn in light are the pale pink of
        /// the danger notice's fill, which the section's surface shows
        /// through. Red text never fills an area; a notice's box does.
        private static func noticeFillPixels(
            _ browser: StorageFolderBrowser
        ) async throws -> Int {
            let view = Form { FolderPicker(browser: browser) }
                .formStyle(.grouped)
                .appearance(mode: .light, accent: .standard, tone: .standard)
            return try await SnapshotTestSupport.withHostedWindow(
                view,
                size: size
            ) { window, host in
                window.appearance = NSAppearance(named: .aqua)
                try await SnapshotTestSupport.settle(host)
                let bitmap = try #require(
                    host.bitmapImageRepForCachingDisplay(in: host.bounds)
                )
                host.cacheDisplay(in: host.bounds, to: bitmap)
                var count = 0
                for y in 0..<bitmap.pixelsHigh {
                    for x in 0..<bitmap.pixelsWide {
                        guard
                            let c = bitmap.colorAt(x: x, y: y)?
                                .usingColorSpace(.sRGB)
                        else { continue }
                        if c.redComponent > 0.85,
                            c.redComponent - c.greenComponent > 0.04,
                            c.redComponent - c.blueComponent > 0.04
                        {
                            count += 1
                        }
                    }
                }
                return count
            }
        }
    }
#endif

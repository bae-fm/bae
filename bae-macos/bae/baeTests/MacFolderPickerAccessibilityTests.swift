#if BAE_OAUTH_PROVIDERS
    import AppKit
    import ApplicationServices
    import BaeKit
    import SwiftUI
    import Testing

    @testable import bae

    @MainActor
    @Suite("Folder picker accessibility")
    struct MacFolderPickerAccessibilityTests {
        @Test("the OneDrive folder is one named button that chooses it")
        func pressingTheFolderChoosesIt() async throws {
            let music = BridgeStorageFolder(
                name: "Music",
                location: .oneDrive(
                    driveId: "drive",
                    folderId: "music",
                    accountLabel: nil,
                    folderLabel: nil
                )
            )
            let listing = StorageFolderBrowser.Listing(
                path: [],
                folders: [music]
            )
            let browser = StorageFolderBrowser(
                staged: .oneDrive,
                listing: listing,
                nests: true
            )

            try await SnapshotTestSupport.withHostedWindow(
                MacFolderPicker(browser: browser, listing: listing),
                size: NSSize(width: 460, height: 180)
            ) { window, host in
                try await SnapshotTestSupport.settle(host)
                let subject = "the OneDrive folder picker"
                let tree = try AccessibilityTree.read(
                    of: window,
                    subject: subject
                )
                let names = tree.nodes.filter { $0.role == kAXButtonRole }
                    .map { $0.title ?? $0.description }
                #expect(names == ["Music"])

                let buttons = try AccessibilityTree.elements(
                    of: window,
                    roles: [kAXButtonRole],
                    subject: subject
                )
                let folder = try #require(buttons.first)
                #expect(
                    AXUIElementPerformAction(folder, kAXPressAction as CFString)
                        == .success
                )
                try await Wait.until { browser.selected == music }
            }
        }
    }
#endif

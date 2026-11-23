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
        @Test("one named OneDrive folder button chooses and opens the folder")
        func choosingAndOpeningTheFolder() async throws {
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
                provider: .oneDrive,
                operations: StorageFolderBrowser.Operations(
                    signIn: { _ in },
                    top: { listing },
                    inside: { _ in [] },
                    make: { _, _ in throw CancellationError() }
                )
            )
            browser.signIn()
            try await Wait.until { browser.listing != nil }

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
                let nodes = tree.nodes.filter { $0.role == kAXButtonRole }
                #expect(nodes.map { $0.title ?? $0.description } == ["Music"])
                let node = try #require(nodes.first)

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
                try await Wait.untilLaidOut(host) {
                    try AccessibilityTree.read(of: window, subject: subject)
                        .nodes
                        .contains {
                            $0.role == kAXButtonRole
                                && ($0.title ?? $0.description) == "Music"
                                && $0.selected == true
                        }
                }

                try openFolder(at: node, in: window)
                try await Wait.until { browser.listing?.path == [music] }
            }
        }

        private func openFolder(
            at node: AccessibilityTree.Node,
            in window: NSWindow
        ) throws {
            let point = window.convertPoint(
                toScreen: NSPoint(
                    x: node.x + node.width / 2,
                    y: window.frame.height - node.y - node.height / 2
                )
            )
            let element = try #require(
                window.accessibilityHitTest(point) as? NSAccessibilityProtocol
            )
            let action = try #require(
                element.accessibilityCustomActions()?
                    .first {
                        $0.name == String(localized: "Open folder")
                    }
            )
            let perform = try #require(action.handler)
            #expect(perform())
        }
    }
#endif

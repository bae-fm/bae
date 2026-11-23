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
        @Test(
            "VoiceOver reads the Dropbox folder picker heading before the rows"
        )
        func readsHeadingBeforeFolderRows() async throws {
            let music = BridgeStorageFolder(
                name: "Music",
                location: .dropbox(
                    location: .sharedFolder(namespaceId: "music"),
                    accountLabel: nil,
                    folderLabel: nil
                )
            )
            let listing = StorageFolderBrowser.Listing(
                path: [],
                folders: [music]
            )
            let browser = StorageFolderBrowser(
                staged: .dropbox,
                listing: listing,
                nests: false,
                selected: music
            )
            try await SnapshotTestSupport.withHostedWindow(
                MacFolderPicker(browser: browser, listing: listing),
                size: NSSize(width: 460, height: 180)
            ) { window, host in
                try await SnapshotTestSupport.settle(host)
                let tree = try AccessibilityTree.read(
                    of: window,
                    subject: "the Dropbox folder picker"
                )
                let words = tree.nodes.compactMap { node -> String? in
                    guard node.role == kAXStaticTextRole,
                        case .text(let words)? = node.value
                    else { return nil }
                    return words
                }
                #expect(
                    words == [
                        String(localized: browser.provider.words.heading),
                        music.name,
                    ]
                )
            }
        }

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
                #expect(nodes.map(\.controlName) == ["Music"])
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
                                && $0.controlName == "Music"
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
            let point = window.convertPoint(toScreen: node.center(in: window))
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

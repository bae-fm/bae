import AppKit
import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("LibraryRemovalConfirmation.message")
struct LibrarySettingsTabForgetMessageTests {

    @Test("synced and never-synced messages are distinct and non-empty")
    func variantsDiffer() {
        let synced = LibraryRemovalConfirmation.message(hasCloudHome: true)
        let local = LibraryRemovalConfirmation.message(hasCloudHome: false)
        #expect(!synced.isEmpty)
        #expect(!local.isEmpty)
        #expect(synced != local)
    }
}

/// The sync section shows the folder's name coven recorded at the latest
/// connection, which may have come after the library opened.
@MainActor
@Suite("The Library tab's location")
struct LibrarySettingsTabStorageReadTests {

    @Test("opening the tab reads the location coven recorded since")
    func openingTheTabReadsTheLocation() async throws {
        let renamed = PreviewData.previewDropboxStorage
        let configStore = PreviewData.connectedConfigStore(storage: renamed)
        // The preview's storage answers with the renamed folder, while the
        // store still holds what the library read as it opened.
        let tab = librarySettingsTabPreview(
            configStore: configStore,
            reads: LibrarySettingsTabPreviewReads()
        )
        configStore.applyStorage(
            .dropbox(
                location: .sharedFolder(namespaceId: "1234567890"),
                accountLabel: "name@example.com",
                folderLabel: BridgeFolderLabel(
                    name: "Old Music",
                    webUrl: "https://www.dropbox.com/home/Old%20Music"
                )
            )
        )

        try await SnapshotTestSupport.withHostedWindow(
            tab,
            size: NSSize(width: 500, height: 640)
        ) { _, _ in
            try await Wait.until { configStore.storage == renamed }
        }
    }
}

import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Add Discogs Key... on the empty library and the empty Import tab: offered
/// while core says Discogs can't be used, gone once a key is saved, and
/// landing on the Import pane of Settings, where the key is entered.
///
/// Read off the drawn window: these are SwiftUI buttons with no AppKit
/// control behind them, so what the person sees is what is asserted.
@MainActor
@Suite("Add Discogs Key on the empty states")
struct AddDiscogsKeyButtonTests {
    private static let size = NSSize(width: 600, height: 400)
    private static let addKey = "Add Discogs Key"
    private static let importFolder = "Import Folder"

    /// How many times Settings was asked for, and on which pane.
    @MainActor
    private final class SettingsRequests {
        var count = 0
        var tabWhenOpened: SettingsTab?
    }

    @Test("an empty library with no key offers it beside Import Folder")
    func emptyLibraryOffersTheKey() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            hosted(emptyLibrary(), config: configStore(discogsUsable: false)),
            size: Self.size
        ) { _, host in
            let labels = try await text(in: host).map(\.text)

            #expect(labels.carrying(Self.importFolder))
            #expect(labels.carrying(Self.addKey))
        }
    }

    @Test("an empty library with a key does not")
    func emptyLibraryWithAKeyDoesNot() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            hosted(emptyLibrary(), config: configStore(discogsUsable: true)),
            size: Self.size
        ) { _, host in
            let labels = try await text(in: host).map(\.text)

            #expect(labels.carrying(Self.importFolder))
            #expect(!labels.carrying(Self.addKey))
        }
    }

    @Test(
        "an empty Import tab offers it only while there is no key",
        arguments: [false, true]
    )
    func emptyImportTabFollowsTheKey(discogsUsable: Bool) async throws {
        try await SnapshotTestSupport.withHostedWindow(
            hosted(
                ImportEmptyState(),
                config: configStore(discogsUsable: discogsUsable)
            ),
            size: Self.size
        ) { _, host in
            let labels = try await text(in: host).map(\.text)

            #expect(labels.carrying("Add a folder to import music from"))
            #expect(labels.carrying(Self.addKey) == !discogsUsable)
        }
    }

    /// Core sends the new config once the key is stored; the button reads
    /// that and goes without the view being rebuilt.
    @Test("saving a key takes the button away")
    func savingAKeyTakesTheButtonAway() async throws {
        let config = configStore(discogsUsable: false)
        try await SnapshotTestSupport.withHostedWindow(
            hosted(emptyLibrary(), config: config),
            size: Self.size
        ) { _, host in
            let before = try await text(in: host).map(\.text)
            try #require(before.carrying(Self.addKey))

            config.config = configStore(discogsUsable: true).config

            let after = try await text(in: host).map(\.text)
            #expect(!after.carrying(Self.addKey))
        }
    }

    @Test("pressing it opens Settings on the Import pane")
    func pressingItOpensImportSettings() async throws {
        let navigation = SettingsNavigation()
        let requests = SettingsRequests()
        let view = hosted(
            emptyLibrary(),
            config: configStore(discogsUsable: false),
            navigation: navigation,
            openSettings: OpenSettingsWindowAction {
                requests.count += 1
                requests.tabWhenOpened = navigation.selectedTab
            }
        )
        try await SnapshotTestSupport.withHostedWindow(view, size: Self.size) {
            window,
            host in
            let lines = try await text(in: host)
            let button = try #require(
                lines.first {
                    $0.text.contains(Self.addKey)
                        && !$0.text.contains(Self.importFolder)
                }
            )

            try HostedInput.click(
                at: NSPoint(
                    x: button.boundingBox.midX * Self.size.width,
                    y: button.boundingBox.midY * Self.size.height
                ),
                in: window
            )
            try await SnapshotTestSupport.settle(host)

            #expect(requests.count == 1)
            #expect(requests.tabWhenOpened == .importing)
        }
    }

    // MARK: - Hosting

    private func emptyLibrary() -> some View {
        LibraryEmptyState(title: "No albums")
    }

    private func configStore(discogsUsable: Bool) -> ConfigStore {
        PreviewData.makeConfigStore(
            libraryFullWidth: false,
            discogsUsable: discogsUsable
        )
    }

    private func hosted<Content: View>(
        _ content: Content,
        config: ConfigStore,
        navigation: SettingsNavigation = SettingsNavigation(),
        openSettings: OpenSettingsWindowAction = OpenSettingsWindowAction {}
    ) -> some View {
        let uiStore = UiStore()
        return
            content
            .environment(uiStore)
            .environment(
                PreviewData.countedBrowseSession(
                    albumCount: 0,
                    uiStore: uiStore
                )
            )
            .environment(config)
            .environment(navigation)
            .environment(\.openSettingsWindow, openSettings)
            .frame(width: Self.size.width, height: Self.size.height)
            .windowBackground()
    }

    private func text(
        in host: NSView
    ) async throws -> [SnapshotTestSupport.RecognizedLine] {
        try await SnapshotTestSupport.settle(host)
        let png = try await SnapshotTestSupport.capturePNG(
            host,
            size: Self.size
        )
        return try await SnapshotTestSupport.recognizedText(in: png)
    }
}

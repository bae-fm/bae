import AppKit
import SwiftUI
import Testing

@testable import bae

/// An album whose page could not be read may be on the list twice, and its
/// card says so.
@MainActor
@Suite("A card whose album links could not be read")
struct AlbumLinksUnreadTests {
    @Test("the card says its album may also be listed separately")
    func theCardSaysSo() async throws {
        let size = NSSize(width: 660, height: 160)
        let unread = try await FindOnlineRendering.text(
            ReleaseGroupCard(group: PreviewData.searchGroupLinksUnread)
                .importPreviewEnvironment(),
            size: size
        )
        let read = try await FindOnlineRendering.text(
            ReleaseGroupCard(group: PreviewData.searchGroupExact)
                .importPreviewEnvironment(),
            size: size
        )
        #expect(
            unread.contains { $0.contains("may also be listed separately") }
        )
        #expect(!read.contains { $0.contains("may also be listed separately") })
    }
}

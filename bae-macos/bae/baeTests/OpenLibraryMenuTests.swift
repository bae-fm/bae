import AppKit
import BaeKit
import Testing

@testable import bae

/// File ▸ Open Library's items follow this Mac's libraries while the menu is
/// closed, and hold while it is open.
@MainActor
@Suite("Open Library menu")
struct OpenLibraryMenuTests {
    @Test("Open Library follows the libraries while it is closed")
    func openLibraryFollowsTheLibraries() {
        let openLibrary = OpenLibraryMenu()
        #expect(
            Self.libraryTitles(openLibrary.menu) == ["No Libraries"]
        )

        var active = PreviewData.welcomeLibraries[0]
        active.isActive = true
        let other = PreviewData.welcomeLibraries[1]
        openLibrary.show([active, other])
        let items = openLibrary.menu.items
        #expect(
            Self.libraryTitles(openLibrary.menu) == [active.name, other.name]
        )
        #expect(items[0].state == .on)
        #expect(items[1].state == .off)
        #expect(items.prefix(2).map(\.keyEquivalent) == ["1", "2"])
        #expect(items[0].keyEquivalentModifierMask == [.command, .shift])
        #expect(items[1].representedObject as? String == other.id)

        // An open menu keeps the items it opened with.
        openLibrary.menuWillOpen(openLibrary.menu)
        openLibrary.show([other])
        #expect(
            openLibrary.menu.items.map(ObjectIdentifier.init)
                == items.map(ObjectIdentifier.init)
        )
        openLibrary.menuDidClose(openLibrary.menu)
        #expect(Self.libraryTitles(openLibrary.menu) == [other.name])
    }

    /// The titles of Open Library's items above its separator.
    private static func libraryTitles(_ menu: NSMenu) -> [String] {
        Array(menu.items.prefix { !$0.isSeparatorItem }.map(\.title))
    }
}

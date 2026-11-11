import BaeKit
import Foundation
import Testing

@testable import bae

/// The element list a shot writes, proved on one known shot: the Enter code
/// sheet, whose words, buttons and field the board draws.
@MainActor
struct ShotElementListTests {
    private static let board = "mac.welcome.add-code.entering"

    private func list() async throws -> ShotElementList {
        let scene = try #require(
            ShotScene.all.first { $0.id == Self.board }
        )
        return
            try await ShotRenderer.render(
                scene,
                mode: .dark,
                tone: .standard,
                accent: .standard
            )
            .elements
    }

    /// What the tree holds today: the sheet's text field, an AppKit view,
    /// with its box. SwiftUI's own elements (the title, the message, the
    /// buttons) are absent: the hosting view lists none in this process.
    @Test
    func listsTheSheetsTextField() async throws {
        let list = try await list()
        let json = try #require(String(bytes: try list.json(), encoding: .utf8))

        #expect(list.board == Self.board, "\(json)")
        #expect(list.platform == "macos")
        // The sheet is 190 points tall; the hosted view lays out a point short.
        #expect(abs(list.size.height - 190) <= 1, "\(json)")
        #expect(list.size.width == 400)
        #expect(list.scale == 2)

        let field = try #require(
            list.elements.first { $0.role == "field" },
            "\(json)"
        )
        #expect(field.native == "AXTextField")
        #expect(field.enabled)
        #expect(field.parent == nil)
        #expect(abs(field.bounds.x - 25) <= 1, "\(json)")
        #expect(abs(field.bounds.width - 350) <= 1, "\(json)")
        #expect(field.bounds.y > 60, "\(json)")
    }

    /// What the board's elements need and the tree lacks: the sheet's title,
    /// message and buttons. Passes the day the tree lists them.
    @Test
    func listsTheSheetsWordsAndButtons() async throws {
        let list = try await list()
        await withKnownIssue("SwiftUI lists no elements of its own here") {
            for name in ["Cancel", "Continue"] {
                #expect(
                    list.elements.contains {
                        $0.role == "button" && $0.label == name
                    }
                )
            }
            let title = try #require(
                list.elements.first {
                    $0.role == "text" && $0.label == "Enter code"
                }
            )
            #expect(title.text != nil)
        }
    }

    @Test
    func everyBoxLiesInTheShotAndEveryParentComesFirst() async throws {
        let list = try await list()
        for (index, element) in list.elements.enumerated() {
            #expect(element.bounds.x >= -1)
            #expect(element.bounds.y >= -1)
            #expect(
                element.bounds.x + element.bounds.width <= list.size.width + 1
            )
            #expect(
                element.bounds.y + element.bounds.height
                    <= list.size.height + 1
            )
            if let parent = element.parent {
                #expect(parent < index)
            }
        }
    }
}

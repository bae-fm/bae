import BaeKit
import Foundation
import Testing

@testable import bae

/// The element list a shot writes, proved on two known shots: the Enter code
/// sheet, whose words, buttons and field the boards draw, and an alert.
@MainActor
struct ShotElementListTests {
    private func list(_ id: String) async throws -> ShotElementList {
        let scene = try #require(ShotScene.all.first { $0.id == id })
        return try await ShotRenderer.render(scene).elements
    }

    private func json(_ list: ShotElementList) throws -> String {
        try #require(String(bytes: try list.json(), encoding: .utf8))
    }

    @Test
    func listsTheSheetsWordsFieldAndButtons() async throws {
        let list = try await list("mac.welcome.add-code.entering")
        let json = try json(list)

        #expect(list.board == "mac.welcome.add-code.entering", "\(json)")
        #expect(list.platform == "macos")
        #expect(list.size.width == 400)
        #expect(abs(list.size.height - 190) <= 1, "\(json)")
        #expect(list.scale == 2)

        let title = try #require(
            list.elements.first {
                $0.role == "text" && $0.label == "Enter code"
            },
            "\(json)"
        )
        #expect(title.text?.size == 13, "\(json)")
        #expect(title.text?.weight == 600, "\(json)")
        #expect(title.text?.color.hasPrefix("#FFFFFF") == true, "\(json)")
        #expect(abs(title.bounds.x - 16) <= 1, "\(json)")
        #expect(abs(title.bounds.y - 16) <= 1, "\(json)")

        let message = try #require(
            list.elements.first {
                $0.role == "text"
                    && $0.label
                        == "Enter your restore code, or an invite to a library."
            },
            "\(json)"
        )
        #expect(message.text?.weight == 400, "\(json)")

        let field = try #require(
            list.elements.first { $0.role == "field" },
            "\(json)"
        )
        #expect(field.native == "AXTextField")
        #expect(field.placeholder == "Restore code or invite", "\(json)")
        #expect(abs(field.bounds.x - 26) <= 1, "\(json)")
        #expect(abs(field.bounds.width - 348) <= 1, "\(json)")

        let cancel = try #require(
            list.elements.first { $0.role == "button" && $0.label == "Cancel" },
            "\(json)"
        )
        #expect(cancel.enabled)
        let next = try #require(
            list.elements.first {
                $0.role == "button" && $0.label == "Continue"
            },
            "\(json)"
        )
        #expect(!next.enabled, "nothing is typed yet: \(json)")
    }

    /// An alert is drawn from AppKit's views; its title, message and
    /// buttons are listed where the image draws them.
    @Test
    func listsAnAlertsWordsAndButtons() async throws {
        let list = try await list("mac.welcome.remove.failed")
        let json = try json(list)

        let texts = list.elements.filter { $0.role == "text" }
        #expect(
            texts.compactMap(\.label) == [
                "Couldn’t remove “Home music”",
                "Couldn’t reach the cloud. Check your connection and try again.",
            ],
            "\(json)"
        )
        #expect(texts.first?.text?.weight == 700, "\(json)")
        let buttons = list.elements.filter { $0.role == "button" }
        #expect(buttons.compactMap(\.label) == ["Cancel", "Retry"], "\(json)")
        try #require(buttons.count == 2, "\(json)")
        // The buttons sit side by side below the message, inside the shot.
        #expect(buttons[0].bounds.y == buttons[1].bounds.y, "\(json)")
        #expect(buttons[0].bounds.x < buttons[1].bounds.x, "\(json)")
        let message = try #require(texts.last, "\(json)")
        #expect(buttons[0].bounds.y > message.bounds.y, "\(json)")
    }

    @Test
    func everyBoxLiesInTheShotAndEveryParentComesFirst() async throws {
        for id in [
            "mac.welcome.add-code.entering", "mac.welcome.libraries",
            "mac.welcome.remove.failed",
        ] {
            let list = try await list(id)
            for (index, element) in list.elements.enumerated() {
                #expect(element.bounds.x >= -1)
                #expect(element.bounds.y >= -1)
                #expect(
                    element.bounds.x + element.bounds.width
                        <= list.size.width + 1
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
}

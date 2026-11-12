import BaeKit
import Foundation
import Testing

@testable import bae

/// The element list a shot writes, proved on two known shots: the Enter code
/// sheet, whose words, buttons and field the boards draw, and an alert, each
/// over the welcome window.
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
        #expect(list.size.width == 690)
        #expect(list.size.height == 600)
        // The sheet hangs 36 points below the window's top, centred.
        let left = (list.size.width - 400) / 2
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
        #expect(abs(title.bounds.x - (left + 16)) <= 1, "\(json)")
        #expect(abs(title.bounds.y - (36 + 16)) <= 1, "\(json)")

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
        #expect(abs(field.bounds.x - (left + 26)) <= 1, "\(json)")
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

        func element(_ role: String, _ label: String) throws
            -> ShotElementList.Element
        {
            try #require(
                list.elements.first { $0.role == role && $0.label == label },
                "\(json)"
            )
        }
        let title = try element("text", "Could not remove Home music")
        let message = try element(
            "text",
            "Could not reach the cloud. Check your connection and try again."
        )
        #expect(title.text?.weight == 700, "\(json)")
        let cancel = try element("button", "Cancel")
        let retry = try element("button", "Retry")
        // The buttons sit side by side below the message, inside the shot.
        #expect(cancel.bounds.y == retry.bounds.y, "\(json)")
        #expect(cancel.bounds.x < retry.bounds.x, "\(json)")
        #expect(cancel.bounds.y > message.bounds.y, "\(json)")
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

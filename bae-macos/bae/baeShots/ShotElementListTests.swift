import BaeKit
import Foundation
import Testing

@testable import bae

/// The element list a shot writes, proved on two known shots: the Enter code
/// sheet and the Appearance settings pane, whose words, buttons, field and
/// pop-ups the boards draw.
@MainActor
struct ShotElementListTests {
    private func list(
        _ id: String,
        tone: SurfaceTone = .standard,
        accent: AccentChoice = .standard
    ) async throws -> ShotElementList {
        let scene = try #require(ShotScene.all.first { $0.id == id })
        return
            try await ShotRenderer.render(
                scene,
                mode: .dark,
                tone: tone,
                accent: accent
            )
            .elements
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

    /// The pane's pop-ups and accent buttons say the tone and accent the
    /// shot was staged with, which the PNG draws.
    @Test(
        arguments: [
            (SurfaceTone.slate, AccentChoice.teal, "Slate", "Teal"),
            (.midnight, .blue, "Midnight", "Blue"),
            (.plum, .purple, "Plum", "Purple"),
        ]
    )
    func listsAPaneWithPopUpsAndButtons(
        tone: SurfaceTone,
        accent: AccentChoice,
        toneName: String,
        accentName: String
    ) async throws {
        let list = try await list("appearance", tone: tone, accent: accent)
        let json = try json(list)

        let theme = try #require(
            list.elements.first { $0.role == "text" && $0.label == "Theme" },
            "\(json)"
        )
        #expect(theme.text?.size == 13, "\(json)")
        let popups = list.elements.filter { $0.role == "popup" }
        #expect(popups.compactMap(\.value) == ["Dark", toneName], "\(json)")
        for popup in popups {
            #expect(popup.bounds.height > 20, "\(json)")
        }
        #expect(
            list.elements.contains {
                $0.role == "text" && $0.label == accentName
            },
            "\(json)"
        )
        let chosen = list.elements.filter {
            $0.role == "button" && $0.selected == true
        }
        #expect(chosen.map(\.label) == [accentName], "\(json)")
    }

    @Test
    func everyBoxLiesInTheShotAndEveryParentComesFirst() async throws {
        for id in ["mac.welcome.add-code.entering", "appearance"] {
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

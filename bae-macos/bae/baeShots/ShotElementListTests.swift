import AppKit
import BaeKit
import Foundation
import Testing

@testable import bae

/// The element list a shot writes, proved on two known shots: the Enter code
/// sheet, whose words, buttons and field the boards draw, and an alert, each
/// over the welcome window.
@MainActor
struct ShotElementListTests {
    private let display: ShotDisplay

    init() async throws {
        display = try await ShotDisplay.shared()
    }

    private func list(_ id: String) async throws -> ShotElementList {
        var scene = try #require(ShotScene.all.first { $0.id == id })
        // A component is drawn at its board's size, which only a builds
        // shots run has; these tests read the tree, so the scene's own
        // room is its frame.
        if case .component = scene.frame { scene.frame = .content }
        return try await ShotRenderer.render(scene, on: display).elements
    }

    private func json(_ list: ShotElementList) throws -> String {
        try #require(String(bytes: try list.json(), encoding: .utf8))
    }

    @Test
    func unsyncedLibraryRowListsTheStatusAndBothButtons() async throws {
        let list = try await list("mac.settings.sync-setup.choose-cloud")

        #expect(
            list.elements.contains {
                $0.role == "button"
                    && $0.label == "Sync, This library is only on this Mac."
            }
        )
        #expect(
            list.elements.contains {
                $0.role == "text" && $0.label == "Not synced"
            }
        )
        #expect(
            list.elements.contains {
                $0.role == "button" && $0.label == "Set up a cloud home…"
            }
        )
    }

    @Test(arguments: ["empty", "filled"])
    func listsSecureFieldsByWhetherTheyAreFilled(state: String) async throws {
        let list = try await list("mac.settings.sync.s3-replace-key-\(state)")
        let secret = try #require(
            list.elements.first {
                $0.role == "field" && $0.label == "Secret access key"
            }
        )
        #expect(
            secret.native == NSAccessibility.Subrole.secureTextField.rawValue
        )
        #expect(secret.value == (state == "filled" ? "\u{F79A}" : nil))

        let accessKey = try #require(
            list.elements.first {
                $0.role == "field" && $0.label == "Access key ID"
            }
        )
        #expect(accessKey.native == NSAccessibility.Role.textField.rawValue)
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
        #expect(title.text?.weight == 600, "\(json)")
        let cancel = try element("button", "Cancel")
        let retry = try element("button", "Retry")
        // The buttons sit side by side below the message, inside the shot.
        #expect(cancel.bounds.y == retry.bounds.y, "\(json)")
        #expect(cancel.bounds.x < retry.bounds.x, "\(json)")
        #expect(cancel.bounds.y > message.bounds.y, "\(json)")
    }

    /// A titled window lists its title before its content, as VoiceOver reads
    /// it and a board draws it, and leaves out the window's close, minimise
    /// and zoom buttons, which a board hides. The window sits past every
    /// display, so it takes no focus.
    @Test
    func listsATitledWindowsTitleFirstWithoutItsButtons() throws {
        let size = CGSize(width: 400, height: 200)
        let right = NSScreen.screens.map(\.frame.maxX).max() ?? 0
        let window = NSWindow(
            contentRect: NSRect(
                origin: NSPoint(x: right + 1_000, y: 0),
                size: size
            ),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.animationBehavior = .none
        window.isReleasedWhenClosed = false
        window.title = "Window Title"
        let body = NSTextField(labelWithString: "Body words")
        body.frame = NSRect(x: 20, y: 20, width: 200, height: 20)
        window.contentView?.addSubview(body)
        window.orderFront(nil)
        defer { window.close() }

        let list = try ShotElementList.read(
            board: "a titled window",
            window: window,
            size: size,
            scale: 2
        )
        let json = try json(list)
        #expect(
            list.elements.compactMap(\.label) == [
                "Window Title", "Body words",
            ],
            "\(json)"
        )
        #expect(!list.elements.contains { $0.role == "button" }, "\(json)")
        for (index, element) in list.elements.enumerated() {
            if let parent = element.parent {
                #expect(parent < index, "\(json)")
            }
        }
    }

    /// macOS reports a spinner as a busy indicator; the shot lists it with
    /// the role progress, as VoiceOver reads it.
    @Test
    func listsASpinnerAsProgress() async throws {
        let list = try await list("mac.component.spinner.alone")
        let json = try json(list)
        let spinner = try #require(
            list.elements.first { $0.native == "AXBusyIndicator" },
            "\(json)"
        )
        #expect(spinner.role == "progress", "\(json)")
    }

    /// macOS gives a radio button its checked state as a value of 1 or 0; the
    /// list writes it as `selected` and leaves the value out, as the board
    /// side does.
    @Test
    func listsARadiosCheckedStateAsSelected() {
        func radio(_ value: Double) -> ShotElementList.Element {
            ShotElementList.Element(
                node: AccessibilityTree.Node(
                    role: NSAccessibility.Role.radioButton.rawValue,
                    subrole: nil,
                    title: "Albums",
                    description: nil,
                    placeholder: nil,
                    value: .number(value),
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                    enabled: true,
                    selected: nil,
                    focused: nil,
                    style: nil,
                    parent: nil
                )
            )
        }
        #expect(radio(1).role == "radio")
        #expect(radio(1).selected == true)
        #expect(radio(1).value == nil)
        #expect(radio(0).selected == false)
        #expect(radio(0).value == nil)
    }

    @Test
    func everyBoxLiesInTheShotAndEveryParentComesFirst() async throws {
        for id in [
            "mac.welcome.add-code.entering", "mac.welcome.libraries",
            "mac.welcome.remove.failed",
        ] {
            let list = try await list(id)
            let json = try json(list)
            for (index, element) in list.elements.enumerated() {
                #expect(element.bounds.x >= -1, "\(json)")
                #expect(element.bounds.y >= -1, "\(json)")
                #expect(
                    element.bounds.x + element.bounds.width
                        <= list.size.width + 1,
                    "\(json)"
                )
                #expect(
                    element.bounds.y + element.bounds.height
                        <= list.size.height + 1,
                    "\(json)"
                )
                if let parent = element.parent {
                    #expect(parent < index, "\(json)")
                }
            }
        }
    }

    @Test
    func leavesOutAScrollViewsScrollBar() async throws {
        // Save as runs past the window's bottom, so its pane scrolls.
        let list = try await list("mac.settings.save-as")
        let json = try json(list)

        #expect(
            !list.elements.contains {
                $0.native == NSAccessibility.Role.scrollBar.rawValue
            },
            "\(json)"
        )
        #expect(
            !list.elements.contains { $0.role == "button" && $0.label == nil },
            "\(json)"
        )
    }
}

extension ShotElementListTests {
    @Test
    func listsTheSheetsWordsFieldAndButtons() async throws {
        let list = try await list("mac.welcome.add-code.entering")
        let json = try json(list)

        #expect(list.board == "mac.welcome.add-code.entering", "\(json)")
        #expect(list.platform == "macos")
        #expect(list.size.width == 690)
        #expect(list.size.height == 600)
        #expect(list.scale == 2)

        let sheet = try #require(
            list.elements.first {
                $0.native == NSAccessibility.Role.sheet.rawValue
            },
            "\(json)"
        )
        #expect(sheet.bounds.width == 400, "\(json)")
        let left = (list.size.width - sheet.bounds.width) / 2
        #expect(abs(sheet.bounds.x - left) <= 1, "\(json)")

        let title = try #require(
            list.elements.first {
                $0.role == "text" && $0.label == "Enter code"
            },
            "\(json)"
        )
        #expect(title.text?.size == 13, "\(json)")
        #expect(title.text?.weight == 600, "\(json)")
        #expect(title.text?.color.hasPrefix("#F5F5F6") == true, "\(json)")
        #expect(abs(title.bounds.x - (sheet.bounds.x + 16)) <= 1, "\(json)")
        #expect(abs(title.bounds.y - (sheet.bounds.y + 18)) <= 1, "\(json)")

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
        // VoiceOver focuses the whole box between the sheet’s 16-point insets.
        #expect(abs(field.bounds.x - (sheet.bounds.x + 16)) <= 1, "\(json)")
        #expect(
            abs(field.bounds.width - (sheet.bounds.width - 2 * 16)) <= 1,
            "\(json)"
        )

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
}

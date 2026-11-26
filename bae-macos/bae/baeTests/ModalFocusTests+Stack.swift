import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

extension ModalFocusTests {
    @Test("A layer's panel covers the window, follows it and holds the key")
    func panelFollowsAndKeepsKey() async throws {
        try await withHost { window, _, presented in
            autoreleasepool { NSApp.activate(ignoringOtherApps: true) }
            #expect(await holds { NSApp.isActive }, "the app is active")
            presented.layers = [sheetLayer()]
            let panel = try await LayerPanels.top()
            #expect(ModalWindows.shared.parent(of: panel) === window)
            #expect(panel.frame == window.frame)
            #expect(!panel.hidesOnDeactivate)
            #expect(await holds { panel.isKeyWindow }, "the layer is key")

            autoreleasepool {
                window.setFrame(
                    window.frame.offsetBy(dx: 30, dy: -20),
                    display: true
                )
            }
            #expect(await holds { panel.frame == window.frame })
            autoreleasepool {
                window.setContentSize(NSSize(width: 500, height: 350))
            }
            #expect(await holds { panel.frame == window.frame })

            autoreleasepool { window.makeKey() }
            #expect(
                await holds { panel.isKeyWindow },
                "the key came back to the layer"
            )
        }
    }

    @Test("A layer covers the frame of the view that presents it")
    func panelCoversThePresentingView() async throws {
        try await withHost { window, _, presented in
            presented.layers = [
                MacLayer(id: "sheet", cover: .presentingView) {
                    Color.red
                }
            ]
            let panel = try await LayerPanels.top()
            let view = try #require(window.contentView)
            let frame = window.convertToScreen(
                view.convert(view.bounds, to: nil)
            )
            #expect(panel.frame == frame)
        }
    }

    @Test(
        "An alert over a layer is its child, and closing the layers withdraws them top-down"
    )
    func stackWithdrawsTopDown() async throws {
        try await withHost { window, _, presented in
            autoreleasepool { NSApp.activate(ignoringOtherApps: true) }
            presented.layers = [sheetLayer(), documentLayer()]
            try await SnapshotTestSupport.untilWindows {
                ModalWindows.shared.layerPanels.count == 2
            }
            let panels = ModalWindows.shared.layerPanels
            #expect(ModalWindows.shared.parent(of: panels[0]) === window)
            #expect(ModalWindows.shared.parent(of: panels[1]) === panels[0])

            presented.alert = true
            try await SnapshotTestSupport.untilWindows {
                ModalWindows.shared.panels.count == 3
            }
            let alert = try #require(ModalWindows.shared.panels.last)
            #expect(ModalWindows.shared.parent(of: alert) === panels[1])
            #expect(await holds { alert.isKeyWindow })

            presented.layers = [sheetLayer()]
            #expect(
                await holds { ModalWindows.shared.panels == [panels[0]] },
                "the alert and the layer over it are withdrawn"
            )
            #expect(
                await holds { panels[0].isKeyWindow },
                "the key went back to the window under them"
            )
            presented.layers = []
            try await LayerPanels.closed()
        }
    }

    // MARK: - Environment

    /// The words an element carries: its title, description and value.
    static func words(of node: AccessibilityTree.Node) -> [String] {
        var words = [node.title, node.description].compactMap { $0 }
        if case .text(let text) = node.value { words.append(text) }
        return words
    }

    private struct EnvironmentProbe: View {
        @Environment(UiStore.self)
        private var uiStore
        @Environment(\.colorScheme)
        private var scheme

        var body: some View {
            Text(
                verbatim:
                    "\(uiStore.document?.name ?? "no document") \(scheme == .dark ? "dark" : "light")"
            )
        }
    }

    private struct EnvironmentHost: View {
        let uiStore: UiStore
        let presented: Presented

        var body: some View {
            Color.clear
                .frame(width: 400, height: 300)
                .macLayers(presented.layers)
                .environment(uiStore)
                .environment(\.colorScheme, .dark)
        }
    }

    @Test("A layer reads the presenting view's stores and appearance")
    func layerReadsTheEnvironment() async throws {
        let uiStore = UiStore()
        uiStore.presentDocument(name: "rip.log", text: "Log")
        let presented = Presented()
        presented.layers = [
            MacLayer(id: "probe") { EnvironmentProbe() }
        ]
        try await SnapshotTestSupport.withHostedWindow(
            EnvironmentHost(uiStore: uiStore, presented: presented),
            size: NSSize(width: 400, height: 300)
        ) { _, _ in
            let panel = try await LayerPanels.top()
            try await SnapshotTestSupport.settle(
                try LayerPanels.content(of: panel)
            )
            let tree = try AccessibilityTree.read(of: panel, subject: "probe")
            #expect(
                tree.nodes.contains {
                    Self.words(of: $0).contains("rip.log dark")
                },
                "the layer reads the presenting view's store and scheme: \(tree.nodes.map { Self.words(of: $0) })"
            )
            presented.layers = []
            try await LayerPanels.closed()
        }
    }

    // MARK: - Paste

    private struct PasteLayer: View {
        @State
        private var text = ""
        @FocusState
        private var focused: Bool

        var body: some View {
            TextField("Name", text: $text)
                .focused($focused)
                .onAppear { focused = true }
                .frame(width: 200)
        }
    }

    @Test("A paste through the main menu lands in a text field of a layer")
    func pasteReachesTheLayer() async throws {
        try await withHost { _, _, presented in
            autoreleasepool { NSApp.activate(ignoringOtherApps: true) }
            presented.layers = [
                MacLayer(id: "field") {
                    MacModal(onDismiss: {}) { PasteLayer() }
                }
            ]
            let panel = try await LayerPanels.top()
            #expect(await holds { panel.isKeyWindow }, "the layer is key")
            let pasteboard = NSPasteboard.general
            let saved = pasteboard.string(forType: .string)
            defer {
                pasteboard.clearContents()
                if let saved { pasteboard.setString(saved, forType: .string) }
            }
            pasteboard.clearContents()
            pasteboard.setString("pasted words", forType: .string)

            // The Edit menu's Paste sends `paste:` down the key window's
            // responder chain.
            let field = try #require(
                SnapshotTestSupport.descendants(
                    of: try LayerPanels.content(of: panel)
                )
                .compactMap { $0 as? NSTextField }.first
            )
            HostedInput.focus(field, in: panel)
            let sent = autoreleasepool {
                NSApp.sendAction(
                    #selector(NSText.paste(_:)),
                    to: nil,
                    from: nil
                )
            }
            #expect(sent)
            try await SnapshotTestSupport.settle(
                try LayerPanels.content(of: panel)
            )
            let tree = try AccessibilityTree.read(of: panel, subject: "field")
            #expect(
                tree.nodes.contains {
                    Self.words(of: $0).contains("pasted words")
                },
                "the layer's text field holds the pasted words: \(tree.nodes.map { Self.words(of: $0) })"
            )
        }
    }

    @Test("Tab in a system sheet never reaches the window under it")
    func systemSheet() async throws {
        try await withHost { window, host, presented in
            presented.systemSheet = true
            try await SnapshotTestSupport.untilSheet(on: window, attached: true)
            let sheet = try #require(window.attachedSheet)
            let content = try #require(sheet.contentView)
            for _ in 0..<8 {
                HostedInput.selectNextKey(in: sheet)
                try await SnapshotTestSupport.settle(content)
                let responder = autoreleasepool {
                    sheet.firstResponder as? NSView
                }
                #expect(responder.map { $0.isDescendant(of: content) } ?? true)
                #expect(
                    autoreleasepool {
                        (window.firstResponder as? NSView)
                            .map { !$0.isDescendant(of: host) || $0 === host }
                            ?? true
                    }
                )
            }
        }
    }
}

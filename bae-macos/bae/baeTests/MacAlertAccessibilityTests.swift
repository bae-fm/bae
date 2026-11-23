import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// VoiceOver announces a Mac alert by its title.
@MainActor
@Suite("Mac alert accessibility")
struct MacAlertAccessibilityTests {
    @Test("the alert is one element named by its title that holds its words")
    func theAlertIsNamedByItsTitle() async throws {
        let alert = MacAlert(
            MacAlertWords(
                Text(verbatim: "Error"),
                message: Text(verbatim: "Something went wrong.")
            ),
            buttons: [MacAlertButton(Text(verbatim: "OK"))]
        )
        try await SnapshotTestSupport.withHostedWindow(
            alert,
            size: NSSize(width: 300, height: 200)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "the alert"
            )
            let named = tree.nodes.enumerated()
                .filter {
                    $0.element.description == "Error"
                }
            let element = try #require(
                named.first { entry in
                    tree.nodes.contains { $0.parent == entry.offset }
                },
                "no element named Error holds the alert's words"
            )
            func isHeld(_ index: Int) -> Bool {
                var up = tree.nodes[index].parent
                while let current = up {
                    if current == element.offset { return true }
                    up = tree.nodes[current].parent
                }
                return false
            }
            let held = tree.nodes.indices.filter(isHeld).map { tree.nodes[$0] }
            #expect(
                held.contains { $0.role == "AXButton" },
                "the named element holds the buttons"
            )
            #expect(
                held.contains { $0.role == "AXStaticText" },
                "the named element holds the words"
            )
        }
    }
}

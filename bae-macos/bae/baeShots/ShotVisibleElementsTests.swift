import CoreGraphics
import Testing

@testable import bae

/// The shots list only what a person can see: a row scrolled out of its
/// scroll area, or beyond the window's content, is left out.
struct ShotVisibleElementsTests {
    private func node(
        _ role: String,
        _ label: String,
        x: Double = 0,
        y: Double,
        width: Double = 100,
        height: Double = 20,
        parent: Int?
    ) -> AccessibilityTree.Node {
        AccessibilityTree.Node(
            role: role,
            subrole: nil,
            title: nil,
            description: label,
            placeholder: nil,
            value: nil,
            x: x,
            y: y,
            width: width,
            height: height,
            enabled: true,
            selected: nil,
            focused: nil,
            style: nil,
            parent: parent
        )
    }

    private func labels(_ tree: AccessibilityTree) -> [String?] {
        tree.nodes.map(\.description)
    }

    @Test
    func leavesOutARowBelowTheWindow() {
        let tree = AccessibilityTree(nodes: [
            node("AXButton", "inside", y: 100, parent: nil),
            node("AXButton", "below", y: 450, parent: nil),
            node("AXButton", "straddling", y: 410, parent: nil),
        ])
        let shown = tree.leavingOutWhatIsNotVisible(
            windowSize: CGSize(width: 400, height: 420)
        )
        #expect(labels(shown) == ["inside", "straddling"])
    }

    @Test
    func leavesOutARowScrolledOutOfItsScrollArea() {
        let tree = AccessibilityTree(nodes: [
            node(
                "AXScrollArea",
                "pane",
                y: 0,
                width: 400,
                height: 200,
                parent: nil
            ),
            node("AXButton", "in view", y: 50, parent: 0),
            node("AXButton", "scrolled out", y: 250, parent: 0),
            node("AXButton", "its child", y: 255, parent: 2),
            node("AXButton", "outside the pane", y: 300, parent: nil),
        ])
        let shown = tree.leavingOutWhatIsNotVisible(
            windowSize: CGSize(width: 400, height: 600)
        )
        #expect(labels(shown) == ["pane", "in view", "outside the pane"])
        #expect(shown.nodes[1].parent == 0)
    }
}

import AppKit
import BaeKit
import Testing

/// The panels of the layers that bae draws over a window (`MacLayer`), which
/// the tests read where the layer's controls live.
@MainActor
enum LayerPanels {
    /// The panel of the layer on top, once `count` layers are showing.
    static func top(count: Int = 1) async throws -> NSPanel {
        try await SnapshotTestSupport.untilWindows {
            ModalWindows.shared.layerPanels.count >= count
        }
        return try #require(ModalWindows.shared.layerPanels.last)
    }

    /// Waits until something in `panel` has the keyboard focus, which a layer's
    /// content takes as it appears.
    static func focused(_ panel: NSPanel) async throws {
        try await SnapshotTestSupport.untilWindows {
            let tree = try? AccessibilityTree.read(of: panel, subject: "layer")
            return tree?.nodes.contains { $0.focused == true } ?? false
        }
    }

    /// Waits until no layer is showing.
    static func closed() async throws {
        try await SnapshotTestSupport.untilWindows {
            ModalWindows.shared.layerPanels.isEmpty
        }
    }

    /// The panel's content view, which hosts what the layer draws.
    static func content(of panel: NSPanel) throws -> NSView {
        try #require(panel.contentView)
    }
}

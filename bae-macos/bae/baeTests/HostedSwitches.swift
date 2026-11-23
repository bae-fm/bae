import AppKit
import ApplicationServices
import Testing

/// bae's switches in a hosted window. `MacSwitch` draws its own track and
/// knob, so no AppKit control stands behind it; the one thing that holds it
/// is the system switch it gives VoiceOver. A test finds the switches in the
/// window's accessibility tree, as VoiceOver does, and presses one as a
/// person does, with a click where it is drawn: SwiftUI's switch lists no
/// press action in the tree.
@MainActor
enum HostedSwitches {
    /// Every switch `window` holds, in the order VoiceOver reads them.
    static func all(in window: NSWindow) throws -> [AccessibilityTree.Node] {
        try AccessibilityTree.read(of: window, subject: "the hosted switches")
            .nodes
            .filter { $0.role == kAXCheckBoxRole }
    }

    /// A click on `control` in `window`, which moves an enabled switch to its
    /// other side and leaves a disabled one as it is.
    static func press(
        _ control: AccessibilityTree.Node,
        in window: NSWindow
    ) throws {
        try HostedInput.click(
            at: control.center(in: window),
            in: window
        )
    }
}

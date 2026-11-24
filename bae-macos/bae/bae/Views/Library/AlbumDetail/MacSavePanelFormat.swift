import AppKit

/// The format pop-up. It reads out the chosen format as its name, as the
/// board names it.
final class MacSavePanelFormatPopUp: NSPopUpButton {
    override func accessibilityLabel() -> String? {
        titleOfSelectedItem
    }
}

/// The Save as panel's format line: the "Format:" label beside the format
/// pop-up. The panel shows it as its accessory view.
final class MacSavePanelFormat: NSView {
    /// The pop-up's frame in this view.
    static let popupFrame = NSRect(x: 54, y: 5, width: 190, height: 24)

    init(popup: MacSavePanelFormatPopUp) {
        super.init(frame: NSRect(x: 0, y: 0, width: 250, height: 34))
        let label = NSTextField(labelWithString: String(localized: "Format:"))
        label.frame = NSRect(x: 0, y: 10, width: 50, height: 14)
        addSubview(label)
        addSubview(popup)
        // VoiceOver reads a control by the label beside it only when the
        // control names that label.
        popup.setAccessibilityTitleUIElement(label)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        preconditionFailure("MacSavePanelFormat is made in code")
    }
}

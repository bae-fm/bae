import AppKit

/// The Save as panel's format line: the "Format:" label beside the format
/// pop-up. The panel shows it as its accessory view.
final class MacSavePanelFormat: NSView {
    /// The pop-up's frame in this view.
    static let popupFrame = NSRect(x: 54, y: 5, width: 190, height: 24)

    init(popup: NSPopUpButton) {
        super.init(frame: NSRect(x: 0, y: 0, width: 250, height: 34))
        let label = NSTextField(labelWithString: String(localized: "Format:"))
        label.frame = NSRect(x: 0, y: 7, width: 50, height: 20)
        addSubview(label)
        addSubview(popup)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        preconditionFailure("MacSavePanelFormat is made in code")
    }
}

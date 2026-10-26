import AppKit

extension NSTextField {
    /// Show `text`, writing the field only when it shows something else: a
    /// clock label is set at every position tick, and its text changes at
    /// most once a second.
    func show(_ text: String) {
        if stringValue != text {
            stringValue = text
        }
    }
}

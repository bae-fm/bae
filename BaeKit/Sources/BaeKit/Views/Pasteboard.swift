#if canImport(AppKit)
    import AppKit
#else
    import UIKit
#endif

/// The general pasteboard, for the shared views' copy buttons.
enum Pasteboard {
    /// Put `value` on the general pasteboard as plain text.
    static func copy(_ value: String) {
        #if canImport(AppKit)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(value, forType: .string)
        #else
            UIPasteboard.general.string = value
        #endif
    }
}
